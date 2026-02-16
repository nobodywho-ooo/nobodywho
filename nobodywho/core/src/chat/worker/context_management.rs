//! Context management for ChatWorker.
//!
//! Implements methods for syncing the LLM context with rendered chat templates, performing
//! context shifts when the context window fills up, and managing the token cache for efficient
//! prefix reuse.

use crate::errors::{ContextSyncError, RenderError, ShiftError};
use crate::llm::{GlobalInferenceLockToken, Worker};
use crate::template::ChatTemplateContext;
use llama_cpp_2::token::LlamaToken;
use std::cmp::min;
use std::sync::MutexGuard;
use tracing::info;

use super::super::{Message, Role};
use super::ChatWorker;

/// Utility function for prefix caching
/// Given a rendered chat template (intended for the LLM's context),
/// it compares with the tokens currently in the LLM's context, to find a common prefix.
/// The return value is a tuple of:
/// - the index of the first differing token
///   and
/// - the tokens that should be read into the context (starting at that index)
pub(crate) fn find_prefix_index_and_difference_with_tokens_in_context(
    tokens_in_context: &[LlamaToken],
    tokens: &[LlamaToken],
) -> (usize, Vec<LlamaToken>) {
    if tokens_in_context.is_empty() {
        return (0, tokens.to_owned());
    }

    let longest_common_prefix_index = tokens_in_context
        .iter()
        .zip(tokens.iter())
        .position(|(a, b)| a != b);

    let (index, difference): (usize, Vec<LlamaToken>) = match longest_common_prefix_index {
        Some(i) => (i, tokens[i..].to_vec()),
        None => {
            if tokens.len() <= tokens_in_context.len() {
                (tokens.len(), vec![])
            } else {
                (
                    tokens_in_context.len(),
                    tokens[(tokens_in_context.len())..].to_vec(),
                )
            }
        }
    };

    (index, difference)
}

impl Worker<'_, ChatWorker> {
    /// Compare tokens from a template-rendered chat history with the tokens in the LLM's context,
    /// and perform the LLM 'reading' to make the LLM's context match the rendered tokens exactly.
    /// Because this invokes the model, this is potentially an expensive method to call.
    pub(crate) fn sync_context_with_render(
        &mut self,
        rendered_tokens: Vec<LlamaToken>,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<(), ContextSyncError> {
        let (prefix_index, token_difference) =
            find_prefix_index_and_difference_with_tokens_in_context(
                &self.extra.tokens_in_context,
                &rendered_tokens,
            );

        self.remove_all_tokens_from_index_from_ctx(prefix_index)?;
        if !token_difference.is_empty() {
            self.read_tokens(token_difference, inference_lock_token)?;
        }
        self.extra.tokens_in_context = rendered_tokens;

        Ok(())
    }

    pub(crate) fn context_shift(&mut self) -> Result<(), ShiftError> {
        info!("Context shift happens!");
        let target_token_size = (self.ctx.n_ctx() / 2) as usize;
        let mut messages = self.extra.messages.clone();

        // Find indices to preserve
        let system_end = if matches!(messages[0].role(), Role::System) {
            1
        } else {
            0
        };
        let first_user_message_index =
            self.find_next_user_message(&messages, system_end)
                .ok_or(ShiftError::Message(
                    "No first user message in chat history".into(),
                ))?;
        let first_deletable_index = self
            .find_next_user_message(&messages, first_user_message_index + 1)
            .ok_or(ShiftError::Message("No deletable messages".into()))?; // Assuming assistant after user
        let mut last_deletable_index = self
            .find_start_of_last_n_user_messages(&messages, 2)
            .ok_or(ShiftError::Message(
                "Less than two user messages in chat history.".into(),
            ))?
            - 1;

        // Two is the smallest number of messages we can delete as we need to preserve the message structure.
        // There might be a better start guess here.
        let mut messages_to_delete = 2;

        // Delete messages until context is small enough or only essential messages are left.
        // Double the number of messages to delete each iteration. This is a simple and kind of stupid solution, as it might overshoot by a lot.
        // Plenty of optimization options here.

        loop {
            // No non-essential messages left to delete or the new context has reached desired size.
            if first_deletable_index > last_deletable_index
                || self
                    .ctx
                    .model
                    .str_to_token(
                        &self.extra.chat_template.render_unhandled(
                            &messages,
                            &ChatTemplateContext {
                                enable_thinking: self.extra.allow_thinking,
                                tools: self.extra.tools.clone(),
                                tool_format: self.extra.tool_format.clone(),
                            },
                        )?,
                        self.add_bos,
                    )?
                    .len()
                    <= target_token_size
            {
                break;
            }
            let target_delete_index = min(
                first_deletable_index + messages_to_delete - 1,
                last_deletable_index,
            );

            // Find the first user message after target delete index and choose the message before.
            // This is to ensure that resulting chat history still follows the user then assistant format
            let delete_index = min(
                self.find_next_user_message(&messages, target_delete_index + 1)
                    .ok_or(ShiftError::Message(
                        "Could find user message supposed to be there".into(),
                    ))?
                    - 1,
                last_deletable_index,
            ); // should never fail
            messages.drain(first_deletable_index..=delete_index);
            messages_to_delete *= 2;

            let messages_deleted = delete_index - first_deletable_index + 1;

            last_deletable_index -= messages_deleted;
        }

        self.extra.messages = messages;
        Ok(())
    }

    pub(crate) fn find_next_user_message(
        &self,
        messages: &[Message],
        start_index: usize,
    ) -> Option<usize> {
        messages[start_index..]
            .iter()
            .position(|msg| msg.role() == &Role::User)
            .map(|pos| pos + start_index)
    }

    pub(crate) fn find_start_of_last_n_user_messages(
        &self,
        messages: &[Message],
        n: usize,
    ) -> Option<usize> {
        let user_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, msg)| msg.role() == &Role::User)
            .map(|(idx, _)| idx)
            .collect();

        if user_indices.len() >= n {
            Some(user_indices[user_indices.len() - n])
        } else {
            None
        }
    }

    pub(crate) fn get_render_as_tokens(&mut self) -> Result<Vec<LlamaToken>, RenderError> {
        let render_as_string = self.extra.chat_template.render(
            &self.extra.messages,
            &ChatTemplateContext {
                enable_thinking: self.extra.allow_thinking,
                tools: self.extra.tools.clone(),
                tool_format: self.extra.tool_format.clone(),
            },
        )?;

        let render_as_tokens = self
            .ctx
            .model
            .str_to_token(&render_as_string, self.add_bos)?;
        Ok(render_as_tokens)
    }
}
