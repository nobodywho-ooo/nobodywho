//! Generic inference pipeline shared by all worker types.
//!
//! This module contains the inference primitives that are independent of chat history:
//! - KV-cache synchronisation (`sync_context`)
//! - Token sampling and decoding (`sample_and_decode_next_token`)
//! - The generation loop (`generate_response_until_done`)
//! - Stream helper (`wrap_respond`)
//!
//! `ChatWorker` (and future stateless workers) layer their own state management on top of
//! these primitives. Neither the trait nor the impl block knows about message history,
//! context shifting, or tool calling.

use crate::errors::{ContextSyncError, DecodingError, GenerateResponseError};
use crate::llm::{GlobalInferenceLockToken, PoolingType, Worker, WriteOutput};
use crate::sampler::SamplerConfig;
use crate::tokenizer::{find_chunks_prefix_difference, TokenizerChunk, TokenizerChunks};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use std::sync::MutexGuard;
use tracing::{debug, info, trace, trace_span};

/// Implemented by any worker payload that supports stop-flag-based interruption of the
/// generation loop.
pub(crate) trait Generate {
    fn should_stop(&self) -> bool;
}

impl<'a, T: Generate + PoolingType> Worker<'a, T> {
    /// Diff `target` chunks against `prev` (the KV-cache mirror stored by the caller) and load
    /// only the new tail into the KV cache. Returns the new mirror; the caller is responsible for
    /// storing it.
    ///
    /// This function does **not** render messages, call context_shift, or handle any chat-specific
    /// concerns — those are the caller's responsibility.
    pub(crate) fn sync_context(
        &mut self,
        target: TokenizerChunks,
        prev: &TokenizerChunks,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<TokenizerChunks, ContextSyncError> {
        let prefix_index = find_chunks_prefix_difference(prev, &target);

        // We should never try to sync with an empty render.
        debug_assert!(!target.is_empty());

        // remove_all_tokens_from_index_from_ctx may remove more than just the tokens from
        // prefix_index; it updates self.n_past to indicate the number of tokens still in context.
        let old_n_past = self.n_past;
        self.remove_all_tokens_from_index_from_ctx(prefix_index)?;

        // Use n_past as the actual preserved prefix — may be 0 if a full reset was required
        // (e.g. hybrid/recurrent models that don't support partial seq_rm).
        let chunks_to_read = target.tail(self.n_past as usize);
        if chunks_to_read.n_tokens() > 0 {
            self.read_chunks(chunks_to_read, inference_lock_token)?;
        } else if self.n_past < old_n_past {
            // Truncate-only path: the KV cache was trimmed but no new tokens need to be appended.
            // Re-decode the last remaining token to refresh the logits buffer, which would
            // otherwise contain stale values from whatever the previous decode() call was.
            // llama.cpp requires strictly consecutive positions (Y = X + 1), so we must remove
            // the last token from the KV cache before we can re-decode it.
            self.remove_all_tokens_from_index_from_ctx(self.n_past as usize - 1)?;
            let refresh_tokens = target.tail(self.n_past as usize);
            self.read_chunks(refresh_tokens, inference_lock_token)?;
        }

        Ok(target)
    }

    pub(crate) fn sample_and_decode_next_token(
        &mut self,
        sampler: &mut LlamaSampler,
    ) -> Result<LlamaToken, DecodingError> {
        trace!("Applying sampler");
        let new_token: LlamaToken = sampler.sample(&self.ctx, -1);

        // batch of one
        self.small_batch.clear();
        self.small_batch.add(new_token, self.n_past, &[0], true)?;

        // llm go brr
        let decode_span = trace_span!("write decode", n_past = self.n_past);
        let decode_guard = decode_span.enter();
        self.ctx.decode(&mut self.small_batch)?;
        drop(decode_guard);
        self.n_past += 1; // keep count

        Ok(new_token)
    }

    /// Run the token-generation loop until an end-of-generation token is produced or the stop
    /// flag is set.
    ///
    /// `on_context_full` is called when the KV cache fills up mid-generation. Chat passes a
    /// closure that performs `context_shift` + re-sync + re-read of already-written tokens.
    /// A future stateless caller can pass a closure that returns `Err(...)` immediately.
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
        // Token generation loop
        info!("Worker writing until done");

        // pre-allocating 4096 bytes for the response string
        // 4096 is a very randomly chosen number. how does this affect performance?
        let mut full_response: String = String::with_capacity(4096);
        let mut tokens_written_until_now = TokenizerChunks::new();

        // initialize sampler
        // stateful samplers only live for one response
        let mut sampler = sampler_config.to_stateful(self.ctx.model)?;

        // init stateful decoder for split up tokens like emojis
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        while !self.extra.should_stop() {
            // Check if the context is full
            if self.n_past as u32 == self.ctx.n_ctx() {
                on_context_full(self, &tokens_written_until_now, inference_lock_token)?;
            }

            // Sample next token, no need to use sampler.accept as sample already accepts the token.
            // using sampler.accept() will cause the sampler to crash when using grammar sampling.
            // https://github.com/utilityai/llama-cpp-rs/issues/604
            let new_token = self.sample_and_decode_next_token(&mut sampler)?;

            tokens_written_until_now.append(TokenizerChunk::new_text(vec![new_token]));

            // Attempt to convert token(s) to bytes
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

            // Attempt to convert bytes to utf8 string.
            let max_len = decoder
                .max_utf8_buffer_length(token_bytes.len())
                .unwrap_or(32);
            let mut token_str = String::with_capacity(max_len);

            // this is where the utf-8 decoder handles partial unicode
            // it'll write whatever printable chars it can into `token_str`
            // and retain partial codepoints for next decoding attempt
            let (_result, _bytes_read, _had_errors) =
                decoder.decode_to_string(&token_bytes, &mut token_str, false);

            // XXX: this literal '<eos>' token match is a fucked hotfix for gemma4. it seems like
            // some gemma4 models will emit a *wrong* eos token (doesn't match the expected format)
            // after tool calls. This doesn't trigger the is_eog_token match in llama.cpp and
            // causes a bad infinite generation loop.
            // it seems like vllm also has a codepath to handle this specific case:
            // https://docs.vllm.ai/en/stable/api/vllm/model_executor/models/gemma4_utils/#vllm.model_executor.models.gemma4_utils.has_tool_response_tag
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

        // we're done!
        debug!(%full_response, "Sending out");
        respond(WriteOutput::Done(full_response));
        Ok(self)
    }
}

/// Wraps a response callback to do two things:
/// 1. Save a copy of the completed response (via a channel) before forwarding it.
/// 2. Stop emitting tokens to the outer callback once a `tool_call_begin_token` has been seen.
pub(crate) fn wrap_respond<F>(
    respond: F,
    tool_call_begin_token: Option<String>,
) -> (
    impl FnMut(WriteOutput),
    std::sync::mpsc::Receiver<String>,
)
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
