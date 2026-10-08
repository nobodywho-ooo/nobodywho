//! Generic inference pipeline, independent of chat history.

use crate::chat::ChatSampler;
use crate::errors::{ContextSyncError, DecodingError, MultimodalError, ReadError, RollbackError};
use crate::llm::{GlobalInferenceLockToken, GLOBAL_INFERENCE_LOCK};
use crate::tokenizer::{
    find_chunks_prefix_difference, ProjectionModel, Tokenizer, TokenizerChunk, TokenizerChunks,
};
use llama_cpp_2::context::kv_cache::KvCacheConversionError;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::mtmd::MtmdBitmap;
use llama_cpp_2::mtmd::MtmdInputChunks;
use llama_cpp_2::speculative::{MtpSpeculative, MtpSpeculativeError};
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::{LlamaStateSeqFlags, SeqState};
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::sync::MutexGuard;
use tracing::{debug, debug_span, trace, trace_span, warn};

pub(crate) fn acquire_inference_lock() -> MutexGuard<'static, GlobalInferenceLockToken> {
    GLOBAL_INFERENCE_LOCK.lock().unwrap()
}

/// MTP state.
///
/// When we do speculative decoding, we basically want to do (in psuedo-C):
/// ```c
/// llama_token token = llama_sampler_sample(ctx, sampler);
/// printf("%i", token);
///
/// // Generate guess tokens with the draft model.
/// llama_token drafts[100];
/// int n_drafts = speculative_drafts(ctx, n_past, token, &drafts, 100, ...);
///
/// // Assume all the tokens were correct, and decode them all at once.
/// llama_batch batch = llama_batch_init(...);
/// llama_batch_add(batch, token);
/// for (int i = 0; i < n_drafts; i++) {
///     llama_batch_add(batch, drafts[i]);
/// }
/// llama_decode(ctx, batch);
///
/// // Extra MTP post-processing?
/// speculative_process(ctx, batch);
///
/// // Sample actual tokens
/// int n_accepted = 0;
/// for (int i = 0; i < n_drafts; i++) {
///     token = llama_sampler_sample(ctx, sampler);
///     if (token == drafts[i]) {
///         printf("%i", token);
///         n_accepted += 1;
///     } else {
///         break;
///     }
/// }
///
/// // Tell MTP which tokens were accepted.
/// speculative_accept(ctx, n_accepted);
///
/// // Remove incorrectly guessed tokens from KV-cache.
/// llama_memory_seq_rm(ctx, ...);
/// ```
///
/// When we actually go do this in [`InferenceEngine::next_token`], we reorder
/// things so that the accepting happens later, such that we can sample one
/// token at a time instead.
#[derive(Debug)]
pub(crate) struct SpeculativeEngine<'a> {
    ctx: MtpSpeculative<'a>,
    /// The in-progress drafts.
    drafts: Vec<LlamaToken>,
    /// The number of accepted drafts.
    n_accepted: usize,
    /// Whether we still need to tell the MTP state which tokens are accepted.
    needs_accept: bool,
    /// Statistics.
    total_proposed: u64,
    total_accepted: u64,
}

impl<'a> SpeculativeEngine<'a> {
    pub(crate) fn new(ctx: MtpSpeculative<'a>) -> Self {
        Self {
            ctx,
            drafts: Vec::new(),
            n_accepted: 0,
            needs_accept: false,
            total_proposed: 0,
            total_accepted: 0,
        }
    }

    /// Update MTP state to accept in-progress drafts.
    ///
    /// This should only happen once per draft state.
    fn accept_drafts(&mut self) -> Result<(), MtpSpeculativeError> {
        if self.needs_accept {
            let (accepted, declined) = self.drafts.split_at(self.n_accepted);
            trace!(?accepted, ?declined, "accepting draft");

            // FIXME(madsmtm): Why does this need to be called?
            self.ctx.accept(self.n_accepted as u16)?;

            self.needs_accept = false;
        }

        self.total_proposed += self.drafts.len() as u64;
        self.total_accepted += self.n_accepted as u64;

        Ok(())
    }

