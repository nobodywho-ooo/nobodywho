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
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::sync::MutexGuard;
use tracing::{debug, debug_span, trace, trace_span, warn};

pub(crate) fn acquire_inference_lock() -> MutexGuard<'static, GlobalInferenceLockToken> {
    GLOBAL_INFERENCE_LOCK.lock().unwrap()
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

#[derive(Debug)]
pub(crate) struct BatchCapacity {
    pub(crate) tokens: usize,
    pub(crate) sequences: usize,
}

/// The state of an in-progress draft.
#[derive(Debug)]
struct DraftState {
    drafts: Vec<LlamaToken>,
    /// The number of accepted drafts.
    accepted: usize,
    /// Whether we still need to tell the MTP state which tokens are accepted.
    needs_accept: bool,
}

#[derive(Debug)]
pub(crate) struct InferenceEngine<'a> {
    pub(crate) ctx: EngineContext<'a>,
    projection_model: Option<&'a ProjectionModel>,
    n_past: i32,
    tokenizer: Tokenizer<'a>,
    // Configured limits before llama.cpp's internal rounding.
    batch_capacity: BatchCapacity,
    /// Batch that's used when decoding. Stored here to re-use the allocation.
    batch: LlamaBatch<'static>,
    use_embeddings: bool,
    draft_state: DraftState,
    pub(crate) mtp_drafts_proposed: u64,
    pub(crate) mtp_drafts_accepted: u64,
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

        Self {
            n_past: 0,
            ctx,
            batch_capacity,
            batch,
            projection_model,
            tokenizer,
            use_embeddings,
            draft_state: DraftState {
                drafts: Vec::new(),
                accepted: 0,
                needs_accept: false,
            },
            mtp_drafts_proposed: 0,
            mtp_drafts_accepted: 0,
        }
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub(crate) fn reset_context(&mut self) -> Result<(), MtpSpeculativeError> {
        self.accept_drafts()?;
        self.draft_state.drafts.clear();
        self.draft_state.accepted = 0;
        self.ctx.clear_kv_cache();
        self.n_past = 0;
        Ok(())
    }

    pub(crate) fn reset_mtp_stats(&mut self) {
        self.mtp_drafts_proposed = 0;
        self.mtp_drafts_accepted = 0;
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
        if let EngineContext::Speculative(spec) = &mut self.ctx {
            spec.process(&self.batch)?;
        }

        self.n_past += tokens.len() as i32;
        // A new prompt (or context-shift replay) invalidates any drafts
        self.draft_state.drafts.clear();
        self.draft_state.accepted = 0;

        debug!("Completed read tokens operation, n_past: {}", self.n_past);

        Ok(self)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    /// Remove everything in the KV cache from `index` onward.
    ///
    /// Returns `(effective_prefix, trimmed)` where:
    /// - `effective_prefix` is the number of tokens still valid in the KV cache
    /// - `trimmed` is how many positions were evicted.
    fn remove_all_tokens_from_index_from_ctx(
        &mut self,
        index: usize,
    ) -> Result<(usize, i32), KvCacheConversionError> {
        if self.n_past <= index as i32 {
            return Ok((index, 0));
        }

        let before = self.n_past;
        let seq_rm_success = self
            .ctx
            .clear_kv_cache_seq(Some(0), Some(index as u32), None)?;

        if seq_rm_success {
            self.n_past = index as i32;
            Ok((index, before - self.n_past))
        } else {
            // Partial sequence removal is not supported by this model's memory type
            // (e.g. hybrid models with recurrent components). Fall back to full reset,
            // which leaves the cache empty — so the effective prefix is 0.
            warn!(
                index,
                n_past = self.n_past,
                "Partial KV cache removal not supported, falling back to full context reset"
            );
            self.ctx.clear_kv_cache();
            self.n_past = 0;
            Ok((0, before))
        }
    }

    /// Update MTP state to accept in-progress drafts.
    ///
    /// This should only happen once per draft state.
    fn accept_drafts(&mut self) -> Result<(), MtpSpeculativeError> {
        if self.draft_state.needs_accept {
            let (accepted, declined) = self.draft_state.drafts.split_at(self.draft_state.accepted);
            trace!(?accepted, ?declined, "accepting draft");

            let EngineContext::Speculative(spec) = &mut self.ctx else {
                unreachable!("only context should not have drafts");
            };
            spec.accept(self.draft_state.accepted as u16)?;

            self.draft_state.needs_accept = false;
        }

        self.mtp_drafts_proposed += self.draft_state.drafts.len() as u64;
        self.mtp_drafts_accepted += self.draft_state.accepted as u64;

        Ok(())
    }

    fn roll_back_declined_drafts(&mut self) -> Result<(), RollbackError> {
        let declined = self.draft_state.drafts.len() - self.draft_state.accepted;
        if 0 < declined {
            // Remove declined drafts from the KV cache.
            let keep_up_to = self.n_past as u32;
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
                return Err(RollbackError::MtpPartialRollbackUnsupported);
            }
        }

        Ok(())
    }

    /// Diff `target` chunks against `prev` and load only the new tail into the KV cache.
    /// Returns the new KV-cache mirror; the caller is responsible for storing it.
    pub(crate) fn sync_context(
        &mut self,
        target: TokenizerChunks,
        prev: &TokenizerChunks,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<TokenizerChunks, ContextSyncError> {
        self.accept_drafts()?;
        self.draft_state.drafts.clear();
        self.draft_state.accepted = 0;

        let prefix_index = find_chunks_prefix_difference(prev, &target);

        debug_assert!(!target.is_empty());

        let (effective_prefix, trimmed) =
            self.remove_all_tokens_from_index_from_ctx(prefix_index)?;

        let chunks_to_read = target.tail(effective_prefix);
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
        let in_progress_drafts = (self.draft_state.drafts.len() - self.draft_state.accepted) as u32;
        self.n_past as u32 + in_progress_drafts == self.ctx.n_ctx()
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
    pub(crate) fn next_token(
        &mut self,
        sampler: &mut ChatSampler,
    ) -> Result<LlamaToken, DecodingError> {
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
        let token = if let Some(draft) = self.draft_state.drafts.get(self.draft_state.accepted) {
            let token = sampler
                .active()
                .sample(&self.ctx, self.draft_state.accepted as _);
            sampler.observe(token);

            // Fast path: If the token matches what the draft model predicted,
            // return the token.
            if token == *draft {
                self.draft_state.accepted += 1;
                self.n_past += 1;
                return Ok(token);
            }

            // Otherwise decode new tokens.
            token
        } else {
            // No need to use `sampler.accept` as `.sample` already accepts
            // the token: https://github.com/utilityai/llama-cpp-rs/issues/604
            let token = sampler.active().sample(&self.ctx, -1);
            sampler.observe(token);
            token
        };
        drop(span);

        // Reset draft state.
        self.accept_drafts()?;
        self.roll_back_declined_drafts()?;
        self.draft_state.drafts.clear();
        self.draft_state.accepted = 0;

        // Create new drafts.
        //
        // FIXME(madsmtm): Maybe avoid starting a whole new draft if the
        // token is an EOG token (then we'd rather decode just that token).
        let drafts = if let EngineContext::Speculative(spec) = &mut self.ctx {
            let _span = trace_span!("draft", n_past = self.n_past, ?token).entered();
            let mut drafts = spec.draft(self.n_past, token, &[])?;

            // Make sure we later `.accept(...)` the drafts.
            self.draft_state.needs_accept = !drafts.is_empty();

            // Clamp drafts so the verify batch [pending, drafts...] stays
            // within the context window:
            let room = usize::try_from(self.ctx.n_ctx() as i32 - self.n_past - 1).unwrap_or(0);
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

        let span = trace_span!("decode", n_past = self.n_past).entered();
        self.ctx.decode(&mut self.batch)?;
        drop(span);

        if let EngineContext::Speculative(spec) = &mut self.ctx {
            // Keep MTP state in sync.
            //
            // FIXME(madsmtm): This seems to synchronize the context, can we
            // avoid that somehow?
            let _span = trace_span!("mtp_process").entered();
            spec.process(&self.batch)?;
        }

        self.draft_state.drafts = drafts;

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
