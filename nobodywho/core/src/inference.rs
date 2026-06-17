//! Generic inference pipeline, independent of chat history.

use crate::errors::{ContextSyncError, DecodingError, GenerateResponseError};
use crate::llm::{PoolingType, Worker, WriteOutput};
use crate::sampler::SamplerConfig;
use crate::tokenizer::{find_chunks_prefix_difference, TokenizerChunk, TokenizerChunks};
use lazy_static::lazy_static;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use std::sync::{Mutex, MutexGuard};
use tracing::{debug, info, trace, trace_span};

#[derive(Debug)]
pub(crate) struct GlobalInferenceLockToken;

lazy_static! {
    static ref GLOBAL_INFERENCE_LOCK: Mutex<GlobalInferenceLockToken> =
        Mutex::new(GlobalInferenceLockToken);
}

pub(crate) fn acquire_inference_lock() -> MutexGuard<'static, GlobalInferenceLockToken> {
    GLOBAL_INFERENCE_LOCK.lock().unwrap()
}

pub(crate) trait Generate {
    fn should_stop(&self) -> bool;
}

impl<'a, T: Generate + PoolingType> Worker<'a, T> {
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

    /// Run the token-generation loop until EOG or the stop flag.
    ///
    /// `on_context_full` is called when the KV cache fills up mid-generation. Chat passes a
    /// closure that does `context_shift` + re-sync + re-read of already-written tokens.
    /// A stateless caller can pass a closure that immediately returns `Err(...)`.
    pub(crate) fn generate_response_until_done<F, H>(
        &mut self,
        sampler_config: SamplerConfig,
        mut respond: F,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
        mut on_context_full: H,
    ) -> Result<&mut Self, GenerateResponseError>
    where
        F: FnMut(WriteOutput),
        H: FnMut(
            &mut Worker<'a, T>,
            &TokenizerChunks,
            &MutexGuard<'_, GlobalInferenceLockToken>,
        ) -> Result<(), GenerateResponseError>,
    {
        info!("Worker writing until done");

        let mut full_response: String = String::with_capacity(4096);
        let mut tokens_written_until_now = TokenizerChunks::new();

        let mut sampler = sampler_config.to_stateful(self.ctx.model)?;
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        while !self.extra.should_stop() {
            if self.n_past as u32 == self.ctx.n_ctx() {
                on_context_full(self, &tokens_written_until_now, inference_lock_token)?;
            }

            // Do not use sampler.accept — it crashes with grammar sampling.
            // https://github.com/utilityai/llama-cpp-rs/issues/604
            let new_token = self.sample_and_decode_next_token(&mut sampler)?;

            tokens_written_until_now.append(TokenizerChunk::new_text(vec![new_token]));

            let token_bytes = match self
                .ctx
                .model
                .token_to_piece_bytes(new_token, 8, true, None)
            {
                Err(llama_cpp_2::TokenToStringError::InsufficientBufferSpace(i)) => {
                    self.ctx.model.token_to_piece_bytes(
                        new_token,
                        (-i).try_into().expect("Error buffer size is positive"),
                        true,
                        None,
                    )
                }
                x => x,
            }?;

            let max_len = decoder
                .max_utf8_buffer_length(token_bytes.len())
                .unwrap_or(32);
            let mut token_str = String::with_capacity(max_len);

            // Partial unicode: the decoder retains incomplete codepoints across calls.
            let (_result, _bytes_read, _had_errors) =
                decoder.decode_to_string(&token_bytes, &mut token_str, false);

            // XXX: gemma4 hotfix — some gemma4 models emit a wrong EOS token after tool calls
            // that llama.cpp's is_eog_token does not catch, causing an infinite generation loop.
            // vllm handles the same case: https://docs.vllm.ai/en/stable/api/vllm/model_executor/models/gemma4_utils/#vllm.model_executor.models.gemma4_utils.has_tool_response_tag
            let gemma4_eog_hotfix = token_str == "<eos>" && new_token == LlamaToken::new(1);

            let has_eog = self.ctx.model.is_eog_token(new_token) || gemma4_eog_hotfix;
            trace!(?new_token, ?token_str, ?has_eog);

            if !has_eog {
                full_response.push_str(&token_str);
                trace!(?token_str, "Sending out token:");
                respond(WriteOutput::Token(token_str.to_string()));
            }

            if has_eog {
                break;
            }
        }

        debug!(%full_response, "Sending out");
        respond(WriteOutput::Done(full_response));
        Ok(self)
    }
}

/// Wraps a respond callback to capture the completed response and suppress tokens after
/// `tool_call_begin_token`.
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