    fn roll_back_declined_drafts(&mut self, keep_up_to: u32) -> Result<(), RollbackError> {
        let declined = self.drafts.len() - self.n_accepted;
        if declined > 0 {
            // Remove declined drafts from the KV cache.
            let rolled_back = self.ctx.target_context_mut().clear_kv_cache_seq(
                Some(0),
                Some(keep_up_to),
                None,
            )?;
            if !rolled_back {
                // Recurrent memory only rolls back as many tokens as the context keeps
                // snapshots for (`n_rs_seq`), and only on architectures that support it.
                // Unlike `remove_tokens_from` we cannot fall back to a full reset here —
                // that would drop the prompt mid-generation. Leaving the rejected drafts'
                // KV in place would silently corrupt subsequent decodes, so fail loudly.
                return Err(RollbackError::MtpPartialRollbackUnsupported);
            }
        }

        Ok(())
    }

    /// Hand MTP a batch the target just decoded, starting at position `start`.
    fn process(&mut self, batch: &LlamaBatch, start: i32) -> Result<(), MtpSpeculativeError> {
        self.clear_draft_cache_from(start);
        self.ctx.process(batch)
    }

    /// Draft tokens to follow `token`, which goes at position `n_past`.
    fn draft(
        &mut self,
        n_past: i32,
        token: LlamaToken,
    ) -> Result<Vec<LlamaToken>, MtpSpeculativeError> {
        self.clear_draft_cache_from(n_past);
        self.ctx.draft(n_past, token, &[])
    }

    /// The draft context has a cache of its own, which MTP doesn't trim for every model.
    /// It keeps rejected drafts and positions the target rewound past, so clear what
    /// is about to be decoded again.
    fn clear_draft_cache_from(&mut self, position: i32) {
        let cleared =
            self.ctx
                .draft_context_mut()
                .clear_kv_cache_seq(Some(0), Some(position as u32), None);
        if !matches!(cleared, Ok(true)) {
            warn!(?cleared, position, "Failed to clear the MTP draft cache");
        }
    }
}

/// The low-level inference state for a single llama.cpp context.
///
/// Holds everything needed to read tokens/media into the KV cache and sample new tokens,
/// independent of any higher-level concept like chat history. Both `Worker` (encoder /
/// crossencoder) and `Chat` own one of these.
/// The context(s) an inference engine owns.
///
/// A solo engine holds one [`LlamaContext`] and drives it directly. An
/// MTP-speculative engine holds a target + draft pair wrapped in
/// [`MtpSpeculative`]; call sites that just need "the target context"
/// go through [`Deref`] / [`DerefMut`], so most of the engine code is
/// unchanged.
#[derive(Debug)]
pub(crate) enum EngineContext<'a> {
    Solo(LlamaContext<'a>),
    Speculative(SpeculativeEngine<'a>),
}

impl<'a> std::ops::Deref for EngineContext<'a> {
    type Target = LlamaContext<'a>;
    fn deref(&self) -> &LlamaContext<'a> {
        match self {
            Self::Solo(c) => c,
            Self::Speculative(s) => s.ctx.target_context(),
        }
    }
}

impl<'a> std::ops::DerefMut for EngineContext<'a> {
    fn deref_mut(&mut self) -> &mut LlamaContext<'a> {
        match self {
            Self::Solo(c) => c,
            Self::Speculative(s) => s.ctx.target_context_mut(),
        }
    }
}

/// Recurrent state of the sequence after the first `n_tokens` of the KV mirror.
/// Recurrent memory can't be cut partway, so rewinding restores one of these instead.
#[derive(Debug)]
struct Checkpoint {
    state: SeqState,
    n_tokens: usize,
}

/// Only the recurrent part of the state, kept in device memory. Taking a new
/// on-device snapshot invalidates the previous one, so there is at most one.
const CHECKPOINT_FLAGS: LlamaStateSeqFlags = LlamaStateSeqFlags::from_bits(
    LlamaStateSeqFlags::PARTIAL_ONLY.bits() | LlamaStateSeqFlags::ON_DEVICE.bits(),
);

#[derive(Debug)]
pub(crate) struct BatchCapacity {
    pub(crate) tokens: usize,
    pub(crate) sequences: usize,
}

