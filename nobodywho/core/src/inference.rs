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
use llama_cpp_2::speculative::MtpSpeculative;
use llama_cpp_2::token::LlamaToken;
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
    Speculative(MtpSpeculative<'a>),
}

impl<'a> std::ops::Deref for EngineContext<'a> {
    type Target = LlamaContext<'a>;
    fn deref(&self) -> &LlamaContext<'a> {
        match self {
            Self::Solo(c) => c,
            Self::Speculative(s) => s.target_context(),
        }
    }
}

impl<'a> std::ops::DerefMut for EngineContext<'a> {
    fn deref_mut(&mut self) -> &mut LlamaContext<'a> {
        match self {
            Self::Solo(c) => c,
            Self::Speculative(s) => s.target_context_mut(),
        }
    }
}

impl<'a> EngineContext<'a> {
    fn mtp_process(
        &mut self,
        batch: &LlamaBatch<'a>,
    ) -> Result<(), llama_cpp_2::speculative::MtpSpeculativeError> {
        if let Self::Speculative(spec) = self {
            spec.process(batch)?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub(crate) struct InferenceEngine<'a> {
    pub(crate) ctx: EngineContext<'a>,
    projection_model: Option<&'a ProjectionModel>,
    n_past: i32,
    tokenizer: Tokenizer<'a>,
    // The configured n_batch (= planned n_ctx before llama.cpp's internal rounding).
    // Used to guard against sending more tokens than the context can decode in one batch.
    n_batch: usize,
    big_batch: LlamaBatch<'a>,
    small_batch: LlamaBatch<'a>,
    use_embeddings: bool,
    /// Deferred-decode "pending sample": a token sampled from the
    /// target but not yet decoded into the KV cache
    /// Invariants:
    /// - `None` after `read_text_tokens` and `reset_context`.
    /// - `Some(t)` between speculative iterations, where `t` is the
    ///   target's sample for the next-to-emit position and is *not* in
    ///   the KV cache.
    pending: Option<LlamaToken>,
    pub(crate) mtp_drafts_proposed: u64,
    pub(crate) mtp_drafts_accepted: u64,
}

impl<'a> InferenceEngine<'a> {
    pub(crate) fn new(
        ctx: EngineContext<'a>,
        big_batch: LlamaBatch<'a>,
        small_batch: LlamaBatch<'a>,
        projection_model: Option<&'a ProjectionModel>,
        n_batch: usize,
        tokenizer: Tokenizer<'a>,
        use_embeddings: bool,
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
            pending: None,
            mtp_drafts_proposed: 0,
            mtp_drafts_accepted: 0,
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn reset_context(&mut self) -> &mut Self {
        self.ctx.clear_kv_cache();
        self.n_past = 0;
        self.pending = None;
        self
    }

    pub(crate) fn reset_mtp_stats(&mut self) {
        self.mtp_drafts_proposed = 0;
        self.mtp_drafts_accepted = 0;
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

        // Keep the MTP draft ctx's hidden state in sync (no-op on solo).
        self.ctx.mtp_process(&self.big_batch)?;

        self.n_past += tokens.len() as i32;
        // A new prompt (or context-shift replay) invalidates any deferred
        // pending sample from a previous generation. 
        self.pending = None;

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
        } else {
            // Partial sequence removal is not supported by this model's memory type
            // (e.g. hybrid models with recurrent components). Fall back to full reset.
            warn!(
                index,
                n_past = self.n_past,
                "Partial KV cache removal not supported, falling back to full context reset"
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

    /// Detach the deferred MTP `pending` token so a context-shift KV replay
    /// can run without desyncing the stateful sampler.
    pub(crate) fn take_pending(&mut self) -> Option<LlamaToken> {
        self.pending.take()
    }

    pub(crate) fn restore_pending(&mut self, pending: Option<LlamaToken>) {
        self.pending = pending;
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

    pub(crate) fn sample_and_decode_next_tokens(
        &mut self,
        sampler: &mut LlamaSampler,
    ) -> Result<Vec<LlamaToken>, DecodingError> {
        match &self.ctx {
            EngineContext::Solo(_) => self.sample_and_decode_solo(sampler),
            EngineContext::Speculative(_) => self.sample_and_decode_speculative(sampler),
        }
    }

    fn sample_and_decode_solo(
        &mut self,
        sampler: &mut LlamaSampler,
    ) -> Result<Vec<LlamaToken>, DecodingError> {
        trace!("Applying sampler (solo)");
        let new_token: LlamaToken = sampler.sample(&self.ctx, -1);

        self.small_batch.clear();
        self.small_batch.add(new_token, self.n_past, &[0], true)?;

        let decode_span = trace_span!("write decode", n_past = self.n_past);
        let decode_guard = decode_span.enter();
        self.ctx.decode(&mut self.small_batch)?;
        drop(decode_guard);
        self.n_past += 1;

        Ok(vec![new_token])
    }

    fn sample_and_decode_speculative(
        &mut self,
        sampler: &mut LlamaSampler,
    ) -> Result<Vec<LlamaToken>, DecodingError> {
        trace!("Applying sampler (MTP speculative, deferred)");
        let pending = match self.pending {
            Some(p) => p,
            None => sampler.sample(&self.ctx, -1),
        };

        if self.ctx.model.is_eog_token(pending) {
            trace!(?pending, "MTP: pending is EOG, short-circuiting");
            self.pending = None;
            return Ok(vec![pending]);
        }

        let mut drafts = {
            let EngineContext::Speculative(spec) = &mut self.ctx else {
                unreachable!("sample_and_decode_speculative called on solo ctx");
            };
            spec.draft(self.n_past, pending, &[])?
        };
        let accept_owed = !drafts.is_empty();

        // Clamp drafts so the verify batch [pending, drafts...] stays
        // within the context window: 
        let room = usize::try_from(self.ctx.n_ctx() as i32 - self.n_past - 1).unwrap_or(0);
        drafts.truncate(room);
        let k_max = drafts.len();

        if k_max == 0 {
            trace!(?pending, "MTP: no draft proposals to verify");
            self.small_batch.clear();
            self.small_batch.add(pending, self.n_past, &[0], true)?;
            self.ctx.decode(&mut self.small_batch)?;
            self.ctx.mtp_process(&self.small_batch)?;
            if accept_owed {
                let EngineContext::Speculative(spec) = &mut self.ctx else {
                    unreachable!();
                };
                spec.accept(0)?;
            }
            let new_pending = sampler.sample(&self.ctx, -1);
            self.n_past += 1;
            self.pending = Some(new_pending);
            return Ok(vec![pending]);
        }

        self.big_batch.clear();
        self.big_batch.add(pending, self.n_past, &[0], true)?;
        for (i, &d) in drafts.iter().enumerate() {
            self.big_batch
                .add(d, self.n_past + 1 + i as i32, &[0], true)?;
        }
        {
            let decode_span = trace_span!("mtp verify decode", n_past = self.n_past, k_max);
            let _decode_guard = decode_span.enter();
            self.ctx.decode(&mut self.big_batch)?;
        }
        self.ctx.mtp_process(&self.big_batch)?;

        let mut accepted_drafts: Vec<LlamaToken> = Vec::with_capacity(k_max);
        let mut new_pending = None;
        for (i, &draft) in drafts.iter().enumerate() {
            let ti = sampler.sample(&self.ctx, i as i32);
            if self.ctx.model.is_eog_token(ti) {
                trace!(?ti, "MTP: target sampled EOG during verify, stopping");
                new_pending = Some(ti);
                break;
            }
            if ti != draft {
                new_pending = Some(ti);
                break;
            }
            accepted_drafts.push(draft);
        }
        let new_pending = new_pending.unwrap_or_else(|| sampler.sample(&self.ctx, k_max as i32));
        let j = accepted_drafts.len();

        if j < k_max {
            let keep_up_to = (self.n_past + 1 + j as i32) as u32;
            let rolled_back = self
                .ctx
                .clear_kv_cache_seq(Some(0), Some(keep_up_to), None)?;
            if !rolled_back {
                // Recurrent / hybrid-recurrent memory types reject partial
                // removal (Ok(false)). Unlike `remove_all_tokens_from_index_from_ctx`
                // we cannot fall back to a full reset here — that would drop the
                // prompt mid-generation. Leaving the rejected drafts' KV in place
                // would silently corrupt subsequent decodes, so fail loudly. MTP
                // targets attention models, where partial removal is supported.
                return Err(DecodingError::MtpPartialRollbackUnsupported);
            }
        }

        {
            let EngineContext::Speculative(spec) = &mut self.ctx else {
                unreachable!();
            };
            spec.accept(j as u16)?;
        }

        self.n_past += 1 + j as i32;
        self.pending = Some(new_pending);
        self.mtp_drafts_proposed += k_max as u64;
        self.mtp_drafts_accepted += j as u64;

        trace!(
            j,
            k_max,
            ?pending,
            ?new_pending,
            "MTP: deferred iteration complete"
        );

        let mut emitted = Vec::with_capacity(1 + j);
        emitted.push(pending);
        emitted.extend_from_slice(&accepted_drafts);
        Ok(emitted)
    }
}
