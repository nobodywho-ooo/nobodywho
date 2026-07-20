//! Generic inference pipeline, independent of chat history.

use crate::errors::{ContextSyncError, DecodingError, MultimodalError, ReadError};
use crate::llm::{GlobalInferenceLockToken, WriteOutput, GLOBAL_INFERENCE_LOCK};
use crate::tokenizer::{
    find_chunks_prefix_difference, ProjectionModel, Tokenizer, TokenizerChunk, TokenizerChunks,
};
use llama_cpp_2::context::kv_cache::KvCacheConversionError;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::mtmd::MtmdBitmap;
use llama_cpp_2::mtmd::MtmdInputChunks;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::{LlamaStateSeqFlags, SeqState};
use std::path::Path;
use std::rc::Rc;
use std::sync::MutexGuard;
use tracing::{debug, debug_span, trace, trace_span, warn};

pub(crate) fn acquire_inference_lock() -> MutexGuard<'static, GlobalInferenceLockToken> {
    GLOBAL_INFERENCE_LOCK.lock().unwrap()
}

pub(crate) fn wrap_respond<F>(
    respond: F,
    tool_call_begin_token: Option<String>,
) -> (impl FnMut(WriteOutput), std::sync::mpsc::Receiver<String>)
where
    F: Fn(WriteOutput),
{
    let (resp_sender, resp_receiver) = std::sync::mpsc::channel();
    let mut emitting = true;

    let wrapped_respond = move |x| {
        match &x {
            WriteOutput::Token(tok) if tool_call_begin_token.as_ref() == Some(tok) => {
                emitting = false;
            }
            WriteOutput::Done(resp) => {
                resp_sender
                    .send(resp.clone())
                    .expect("Failed sending response");
            }
            WriteOutput::Token(_) | WriteOutput::Error(_) => (),
        }
        if emitting {
            respond(x)
        }
    };
    (wrapped_respond, resp_receiver)
}

/// Sequence-state snapshot used to rewind recurrent / hybrid-recurrent
/// contexts (Mamba, RWKV, Gated Delta Networks, Qwen3.5) where
/// `clear_kv_cache_seq` cannot unroll the running state.
///
/// `data` is a handle into llama.cpp's on-device state slot for
/// [`SEQ_ID`]. It is bound by the ON_DEVICE contract: getting a fresh
/// checkpoint invalidates any prior one, so at most one checkpoint can
/// exist per engine at a time.
#[derive(Debug)]
struct Checkpoint {
    data: SeqState,
    n_past: i32,
}

/// The low-level inference state for a single llama.cpp context.
///
/// Holds everything needed to read tokens/media into the KV cache and sample new tokens,
/// independent of any higher-level concept like chat history. Both `Worker` (encoder /
/// crossencoder) and `Chat` own one of these.
#[derive(Debug)]
pub(crate) struct InferenceEngine<'a> {
    pub(crate) ctx: LlamaContext<'a>,
    projection_model: Option<&'a ProjectionModel>,
    n_past: i32,
    tokenizer: Tokenizer<'a>,
    // The configured n_batch (= planned n_ctx before llama.cpp's internal rounding).
    // Used to guard against sending more tokens than the context can decode in one batch.
    n_batch: usize,
    big_batch: LlamaBatch<'a>,
    small_batch: LlamaBatch<'a>,
    use_embeddings: bool,
    /// Whether the underlying architecture needs checkpoint-based rewinds
    /// (recurrent or hybrid-recurrent memory backend). See
    /// [`crate::llm::Model::needs_checkpointing`].
    needs_checkpointing: bool,
    /// On-device snapshot of the last committed context, used to rewind
    /// on architectures where `clear_kv_cache_seq` fails on partial trims.
    /// See [`Self::save_checkpoint`] and [`Self::try_restore_checkpoint`].
    checkpoint: Option<Checkpoint>,
}

const SEQ_ID: i32 = 0;
const CHECKPOINT_FLAGS: LlamaStateSeqFlags = LlamaStateSeqFlags::from_bits(
    LlamaStateSeqFlags::PARTIAL_ONLY.bits() | LlamaStateSeqFlags::ON_DEVICE.bits(),
);

