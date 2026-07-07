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
use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::sync::MutexGuard;
use tracing::{debug, debug_span, trace, trace_span, warn};

// Thread-locals for per-sampler timing in benchmarks.
//
// SAMPLE_TIMING      — all sampler.sample() calls (free + grammar).
// GRAMMAR_SAMPLE_TIMING — only calls where GRAMMAR_TIMING_ACTIVE is true,
//                         i.e. grammar-constrained tokens only.
// GRAMMAR_TIMING_ACTIVE — flag set by the chat loop around the grammar branch.
//
// All zero-cost in release builds (gated on #[cfg(test)]).
thread_local! {
    pub(crate) static SAMPLE_TIMING: Cell<(u128, u64)> = const { Cell::new((0, 0)) };
    pub(crate) static GRAMMAR_SAMPLE_TIMING: Cell<(u128, u64)> = const { Cell::new((0, 0)) };
    pub(crate) static GRAMMAR_TIMING_ACTIVE: Cell<bool> = const { Cell::new(false) };
}

#[cfg(test)]
pub(crate) fn reset_sample_timing() {
    SAMPLE_TIMING.with(|c| c.set((0, 0)));
    GRAMMAR_SAMPLE_TIMING.with(|c| c.set((0, 0)));
    GRAMMAR_TIMING_ACTIVE.with(|c| c.set(false));
}

/// Returns `(total_nanos, call_count)` for ALL sampler.sample() calls.
#[cfg(test)]
pub(crate) fn read_sample_timing() -> (u128, u64) {
    SAMPLE_TIMING.with(|c| c.get())
}

/// Returns `(total_nanos, call_count)` for grammar-constrained tokens only.
#[cfg(test)]
pub(crate) fn read_grammar_sample_timing() -> (u128, u64) {
    GRAMMAR_SAMPLE_TIMING.with(|c| c.get())
}

#[cfg(test)]
pub(crate) fn set_grammar_timing_active(active: bool) {
    GRAMMAR_TIMING_ACTIVE.with(|c| c.set(active));
}

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
}

impl<'a> InferenceEngine<'a> {
    pub(crate) fn new(
        ctx: LlamaContext<'a>,
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
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn reset_context(&mut self) -> &mut Self {
        self.ctx.clear_kv_cache();
        self.n_past = 0;
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

    pub(crate) fn sample_and_decode_next_token(
        &mut self,
        sampler: &mut LlamaSampler,
    ) -> Result<LlamaToken, DecodingError> {
        trace!("Applying sampler");
        #[cfg(test)]
        let t0 = std::time::Instant::now();
        let new_token: LlamaToken = sampler.sample(&self.ctx, -1);
        #[cfg(test)]
        {
            let elapsed = t0.elapsed().as_nanos();
            if GRAMMAR_TIMING_ACTIVE.with(|c| c.get()) {
                GRAMMAR_SAMPLE_TIMING.with(|c| {
                    let (n, count) = c.get();
                    c.set((n + elapsed, count + 1));
                });
            } else {
                SAMPLE_TIMING.with(|c| {
                    let (n, count) = c.get();
                    c.set((n + elapsed, count + 1));
                });
            }
        }

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
