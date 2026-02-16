//! Token generation and response handling for ChatWorker.
//!
//! Implements methods for generating model responses including token sampling, decoding,
//! context updates, and streaming output through callback functions.

use crate::errors::{DecodingError, GenerateResponseError, WrappedResponseError};
use crate::llm::{GlobalInferenceLockToken, Worker, WriteOutput};
use crate::sampler_config::SamplerConfig;
use llama_cpp_2::model::Special;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use std::sync::MutexGuard;
use tracing::{debug, trace, trace_span};

use super::ChatWorker;

impl Worker<'_, ChatWorker> {
    // ---------- IMPORTANT ----------
    // Should only be used under a global inference lock
    // This is a safety meassure to prevent bugs from multiple
    // contexts with the same model. It might not be necessary
    // but assume it is.
    pub(crate) fn generate_response_until_done<F>(
        &mut self,
        sampler_config: SamplerConfig,
        mut respond: F,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, GenerateResponseError>
    where
        F: FnMut(WriteOutput),
    {
        // Token generation loop
        tracing::info!("Worker writing until done");

        // pre-allocating 4096 bytes for the response string
        // 4096 is a very randomly chosen number. how does this affect performance?
        let mut full_response: String = String::with_capacity(4096);
        let mut tokens_written_until_now = vec![];

        // initialize sampler
        // stateful samplers only live for one response
        let mut sampler = sampler_config.to_stateful(self.ctx.model)?;
        let mut token_bytes_vec = Vec::new();

        while !self.should_stop() {
            // Check if the context is full
            if self.n_past as u32 == self.ctx.n_ctx() {
                self.context_shift()?;
                let rendered_tokens = self.get_render_as_tokens()?;
                self.sync_context_with_render(rendered_tokens, inference_lock_token)?;
                self.read_tokens(tokens_written_until_now.clone(), inference_lock_token)?;
                // do not update tokens_in_context as this is done later by ask
            }

            // Sample next token, no need to use sampler.accept as sample already accepts the token.
            // using sampler.accept() will cause the sampler to crash when using grammar sampling.
            // https://github.com/utilityai/llama-cpp-rs/issues/604
            let new_token = self.sample_and_decode_next_token(&mut sampler)?;
            tokens_written_until_now.push(new_token);

            // Attempt to convert token(s) to bytes
            let token_bytes = self
                .ctx
                .model
                .token_to_bytes(new_token, Special::Tokenize)?;

            token_bytes_vec.extend(token_bytes);

            // Attempt to convert bytes to utf8 string.

            let token_str = match std::str::from_utf8(&token_bytes_vec) {
                Ok(str) => str,
                Err(_) => {
                    if token_bytes_vec.len() > 4 {
                        "�"
                    } else {
                        continue;
                    }
                }
            };

            // Basic solution to split up graphemes. If the current token bytes cannot
            // be converted into a string then we try to read more tokens till we have
            // at least four bytes. If these still cannot be converted into a string,
            // we assume that the model/sampler has produced a useless token somewhere.
            // This we currently handle by discarding all of the current bytes, but more
            // intelligent solutions could be a good idea.

            trace!(?new_token, ?token_str);
            let has_eog = self.ctx.model.is_eog_token(new_token);

            if !has_eog {
                full_response.push_str(token_str);
                trace!(?token_str, "Sending out token:");
                respond(WriteOutput::Token(token_str.to_string()));
            }

            // done using token_str, so now we can clear token_bytes_vec
            token_bytes_vec.clear();

            if has_eog {
                break;
            }
        }

        // we're done!
        debug!(%full_response, "Sending out");
        respond(WriteOutput::Done(full_response));
        Ok(self)
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

    pub(crate) fn wrapped_update_context_and_generate_response<F>(
        &mut self,
        sampler: SamplerConfig,
        respond: F,
        tool_call_begin_token: Option<String>,
    ) -> Result<String, WrappedResponseError>
    where
        F: Fn(WriteOutput) + Clone,
    {
        // Check how much of the current KVCache we can keep
        let mut rendered_tokens = self.get_render_as_tokens()?;

        if rendered_tokens.len() > self.ctx.n_ctx() as usize {
            self.context_shift()?;
            rendered_tokens = self.get_render_as_tokens()?;
        }

        let _gil_guard = crate::llm::GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();
        self.sync_context_with_render(rendered_tokens, &inference_lock_token)?;

        // wrap the response callback to keep a copy of the completed response
        // and to avoid emitting tool calls
        let (wrapped_respond, resp_receiver) = wrap_respond(respond.clone(), tool_call_begin_token);

        // llm go brrr
        self.generate_response_until_done(sampler, wrapped_respond, &inference_lock_token)?;

        Ok(resp_receiver.recv()?)
    }
}

/// wraps a response function in a closure to do two things:
/// 1. save a copy of the response (using a channel) before sending it out
/// 2. skip emitting once a tool_call_begin_token has been seen
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
            WriteOutput::Token(_) => (),
        }
        if emitting {
            respond(x)
        }
    };
    (wrapped_respond, resp_receiver)
}