impl<'a> InferenceEngine<'a> {
    #[allow(clippy::too_many_arguments)]
    pub(crate) fn new(
        ctx: LlamaContext<'a>,
        big_batch: LlamaBatch<'a>,
        small_batch: LlamaBatch<'a>,
        projection_model: Option<&'a ProjectionModel>,
        n_batch: usize,
        tokenizer: Tokenizer<'a>,
        use_embeddings: bool,
        needs_checkpointing: bool,
    ) -> Self {
        Self {
            n_past: 0,
            ctx,
            big_batch,
            small_batch,
            projection_model,
            n_batch,
            tokenizer,
            use_embeddings,
            needs_checkpointing,
            checkpoint: None,
        }
    }

    /// True for recurrent / hybrid-recurrent architectures that need the
    /// two-pass sync + checkpoint restore path.
    pub(crate) fn needs_checkpointing(&self) -> bool {
        self.needs_checkpointing
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn reset_context(&mut self) -> &mut Self {
        self.ctx.clear_kv_cache();
        self.n_past = 0;
        // Any previously saved checkpoint refers to state that no longer exists.
        self.checkpoint = None;
        self
    }

    /// Capture an on-device snapshot of the current sequence state.
    ///
    /// Intended to be called at "stable" boundaries where the caller may
    /// later want to rewind — typically at the end of a chat sync just
    /// before generation starts. Overwrites any previously saved
    /// checkpoint (llama.cpp keeps at most one on-device slot per seq).
    ///
    /// This is a no-op on architectures that don't need it (attention-only
    /// models, where the partial-state size is 0). Failure to save is
    /// logged and clears any stale checkpoint; it never propagates as an
    /// error because checkpointing is best-effort.
    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn save_checkpoint(&mut self) {
        // Attention-only architectures never need to rewind via a
        // checkpoint — `clear_kv_cache_seq` handles partial trims for
        // them. Skip the state_seq_get roundtrip on those.
        if !self.needs_checkpointing {
            return;
        }
        match self.ctx.state_seq_get(SEQ_ID, CHECKPOINT_FLAGS) {
            Ok(data) => {
                trace!(
                    n_past = self.n_past,
                    bytes = data.byte_len(),
                    "Saved checkpoint"
                );
                self.checkpoint = Some(Checkpoint {
                    data,
                    n_past: self.n_past,
                });
            }
            Err(err) => {
                warn!(
                    ?err,
                    "Failed to save sequence checkpoint; clearing any stale one"
                );
                self.checkpoint = None;
            }
        }
    }

    /// Attempt to rewind the context to `target_pos` using the saved
    /// checkpoint.
    ///
    /// Returns `true` on success — the caller can treat this as an
    /// equivalent to a successful partial `clear_kv_cache_seq`, with
    /// `n_past` now at `min(checkpoint.n_past, target_pos)`. Any tokens
    /// between the checkpoint position and the previous `n_past` are
    /// discarded from the KV cache.
    ///
    /// Returns `false` if no usable checkpoint exists (either not saved,
    /// or saved at a position strictly greater than `target_pos`). The
    /// caller should fall back to a full context reset.
    #[tracing::instrument(level = "trace", skip(self))]
    fn try_restore_checkpoint(&mut self, target_pos: i32) -> bool {
        let Some(ckpt) = self.checkpoint.as_ref() else {
            trace!("No checkpoint to restore from");
            return false;
        };
        if ckpt.n_past > target_pos {
            trace!(
                ckpt_n_past = ckpt.n_past,
                target_pos,
                "Checkpoint is past rewind target; cannot use"
            );
            return false;
        }
        if let Err(err) = self.ctx.state_seq_set(&ckpt.data, SEQ_ID) {
            warn!(?err, "Failed to restore sequence checkpoint");
            return false;
        }
        // Restoring on-device state consumes the slot. The handle is no
        // longer valid; drop it so we don't accidentally re-use it.
        let restored_pos = ckpt.n_past;
        self.checkpoint = None;
        self.n_past = restored_pos;
        // Discard any KV entries that were logged past the restored
        // position. On pure-attention paths this is trivial; on hybrid
        // archs the recurrent state is already back, so the remaining
        // work is just marking attention cells as free.
        //
        // This returns Ok(true) for both memory types today. A false/Err
        // would mean stale attention cells survived past `restored_pos` and
        // would corrupt the next decode, so surface it loudly rather than
        // swallowing it — a future change to recurrent-rollback semantics is
        // the realistic way this could start returning false.
        match self
            .ctx
            .clear_kv_cache_seq(Some(SEQ_ID as u32), Some(restored_pos as u32), None)
        {
            Ok(true) => {}
            other => {
                warn!(
                    ?other,
                    restored_pos,
                    "clear_kv_cache_seq did not free attention cells past the restored \
                     position; next decode may be corrupted"
                );
                debug_assert!(
                    matches!(other, Ok(true)),
                    "clear_kv_cache_seq after checkpoint restore returned {other:?}"
                );
            }
        }
        trace!(restored_pos, target_pos, "Restored from checkpoint");
        true
    }

    pub(crate) fn read_chunks(
        &mut self,
        chunks: TokenizerChunks,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, ReadError> {
        for chunk in chunks.into_iter() {
            match chunk {
                TokenizerChunk::Text(tokens, _) => {
                    self.read_text_tokens(tokens, inference_lock_token)?;
                }
                TokenizerChunk::Image(embeddings, _) | TokenizerChunk::Audio(embeddings, _) => {
                    self.read_media_embeddings(embeddings, inference_lock_token)?;
                }
            }
        }

        Ok(self)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn read_media_embeddings(
        &mut self,
        embeddings: Rc<MtmdInputChunks>,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, ReadError> {
        let projection_model = self
            .projection_model
            .as_ref()
            .ok_or(ReadError::ProjectionModelNotInitialized)?;

        let n_tokens = embeddings.as_ref().total_tokens();
        debug!(n_tokens, "Reading media embeddings:");

        let decode_span = debug_span!("read media embeddings", n_tokens = n_tokens);
        let decode_guard = decode_span.enter();
        let n_ctx = self.ctx.n_ctx() as i32;
        self.n_past = embeddings.eval_chunks(
            &projection_model.ctx,
            &self.ctx,
            self.n_past,
            0,
            n_ctx,
            true,
        )?;

        drop(decode_guard);
        debug!(
            "Completed read media embeddings operation, n_past: {}",
            self.n_past
        );

        Ok(self)
    }

    // ---------- IMPORTANT ----------
    // Should only be used under a global inference lock
    // This is a safety meassure to prevent bugs from multiple
    // contexts with the same model. It might not be necessary
    // but assume it is.
    #[tracing::instrument(level = "trace", skip(self))]
    fn read_text_tokens(
        &mut self,
        tokens: Vec<LlamaToken>,
        _inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, ReadError> {
        let n_tokens = tokens.len();
        debug!(n_tokens, "Reading tokens:");

        // can't read nothing
        debug_assert!(!tokens.is_empty());

        if n_tokens > self.n_batch {
            return Err(ReadError::InputExceedsContext {
                n_tokens,
                n_ctx: self.n_batch,
            });
        }

        {
            debug!("Populating batch");
            // make batch
            self.big_batch.clear();
            let seq_ids = &[0];
            for (i, token) in (0..).zip(tokens.iter()) {
                // For LLM workers only the last token's logits are needed (sampling).
                // For encoder workers every token must be marked as an output so the
                // pooling layer has hidden states to work with — otherwise llama.cpp
                // logs "embeddings required but some input tokens were not marked as
                // outputs -> overriding" and silently flips them on for us.
                let output_logits = self.use_embeddings || i == n_tokens - 1;
                self.big_batch
                    .add(*token, self.n_past + i as i32, seq_ids, output_logits)?;
            }
        }

        // llm go brr
        let decode_span = debug_span!("read decode", n_tokens = n_tokens);
        let decode_guard = decode_span.enter();
        self.ctx.decode(&mut self.big_batch)?;
        drop(decode_guard);
        // brrr

        self.n_past += tokens.len() as i32;

        debug!("Completed read tokens operation, n_past: {}", self.n_past);

        Ok(self)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn remove_all_tokens_from_index_from_ctx(
        &mut self,
        index: usize,
    ) -> Result<i32, KvCacheConversionError> {
        if self.n_past <= index as i32 {
            return Ok(0);
        }

        let before = self.n_past;
        let seq_rm_success = self
            .ctx
            .clear_kv_cache_seq(Some(0), Some(index as u32), None)?;

        if seq_rm_success {
            self.n_past = index as i32;
        } else if self.try_restore_checkpoint(index as i32) {
            // Rewound via the saved checkpoint. n_past is now at the
            // checkpoint's position (≤ index); the caller will read any
            // remaining tail from n_past forward.
            trace!(
                index,
                n_past = self.n_past,
                "Partial KV cache removal not supported; recovered via checkpoint"
            );
        } else {
            // Partial sequence removal is not supported by this model's memory type
            // (e.g. hybrid models with recurrent components) AND no usable
            // checkpoint exists. Fall back to full reset.
            warn!(
                index,
                n_past = self.n_past,
                "Partial KV cache removal not supported and no usable checkpoint; falling back to full context reset"
            );
            self.reset_context();
        }

        Ok(before - self.n_past)
    }

    /// Diff `target` chunks against `prev` and load only the new tail into the KV cache.
    /// Returns the new KV-cache mirror; the caller is responsible for storing it.
    pub(crate) fn sync_context(
        &mut self,
        target: TokenizerChunks,
        prev: &TokenizerChunks,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<TokenizerChunks, ContextSyncError> {
        let prefix_index = find_chunks_prefix_difference(prev, &target);

        debug_assert!(!target.is_empty());

        let trimmed = self.remove_all_tokens_from_index_from_ctx(prefix_index)?;

        let chunks_to_read = target.tail(self.n_past as usize);
        if chunks_to_read.n_tokens() > 0 {
            self.read_chunks(chunks_to_read, inference_lock_token)?;
        } else if trimmed > 0 {
            // Truncate-only: KV cache was trimmed but no new tokens need appending.
            // Re-decode the last token to refresh stale logits — llama.cpp requires
            // consecutive positions so we must evict it before re-reading.
            self.remove_all_tokens_from_index_from_ctx(self.n_past as usize - 1)?;
            self.read_chunks(target.tail(self.n_past as usize), inference_lock_token)?;
        }

        Ok(target)
    }

    pub(crate) fn n_past(&self) -> u32 {
        self.n_past as u32
    }

    pub(crate) fn is_context_full(&self) -> bool {
        self.n_past as u32 == self.ctx.n_ctx()
    }

    pub(crate) fn tokenize(
        &self,
        text: String,
        bitmaps: Vec<&MtmdBitmap>,
    ) -> Result<TokenizerChunks, crate::errors::TokenizationError> {
        self.tokenizer.tokenize(text, bitmaps)
    }

    pub(crate) fn load_image(&self, path: &Path) -> Result<MtmdBitmap, MultimodalError> {
        self.projection_model
            .as_ref()
            .ok_or(MultimodalError::ProjectionModelNotInitialized)?
            .load_image(path)
    }

    pub(crate) fn load_audio(&self, path: &Path) -> Result<MtmdBitmap, MultimodalError> {
        self.projection_model
            .as_ref()
            .ok_or(MultimodalError::ProjectionModelNotInitialized)?
            .load_audio(path)
    }

    pub(crate) fn sample_and_decode_next_token(
        &mut self,
        sampler: &mut LlamaSampler,
    ) -> Result<LlamaToken, DecodingError> {
        trace!("Applying sampler");
        let new_token: LlamaToken = sampler.sample(&self.ctx, -1);

        self.small_batch.clear();
        self.small_batch.add(new_token, self.n_past, &[0], true)?;

        let decode_span = trace_span!("write decode", n_past = self.n_past);
        let decode_guard = decode_span.enter();
        self.ctx.decode(&mut self.small_batch)?;
        drop(decode_guard);
        self.n_past += 1;

        Ok(new_token)
    }
}
