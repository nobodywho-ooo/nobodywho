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
    /// Notify the MTP draft context of a batch that was just decoded on
    /// the target. No-op on the solo path. Must be called after every
    /// target decode when MTP is active so the draft ctx's hidden-state
    /// carryover stays in sync.
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
    /// target but not yet decoded into the KV cache. On the MTP
    /// speculative path, each iteration's batch begins with this
    /// pending token followed by the drafter's proposals, so the
    /// bonus/replacement token from iteration N gets its decode "for
    /// free" as part of iteration N+1's batch — one fewer decode call
    /// per emit than the eager approach.
    ///
    /// Invariants:
    /// - `None` after `read_text_tokens` and `reset_context`.
    /// - `Some(t)` between speculative iterations, where `t` is the
    ///   target's sample for the next-to-emit position and is *not* in
    ///   the KV cache.
    /// - Solo path never sets it.
    pending: Option<LlamaToken>,
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
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn reset_context(&mut self) -> &mut Self {
        self.ctx.clear_kv_cache();
        self.n_past = 0;
        self.pending = None;
        self
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
        // pending sample from a previous generation. The next call to
        // `sample_and_decode_speculative` will re-seed pending from the
        // freshly-decoded state's `-1` logits.
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

    /// Sample and decode 1..K+1 tokens.
    ///
    /// On the solo path this is a plain "sample one, decode one" step
    /// returning a single-element vec — identical semantics to the old
    /// `sample_and_decode_next_token`.
    ///
    /// On the MTP-speculative path this drafts up to K tokens, decodes
    /// them in one batch, verifies each against the target's sampling,
    /// and returns the accepted prefix (plus one bonus token). The
    /// returned vec has length in `1..=K+1`.
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

    /// Deferred-decode MTP sample.
    ///
    /// Consolidates the eager path's two decode calls (drafts batch +
    /// bonus single-token) into a single `[pending, drafts...]` batch
    /// of K+1 tokens. The bonus token from iteration N becomes
    /// iteration N+1's pending — decoded "for free" as the first entry
    /// of that iteration's batch.
    ///
    /// See `pending` on [`InferenceEngine`] for the state invariant.
    fn sample_and_decode_speculative(
        &mut self,
        sampler: &mut LlamaSampler,
    ) -> Result<Vec<LlamaToken>, DecodingError> {
        trace!("Applying sampler (MTP speculative, deferred)");
        // 1. Seed pending lazily on the first speculative iteration
        //    after prompt processing.
        let pending = match self.pending {
            Some(p) => p,
            None => {
                let p = sampler.sample(&self.ctx, -1);
                sampler.accept(p);
                p
            }
        };

        // 2. Fast-path EOG on pending: don't touch KV, don't ask the
        //    drafter — chat loop breaks on EOG in the return vec.
        if self.ctx.model.is_eog_token(pending) {
            trace!(?pending, "MTP: pending is EOG, short-circuiting");
            self.pending = None;
            return Ok(vec![pending]);
        }

        // 3. Ask the drafter for up to n_max proposals starting after
        //    pending.
        let drafts = {
            let EngineContext::Speculative(spec) = &mut self.ctx else {
                unreachable!("sample_and_decode_speculative called on solo ctx");
            };
            spec.draft(self.n_past, pending, &[])?
        };
        let k_max = drafts.len();

        // 4. Empty drafts: decode pending as a single token, re-seed
        //    pending. Same shape/cost as the solo path.
        if k_max == 0 {
            trace!(?pending, "MTP: drafter returned no proposals");
            self.small_batch.clear();
            self.small_batch.add(pending, self.n_past, &[0], true)?;
            self.ctx.decode(&mut self.small_batch)?;
            self.ctx.mtp_process(&self.small_batch)?;
            {
                let EngineContext::Speculative(spec) = &mut self.ctx else {
                    unreachable!();
                };
                spec.accept(0)?;
            }
            let new_pending = sampler.sample(&self.ctx, -1);
            sampler.accept(new_pending);
            self.n_past += 1;
            self.pending = Some(new_pending);
            return Ok(vec![pending]);
        }

        // 5. Build batch = [pending, drafts...] at positions
        //    [n_past, n_past+1, ..., n_past+k_max]. K+1 entries, all
        //    logits=true.
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

        // 6. Verify. Batch idx i's logits predict position n_past+i+1
        //    = draft[i]'s position, so sample at batch idx i and
        //    compare to drafts[i].
        let mut accepted_drafts: Vec<LlamaToken> = Vec::with_capacity(k_max);
        let mut new_pending: Option<LlamaToken> = None;
        for (i, &draft) in drafts.iter().enumerate() {
            let ti = sampler.sample(&self.ctx, i as i32);
            sampler.accept(ti);
            if ti == draft {
                accepted_drafts.push(draft);
            } else {
                new_pending = Some(ti);
                break;
            }
        }
        // 7. If all K drafts matched, sample the bonus at batch idx
        //    k_max (last position, predicts n_past+k_max+1).
        if new_pending.is_none() {
            let bonus = sampler.sample(&self.ctx, k_max as i32);
            sampler.accept(bonus);
            new_pending = Some(bonus);
        }
        let new_pending = new_pending.expect("new_pending is set above");
        let j = accepted_drafts.len();

        // 8. KV rollback for partial acceptance. Keep positions
        //    [0..n_past+1+j] (pending + accepted drafts).
        if j < k_max {
            let keep_up_to = (self.n_past + 1 + j as i32) as u32;
            let _ = self
                .ctx
                .clear_kv_cache_seq(Some(0), Some(keep_up_to), None)?;
        }

        // 9. Tell the drafter how many drafts stuck.
        {
            let EngineContext::Speculative(spec) = &mut self.ctx else {
                unreachable!();
            };
            spec.accept(j as u16)?;
        }

        // 10. Bookkeeping: n_past advances by (pending + j drafts).
        //     new_pending is the bonus/replacement, held for next iter.
        self.n_past += 1 + j as i32;
        self.pending = Some(new_pending);

        trace!(
            j,
            k_max,
            ?pending,
            ?new_pending,
            "MTP: deferred iteration complete"
        );

        // 11. Emit pending (confirmed this iteration) + accepted drafts.
        let mut emitted = Vec::with_capacity(1 + j);
        emitted.push(pending);
        emitted.extend_from_slice(&accepted_drafts);
        Ok(emitted)
    }
}