#[derive(Debug)]
pub(crate) struct InferenceEngine<'a> {
    pub(crate) ctx: EngineContext<'a>,
    projection_model: Option<&'a ProjectionModel>,
    /// The token position in the KV cache that we've logically read.
    ///
    /// This does not include drafts.
    n_past: i32,
    tokenizer: Tokenizer<'a>,
    // Configured limits before llama.cpp's internal rounding.
    batch_capacity: BatchCapacity,
    /// Batch that's used when decoding. Stored here to re-use the allocation.
    batch: LlamaBatch<'static>,
    use_embeddings: bool,
    /// Our account of the KV cache at positions `[0, n_past)`, together with
    /// `pending_generated`.
    kv_mirror: TokenizerChunks,
    /// Generated tokens not yet merged into `kv_mirror`, so each one isn't a re-hash.
    pending_generated: Vec<LlamaToken>,
    /// Whether the model has recurrent memory, which needs checkpoints to rewind.
    needs_checkpoints: bool,
    /// Always covers a prefix of `kv_mirror` that hasn't changed since it was taken.
    checkpoint: Option<Checkpoint>,
}

impl<'a> InferenceEngine<'a> {
    pub(crate) fn new(
        ctx: EngineContext<'a>,
        projection_model: Option<&'a ProjectionModel>,
        batch_capacity: BatchCapacity,
        tokenizer: Tokenizer<'a>,
        use_embeddings: bool,
    ) -> Self {
        // The batch limit is sequence IDs per token; each embedding token
        // belongs to one sequence.
        let batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);
        let needs_checkpoints = ctx.model.is_recurrent() || ctx.model.is_hybrid();

        Self {
            n_past: 0,
            ctx,
            batch_capacity,
            batch,
            projection_model,
            tokenizer,
            use_embeddings,
            kv_mirror: TokenizerChunks::new(),
            pending_generated: Vec::new(),
            needs_checkpoints,
            checkpoint: None,
        }
    }

    pub(crate) fn needs_checkpoints(&self) -> bool {
        self.needs_checkpoints
    }

    /// Our record of the chunks in the KV cache at positions `[0, n_past)`; merges pending generated tokens first.
    pub(crate) fn kv_mirror(&mut self) -> &TokenizerChunks {
        self.flush_generated();
        debug_assert_eq!(self.kv_mirror.n_positions(), self.n_past as usize);
        &self.kv_mirror
    }

    /// Merge `pending_generated` into `kv_mirror`.
    fn flush_generated(&mut self) {
        if !self.pending_generated.is_empty() {
            let generated = std::mem::take(&mut self.pending_generated);
            self.kv_mirror.append(TokenizerChunk::new_text(generated));
        }
    }

    /// Record that positions from `n_tokens` onward were removed from the KV cache.
    fn truncate_mirror(&mut self, n_tokens: usize) {
        self.flush_generated();
        self.kv_mirror.truncate(n_tokens);
        // If we go back before the checkpoint, it is no longer valid.
        if self
            .checkpoint
            .as_ref()
            .is_some_and(|c| n_tokens < c.n_tokens)
        {
            self.checkpoint = None;
        }
    }

    /// Record that the KV cache was emptied.
    fn clear_mirror(&mut self) {
        self.kv_mirror = TokenizerChunks::new();
        self.pending_generated.clear();
        self.checkpoint = None;
    }

    /// Snapshot the recurrent state at the current end of the KV cache.
    #[tracing::instrument(level = "trace", skip(self))]
    fn save_checkpoint(&mut self) {
        match self.ctx.state_seq_get(0, CHECKPOINT_FLAGS) {
            Ok(state) => {
                let n_tokens = self.kv_mirror().n_tokens();
                trace!(
                    n_tokens,
                    n_past = self.n_past,
                    bytes = state.byte_len(),
                    "Saved checkpoint"
                );
                self.checkpoint = Some(Checkpoint { state, n_tokens });
            }
            Err(error) => {
                // The failed snapshot already invalidated the previous on-device one.
                warn!(%error, "Failed to save checkpoint");
                self.checkpoint = None;
            }
        }
    }

    /// Rewind to the checkpoint if it lies at or before token `index`, returning
    /// the tokens kept. `None` means the caller has to reset the whole context.
    #[tracing::instrument(level = "trace", skip(self))]
    fn restore_checkpoint(&mut self, index: usize) -> Option<usize> {
        let Some(checkpoint) = self.checkpoint.as_ref() else {
            trace!("No checkpoint to restore from");
            return None;
        };
        let n_tokens = checkpoint.n_tokens;
        if index < n_tokens {
            trace!(
                n_tokens,
                index,
                "Checkpoint is past the rewind target; cannot use"
            );
            return None;
        }
        if let Err(error) = self.ctx.state_seq_set(&checkpoint.state, 0) {
            warn!(%error, "Failed to restore checkpoint");
            return None;
        }
        // The restore only covers the recurrent state; drop the attention cells after it.
        let (_, position) = self.kv_mirror.cut_at(n_tokens);
        match self
            .ctx
            .clear_kv_cache_seq(Some(0), Some(position as u32), None)
        {
            Ok(true) => {}
            other => {
                // should be unreachable, since the recurrent state was
                // just set to the checkpoint, which is before position
                warn!(
                    ?other,
                    position, "Failed to clear the KV cache after the checkpoint"
                );
                return None;
            }
        }
        self.n_past = position as i32;
        self.truncate_mirror(n_tokens);
        trace!(n_tokens, index, n_past = self.n_past, "Restored checkpoint");
        Some(n_tokens)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn reset_context(&mut self) -> Result<(), MtpSpeculativeError> {
        if let EngineContext::Speculative(spec) = &mut self.ctx {
            spec.accept_drafts()?;
            spec.drafts.clear();
            spec.n_accepted = 0;
        }
        self.ctx.clear_kv_cache();
        self.n_past = 0;
        self.clear_mirror();
        Ok(())
    }

    pub(crate) fn reset_mtp_stats(&mut self) {
        if let EngineContext::Speculative(spec) = &mut self.ctx {
            spec.total_proposed = 0;
            spec.total_accepted = 0;
        }
    }

    pub(crate) fn mtp_acceptance_rate(&self) -> Option<f32> {
        if let EngineContext::Speculative(spec) = &self.ctx {
            let proposed = spec.total_proposed;
            if proposed > 0 {
                Some(spec.total_accepted as f32 / proposed as f32)
            } else {
                None
            }
        } else {
            None
        }
    }

    pub(crate) fn read_strings_batched<T, E>(
        &mut self,
        texts: Vec<String>,
        mut output_for_sequence: impl FnMut(&EngineContext<'a>, i32) -> Result<T, E>,
    ) -> Result<Vec<T>, BatchedReadError<E>> {
        let tokenized_inputs = texts
            .into_iter()
            .map(|text| {
                let chunks = self.tokenize(text, vec![])?;
                let mut tokens = vec![];
                for chunk in chunks {
                    match chunk {
                        TokenizerChunk::Text(chunk_tokens, _) => tokens.extend(chunk_tokens),
                        TokenizerChunk::Image(_, _) | TokenizerChunk::Audio(_, _) => {
                            unreachable!("text tokenization produced media chunks")
                        }
                    }
                }
                Ok(tokens)
            })
            .collect::<Result<Vec<_>, crate::errors::TokenizationError>>()
            .map_err(ReadError::FailedToTokenize)?;

        let token_counts = tokenized_inputs.iter().map(Vec::len).collect::<Vec<_>>();
        let ranges = embedding_batch_ranges(
            &token_counts,
            self.batch_capacity.tokens,
            self.batch_capacity.sequences,
        )?;
        let mut outputs = Vec::with_capacity(tokenized_inputs.len());

        for range in ranges {
            self.batch.clear();
            for (sequence_id, tokens) in tokenized_inputs[range.clone()].iter().enumerate() {
                self.batch
                    .add_sequence(tokens, sequence_id as i32, true)
                    .map_err(ReadError::BatchAdd)?;
            }

            let n_tokens = self.batch.n_tokens();
            let n_sequences = range.len();
            let inference_lock_token = acquire_inference_lock();
            self.reset_context().expect("failed resetting context");

            let decode_span = debug_span!(
                "read embedding batch",
                n_tokens = n_tokens,
                n_sequences = n_sequences
            );
            let decode_guard = decode_span.enter();
            self.ctx
                .decode(&mut self.batch)
                .map_err(ReadError::Decode)?;
            drop(decode_guard);

            for sequence_id in 0..n_sequences {
                outputs.push(
                    output_for_sequence(&self.ctx, sequence_id as i32)
                        .map_err(BatchedReadError::Output)?,
                );
            }
            drop(inference_lock_token);

            debug!(n_tokens, n_sequences, "Completed embedding batch");
        }

        Ok(outputs)
    }

    pub(crate) fn read_chunks(
        &mut self,
        chunks: TokenizerChunks,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, ReadError> {
        for chunk in chunks.into_iter() {
            self.kv_mirror();
            match &chunk {
                TokenizerChunk::Text(tokens, _) => {
                    self.read_text_tokens(tokens, inference_lock_token)?;
                }
                TokenizerChunk::Image(embeddings, _) | TokenizerChunk::Audio(embeddings, _) => {
                    self.read_media_embeddings(embeddings.clone(), inference_lock_token)?;
                }
            }
            self.kv_mirror.append(chunk);
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
        tokens: &[LlamaToken],
        _inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, ReadError> {
        let n_tokens = tokens.len();
        debug!(n_tokens, "Reading tokens:");

        // can't read nothing
        debug_assert!(!tokens.is_empty());

        if n_tokens > self.batch_capacity.tokens {
            return Err(ReadError::InputExceedsContext {
                n_tokens,
                n_ctx: self.batch_capacity.tokens,
            });
        }

        {
            debug!("Populating batch");
            // make batch
            self.batch.clear();
            let seq_ids = &[0];
            for (i, token) in (0..).zip(tokens.iter()) {
                // For LLM workers only the last token's logits are needed (sampling).
                // For encoder workers every token must be marked as an output so the
                // pooling layer has hidden states to work with — otherwise llama.cpp
                // logs "embeddings required but some input tokens were not marked as
                // outputs -> overriding" and silently flips them on for us.
                let output_logits = self.use_embeddings || i == n_tokens - 1;
                self.batch
                    .add(*token, self.n_past + i as i32, seq_ids, output_logits)?;
            }
        }

        // llm go brr
        let decode_span = debug_span!("read decode", n_tokens = n_tokens);
        let decode_guard = decode_span.enter();
        self.ctx.decode(&mut self.batch)?;
        drop(decode_guard);
        // brrr

        // Keep the MTP draft ctx's hidden state in sync.
        if let EngineContext::Speculative(s) = &mut self.ctx {
            s.process(&self.batch, self.n_past)?;
            // A new prompt (or context-shift replay) invalidates in-progress
            // drafts.
            //
            // FIXME(madsmtm): Should we accept the previous drafts here?
            s.drafts.clear();
            s.n_accepted = 0;
        }

        self.n_past += tokens.len() as i32;

        debug!("Completed read tokens operation, n_past: {}", self.n_past);

        Ok(self)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    /// Remove tokens from `index` onward from the KV cache, and return how many are kept.
    /// That can be fewer than `index`: media is never split, recurrent models can only go
    /// back to a checkpoint, and without one the whole context is reset.
    fn remove_kv_cache_suffix(
        &mut self,
        start_index: usize,
    ) -> Result<usize, KvCacheConversionError> {
        if self.kv_mirror().n_tokens() <= start_index {
            return Ok(self.kv_mirror.n_tokens());
        }

        // The cache is cut by position, which falls behind the token count after M-RoPE media.
        // Media can't be split, so a cut inside one moves back to its start and it is re-read.
        let (index, position) = self.kv_mirror.cut_at(start_index);
        // Recurrent and hybrid models can only cut within the last decode, which we don't have a way of checking.
        // Instead those models always fall back to the last checkpoint or a full reset.
        let seq_rm_success = if !self.needs_checkpoints() || index == 0 {
            self.ctx
                .clear_kv_cache_seq(Some(0), Some(position as u32), None)?
        } else {
            false
        };

        if seq_rm_success {
            self.n_past = position as i32;
            self.truncate_mirror(index);
            Ok(index)
        } else if let Some(kept) = self.restore_checkpoint(index) {
            // Recurrent memory can't be cut partway, but the checkpoint gets close.
            Ok(kept)
        } else {
            // Partial sequence removal is not supported by this model's memory type
            // (e.g. hybrid models with recurrent components), and no checkpoint
            // helps. Fall back to full reset, which leaves the cache empty — so the
            // effective prefix is 0.
            warn!(
                index,
                n_past = self.n_past,
                "Partial KV cache removal not supported and no usable checkpoint, falling back to full context reset"
            );
            self.ctx.clear_kv_cache();
            self.n_past = 0;
            self.clear_mirror();
            Ok(0)
        }
    }

    /// Read `target` from where the KV mirror ends up to token `end`. The mirror must be
    /// a prefix of `target`, so its length is how far into `target` the cache already is.
    fn read_until(
        &mut self,
        target: &TokenizerChunks,
        end: usize,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<(), ReadError> {
        let start = self.kv_mirror().n_tokens();
        if start < end {
            let mut chunks = target.tail(start);
            chunks.truncate(end - start);
            self.read_chunks(chunks, inference_lock_token)?;
        }
        Ok(())
    }

    /// Diff `target` chunks against `kv_mirror` and load only the new tail into the KV cache.
    ///
    /// `checkpoint_at` is a token index in `target` to save a checkpoint at, if the cache
    /// isn't already past it; only pass it if [`Self::needs_checkpoints`].
    pub(crate) fn sync_context(
        &mut self,
        target: TokenizerChunks,
        checkpoint_at: Option<usize>,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<(), ContextSyncError> {
        if let EngineContext::Speculative(spec) = &mut self.ctx {
            // Clear draft state.
            spec.accept_drafts()?;
            spec.roll_back_declined_drafts(self.n_past as _)?;
            spec.drafts.clear();
            spec.n_accepted = 0;
        }

        debug_assert!(!target.is_empty());
        let end = target.n_tokens();

        // All indices here count tokens into `target`. After the cut the mirror is
        // `target[..kept]`, and `read_until` continues from there.
        let cached = self.kv_mirror().n_tokens();
        let diverge = find_chunks_prefix_difference(&self.kv_mirror, &target);
        let mut kept = self.remove_kv_cache_suffix(diverge)?;
        if kept == end && kept < cached {
            // The target ends inside the cache, whose logits are for a token we just
            // removed. Read the last token again to get its logits.
            kept = self.remove_kv_cache_suffix(end - 1)?;
        }

        // The cache can't be saved at a point it is already past, and an older checkpoint
        // survives the cut only at or before `kept`, so a new one is never further back.
        let checkpoint_at = checkpoint_at.filter(|&ckpt| {
            kept <= ckpt && self.checkpoint.as_ref().is_none_or(|c| c.n_tokens < ckpt)
        });
        if let Some(at) = checkpoint_at {
            self.read_until(&target, at, inference_lock_token)?;
            self.save_checkpoint();
        }
        self.read_until(&target, end, inference_lock_token)?;

        Ok(())
    }

    fn in_progress_drafts(&self) -> i32 {
        if let EngineContext::Speculative(spec) = &self.ctx {
            (spec.drafts.len() - spec.n_accepted) as i32
        } else {
            0
        }
    }

    /// Tokens in the KV cache including drafts. Each takes a slot of the context,
    /// though M-RoPE media spans fewer positions than that.
    pub(crate) fn actual_context_size(&self) -> i32 {
        (self.kv_mirror.n_tokens() + self.pending_generated.len()) as i32
            + self.in_progress_drafts()
    }

    pub(crate) fn is_context_full(&self) -> bool {
        self.actual_context_size() >= self.ctx.n_ctx() as i32
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

    /// Sample and decode the next token.
    ///
    /// This abstracts over the speculative decoder's otherwise multi-token
    /// design by storing the draft / in-progress tokens internally.
    pub(crate) fn next_token(
        &mut self,
        sampler: &mut ChatSampler,
    ) -> Result<LlamaToken, DecodingError> {
        let _span = trace_span!("next_token").entered();
        // Somewhat un-intuitively, we actually want to sample first, before
        // attempting to generate new logits / tokens.
        //
        // This done for two reasons:
        // 1. Right after prefilling, the next token has already been
        //    generated (along with logits for all other tokens).
        // 2. Decoding is asynchronous, and sampling here implicitly
        //    synchronizes with it.
        //
        // The second point in particular is important for performance:
        // ideally, we always want a decoding stage in progress, so that all
        // the various other work we do (including the work the user does)
        // isn't going to block inference.

        let span = trace_span!("sample").entered();
        let token = if let EngineContext::Speculative(spec) = &mut self.ctx {
            if let Some(draft) = spec.drafts.get(spec.n_accepted) {
                let token = sampler.sample(spec.ctx.target_context_mut(), spec.n_accepted as _);

                // Fast path: If the token matches what the draft model predicted,
                // return the token.
                if token == *draft {
                    spec.n_accepted += 1;
                    self.pending_generated.push(token);
                    self.n_past += 1;
                    return Ok(token);
                }

                // Otherwise decode new tokens.
                token
            } else {
                sampler.sample(&self.ctx, -1)
            }
        } else {
            sampler.sample(&self.ctx, -1)
        };
        drop(span);

        // Reset draft state.
        if let EngineContext::Speculative(spec) = &mut self.ctx {
            spec.accept_drafts()?;
            spec.roll_back_declined_drafts(self.n_past as u32)?;
            spec.drafts.clear();
            spec.n_accepted = 0;
        }

        // Create new drafts.
        //
        // FIXME(madsmtm): Maybe avoid starting a whole new draft if the
        // token is an EOG token (then we'd rather decode just that token).
        let drafts = if let EngineContext::Speculative(spec) = &mut self.ctx {
            let _span = trace_span!("draft", n_past = self.n_past, ?token).entered();
            let mut drafts = spec.draft(self.n_past, token)?;

            // Make sure we later `.accept(...)` the drafts.
            spec.needs_accept = !drafts.is_empty();

            // Clamp drafts so the verify batch [pending, drafts...] stays
            // within the context window:
            let used = self.kv_mirror.n_tokens() + self.pending_generated.len();
            let room = (spec.ctx.target_context().n_ctx() as usize).saturating_sub(used + 1);
            drafts.truncate(room);

            trace!(?drafts);
            drafts
        } else {
            Vec::new()
        };

        self.batch.clear();
        self.batch.add(token, self.n_past, &[0], true)?;
        for (i, &d) in drafts.iter().enumerate() {
            self.batch.add(d, self.n_past + 1 + i as i32, &[0], true)?;
        }

        // llm go brr?
        //
        // We _start_ the decoding here, though we don't wait for it to finish
        // (see comment further up), so beware that timings might be somewhat
        // confusing if you're trying to benchmark.
        let span = trace_span!("decode", n_past = self.n_past).entered();
        self.ctx.decode(&mut self.batch)?;
        drop(span);

        if let EngineContext::Speculative(spec) = &mut self.ctx {
            // Keep MTP state in sync.
            //
            // FIXME(madsmtm): This seems to synchronize the context, can we
            // avoid that somehow?
            let _span = trace_span!("mtp_process").entered();
            spec.process(&self.batch, self.n_past)?;

            spec.drafts = drafts;
        }

        self.pending_generated.push(token);
        self.n_past += 1;

        Ok(token)
    }
}

pub(crate) enum BatchedReadError<E> {
    Read(ReadError),
    Output(E),
}

impl<E> From<ReadError> for BatchedReadError<E> {
    fn from(error: ReadError) -> Self {
        Self::Read(error)
    }
}

fn embedding_batch_ranges(
    token_counts: &[usize],
    n_batch: usize,
    n_seq_max: usize,
) -> Result<Vec<Range<usize>>, ReadError> {
    let mut ranges = vec![];
    let mut start = 0;

    while start < token_counts.len() {
        let mut end = start;
        let mut n_tokens = 0;

        while end < token_counts.len() && end - start < n_seq_max.max(1) {
            let next_tokens = token_counts[end];
            if next_tokens > n_batch {
                return Err(ReadError::InputExceedsContext {
                    n_tokens: next_tokens,
                    n_ctx: n_batch,
                });
            }
            if end > start && n_tokens + next_tokens > n_batch {
                break;
            }
            n_tokens += next_tokens;
            end += 1;
        }

        ranges.push(start..end);
        start = end;
    }

    Ok(ranges)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn embedding_batches_respect_token_capacity() {
        assert_eq!(
            embedding_batch_ranges(&[3, 4, 5], 7, 10).unwrap(),
            vec![0..2, 2..3]
        );
    }

    #[test]
    fn embedding_batches_respect_sequence_capacity() {
        assert_eq!(
            embedding_batch_ranges(&[1, 1, 1, 1, 1], 10, 2).unwrap(),
            vec![0..2, 2..4, 4..5]
        );
        assert_eq!(
            embedding_batch_ranges(&[1, 1, 1], 10, 1).unwrap(),
            vec![0..1, 1..2, 2..3]
        );
    }

    #[test]
    fn embedding_batches_reject_oversized_inputs() {
        assert!(matches!(
            embedding_batch_ranges(&[3, 8], 7, 10),
            Err(ReadError::InputExceedsContext {
                n_tokens: 8,
                n_ctx: 7
            })
        ));
    }
}
