//! High-level chat API for conversational AI with tool calling support.
//!
//! This module provides an ergonomic interface for chat-based interactions with language models,
//! including support for streaming responses, tool calling, and conversation management.
//!
//! # Quick Start
//!
//! ```
//! use nobodywho::chat::ChatBuilder;
//! use nobodywho::llm;
//! use std::sync::Arc;
//!
//! # fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let model = Arc::new(llm::get_model("model.gguf", true, None, None, None)?);
//!
//! let chat = ChatBuilder::new(model)
//!     .with_system_prompt(Some("You are a helpful assistant"))
//!     .build()?;
//!
//! let response = chat.ask("Hello!").blocking_completed()?;
//! # Ok(())
//! # }
//! ```
//!

pub use crate::content::{ContentPart, MessageContent};
use crate::errors::{
    CompleteError, CompletionError, ContextSyncError, GenerateResponseError, InitWorkerError,
    InvalidContextShiftOptions, InvalidHistoryError, MultimodalError, RenderError, SayError,
    SetterError, ShiftError, TokenizeError, ToolCallingSetupError,
};
use crate::inference::InferenceEngine;
use crate::llm;
use crate::output_format::{self, FormatError, ModelOutput, ResolvedFormat};
use crate::response_parser::ResponseParser;
use crate::sampler::{read_sampler_from_metadata, GrammarFactory, SamplerConfig};
use crate::template::{select_template, ChatTemplate, ChatTemplateContext};
use crate::tokenizer::{
    find_chunks_prefix_difference, ChunkId, Promptable, TokenizerChunk, TokenizerChunks,
};
use crate::tool_calling::{Tool, ToolCall};
use ahash::AHasher;
use indexmap::IndexMap;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::mtmd::MtmdBitmap;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::hash::Hasher;
use std::mem;
use std::num::NonZeroUsize;
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::{debug, error, info, warn};
use yoke::{Yoke, Yokeable};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User {
        content: MessageContent,
    },
    // The optional tool_calls field distinguishes a plain assistant response
    // from one that includes tool calls. When tool_calls is Some, content holds
    // whatever the model said before the first call.
    // https://github.com/QwenLM/Qwen3/blob/e5a1d326/docs/source/framework/function_call.md
    Assistant {
        content: MessageContent,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },
    System {
        content: MessageContent,
    },
    Tool {
        name: String,
        content: MessageContent,
    },
}

impl Message {
    pub fn is_user(&self) -> bool {
        matches!(self, Message::User { .. })
    }

    pub fn is_assistant(&self) -> bool {
        matches!(self, Message::Assistant { .. })
    }

    pub fn is_system(&self) -> bool {
        matches!(self, Message::System { .. })
    }

    pub fn is_tool(&self) -> bool {
        matches!(self, Message::Tool { .. })
    }

    pub fn has_tool_calls(&self) -> bool {
        matches!(
            self,
            Message::Assistant {
                tool_calls: Some(_),
                ..
            }
        )
    }

    pub fn content_ref(&self) -> &MessageContent {
        match self {
            Message::User { content, .. }
            | Message::Assistant { content, .. }
            | Message::System { content, .. }
            | Message::Tool { content, .. } => content,
        }
    }

    pub fn content_mut(&mut self) -> &mut MessageContent {
        match self {
            Message::User { content, .. }
            | Message::Assistant { content, .. }
            | Message::System { content, .. }
            | Message::Tool { content, .. } => content,
        }
    }

    /// The content flattened to text, with a media marker holding the position
    /// of each media part.
    pub fn content(&self) -> String {
        self.content_ref().to_string()
    }

    /// Ids of the bitmaps this message's media parts reference, in order.
    pub fn media_ids(&self) -> Vec<&str> {
        self.content_ref()
            .media_parts()
            .into_iter()
            .filter_map(ContentPart::id)
            .collect()
    }

    pub fn new_user(content: impl Into<MessageContent>) -> Self {
        Self::User {
            content: content.into(),
        }
    }

    pub fn new_assistant(content: impl Into<MessageContent>) -> Self {
        Self::Assistant {
            content: content.into(),
            tool_calls: None,
        }
    }

    pub fn new_system(content: impl Into<MessageContent>) -> Self {
        Self::System {
            content: content.into(),
        }
    }

    pub fn new_tool(name: String, content: impl Into<MessageContent>) -> Self {
        Self::Tool {
            name,
            content: content.into(),
        }
    }

    fn role(&self) -> &'static str {
        match self {
            Message::User { .. } => "user",
            Message::Assistant { .. } => "assistant",
            Message::System { .. } => "system",
            Message::Tool { .. } => "tool",
        }
    }
}

/// A conversation the chat can hold. Construction is the check: no system
/// message carries media, since the system prompt is stored as plain text and no
/// chat template renders media in that role. The rest of the chat leans on that
/// rather than re-checking it.
///
/// A system message may sit anywhere: a leading one becomes the chat's system
/// prompt, a later one stays in the history for the template to render in place.
#[derive(Clone, Debug, Default, Serialize)]
#[serde(transparent)]
pub struct History(Vec<Message>);

/// Deliberately no `DerefMut`: mutation goes through the methods below, which
/// cannot break the invariant.
impl std::ops::Deref for History {
    type Target = [Message];

    fn deref(&self) -> &[Message] {
        &self.0
    }
}

impl History {
    pub fn new(messages: Vec<Message>) -> Result<Self, InvalidHistoryError> {
        if messages
            .iter()
            .any(|message| message.is_system() && !message.content_ref().media_parts().is_empty())
        {
            return Err(InvalidHistoryError::MediaInSystemMessage);
        }
        Ok(Self(messages))
    }

    pub fn into_vec(self) -> Vec<Message> {
        self.0
    }

    /// Split an OpenAI-shaped array into the two things the chat stores: a
    /// leading system message becomes the prompt, the rest stays history. Cannot
    /// fail, since the type already ruled out media that has no place in text.
    pub fn take_system_prompt(&mut self) -> Option<String> {
        let Some(Message::System { content }) = self.0.first() else {
            return None;
        };
        let system_prompt = content.to_string();
        self.0.remove(0);
        Some(system_prompt)
    }

    /// The conversation as the chat template expects to see it: the system
    /// prompt, which the chat holds as a setting rather than a turn, back in front.
    pub fn with_system_prompt(&self, system_prompt: Option<&str>) -> Self {
        Self(
            system_prompt
                .map(|content| Message::new_system(content.to_string()))
                .into_iter()
                .chain(self.0.iter().cloned())
                .collect(),
        )
    }

    /// Drop `range`, except the system messages in it: those are instructions for
    /// the rest of the conversation, not part of the exchange being forgotten.
    pub fn forget(&mut self, range: std::ops::Range<usize>) {
        let kept: Vec<Message> = self.0[range.clone()]
            .iter()
            .filter(|message| message.is_system())
            .cloned()
            .collect();
        self.0.splice(range, kept);
    }

    /// The only way to mutate a message: system content stays out of reach, so
    /// media cannot get into it.
    pub fn non_system_content_mut(&mut self) -> impl Iterator<Item = &mut MessageContent> {
        self.0
            .iter_mut()
            .filter(|message| !message.is_system())
            .map(Message::content_mut)
    }

    /// These roles may carry media, so appending one needs no check.
    pub fn push_user(&mut self, content: impl Into<MessageContent>) {
        self.0.push(Message::new_user(content));
    }

    pub fn push_assistant(&mut self, content: impl Into<MessageContent>) {
        self.0.push(Message::new_assistant(content));
    }

    pub fn push_tool_calls(
        &mut self,
        content: impl Into<MessageContent>,
        tool_calls: Vec<ToolCall>,
    ) {
        self.0.push(Message::Assistant {
            content: content.into(),
            tool_calls: Some(tool_calls),
        });
    }

    pub fn push_tool_response(&mut self, name: String, content: impl Into<MessageContent>) {
        self.0.push(Message::new_tool(name, content));
    }

    /// The one role that can break the invariant, so the one push that can fail.
    pub fn push_system(
        &mut self,
        content: impl Into<MessageContent>,
    ) -> Result<(), InvalidHistoryError> {
        let content = content.into();
        if !content.media_parts().is_empty() {
            return Err(InvalidHistoryError::MediaInSystemMessage);
        }
        self.0.push(Message::System { content });
        Ok(())
    }
}

/// A trailing assistant message would render without a generation prompt, making
/// the model continue that message instead of replying to the user.
///
/// A property of one request rather than of a history — the chat's own history
/// ends in an assistant message most of the time — so it is checked here rather
/// than by [`History`].
fn check_answerable(messages: &History) -> Result<(), InvalidHistoryError> {
    let Some(last) = messages.last() else {
        return Err(InvalidHistoryError::Empty);
    };
    if !(last.is_user() || last.is_tool()) {
        return Err(InvalidHistoryError::DoesNotEndInUserOrTool { role: last.role() });
    }
    Ok(())
}

/// Indices of the user messages, i.e. the start of each conversational turn.
/// Anything before the first index is a prefix that a context shift never touches.
fn user_message_indices(messages: &[Message]) -> Vec<usize> {
    messages
        .iter()
        .enumerate()
        .filter(|(_, message)| message.is_user())
        .map(|(index, _)| index)
        .collect()
}

/// Settings to apply before a [`complete`](ChatHandle::complete) turn.
///
/// `None` keeps what the chat has; `Some(v)` sets it and leaves it set, like a
/// leading system message sets the system prompt. Fill every field and the turn
/// stops depending on the chat's current state.
#[derive(Clone, Debug, Default)]
pub struct Options {
    pub sampler: Option<SamplerConfig>,
    /// Replaces the chat's template variables wholesale.
    pub template_variables: Option<std::collections::HashMap<String, bool>>,
    /// Re-selects the chat template, so the turn re-prefills from near token
    /// zero. `Some(vec![])` removes the tools.
    pub tools: Option<Vec<Tool>>,
}

impl Options {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sampler(mut self, sampler: SamplerConfig) -> Self {
        self.sampler = Some(sampler);
        self
    }

    pub fn with_template_variables(
        mut self,
        variables: std::collections::HashMap<String, bool>,
    ) -> Self {
        self.template_variables = Some(variables);
        self
    }

    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools = Some(tools);
        self
    }
}

/// Tuning for MTP speculative decoding.
///
/// Attaching one to a chat (via [`ChatBuilder::with_mtp`] or
/// [`ChatConfig::mtp`]) is what *enables* MTP — `None` runs the solo decode
/// path. Requires the [`llm::Model`] to have been loaded with a compatible
/// `draft_model_path`, otherwise worker construction fails with
/// `InitWorkerError::MtpDraftModelNotLoaded`.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MtpConfig {
    /// Maximum draft tokens proposed per speculative step (llama.cpp `n_max`).
    /// Higher values draft more per decode; returns diminish past ~4–6.
    pub k_max: u32,
    /// Minimum draft-token probability the drafter will propose (llama.cpp
    /// `p_min`). `0.0` accepts all proposals; raise it to skip low-confidence
    /// drafts.
    pub p_min: f32,
}

impl Default for MtpConfig {
    fn default() -> Self {
        // Mirrors llama.cpp's MtpSpeculativeParams::default() (n_max=3, p_min=0.0).
        // Binding-side field defaults must mirror these exact values.
        Self {
            k_max: 3,
            p_min: 0.0,
        }
    }
}

/// How a chat forgets old turns when its context is full.
///
/// A turn is a user message and everything up to the next one. System messages
/// and anything before the first user message are always kept.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct ContextShiftOptions {
    /// Turns always kept at the start of the history.
    pub keep_first_turns: usize,
    /// Turns always kept at the end of the history. Must be at least 1, so the
    /// message being answered survives.
    pub keep_last_turns: usize,
    /// Size the rendered history is shrunk to.
    pub target: ShiftTarget,
}

impl Default for ContextShiftOptions {
    fn default() -> Self {
        Self {
            keep_first_turns: 1,
            keep_last_turns: 2,
            target: ShiftTarget::Fraction(0.5),
        }
    }
}

/// Size a context shift shrinks the rendered history to.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ShiftTarget {
    /// A fraction of the context size, in `(0, 1)`.
    Fraction(f32),
    /// A number of tokens, below the context size.
    Tokens(u32),
}

/// [`ContextShiftOptions`] checked against a context size; only
/// [`ContextShiftOptions::parse`] makes one.
#[derive(Debug, Clone, Copy)]
struct ContextShift {
    keep_first_turns: usize,
    keep_last_turns: NonZeroUsize,
    target_tokens: usize,
}

impl ContextShiftOptions {
    fn parse(self, n_ctx: u32) -> Result<ContextShift, InvalidContextShiftOptions> {
        let invalid = InvalidContextShiftOptions;
        let keep_last_turns = NonZeroUsize::new(self.keep_last_turns)
            .ok_or_else(|| invalid("keep_last_turns must be at least 1".into()))?;
        let target_tokens = match self.target {
            ShiftTarget::Fraction(f) if !(f > 0.0 && f < 1.0) => {
                return Err(invalid(format!("target fraction {f} is not in (0, 1)")));
            }
            ShiftTarget::Tokens(t) if t == 0 || t >= n_ctx => {
                return Err(invalid(format!(
                    "target of {t} tokens is not in (0, {n_ctx})"
                )));
            }
            ShiftTarget::Fraction(f) => (n_ctx as f32 * f) as usize,
            ShiftTarget::Tokens(t) => t as usize,
        };
        Ok(ContextShift {
            keep_first_turns: self.keep_first_turns,
            keep_last_turns,
            target_tokens,
        })
    }
}

/// Configuration for chat sessions.
///
/// This struct groups all the settings needed to initialize a chat worker.
/// Use [`ChatBuilder`] for a more ergonomic way to configure these settings.
pub struct ChatConfig {
    /// Available tools for the model to use.
    pub tools: Vec<Tool>,
    /// Context window size.
    pub n_ctx: u32,
    /// System prompt for the chat session.
    pub system_prompt: Option<String>,
    /// Variables to add to the chat template context.
    pub template_variables: std::collections::HashMap<String, bool>,
    /// Sampler configuration for inference.
    pub sampler_config: Option<SamplerConfig>,
    /// MTP speculative decoding config. `Some(..)` enables MTP with the given
    /// tuning; `None` (the default) runs the solo decode path. Requires the
    /// [`llm::Model`] to have been loaded with a compatible `draft_model_path`
    /// (see `llm::get_model`) — otherwise worker construction fails with
    /// `InitWorkerError::MtpDraftModelNotLoaded`.
    pub mtp: Option<MtpConfig>,
    /// Threads used for inference. `None` (the default) detects the host's physical core
    /// count — performance cores only, on Apple silicon — because hyperthread siblings and
    /// efficiency cores slow down ggml's per-node thread barrier. Set it lower to leave CPU
    /// headroom for other work. Values are clamped to the logical CPU count.
    pub n_threads: Option<u32>,
    /// How old turns are forgotten when the context is full. `None` disables
    /// shifting, so a full context is an error instead.
    pub context_shift: Option<ContextShiftOptions>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            n_ctx: 4096,
            template_variables: std::collections::HashMap::new(),
            system_prompt: None,
            tools: Vec::new(),
            sampler_config: None,
            mtp: None,
            n_threads: None,
            context_shift: Some(ContextShiftOptions::default()),
        }
    }
}

/// Builder for creating a [`ChatHandle`] with a fluent API.
///
/// # Example
/// ```
/// use nobodywho::chat::{ChatBuilder};
/// use nobodywho::tool_calling::Tool;
/// use nobodywho::llm;
/// use std::sync::Arc;
///
/// # fn example() -> Result<(), Box<dyn std::error::Error>> {
/// let model = Arc::new(llm::get_model("model.gguf", true, None, None, None)?);
///
/// let my_tool = Tool::new(
///     "example".to_string(),
///     "Example tool".to_string(),
///     serde_json::json!({}),
///     Arc::new(|_| "result".to_string())
/// );
///
/// let chat = ChatBuilder::new(model)
///     .with_context_size(4096)
///     .with_system_prompt(Some("You're a helpful assistant"))
///     .with_tool(my_tool)
///     .build();
/// # Ok(())
/// # }
/// ```
pub struct ChatBuilder {
    model: Arc<llm::Model>,
    config: ChatConfig,
}

impl ChatBuilder {
    /// Create a new chat builder with a model.
    pub fn new(model: Arc<llm::Model>) -> Self {
        Self {
            model,
            config: ChatConfig::default(),
        }
    }

    /// Set the context size for the chat session.
    pub fn with_context_size(mut self, n_ctx: u32) -> Self {
        self.config.n_ctx = n_ctx;
        self
    }

    /// Set the system prompt for the chat session.
    pub fn with_system_prompt<S: Into<String>>(mut self, prompt: Option<S>) -> Self {
        self.config.system_prompt = prompt.map(|s| s.into());
        self
    }

    /// Add a tool that the model can use.
    pub fn with_tool(mut self, tool: Tool) -> Self {
        self.config.tools.push(tool);
        self
    }

    /// Add multiple tools that the model can use.
    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.config.tools.extend(tools);
        self
    }

    /// DEPRECATED: Use with_template_variable("enable_thinking", value) instead.
    #[deprecated(
        since = "0.6.0",
        note = "Use with_template_variable(\"enable_thinking\", value) instead"
    )]
    pub fn with_allow_thinking(mut self, allow_thinking: bool) -> Self {
        self.config
            .template_variables
            .insert("enable_thinking".to_string(), allow_thinking);
        self
    }

    /// Add a single template variable
    pub fn with_template_variable(mut self, variable_name: String, value: bool) -> Self {
        self.config.template_variables.insert(variable_name, value);
        self
    }

    /// Set the template_variables
    pub fn with_template_variables(
        mut self,
        variables: std::collections::HashMap<String, bool>,
    ) -> Self {
        self.config.template_variables = variables;
        self
    }

    /// Set a custom sampler configuration
    pub fn with_sampler(mut self, sampler: SamplerConfig) -> Self {
        self.config.sampler_config = Some(sampler);
        self
    }

    /// Enable MTP speculative decoding for this chat with the given tuning.
    pub fn with_mtp(mut self, config: MtpConfig) -> Self {
        self.config.mtp = Some(config);
        self
    }

    /// Set the number of threads used for inference.
    ///
    /// Leave this unset to detect the host's physical core count (performance cores only, on
    /// Apple silicon), which is usually fastest — hyperthread siblings and efficiency cores
    /// slow down ggml's per-node thread barrier. Set it lower to leave CPU headroom for other
    /// work. The value is clamped to the logical CPU count.
    pub fn with_n_threads(mut self, n_threads: u32) -> Self {
        self.config.n_threads = Some(n_threads);
        self
    }

    /// Set how old turns are forgotten when the context is full; `None` disables it.
    pub fn with_context_shift(mut self, options: Option<ContextShiftOptions>) -> Self {
        self.config.context_shift = options;
        self
    }

    /// Build the chat handle.
    pub fn build(self) -> Result<ChatHandle, InitWorkerError> {
        ChatHandle::new(self.model, self.config)
    }
}

/// Interact with [`Chat`] behind a `Send` + `Sync` mutex.
///
/// Use [`ChatBuilder`] to create a new instance with a fluent API.
#[derive(Clone)]
pub struct ChatHandle {
    chat: Arc<tokio::sync::Mutex<Chat>>,
    should_stop: Arc<AtomicBool>,
}

impl ChatHandle {
    pub fn new(model: Arc<llm::Model>, config: ChatConfig) -> Result<Self, InitWorkerError> {
        // TODO: Allow building the chat asynchronously in a thread?
        //
        // Difficult to do, because most language constructors such as
        // Python's `__new__` are not async-able.
        let chat = Chat::new(model, config)?;
        let should_stop = Arc::clone(&chat.should_stop);
        Ok(Self {
            chat: Arc::new(tokio::sync::Mutex::new(chat)),
            should_stop,
        })
    }

    /// Lock the chat synchronously.
    pub fn blocking_lock(&self) -> tokio::sync::MutexGuard<'_, Chat> {
        self.chat.blocking_lock()
    }

    /// Lock the chat asynchronously.
    pub async fn lock(&self) -> tokio::sync::MutexGuard<'_, Chat> {
        self.chat.lock().await
    }

    /// Send a message and collect tokens as they arrive.
    ///
    /// # Example
    /// ```
    /// # use nobodywho::chat::ChatHandle;
    /// # fn example(chat: &ChatHandle) -> Result<(), nobodywho::errors::CompletionError> {
    /// let mut stream = chat.ask("Tell me a story");
    /// while let Some(token) = stream.blocking_next_token()? {
    ///     print!("{}", token);
    /// }
    /// print!("{}", stream.blocking_completed()?);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// # use nobodywho::chat::ChatHandle;
    /// # async fn example(chat: &ChatHandle) -> Result<(), nobodywho::errors::CompletionError> {
    /// let mut stream = chat.ask("Tell me a story");
    /// while let Some(token) = stream.next_token().await? {
    ///     print!("{}", token);
    /// }
    /// print!("{}", stream.completed().await?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn ask(&self, prompt: impl Promptable) -> TokenStream {
        let prompt = prompt.to_prompt();

        let chat = Arc::clone(&self.chat);
        let worker = CompletionWorker::new(Arc::clone(&self.should_stop), move |msg_tx| {
            // Lock the chat for the whole duration of the generation! This
            // is required by `generator`'s invariants.
            let mut chat = chat.blocking_lock_owned();

            let mut generator = match chat.ask(prompt) {
                Ok(generator) => generator,
                Err(e) => {
                    let _ = msg_tx.send(Err(e.into()));
                    return;
                }
            };

            loop {
                match generator.next(&mut chat, None, true) {
                    Ok(ControlFlow::Continue(text)) => {
                        for text in text.into_iter().filter(|t| !t.is_empty()) {
                            if msg_tx.send(Ok(CompletionChunk::Token(text))).is_err() {
                                // Nobody is receiving any more, so stop generating.
                                return;
                            }
                        }
                    }
                    Ok(ControlFlow::Break(response)) => {
                        let _ = msg_tx.send(Ok(CompletionChunk::Done(response)));
                        return;
                    }
                    Err(e) => {
                        let _ = msg_tx.send(Err(e.into()));
                        return;
                    }
                }
            }
        });
        TokenStream::new(worker)
    }

    /// Answer a full message list, which replaces the chat history.
    ///
    /// The list is the whole conversation: it must be non-empty and end in a user
    /// or tool message. A leading system message becomes the chat's system
    /// prompt; a list without one keeps the prompt the chat already had. A system
    /// message further in stays in the history as a turn of its own, which works
    /// with the chat templates that render the system role mid-conversation. The
    /// response is appended, and a following `ask` continues from there.
    ///
    /// [`Options`] follows the same rule for the chat's other settings.
    ///
    /// # Example
    ///
    /// ```
    /// # use nobodywho::chat::{ChatHandle, Message, Options};
    /// # fn example(chat: &ChatHandle) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut stream = chat.complete(
    ///     vec![
    ///         Message::new_system("You are terse.".to_string()),
    ///         Message::new_user("Who first walked on the moon?".to_string()),
    ///     ],
    ///     Options::new().with_template_variables(
    ///         [("enable_thinking".to_string(), false)].into(),
    ///     ),
    /// )?;
    /// println!("{}", stream.blocking_completed()?);
    /// # Ok(())
    /// # }
    /// ```
    ///
    /// ```
    /// # use nobodywho::chat::{ChatHandle, Message, Options};
    /// # async fn example(chat: &ChatHandle) -> Result<(), Box<dyn std::error::Error>> {
    /// let mut stream = chat.complete(
    ///     vec![Message::new_user("Who first walked on the moon?".to_string())],
    ///     Options::new(),
    /// )?;
    /// println!("{}", stream.completed().await?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn complete(
        &self,
        messages: Vec<Message>,
        options: Options,
    ) -> Result<TokenStream, InvalidHistoryError> {
        let stream = self.completion_stream(messages, options, None, true)?;
        Ok(TokenStream::new(stream.worker))
    }

    /// Generate one assistant response.
    ///
    /// If `execute_tools` is `false`, this returns return tool calls for the
    /// caller to execute and submit in a following request.
    pub fn completion_stream(
        &self,
        messages: Vec<Message>,
        options: Options,
        max_tokens: Option<usize>,
        execute_tools: bool,
    ) -> Result<CompletionStream, InvalidHistoryError> {
        let messages = History::new(messages)?;
        check_answerable(&messages)?;

        let chat = Arc::clone(&self.chat);
        let worker = CompletionWorker::new(Arc::clone(&self.should_stop), move |msg_tx| {
            // Lock the chat for the whole duration of the generation! This
            // is required by `generator`'s invariants.
            let mut chat = chat.blocking_lock_owned();

            let mut generator = match chat.complete(messages, options) {
                Ok(generator) => generator,
                Err(e) => {
                    let _ = msg_tx.send(Err(e.into()));
                    return;
                }
            };

            loop {
                match generator.next(&mut chat, max_tokens, execute_tools) {
                    Ok(ControlFlow::Continue(text)) => {
                        for text in text.into_iter().filter(|t| !t.is_empty()) {
                            if msg_tx.send(Ok(CompletionChunk::Token(text))).is_err() {
                                // Nobody is receiving any more, so stop generating.
                                return;
                            }
                        }
                    }
                    Ok(ControlFlow::Break(response)) => {
                        let _ = msg_tx.send(Ok(CompletionChunk::Done(response)));
                        return;
                    }
                    Err(e) => {
                        let _ = msg_tx.send(Err(e.into()));
                        return;
                    }
                }
            }
        });
        Ok(CompletionStream::new(worker))
    }

    /// Stop the current generation if one is in progress.
    pub fn stop_generation(&self) {
        self.should_stop.store(true, Ordering::Relaxed);
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FinishReason {
    Stop,
    Length,
    ToolCalls,
}

impl FinishReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Stop => "stop",
            Self::Length => "length",
            Self::ToolCalls => "tool_calls",
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct CompletionUsage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
}

impl CompletionUsage {
    pub fn total_tokens(self) -> usize {
        self.prompt_tokens + self.completion_tokens
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CompletionResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
    pub finish_reason: FinishReason,
    pub usage: CompletionUsage,
}

#[derive(Clone, Debug, PartialEq)]
pub enum CompletionChunk {
    Token(String),
    Done(CompletionResponse),
}

pub struct ChatStats {
    pub context_size: u32,
    pub context_used: u32,
}

/// Decoded media the history refers to, kept so every render can re-tokenize it.
/// Ids are content hashes and only mean something within this worker.
struct MediaStore {
    bitmaps: IndexMap<ChunkId, MtmdBitmap>,
}

impl MediaStore {
    fn new() -> Self {
        Self {
            bitmaps: IndexMap::new(),
        }
    }

    /// Store `bitmaps` and tag each with its id, returned in the same order.
    fn register(&mut self, bitmaps: Vec<MtmdBitmap>) -> Result<Vec<ChunkId>, MultimodalError> {
        let mut bitmap_ids = Vec::with_capacity(bitmaps.len());
        for mut bitmap in bitmaps {
            let id = bitmap_id(&bitmap);
            bitmap.set_id(&id)?;
            bitmap_ids.push(id.clone());
            self.bitmaps.entry(id).or_insert(bitmap);
        }
        Ok(bitmap_ids)
    }

    /// Whether this worker has the bitmap an id names. Ids are worker-local, so
    /// content from elsewhere carries ids this answers `false` for.
    fn contains(&self, id: &str) -> bool {
        self.bitmaps.contains_key(id)
    }

    fn get(&self, id: &str) -> Option<&MtmdBitmap> {
        self.bitmaps.get(id)
    }

    /// Drop the bitmaps no message refers to anymore.
    fn retain_referenced(&mut self, messages: &[Message]) {
        let referenced: HashSet<&str> = messages.iter().flat_map(|msg| msg.media_ids()).collect();
        self.bitmaps
            .retain(|id, _| referenced.contains(id.as_str()));
    }
}

fn bitmap_id(bitmap: &MtmdBitmap) -> ChunkId {
    let mut hasher = AHasher::default();
    hasher.write(bitmap.data());
    hasher.finish().to_string()
}

/// Builds the tool-call grammar sampler for the chat's output format, which
/// is detected once in `Chat::new`. `Ok(None)` if `tools` is empty, and an
/// error if tools are requested but no format was detected for this model.
///
/// `grammar_factory` carries the llguidance state between calls: the ~400ms init
/// is paid on the first build and every later one only compiles the grammar.
fn build_tool_sampler(
    model: &llama_cpp_2::model::LlamaModel,
    grammar_factory: &mut Option<GrammarFactory>,
    tools: &[Tool],
    sampler_config: &SamplerConfig,
    output_format: Option<&ResolvedFormat>,
) -> Result<Option<LlamaSampler>, ToolCallingSetupError> {
    if tools.is_empty() {
        return Ok(None);
    }

    let output_format = output_format.ok_or(FormatError::Undetected)?;

    let lark = output_format.grammar(tools)?;
    debug!(grammar = %lark, "Generated tool calling grammar (Lark)");

    // A chat's slice set never changes in practice (the format is detected once
    // and `slice_regexes` is a constant per format), so this builds at most once.
    let slices = output_format.slice_regexes();
    let rebuilt = GrammarFactory::build_if_stale(grammar_factory.as_ref(), model, slices)?;
    let factory = rebuilt
        .as_ref()
        .or(grammar_factory.as_ref())
        .expect("`build_if_stale` returns None only when the held factory serves us");

    let grammar_step = factory.grammar_step("lark", &lark)?;
    let tool_sampler =
        sampler_config.build_sampler_with_prepended_step(model, Some(grammar_step))?;

    // Every fallible function has run, so the rebuilt factory can be committed.
    if rebuilt.is_some() {
        *grammar_factory = rebuilt;
    }

    Ok(Some(tool_sampler))
}

/// The samplers a chat response can draw from: `base` for free generation,
/// `tool` (grammar-constrained) once the response enters a block of tool calls.
#[derive(Clone, Debug)]
pub(crate) struct ChatSampler {
    base_sampler: LlamaSampler,
    tool_call_sampler: Option<LlamaSampler>,
    grammar_activated: bool,
}

impl ChatSampler {
    fn new(base_sampler: LlamaSampler, tool_call_sampler: Option<LlamaSampler>) -> Self {
        Self {
            base_sampler,
            tool_call_sampler,
            grammar_activated: false,
        }
    }

    /// The sampler that should produce the next token.
    fn active(&mut self) -> &mut LlamaSampler {
        if self.grammar_activated {
            self.tool_call_sampler
                .as_mut()
                .expect("tool sampler must exist once the grammar is activated")
        } else {
            &mut self.base_sampler
        }
    }

    pub(crate) fn sample(&mut self, ctx: &LlamaContext<'_>, idx: i32) -> LlamaToken {
        // No need to use `sampler.accept` as `.sample` already accepts
        // the token: https://github.com/utilityai/llama-cpp-rs/issues/604
        self.active().sample(ctx, idx)
    }

    /// Switch to the tool sampler for the rest of the response, now that
    /// `begin` has opened a block of tool calls. Without tools there's no
    /// grammar to switch to.
    fn activate_tool_grammar(&mut self, begin: LlamaToken) {
        if self.grammar_activated {
            return;
        }
        if let Some(tool) = self.tool_call_sampler.as_mut() {
            // The grammar starts at `begin`, which the base sampler produced.
            tool.accept(begin);
            self.grammar_activated = true;
        }
    }

    fn set_base(&mut self, base: LlamaSampler) {
        self.base_sampler = base;
    }

    fn set_tool(&mut self, tool: Option<LlamaSampler>) {
        self.tool_call_sampler = tool;
    }

    /// Reset per-response state (RNG, penalty/DRY history, grammar matcher) and
    /// return to free generation.
    fn reset(&mut self) {
        self.base_sampler.reset();
        if let Some(ts) = self.tool_call_sampler.as_mut() {
            ts.reset();
        }
        self.grammar_activated = false;
    }
}

/// A chat session: owns an [`InferenceEngine`] plus all the conversational state
/// (messages, tools, template, sampler config).
pub struct Chat {
    context: Yoke<ChatContext<'static>, Arc<llm::Model>>,
    should_stop: Arc<AtomicBool>,
    model_output: ModelOutput,
    grammar_factory: Option<GrammarFactory>,
    sampler_config: SamplerConfig,
    messages: History,
    system_prompt: Option<String>,
    template_variables: std::collections::HashMap<String, bool>,
    tools: Vec<Tool>,
    chat_template: ChatTemplate,
    media: MediaStore,
    shift: Option<ContextShift>,
}

/// Chat state that references the model.
#[derive(Debug, Yokeable)]
struct ChatContext<'model> {
    engine: InferenceEngine<'model>,
    /// This actually holds a reference to the model, and should have a
    /// lifetime too: <https://github.com/utilityai/llama-cpp-rs/issues/1126>.
    sampler: ChatSampler,
}

impl Chat {
    fn new(model: Arc<llm::Model>, config: ChatConfig) -> Result<Chat, InitWorkerError> {
        if !model.is_generative_model() {
            let architecture = model
                .language_model
                .meta_val_str("general.architecture")
                .unwrap_or_else(|_| "unknown".into());
            return Err(InitWorkerError::NotAnLLM { architecture });
        }

        let template = select_template(&model.language_model, !config.tools.is_empty())?;

        let sampler_config = match config.sampler_config {
            Some(sc) => sc,
            None => read_sampler_from_metadata(&model.language_model).unwrap_or_default(),
        };

        // Depends only on the model, not on `config.tools`, so detect it once
        // here regardless. Only a hard error if tools were actually requested.
        let output_format = output_format::detect(&model.language_model)
            .and_then(|format| ResolvedFormat::new(format, &model.language_model));
        let model_output = match output_format {
            Ok(format) => {
                debug!(format = format.format().name, "Detected output format");
                ModelOutput::Formatted(Box::new(format))
            }
            Err(e) if config.tools.is_empty() => {
                info!(error = %e, "Failed to detect output format, so responses are read as plain text");
                ModelOutput::plain(&model.language_model)
            }
            Err(e) => return Err(InitWorkerError::ToolCallingSetup(e.into())),
        };

        let mut grammar_factory = None;

        // Build the low-level inference engine via the shared Worker constructor,
        // then take ownership of just the engine for the chat session.
        let context = Yoke::<ChatContext<'_>, _>::try_attach_to_cart(model, |model| {
            // Pre-build the base sampler for `sampler_config`, reused (via
            // `ChatSampler`) for every response.
            let base_sampler = sampler_config.build_sampler(&model.language_model)?;

            let tool_sampler = build_tool_sampler(
                &model.language_model,
                &mut grammar_factory,
                &config.tools,
                &sampler_config,
                model_output.resolved_format(),
            )?;

            let engine = InferenceEngine::new_with_type(
                model,
                config.n_ctx,
                false,
                config.mtp,
                config.n_threads,
                LlamaPoolingType::None,
            )?;

            Ok::<_, InitWorkerError>(ChatContext {
                engine,
                sampler: ChatSampler::new(base_sampler, tool_sampler),
            })
        })?;

        // Parse against the real context, which may be capped below the requested n_ctx.
        let shift = config
            .context_shift
            .map(|options| options.parse(context.get().engine.ctx.n_ctx()))
            .transpose()?;

        Ok(Chat {
            context,
            should_stop: Arc::new(AtomicBool::new(false)),
            model_output,
            grammar_factory,
            sampler_config,
            messages: History::default(),
            system_prompt: config.system_prompt,
            chat_template: template,
            template_variables: config.template_variables,
            tools: config.tools,
            media: MediaStore::new(),
            shift,
        })
    }

    fn engine(&self) -> &InferenceEngine<'_> {
        &self.context.get().engine
    }

    fn engine_mut<R: 'static>(
        &mut self,
        f: impl FnOnce(&mut InferenceEngine<'_>) -> R + 'static,
    ) -> R {
        self.context.with_mut_return(|c| f(&mut c.engine))
    }

    fn sampler_mut<R: 'static>(&mut self, f: impl FnOnce(&mut ChatSampler) -> R + 'static) -> R {
        self.context.with_mut_return(|c| f(&mut c.sampler))
    }

    fn add_assistant_message(&mut self, content: String) {
        self.messages.push_assistant(content);
    }

    fn add_user_message(&mut self, content: impl Into<MessageContent>) {
        self.messages.push_user(content);
    }

    fn add_tool_calls(&mut self, content: impl Into<MessageContent>, tool_calls: Vec<ToolCall>) {
        self.messages.push_tool_calls(content, tool_calls);
    }

    fn add_tool_resp(&mut self, name: String, content: String) {
        self.messages.push_tool_response(name, content);
    }

    /// Compare tokens from a template-rendered chat history with the tokens in the LLM's context,
    /// and perform the LLM 'reading' to make the LLM's context match the rendered tokens exactly.
    /// Because this invokes the model, this is potentially an expensive method to call.
    /// Returns the render the context now holds.
    #[tracing::instrument(level = "debug", skip_all)]
    fn sync_context_with_render(&mut self) -> Result<String, ContextSyncError> {
        let mut render = self.render(&self.messages, true)?;
        let mut chunks = self.chunks_of(&self.messages, render.clone())?;
        if chunks.n_tokens() > self.engine().ctx.n_ctx() as usize {
            self.context_shift(0)?;
            render = self.render(&self.messages, true)?;
            chunks = self.chunks_of(&self.messages, render.clone())?;
        }

        // We should never try to sync with an empty render
        debug_assert!(!chunks.is_empty());

        // If the model supports checkpoints, we save a checkpoint at this position
        // in sync_context: after the user message, before the generation prompt ("<im_start>assistant", etc.)
        let checkpoint_at = self.checkpoint_index(&chunks)?;

        // Diff against the KV mirror and load only the new tail.
        self.engine_mut(move |e| e.sync_context(chunks, checkpoint_at))?;
        self.media.retain_referenced(&self.messages);

        Ok(render)
    }

    /// Where a recurrent model should checkpoint in `render`: the end of the last user
    /// message, which later turns keep while e.g. dropping the answer's thinking.
    fn checkpoint_index(&self, chunks: &TokenizerChunks) -> Result<Option<usize>, RenderError> {
        // Not at a tool response: the next turn may re-render the tool call before it.
        if !self.engine().needs_checkpoints() || !self.messages.last().is_some_and(Message::is_user)
        {
            return Ok(None);
        }
        let history = self.render_as_chunks(&self.messages, false)?;
        // Tokens can merge across the generation prompt, so take the shared prefix, instead of history.n_tokens() - 1.
        Ok(Some(find_chunks_prefix_difference(&history, chunks)))
    }

    /// Drop the fewest whole turns after the kept first ones for the render, plus
    /// `reserved` tokens, to fit the target. System messages are always kept.
    fn context_shift(&mut self, reserved: usize) -> Result<(), ShiftError> {
        info!("Context shift happens!");
        let Some(shift) = self.shift else {
            return Err(ShiftError::Disabled);
        };
        let target_token_size = shift.target_tokens.saturating_sub(reserved);

        let turn_starts = user_message_indices(&self.messages);
        match turn_starts.len() {
            0 => return Err(ShiftError::NoUserMessages),
            1 => return Err(ShiftError::TooFewMessages),
            _ => {}
        }

        let first = shift.keep_first_turns;
        let deletable = turn_starts
            .len()
            .saturating_sub(first + shift.keep_last_turns.get());
        // keep_last_turns is nonzero, so `first + k` always indexes a kept turn.
        let without = |k: usize| {
            let mut messages = self.messages.clone();
            if k > 0 {
                messages.forget(turn_starts[first]..turn_starts[first + k]);
            }
            messages
        };
        let measure_ntokens_without = |k: usize| -> Result<usize, ShiftError> {
            Ok(self.render_as_chunks(&without(k), true)?.n_tokens())
        };

        if measure_ntokens_without(0)? <= target_token_size {
            // We are already under the target, so no shift is needed.
            return Ok(());
        }

        let n_tokens = measure_ntokens_without(deletable)?;
        if n_tokens > target_token_size {
            // The always-kept messages alone are over target.
            // Delete all deletable messages, leaving only the always-kept ones.
            warn!(
                n_tokens,
                target_token_size,
                system_messages = self.messages.iter().filter(|m| m.is_system()).count(),
                "Context shift could not reach its target: nothing left to delete."
            );
            self.messages = without(deletable);
            return Ok(());
        }

        // Smallest k that fits: measure_ntokens_without(lo - 1) is over target,
        // measure_ntokens_without(hi) fits.
        let (mut lo, mut hi) = (1, deletable);
        while lo < hi {
            // Binary search!
            let mid = lo + (hi - lo) / 2;
            if measure_ntokens_without(mid)? <= target_token_size {
                hi = mid;
            } else {
                lo = mid + 1;
            }
        }

        self.messages = without(hi);
        Ok(())
    }

    pub fn ask(&mut self, prompt: impl Promptable) -> Result<TurnGenerator, SayError> {
        // reset the stop flag
        self.should_stop.store(false, Ordering::Relaxed);

        // A prompt is message content, so it becomes the user turn as-is —
        // interleaving and all. Flattening to `<__media__>` markers happens at
        // render time.
        let mut content = prompt.to_prompt();
        self.register_media(&mut content)?;
        self.add_user_message(content);

        Ok(TurnGenerator::new(self)?)
    }

    /// Load each media part's file, register its bitmap and write the bitmap id
    /// back into the part.
    ///
    /// A part already pointing at a registered bitmap is left alone: a bitmap id
    /// is a hash of the bitmap's contents, so an id this worker knows can only
    /// mean that bitmap. Re-handing a conversation to `complete` therefore costs
    /// nothing per image already in the context, rather than a decode per turn.
    ///
    /// The nth media part pairs with the nth `<__media__>` marker in the
    /// flattened text, which holds by construction: both come from the same
    /// list of parts, in the same order.
    fn register_media(&mut self, content: &mut MessageContent) -> Result<(), MultimodalError> {
        let bitmaps = content
            .media_parts()
            .into_iter()
            .map(|part| {
                if part.id().is_some_and(|id| self.media.contains(id)) {
                    return Ok(None);
                }
                match part {
                    ContentPart::Image { path, .. } => self.engine().load_image(path).map(Some),
                    ContentPart::Audio { path, .. } => self.engine().load_audio(path).map(Some),
                    ContentPart::Text { .. } => unreachable!("media_parts filters out text"),
                }
            })
            .collect::<Result<Vec<Option<MtmdBitmap>>, MultimodalError>>()?;

        debug!("Detected bitmaps: {:?}", bitmaps);

        // Keep the position of each loaded bitmap, so its new id lands on the
        // part it came from and the parts we skipped keep the id they had.
        let (positions, loaded): (Vec<usize>, Vec<MtmdBitmap>) = bitmaps
            .into_iter()
            .enumerate()
            .filter_map(|(position, bitmap)| bitmap.map(|bitmap| (position, bitmap)))
            .unzip();

        let bitmap_ids = self.media.register(loaded)?;
        let mut parts = content.media_parts_mut();
        for (position, id) in positions.into_iter().zip(bitmap_ids) {
            parts[position].set_id(id);
        }
        Ok(())
    }

    /// Runs `call` and records its result in the history.
    fn run_tool(&mut self, call: ToolCall) {
        // find the tool
        // this is just a stupid linear search
        // but I think it's probably faster than something fancy as long as we have few tools
        // /shrug I'm happy to be wrong
        let Some(tool) = self.tools.iter().find(|t| t.name == call.name) else {
            // in case the tool isn't found.
            // I *think* this should be impossible, as long as the tool calling grammar
            // works.
            error!(
                tool_name = call.name,
                "Model triggered tool call for invalid tool name:",
            );
            let errmsg = format!("ERROR - Invalid tool name: {}", call.name);
            self.add_tool_resp(call.name, errmsg);
            return;
        };

        // call the tool
        let response = (tool.function)(call.arguments);
        debug!(%call.name, %response, "Tool call result:");

        self.add_tool_resp(call.name, response);
    }

    /// Answer a full message list, which replaces the chat history.
    ///
    /// A leading system message sets the system prompt; without one the current
    /// system prompt is kept. `options` follows the same rule for the settings
    /// it carries. The rest of `messages` becomes the history — including any
    /// later system message, which the chat template renders in place — and the
    /// turn's output is appended as usual, so a following [`ask`](Self::ask)
    /// continues that conversation.
    pub fn complete(
        &mut self,
        mut messages: History,
        options: Options,
    ) -> Result<TurnGenerator, CompleteError> {
        self.should_stop.store(false, Ordering::Relaxed);
        let hoisted = messages.take_system_prompt();
        self.reload_media(&mut messages)?;
        self.apply_options(options)
            .map_err(|error| CompleteError::Options(error.to_string()))?;

        if let Some(system_prompt) = hoisted {
            self.system_prompt = Some(system_prompt);
        }
        self.messages = messages;

        Ok(TurnGenerator::new(self)?)
    }

    /// Re-read the media files referenced by `messages` and relink the parts to
    /// the freshly registered bitmaps, since bitmap ids mean nothing outside the
    /// worker that issued them.
    ///
    /// Media already registered here is not re-read, so a file that changed on
    /// disk goes unnoticed — see [`register_media`](Self::register_media).
    fn reload_media(&mut self, messages: &mut History) -> Result<(), MultimodalError> {
        for content in messages.non_system_content_mut() {
            self.register_media(content)?;
        }
        Ok(())
    }

    /// Ends in the generation prompt if `allow_generation_prompt` and the model is to answer next.
    fn render_as_chunks(
        &self,
        messages: &History,
        allow_generation_prompt: bool,
    ) -> Result<TokenizerChunks, RenderError> {
        self.chunks_of(messages, self.render(messages, allow_generation_prompt)?)
    }

    /// `render` of `messages` as tokens, with the media it refers to.
    fn chunks_of(
        &self,
        messages: &History,
        render: String,
    ) -> Result<TokenizerChunks, RenderError> {
        let messages = &messages.with_system_prompt(self.system_prompt.as_deref());
        let bitmaps: Vec<&MtmdBitmap> = messages
            .iter()
            .flat_map(|msg| msg.media_ids())
            .filter_map(|id| self.media.get(id))
            .collect();
        Ok(self.engine().tokenize(render, bitmaps)?)
    }

    /// `messages` through the chat template, as `render_as_chunks` explains.
    fn render(
        &self,
        messages: &History,
        allow_generation_prompt: bool,
    ) -> Result<String, RenderError> {
        // Callers pass the conversation they want rendered — which may be a
        // shortened one, during a context shift. The system prompt is not part
        // of that, so it is added here.
        let messages = &messages.with_system_prompt(self.system_prompt.as_deref());
        let template_context = ChatTemplateContext::new(
            self.template_variables.clone(),
            (!self.tools.is_empty()).then(|| self.tools.clone()),
        );
        self.chat_template
            .render(messages, &template_context, allow_generation_prompt)
    }

    /// Reset the chat conversation with a new system prompt and tools.
    ///
    /// This is a potentially blocking operation, so you should use this from
    /// a thread pool or similar.
    pub fn reset_chat(
        &mut self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
    ) -> Result<(), SetterError> {
        // Run fallible functions before committing to state.
        let tool_sampler = build_tool_sampler(
            &self.context.backing_cart().language_model,
            &mut self.grammar_factory,
            &tools,
            &self.sampler_config,
            self.model_output.resolved_format(),
        )?;

        self.engine_mut(|e| e.reset_context())?;
        self.sampler_mut(|s| s.set_tool(tool_sampler));
        self.tools = tools;
        self.messages = History::default();
        self.system_prompt = system_prompt;
        self.media = MediaStore::new();
        Ok(())
    }

    /// Set a single template variable.
    pub fn set_template_variable(&mut self, name: String, value: bool) {
        self.template_variables.insert(name, value);
    }

    /// Set all template variables, replacing any existing ones.
    pub fn set_template_variables(&mut self, variables: std::collections::HashMap<String, bool>) {
        self.template_variables = variables;
    }

    /// DEPRECATED: Use set_template_variable("enable_thinking", value) instead.
    #[deprecated(note = "Use set_template_variable(\"enable_thinking\", value) instead")]
    pub fn set_allow_thinking(&mut self, allow_thinking: bool) {
        self.set_template_variable("enable_thinking".to_string(), allow_thinking);
    }

    /// Get all template variables.
    pub fn get_template_variables(&self) -> std::collections::HashMap<String, bool> {
        self.template_variables.clone()
    }

    /// Update the sampler configuration for inference.
    pub fn set_sampler_config(&mut self, sampler_config: SamplerConfig) -> Result<(), SetterError> {
        self.apply_sampler_and_tools(Some(sampler_config), None)
    }

    /// Get the sampler config
    pub fn get_sampler_config(&self) -> SamplerConfig {
        self.sampler_config.clone()
    }

    /// Get context usage statistics.
    pub fn get_stats(&self) -> ChatStats {
        ChatStats {
            context_size: self.engine().ctx.n_ctx(),
            context_used: self.engine().actual_context_size() as u32,
        }
    }

    /// MTP draft acceptance rate for the most recent generation, in `[0.0, 1.0]`.
    ///
    /// The counters reset at the start of each generation.
    /// Returns `None` when no drafts were proposed in the last generation.
    pub fn mtp_acceptance_rate(&self) -> Option<f32> {
        self.engine().mtp_acceptance_rate()
    }

    /// Apply a turn's [`Options`]. The sampler and tools are applied together, so
    /// setting both builds the tool grammar once, and a failure in either leaves
    /// both alone. Template variables cannot fail.
    fn apply_options(&mut self, options: Options) -> Result<(), SetterError> {
        self.apply_sampler_and_tools(options.sampler, options.tools)?;
        if let Some(variables) = options.template_variables {
            self.set_template_variables(variables);
        }
        Ok(())
    }

    /// Apply a new sampler config and/or tool set, building each sampler once.
    ///
    /// The tool sampler chain embeds the config's shift steps, so it has to be
    /// rebuilt when either changes — but only once, even when both do. All-or-
    /// nothing: nothing is committed unless every build succeeds.
    fn apply_sampler_and_tools(
        &mut self,
        sampler_config: Option<SamplerConfig>,
        tools: Option<Vec<Tool>>,
    ) -> Result<(), SetterError> {
        if sampler_config.is_none() && tools.is_none() {
            return Ok(());
        }

        let model = &self.context.backing_cart().language_model;
        let config = sampler_config.as_ref().unwrap_or(&self.sampler_config);
        let effective_tools = tools.as_deref().unwrap_or(&self.tools);

        // Run fallible functions before committing to state.
        let base_sampler = match &sampler_config {
            Some(config) => Some(config.build_sampler(model)?),
            None => None,
        };
        let tool_sampler = build_tool_sampler(
            model,
            &mut self.grammar_factory,
            effective_tools,
            config,
            self.model_output.resolved_format(),
        )?;
        // The template depends only on whether there are any tools, so it is
        // re-selected only when that flips — never on a sampler change, which
        // would alter the render and force a re-prefill.
        let chat_template = match &tools {
            Some(tools) if tools.is_empty() != self.tools.is_empty() => {
                Some(select_template(model, !tools.is_empty())?)
            }
            _ => None,
        };

        if let Some(config) = sampler_config {
            self.sampler_config = config;
        }
        if let Some(base_sampler) = base_sampler {
            self.sampler_mut(|s| s.set_base(base_sampler));
        }
        self.sampler_mut(|s| s.set_tool(tool_sampler));
        if let Some(tools) = tools {
            self.tools = tools;
        }
        if let Some(chat_template) = chat_template {
            self.chat_template = chat_template;
        }
        Ok(())
    }

    /// Set how old turns are forgotten when the context is full; `None` disables it.
    pub fn set_context_shift(
        &mut self,
        options: Option<ContextShiftOptions>,
    ) -> Result<(), SetterError> {
        self.shift = options
            .map(|options| options.parse(self.engine().ctx.n_ctx()))
            .transpose()?;
        Ok(())
    }

    /// Update the system prompt without resetting chat history.
    ///
    /// Replaces the prompt while preserving the conversation history. The model
    /// context is re-synchronized after the change, reusing the KV cache where
    /// possible.
    ///
    /// # Arguments
    ///
    /// * `system_prompt` - New system message to guide the model's behavior
    ///
    /// # Errors
    ///
    /// Returns `SetterError` if the system prompt cannot be changed or if context
    /// synchronization fails.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use nobodywho::chat::ChatBuilder;
    /// # use nobodywho::llm::get_model;
    /// # use std::sync::Arc;
    /// # let model = Arc::new(get_model("model.gguf", true, None, None, None).unwrap());
    /// # let chat = ChatBuilder::new(model).build().unwrap();
    /// # let mut chat = chat.blocking_lock();
    /// chat.set_system_prompt(Some("You are a helpful coding assistant.".to_string()));
    /// ```
    pub fn set_system_prompt(&mut self, system_prompt: Option<String>) {
        self.system_prompt = system_prompt;
    }

    /// Get the system prompt.
    pub fn get_system_prompt(&self) -> Option<String> {
        self.system_prompt.clone()
    }

    /// Update the available tools for the model to use.
    pub fn set_tools(&mut self, tools: Vec<Tool>) -> Result<(), SetterError> {
        self.apply_sampler_and_tools(None, Some(tools))
    }

    /// Set the chat history (lower-level API).
    ///
    /// A leading system message becomes the chat's system prompt. A later one
    /// stays in the history, for the chat template to render in place.
    ///
    /// Media is re-registered from the part paths, since bitmap ids issued by
    /// another worker mean nothing here. Nothing is committed unless that
    /// succeeds.
    pub fn set_chat_history(&mut self, messages: Vec<Message>) -> Result<(), SetterError> {
        let mut messages = History::new(messages)?;

        let hoisted = messages.take_system_prompt();
        self.reload_media(&mut messages)?;

        if let Some(system_prompt) = hoisted {
            self.system_prompt = Some(system_prompt);
        }
        self.messages = messages;

        // We used to call sync_context_with_render here but this can
        // crash as some chat templates will attempt to access fields on
        // messages[0], which will result in an error. So now we never
        // sync with an empty render and we only render when there are
        // messages present in the history.

        self.media.retain_referenced(&self.messages);

        Ok(())
    }

    /// Reset the chat conversation history.
    pub fn reset_history(&mut self) -> Result<(), SetterError> {
        self.set_chat_history(Vec::new())
    }

    /// Get the chat history (lower-level API).
    ///
    /// The system prompt is a separate setting, so it is never part of this list
    /// — see [`get_system_prompt`](Self::get_system_prompt). A system message
    /// placed mid-conversation is part of the history, and does show up here.
    pub fn get_chat_history(&self) -> Vec<Message> {
        self.messages.to_vec()
    }

    /// Tokenize a prompt and return token IDs. Text tokens are `Some(id)`, media embedding
    /// slots are `None` (one per slot consumed in the context window).
    pub fn tokenize(&self, prompt: impl Promptable) -> Result<Vec<Option<i32>>, TokenizeError> {
        let prompt = prompt.to_prompt();
        let bitmaps = prompt
            .media_parts()
            .into_iter()
            .map(|part| match part {
                ContentPart::Image { path, .. } => self.engine().load_image(path),
                ContentPart::Audio { path, .. } => self.engine().load_audio(path),
                ContentPart::Text { .. } => unreachable!("media_parts filters out text"),
            })
            .collect::<Result<Vec<MtmdBitmap>, MultimodalError>>()?;

        let bitmap_refs: Vec<&MtmdBitmap> = bitmaps.iter().collect();
        let chunks = self.engine().tokenize(prompt.to_string(), bitmap_refs)?;
        Ok(chunks.to_token_ids())
    }
}

/// One generation: brings the context up to date with the history, then
/// streams the model's response until the model ends it, `max_tokens`
/// runs out or the chat is stopped. The tool calls themselves aren't
/// streamed.
///
/// This is effectively an iterator over parsed tokens.
#[derive(Debug)]
struct ResponseGenerator {
    /// The tokens that was produced in this generation round.
    produced_tokens: Vec<LlamaToken>,
    /// The number of tokens in the input prompt.
    prompt_tokens: usize,
    state: ResponseGeneratorState,
}

#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
enum ResponseGeneratorState {
    Generating(ResponseParser),
    Finished(GeneratedResponse),
    Invalid,
}

#[derive(Debug)]
struct GeneratedResponse {
    /// What the history keeps of the generation.
    content: String,
    tool_calls: Vec<ToolCall>,
    prompt_tokens: usize,
    completion_tokens: usize,
    hit_token_limit: bool,
}

impl ResponseGenerator {
    fn new(chat: &mut Chat) -> Result<Self, ContextSyncError> {
        let prompt = chat.sync_context_with_render()?;
        let prompt_tokens = chat.engine_mut(|e| e.kv_mirror().n_tokens());

        chat.engine_mut(|e| e.reset_mtp_stats());
        chat.sampler_mut(|s| s.reset());

        let parser = ResponseParser::new(chat.model_output.clone(), chat.tools.clone(), &prompt);

        Ok(Self {
            produced_tokens: Vec::new(),
            prompt_tokens,
            state: ResponseGeneratorState::Generating(parser),
        })
    }

    /// Continue generating the response.
    ///
    /// This returns a single [`ControlFlow::Continue`] per sampled and
    /// decoded token, and will end with a [`ControlFlow::Break`] once no more
    /// tokens should be generated.
    ///
    /// This means that work in between calls to this will leave you with an
    /// in-progress decoding, which is important for performance.
    ///
    /// Invariants:
    /// - Must be passed the same `Chat` as it was constructed with.
    /// - The chat's state shouldn't have been modified in the meantime.
    /// - After `ControlFlow::Break` has been returned, don't call this again.
    fn next(
        &mut self,
        chat: &mut Chat,
        max_tokens: Option<usize>,
    ) -> Result<ControlFlow<GeneratedResponse, Vec<String>>, GenerateResponseError> {
        match mem::replace(&mut self.state, ResponseGeneratorState::Invalid) {
            ResponseGeneratorState::Generating(mut parser) => {
                let hit_token_limit =
                    max_tokens.is_some_and(|max_tokens| max_tokens <= self.produced_tokens.len());

                if chat.should_stop.load(Ordering::Relaxed) || hit_token_limit || parser.ended() {
                    // FIXME: Should we include EOG tokens in the statistics?
                    let eog_tokens = if parser.ended() { 1 } else { 0 };
                    let completion_tokens = self.produced_tokens.len() - eog_tokens;

                    // The parser is still in progress here, so finish it and
                    // transition to the final state.
                    let (text, generation) = parser.finish();

                    let response = GeneratedResponse {
                        content: generation.written,
                        tool_calls: generation.calls,
                        prompt_tokens: self.prompt_tokens,
                        completion_tokens,
                        hit_token_limit,
                    };

                    // We still need to return the final `text` generated
                    // above and from `.finish`, so we defer returning
                    // `ControlFlow::Break(response)` to the next iteration.
                    self.state = ResponseGeneratorState::Finished(response);
                    return Ok(ControlFlow::Continue(text));
                }

                // Do a context shift if the context is full.
                if chat.engine().is_context_full() {
                    // Leave room for the partial response, which is read back
                    // in below.
                    chat.context_shift(self.produced_tokens.len())?;
                    chat.sync_context_with_render()?;
                    if !self.produced_tokens.is_empty() {
                        let mut generated_chunks = TokenizerChunks::new();
                        generated_chunks
                            .append(TokenizerChunk::new_text(self.produced_tokens.clone()));
                        chat.engine_mut(|e| e.read_chunks(generated_chunks))?;
                    }
                }

                let new_token = chat
                    .context
                    .with_mut_return(|ctx| ctx.engine.next_token(&mut ctx.sampler))?;
                assert_ne!(new_token.0, -1, "invalid token generated");

                self.produced_tokens.push(new_token);

                let bytes = chat
                    .context
                    .backing_cart()
                    .language_model
                    .vocab()
                    .token_to_piece(new_token, true, None);
                let was_in_tool_calls = parser.in_tool_calls();
                let text = parser.push(new_token, &bytes);
                if !was_in_tool_calls && parser.in_tool_calls() {
                    chat.sampler_mut(move |s| s.activate_tool_grammar(new_token));
                }

                // Put back the state that we consumed in `mem::replace`
                // above. This is needed becase `parser.finish` consumes
                // the parser.
                self.state = ResponseGeneratorState::Generating(parser);
                Ok(ControlFlow::Continue(text))
            }
            // We don't do a state-transition back here, so the state will end
            // up as `Invalid`, which is fine because the user shouldn't call
            // `.next` again after this.
            ResponseGeneratorState::Finished(response) => Ok(ControlFlow::Break(response)),
            ResponseGeneratorState::Invalid => unreachable!("invalid state: {self:#?}"),
        }
    }
}

/// Answers the history, streaming the response to `on_chunk` and ending
/// with the whole turn. The tools the model calls run until it stops
/// calling them, unless `execute_tools` is false, in which case its first
/// calls end the turn for the caller to run.
#[derive(Debug)]
pub struct TurnGenerator {
    generator: ResponseGenerator,
    usage: CompletionUsage,
}

impl TurnGenerator {
    fn new(chat: &mut Chat) -> Result<Self, ContextSyncError> {
        let generator = ResponseGenerator::new(chat)?;

        Ok(TurnGenerator {
            generator,
            usage: CompletionUsage {
                prompt_tokens: 0,
                completion_tokens: 0,
            },
        })
    }

    /// Continue generating the turn.
    ///
    /// Has similar semantics and invariants as [`ResponseGenerator::next`].
    fn next(
        &mut self,
        chat: &mut Chat,
        max_tokens: Option<usize>,
        execute_tools: bool,
    ) -> Result<ControlFlow<CompletionResponse, Vec<String>>, GenerateResponseError> {
        let remaining =
            max_tokens.map(|max_tokens| max_tokens.saturating_sub(self.usage.completion_tokens));

        // Forward from the current generator if possible.
        let response = match self.generator.next(chat, remaining)? {
            ControlFlow::Continue(text) => return Ok(ControlFlow::Continue(text)),
            ControlFlow::Break(response) => response,
        };

        self.usage.prompt_tokens += response.prompt_tokens;
        self.usage.completion_tokens += response.completion_tokens;

        if response.tool_calls.is_empty() {
            chat.add_assistant_message(response.content.clone());

            return Ok(ControlFlow::Break(CompletionResponse {
                content: response.content,
                tool_calls: Vec::new(),
                finish_reason: if response.hit_token_limit {
                    FinishReason::Length
                } else {
                    FinishReason::Stop
                },
                usage: self.usage,
            }));
        }

        let tool_calls = response.tool_calls;
        debug!(?tool_calls, "Got tool calls:");

        // Whatever the model said before the call is part of the turn,
        // so it goes into the history message alongside the calls.
        chat.add_tool_calls(response.content.clone(), tool_calls.clone());

        if !execute_tools {
            return Ok(ControlFlow::Break(CompletionResponse {
                content: response.content,
                tool_calls,
                finish_reason: FinishReason::ToolCalls,
                usage: self.usage,
            }));
        }

        for call in tool_calls {
            chat.run_tool(call);
        }

        self.generator = ResponseGenerator::new(chat)?;

        // Recurse with the new generator.
        //
        // This should not be able to stack overflow, if the generator returns
        // `ControlFlow::Break` immediately, then we'd hit the
        // `response.tool_calls.is_empty()` branch above and return (assuming
        //  that the model cannot generate a tool call in a single token).
        self.next(chat, max_tokens, execute_tools)
    }

    /// Run generation in a loop, and return the final response once it's done.
    #[cfg(test)]
    fn response(
        mut self,
        chat: &mut Chat,
        max_tokens: Option<usize>,
        execute_tools: bool,
    ) -> Result<CompletionResponse, GenerateResponseError> {
        loop {
            match self.next(chat, max_tokens, execute_tools)? {
                ControlFlow::Continue(_) => continue,
                ControlFlow::Break(response) => return Ok(response),
            }
        }
    }
}

#[derive(Debug)]
pub struct CompletionWorker {
    msg_rx: Option<tokio::sync::mpsc::UnboundedReceiver<Result<CompletionChunk, CompletionError>>>,
    join_handle: Option<std::thread::JoinHandle<()>>,
    should_stop: Arc<AtomicBool>,
}

impl CompletionWorker {
    /// Spawn a new worker thread and generate tokens in a loop until done.
    ///
    /// This uses an unbounded channel such that the worker will eventually
    /// stop (otherwise you could easily get deadlocks if you tried to access
    /// stuff on `Chat` while the `CompletionWorker` still exists. This is
    /// especially bad in GCed languages, where you don't really know when
    /// `CompletionWorker` will be free'd).
    ///
    /// FIXME(madsmtm): Remove this, and instead run one step of completion at
    /// a time (but somehow without deadlocks and inconsistent results).
    fn new<F>(should_stop: Arc<AtomicBool>, worker: F) -> Self
    where
        F: FnOnce(tokio::sync::mpsc::UnboundedSender<Result<CompletionChunk, CompletionError>>)
            + Send
            + 'static,
    {
        let (msg_tx, msg_rx) = tokio::sync::mpsc::unbounded_channel();
        let join_handle = std::thread::spawn(|| worker(msg_tx));

        Self {
            msg_rx: Some(msg_rx),
            join_handle: Some(join_handle),
            should_stop,
        }
    }

    fn msg_rx(
        &mut self,
    ) -> &mut tokio::sync::mpsc::UnboundedReceiver<Result<CompletionChunk, CompletionError>> {
        self.msg_rx.as_mut().expect("called before `Drop`")
    }
}

impl Drop for CompletionWorker {
    fn drop(&mut self) {
        // Signal the worker thread to stop.
        self.should_stop.store(true, Ordering::Relaxed);
        drop(self.msg_rx.take());

        // And wait for it to actually stop.
        if let Some(handle) = self.join_handle.take() {
            if let Err(e) = handle.join() {
                std::panic::resume_unwind(e);
            }
        }
    }
}

#[derive(Debug)]
pub struct CompletionStream {
    worker: CompletionWorker,
}

impl CompletionStream {
    fn new(worker: CompletionWorker) -> Self {
        Self { worker }
    }

    pub fn blocking_next(&mut self) -> Result<Option<CompletionChunk>, CompletionError> {
        match self.worker.msg_rx().blocking_recv() {
            Some(Ok(chunk)) => Ok(Some(chunk)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    pub async fn next(&mut self) -> Result<Option<CompletionChunk>, CompletionError> {
        match self.worker.msg_rx().recv().await {
            Some(Ok(chunk)) => Ok(Some(chunk)),
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }
}

/// Token stream.
///
/// Call [`next_token`](Self::next_token) to drive token-by-token, or
/// [`completed`](Self::completed) to collect the full text.
#[derive(Debug)]
pub struct TokenStream {
    worker: CompletionWorker,
    done: Option<String>,
}

impl TokenStream {
    fn new(worker: CompletionWorker) -> Self {
        Self { worker, done: None }
    }

    /// Return the next token piece, or `None` when generation is finished.
    pub fn blocking_next_token(&mut self) -> Result<Option<String>, CompletionError> {
        match self.worker.msg_rx().blocking_recv() {
            Some(Ok(CompletionChunk::Token(text))) => Ok(Some(text)),
            Some(Ok(CompletionChunk::Done(response))) => {
                self.done = Some(response.content);
                Ok(None)
            }
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Return the next token piece, or `None` when generation is finished.
    pub async fn next_token(&mut self) -> Result<Option<String>, CompletionError> {
        match self.worker.msg_rx().recv().await {
            Some(Ok(CompletionChunk::Token(text))) => Ok(Some(text)),
            Some(Ok(CompletionChunk::Done(response))) => {
                self.done = Some(response.content);
                Ok(None)
            }
            Some(Err(e)) => Err(e),
            None => Ok(None),
        }
    }

    /// Drain all tokens and return the full output text.
    pub fn blocking_completed(&mut self) -> Result<String, CompletionError> {
        loop {
            match self.blocking_next_token()? {
                Some(_) => continue,
                None => return Ok(self.done.clone().expect("done should be set here")),
            }
        }
    }

    /// Drain all tokens and return the full output text.
    pub async fn completed(&mut self) -> Result<String, CompletionError> {
        loop {
            match self.next_token().await? {
                Some(_) => continue,
                None => {
                    return Ok(self.done.clone().expect("done should be set here"));
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampler::SamplerPresets;
    use crate::test_utils;
    use crate::tokenizer::Prompt;

    #[test]
    fn send_sync() {
        fn assert_send<T: Send>() {}
        fn assert_sync<T: Sync>() {}

        assert_send::<Chat>();
        // `Chat` isn't `Sync` because it contains `LlamaContext<'_>`.
        // assert_sync::<Chat>();
        assert_send::<ChatHandle>();
        assert_sync::<ChatHandle>();
        assert_send::<ChatHandle>();
        assert_sync::<ChatHandle>();
        assert_send::<CompletionStream>();
        assert_sync::<CompletionStream>();
        assert_send::<CompletionStream>();
        assert_sync::<CompletionStream>();
        assert_send::<TokenStream>();
        assert_sync::<TokenStream>();
        assert_send::<TokenStream>();
        assert_sync::<TokenStream>();
    }

    /// The render the model answers from, generation prompt included.
    fn render(chat: &Chat, messages: &History) -> Result<TokenizerChunks, RenderError> {
        chat.render_as_chunks(messages, true)
    }

    fn image_system_message() -> Message {
        Message::new_system(vec![
            ContentPart::text("Describe like this:"),
            ContentPart::image("example.png"),
        ])
    }

    #[test]
    fn test_history_rejects_media_in_any_system_message() {
        assert!(matches!(
            History::new(vec![image_system_message(), user("Hi")]),
            Err(InvalidHistoryError::MediaInSystemMessage)
        ));

        // Not just the leading one, which is the one that becomes the plain-text
        // system prompt: no chat template renders media in that role either.
        assert!(matches!(
            History::new(vec![
                user("Hi"),
                assistant("Hello!"),
                image_system_message()
            ]),
            Err(InvalidHistoryError::MediaInSystemMessage)
        ));

        let mut history = History::new(vec![]).expect("an empty history is a fine history");
        assert!(matches!(
            history.push_system(vec![ContentPart::image("example.png")]),
            Err(InvalidHistoryError::MediaInSystemMessage)
        ));
        assert!(history.is_empty());
    }

    #[test]
    fn test_history_takes_only_a_leading_system_prompt() {
        let mut history = History::new(vec![
            Message::new_system("Be terse.".to_string()),
            user("Hi"),
            Message::new_system("Now answer in French.".to_string()),
        ])
        .unwrap();

        assert_eq!(history.take_system_prompt().as_deref(), Some("Be terse."));
        assert_eq!(history.len(), 2);
        // The remaining one is not leading, so it stays a turn of its own.
        assert_eq!(history.take_system_prompt(), None);

        let for_render = history.with_system_prompt(Some("Be terse."));
        assert!(for_render[0].is_system());
        assert_eq!(for_render.len(), 3);
    }

    #[test]
    fn test_history_forget_keeps_system_messages() {
        let mut history = History::new(vec![
            user("first"),
            assistant("first"),
            user("second"),
            Message::new_system("Answer in French.".to_string()),
            assistant("second"),
            user("third"),
        ])
        .unwrap();

        history.forget(2..5);

        assert_eq!(
            serde_json::to_value(&history).unwrap(),
            serde_json::to_value([
                user("first"),
                assistant("first"),
                Message::new_system("Answer in French.".to_string()),
                user("third"),
            ])
            .unwrap()
        );
    }

    // Helper function to verify message structure is valid
    fn assert_valid_message_structure(messages: &[Message]) {
        for i in 1..messages.len() {
            let prev_msg = &messages[i - 1];
            let curr_msg = &messages[i];

            // User should be followed by assistant
            if prev_msg.is_user() {
                assert!(
                    curr_msg.is_assistant(),
                    "User message should be followed by assistant"
                );
            }

            // Assistant: check if it's tool calls or plain assistant message
            if prev_msg.is_assistant() {
                if prev_msg.has_tool_calls() {
                    assert!(
                        curr_msg.is_tool(),
                        "Tool calls should be followed by tool response"
                    );
                } else {
                    assert!(
                        curr_msg.is_user(),
                        "Assistant message should be followed by user"
                    );
                }
            }

            // Tool response should be followed by either another tool response or assistant
            if prev_msg.is_tool() {
                assert!(
                    curr_msg.is_tool() || curr_msg.is_assistant(),
                    "Tool response should be followed by another tool response or assistant"
                );
            }
        }
    }

    #[test]
    fn test_chat_worker() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();

        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 1024,
                ..Default::default()
            },
        )?;

        let resp = worker
            .ask("What is the capital of Denmark?")?
            .response(&mut worker, None, true)?
            .content;
        println!("{}", resp);
        assert!(resp.contains("Copenhagen"));

        let resp = worker
            .ask("What language do they speak there?")?
            .response(&mut worker, None, true)?
            .content;
        println!("{}", resp);
        assert!(resp.contains("Danish"));

        Ok(())
    }

    /// Smoke test: load Gemma-4 base + MTP draft heads with `mtp=true`
    /// and verify a factual generation succeeds end-to-end. Skipped
    /// unless both `TEST_MTP_TARGET_MODEL` and `TEST_MTP_DRAFT_MODEL`
    /// env vars are set to existing files.
    #[test]
    fn test_mtp_gemma4_smoke() -> Result<(), Box<dyn std::error::Error>> {
        let Some(model) = test_utils::load_mtp_models() else {
            eprintln!(
                "skipping test_mtp_gemma4_smoke: \
                 set TEST_MTP_TARGET_MODEL and TEST_MTP_DRAFT_MODEL to enable"
            );
            return Ok(());
        };

        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 1024,
                mtp: Some(MtpConfig::default()),
                ..Default::default()
            },
        )?;

        let resp = worker
            .ask("What is the capital of Denmark?")?
            .response(&mut worker, None, true)?
            .content;
        println!("MTP response: {}", resp);
        assert!(resp.contains("Copenhagen"));

        let resp = worker
            .ask("Are you sure?")?
            .response(&mut worker, None, true)?
            .content;
        println!("MTP response: {}", resp);
        assert!(resp.contains("Yes"));

        Ok(())
    }

    #[test]
    fn test_reset_chat() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                system_prompt: Some("You're a dog. End all responses with 'woof'".into()),
                ..ChatConfig::default()
            },
        )?;

        // do it once
        let resp1 = worker
            .ask("What is the capital of Denmark?")?
            .response(&mut worker, None, true)?
            .content;
        println!("{}", resp1);
        assert!(resp1.to_lowercase().contains("woof"));

        // reset
        let _ = worker.reset_chat(
            Some("You're a cat. End all responses with 'meow'".into()),
            vec![],
        );

        // do it again
        let resp2 = worker
            .ask("What is the capital of Denmark?")?
            .response(&mut worker, None, true)?
            .content;
        println!("{}", resp2);
        assert!(resp2.to_lowercase().contains("meow"));

        Ok(())
    }

    #[test]
    fn test_stop_mid_write() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                system_prompt: Some("You are a counter, only outputting numbers".into()),
                n_ctx: 1024,
                ..ChatConfig::default()
            },
        )?;
        let should_stop = worker.should_stop.clone();

        // ensure that the generationworker resets the flag when creating a new response.
        should_stop.store(true, Ordering::Relaxed);

        let mut generator = worker.ask("Count from 0 to 9")?;

        let response = loop {
            match generator.next(&mut worker, None, true)? {
                ControlFlow::Continue(text) => {
                    for text in text {
                        if text.contains("5") {
                            should_stop.store(true, Ordering::Relaxed);
                        }
                    }
                }
                ControlFlow::Break(response) => break response.content,
            }
        };

        println!("{}", response);

        assert!(response.contains("5"));
        assert!(!response.contains("8"));
        Ok(())
    }

    fn test_tool() -> Tool {
        Tool {
            name: "get_current_temperature".into(),
            description: "Gets the temperature at a given location".into(),
            json_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "location": {
                        "type": "string",
                        "description": "The location to get the temperature for."
                    }
                },
                "required": [
                    "location"
                ]
            }),
            function: Arc::new(|args: serde_json::Value| {
                let Some(location) = args.get("location") else {
                    return "Bad arguments format. Location key was missing.".into();
                };

                if location.as_str() == Some("Copenhagen") {
                    return "13.37°C".into();
                }

                if location.as_str() == Some("Beijing") {
                    return "42.69°C".into();
                }

                "Unknown location.".into()
            }),
        }
    }

    fn dkk_exchange_rate() -> Tool {
        Tool {
            name: "dkk_exchange_rate".into(),
            description: "Gets the exchange rate for DKK to a given currency.".into(),
            json_schema: serde_json::json!({
                "type": "object",
                "properties": {
                    "to-currency": {
                        "type": "string",
                        "description": "The currency to convert to in a three letter code. (eg. \"USD\")"
                    }
                },
                "required": [
                    "to-currency"
                ]
            }),
            function: Arc::new(|args: serde_json::Value| {
                let Some(to_currency) = args.get("to-currency") else {
                    return "Bad arguments format. To currency key was missing.".into();
                };

                if to_currency.as_str() == Some("USD") {
                    debug!("returning 1 DKK = 0.15 USD");
                    return "1 DKK = 0.15 USD".into();
                }

                "Exchange rate not available".into()
            }),
        }
    }

    /// Time three sequential tool-calling turns on the same worker to confirm the
    /// pre-built tool sampler amortizes the llguidance init cost at worker creation.
    /// The one-time llguidance build appears in `setup_ms`; all three turns run at
    /// similar speed. Turns 2 and 3 are slightly slower than turn 1 due to KV cache
    /// growth, not grammar overhead.
    #[test]
    #[ignore = "manual perf benchmark — run with `cargo test bench_pre_built_sampler_amortization -- --ignored --nocapture`"]
    fn bench_pre_built_sampler_amortization() {
        let model = test_utils::load_test_model();
        let setup_start = std::time::Instant::now();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                system_prompt: Some("You're a helpful assistant.".into()),
                n_ctx: 4096,
                tools: vec![test_tool()],
                ..Default::default()
            },
        )
        .expect("Failed making worker");
        let setup_ms = setup_start.elapsed().as_millis();
        eprintln!("[bench] worker setup: {setup_ms} ms");

        // Warmup: one discarded turn to put GPU pipeline in steady state.
        let _ = worker
            .ask("Hello.")
            .expect("warmup failed")
            .response(&mut worker, None, true)
            .expect("warmup failed");

        // Three distinct prompts that should each elicit a tool call.
        let prompts = [
            "What's the temperature in Copenhagen?",
            "Now check the temperature in Beijing.",
            "And one more: temperature in Copenhagen again, please.",
        ];

        for (i, prompt) in prompts.iter().enumerate() {
            let turn_start = std::time::Instant::now();
            let _ = worker
                .ask(*prompt)
                .expect("ask failed")
                .response(&mut worker, None, true)
                .expect("failed generating");
            eprintln!(
                "[bench] turn {} ({} chars): {} ms",
                i + 1,
                prompt.len(),
                turn_start.elapsed().as_millis()
            );
        }
    }

    /// Time the setters that rebuild the tool grammar. The held `GrammarFactory`
    /// means only the first build pays the llguidance init; the rest are just
    /// `create_parser`.
    #[test]
    #[ignore = "manual perf benchmark — run with `cargo test bench_tool_grammar_rebuild -- --ignored --nocapture`"]
    fn bench_tool_grammar_rebuild() {
        let model = test_utils::load_test_model();

        let setup_start = std::time::Instant::now();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 512,
                tools: vec![test_tool()],
                ..Default::default()
            },
        )
        .expect("Failed making worker");
        eprintln!(
            "[bench] worker setup: {} ms",
            setup_start.elapsed().as_millis()
        );

        for i in 0..3 {
            let start = std::time::Instant::now();
            worker
                .apply_sampler_and_tools(Some(SamplerPresets::greedy()), Some(vec![test_tool()]))
                .expect("applying settings");
            eprintln!(
                "[bench] sampler+tools {}: {} ms",
                i + 1,
                start.elapsed().as_millis()
            );
        }
    }

    /// Changing the sampler config rebuilds the tool sampler from the held
    /// `GrammarFactory`. A grammar compiled off a reused factory has to still
    /// drive a real tool call, so exercise one on each side of the change.
    #[test]
    fn tool_calling_survives_a_sampler_config_change() {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 4096,
                tools: vec![test_tool()],
                ..Default::default()
            },
        )
        .expect("Failed making worker");

        let before = worker
            .ask("What is the temperature in Copenhagen?")
            .unwrap()
            .response(&mut worker, None, true)
            .unwrap()
            .content;
        assert!(
            before.contains("13.37"),
            "the tool was not called before the config change: {before}"
        );

        // Rebuilds the tool sampler, reusing the factory built at construction.
        worker
            .set_sampler_config(SamplerPresets::dry())
            .expect("setting a sampler config");

        let after = worker
            .ask("And what is the temperature in Beijing?")
            .unwrap()
            .response(&mut worker, None, true)
            .unwrap()
            .content;
        assert!(
            after.contains("42.69"),
            "the tool was not called after the config change: {after}"
        );
    }

    #[test]
    fn test_tool_chat() {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                system_prompt: Some("You're a helpful assistant.".into()),
                n_ctx: 4096,
                tools: vec![test_tool()],
                ..Default::default()
            },
        )
        .expect("Failed making worker");

        let result = worker
            .ask("I would like to know the temperature in two cities: Copenhagen and Beijing.")
            .expect("fuck")
            .response(&mut worker, None, true)
            .expect("argh")
            .content;

        println!("{result}");
        assert!(result.contains("13.37"));
        assert!(result.contains("42.69"));

        // The history splits the response at the begin token: the preamble is
        // kept as content, the call block itself never is.
        let begin_token = worker
            .model_output
            .resolved_format()
            .expect("the test model has an output format")
            .format()
            .tool_calls
            .begin;
        for message in &worker.messages.into_vec() {
            if let Message::Assistant {
                content,
                tool_calls: Some(_),
            } = message
            {
                assert!(
                    !content.to_string().contains(begin_token),
                    "raw call block stored as content: {content}"
                );
            }
        }
    }

    /// A turn that runs out of tokens partway through an item still ends, as
    /// out of tokens, having generated exactly its budget.
    #[test]
    fn a_turn_out_of_tokens_finishes_with_length() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 1024,
                tools: vec![test_tool()],
                ..Default::default()
            },
        )?;
        let messages = History::new(vec![user("What's the temperature in Copenhagen?")])?;
        let response =
            worker
                .complete(messages, Options::new())?
                .response(&mut worker, Some(8), true)?;
        assert_eq!(response.finish_reason, FinishReason::Length);
        assert_eq!(response.usage.completion_tokens, 8);
        Ok(())
    }

    #[test]
    fn test_multi_tool_call() {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                tools: vec![test_tool(), dkk_exchange_rate()],
                ..Default::default()
            },
        )
        .expect("Failed making worker");

        let result = worker
            .ask("I would like to know the temperature in Copenhagen and the DKK to USD exchange rate.")
            .expect("dammit")
            .response(&mut worker, None, true)
            .unwrap()
            .content;

        println!("{result}");
        assert!(result.contains("13.37"));
        assert!(result.contains("0.15"));
    }

    /// A tool-calling turn keeps whatever the model wrote before the call, so
    /// the next turn sees its own reasoning instead of a bare call.
    #[test]
    fn tool_call_preamble_is_kept_in_history() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 1024,
                tools: vec![test_tool()],
                ..Default::default()
            },
        )
        .expect("Failed making worker");

        let preamble = "Let me look up the temperature in Copenhagen.";
        worker.add_user_message("What is the temperature in Copenhagen?");
        worker.add_tool_calls(
            preamble,
            vec![ToolCall {
                name: "get_current_temperature".into(),
                arguments: serde_json::json!({"location": "Copenhagen"}),
            }],
        );

        let Some(Message::Assistant {
            content,
            tool_calls: Some(_),
        }) = worker.messages.last()
        else {
            panic!("expected an assistant message with tool calls");
        };
        assert_eq!(content.to_string(), preamble);

        // Keeping it in the history is only worth anything if the template puts
        // it back into the prompt next to the call.
        let rendered = worker
            .chat_template
            .render(
                &worker
                    .messages
                    .with_system_prompt(worker.system_prompt.as_deref()),
                &ChatTemplateContext::new(
                    worker.template_variables.clone(),
                    Some(worker.tools.clone()),
                ),
                true,
            )
            .expect("rendering a history with a tool call");
        assert!(
            rendered.contains(preamble),
            "preamble missing from the rendered prompt: {rendered}"
        );
    }

    #[test]
    fn test_set_system_prompt() {
        let model = test_utils::load_test_model();

        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_system_prompt(Some("You are a dog. End all responses with woof."))
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        let dog_response = chat.ask("Hello!").blocking_completed().unwrap();

        assert!(dog_response.to_lowercase().contains("woof"));

        chat.blocking_lock()
            .set_system_prompt(Some("You are a cat. End all responses with meow.".into()));
        let cat_response = chat
            .ask("Now say the correct animal sound!")
            .blocking_completed()
            .unwrap();
        assert!(cat_response.to_lowercase().contains("meow"));
    }

    #[test]
    fn test_setters_on_empty_history_do_not_crash() {
        // Rendering the chat template with neither a system prompt nor any messages
        // would crash, so set_system_prompt(None) and set_tools(..) on an empty
        // history must not immediately sync the context — only the next ask() should.
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(512)
            .build()
            .expect("chat build failed in test");
        let mut chat = chat.blocking_lock();

        chat.set_system_prompt(None);
        assert_eq!(chat.get_system_prompt(), None);

        chat.set_tools(vec![]).unwrap();
        chat.set_tools(vec![test_tool()]).unwrap();

        assert!(chat.get_chat_history().is_empty());
    }

    /// Below keep_first_turns + keep_last_turns turns nothing is deletable, so the
    /// shift leaves the history alone however oversized it is.
    #[test]
    fn test_context_shift_below_deletable_threshold_is_a_noop(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();

        let defaults = ContextShiftOptions::default();
        for turn_count in [2, defaults.keep_first_turns + defaults.keep_last_turns] {
            let mut worker = Chat::new(
                Arc::clone(&model),
                ChatConfig {
                    n_ctx: 512,
                    ..Default::default()
                },
            )?;
            let target_size = (worker.engine().ctx.n_ctx() / 2) as usize;

            // An opening turn far too large for the context, then filler turns.
            worker.add_user_message("padding ".repeat(target_size));
            for turn in 1..turn_count {
                worker.add_assistant_message(format!("answer {turn}"));
                worker.add_user_message(format!("question {turn}"));
            }

            assert_eq!(user_message_indices(&worker.messages).len(), turn_count);
            assert!(render(&worker, &worker.messages)?.n_tokens() > target_size);

            let before = serde_json::to_value(&worker.messages)?;
            worker.context_shift(0)?;
            assert_eq!(
                serde_json::to_value(&worker.messages)?,
                before,
                "{turn_count} turns is below the deletable threshold, \
                 so the shift must leave the history alone"
            );
        }

        Ok(())
    }

    #[test]
    fn test_context_shift_error_cases() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 512,
                ..Default::default()
            },
        )?;

        assert!(matches!(
            worker.context_shift(0),
            Err(ShiftError::NoUserMessages)
        ));

        worker.add_user_message("only".to_string());
        assert!(matches!(
            worker.context_shift(0),
            Err(ShiftError::TooFewMessages)
        ));

        Ok(())
    }

    fn worker_with_turns(
        model: Arc<llm::Model>,
        context_shift: Option<ContextShiftOptions>,
        turns: usize,
    ) -> Result<Chat, Box<dyn std::error::Error>> {
        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 512,
                context_shift,
                ..Default::default()
            },
        )?;
        for i in 0..turns {
            worker.add_user_message(format!("Question {i}: tell me about the number {i}."));
            worker.add_assistant_message(format!(
                "The number {i} is a fine number, and there is plenty to say about it."
            ));
        }
        worker.add_user_message("Thanks!".to_string());
        Ok(worker)
    }

    /// The shift removes the fewest turns that fit, found by brute force here.
    #[test]
    fn test_context_shift_deletes_fewest_turns() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = worker_with_turns(model, Some(ContextShiftOptions::default()), 20)?;
        let target_size = (worker.engine().ctx.n_ctx() / 2) as usize;
        let original = worker.messages.clone();
        let turn_starts = user_message_indices(&original);

        let expected = (1..turn_starts.len() - 2)
            .find(|&k| {
                let mut messages = original.clone();
                messages.forget(turn_starts[1]..turn_starts[1 + k]);
                render(&worker, &messages).unwrap().n_tokens() <= target_size
            })
            .expect("some deletion should reach the target");

        worker.context_shift(0)?;
        let deleted = turn_starts.len() - user_message_indices(&worker.messages).len();
        assert_eq!(deleted, expected);
        assert!(render(&worker, &worker.messages)?.n_tokens() <= target_size);

        Ok(())
    }

    #[test]
    fn test_context_shift_custom_options() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let options = ContextShiftOptions {
            keep_first_turns: 2,
            keep_last_turns: 3,
            target: ShiftTarget::Tokens(200),
        };
        let mut worker = worker_with_turns(model, Some(options), 20)?;
        let before: Vec<String> = worker
            .messages
            .iter()
            .filter(|m| m.is_user())
            .map(|m| m.content().to_string())
            .collect();

        worker.context_shift(0)?;

        let after: Vec<String> = worker
            .messages
            .iter()
            .filter(|m| m.is_user())
            .map(|m| m.content().to_string())
            .collect();
        assert!(after.len() < before.len());
        assert_eq!(after[..2], before[..2]);
        assert_eq!(after[after.len() - 3..], before[before.len() - 3..]);
        assert!(render(&worker, &worker.messages)?.n_tokens() <= 200);
        assert_valid_message_structure(&worker.messages);

        Ok(())
    }

    /// Room reserved for a partial response comes off the target.
    #[test]
    fn test_context_shift_reserves_tokens() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = worker_with_turns(model, Some(ContextShiftOptions::default()), 20)?;
        let target_size = (worker.engine().ctx.n_ctx() / 2) as usize;

        worker.context_shift(100)?;
        assert!(render(&worker, &worker.messages)?.n_tokens() <= target_size - 100);

        Ok(())
    }

    #[test]
    fn test_context_shift_disabled() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = worker_with_turns(model, None, 20)?;
        assert!(render(&worker, &worker.messages)?.n_tokens() > 512);

        assert!(matches!(
            worker.sync_context_with_render(),
            Err(ContextSyncError::Shift(ShiftError::Disabled))
        ));

        Ok(())
    }

    #[test]
    fn test_set_context_shift() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = worker_with_turns(model, Some(ContextShiftOptions::default()), 20)?;

        let invalid = ContextShiftOptions {
            target: ShiftTarget::Tokens(512),
            ..Default::default()
        };
        assert!(matches!(
            worker.set_context_shift(Some(invalid)),
            Err(SetterError::InvalidContextShiftOptions(_))
        ));

        worker.set_context_shift(None)?;
        assert!(matches!(worker.context_shift(0), Err(ShiftError::Disabled)));

        worker.set_context_shift(Some(ContextShiftOptions {
            target: ShiftTarget::Tokens(200),
            ..Default::default()
        }))?;
        worker.context_shift(0)?;
        assert!(render(&worker, &worker.messages)?.n_tokens() <= 200);

        Ok(())
    }

    #[test]
    fn test_context_shift_options_are_validated() {
        let invalid = [
            ShiftTarget::Fraction(1.5),
            ShiftTarget::Fraction(0.0),
            ShiftTarget::Tokens(0),
            ShiftTarget::Tokens(512),
        ]
        .map(|target| ContextShiftOptions {
            target,
            ..Default::default()
        });
        let no_last_turn = ContextShiftOptions {
            keep_last_turns: 0,
            ..Default::default()
        };

        for options in invalid.into_iter().chain([no_last_turn]) {
            assert!(
                options.parse(512).is_err(),
                "{options:?} should be rejected"
            );
        }
        assert!(ContextShiftOptions::default().parse(512).is_ok());
    }

    /// `complete()` can hand over a history that does not start with a user
    /// message. That prefix, and the first turn, both survive the shift.
    #[test]
    fn test_context_shift_preserves_prefix_before_first_user_message(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 512,
                ..Default::default()
            },
        )?;
        let target_size = (worker.engine().ctx.n_ctx() / 2) as usize;

        worker.add_assistant_message("prefix".to_string());
        for (user, assistant) in [
            ("first".to_string(), "first".to_string()),
            ("padding ".repeat(target_size), "large".to_string()),
            ("keep".to_string(), "keep".to_string()),
            ("recent".to_string(), "recent".to_string()),
        ] {
            worker.add_user_message(user);
            worker.add_assistant_message(assistant);
        }
        worker.add_user_message("final".to_string());

        worker.context_shift(0)?;

        assert!(
            matches!(&worker.messages[0], Message::Assistant { content, .. }
                if content.to_string() == "prefix"),
            "the prefix before the first user message should survive: {:?}",
            worker.messages[0]
        );
        assert!(
            matches!(&worker.messages[1], Message::User { content, .. }
                if content.to_string() == "first"),
            "the first turn should survive: {:?}",
            worker.messages[1]
        );
        assert!(render(&worker, &worker.messages)?.n_tokens() <= target_size);

        Ok(())
    }

    #[test]
    fn test_context_shift_with_tool_calls() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();

        // Use a very small context size to force shifting
        let n_ctx = 1024;
        let n_messages = 10;
        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx,
                system_prompt: Some("You are a helpful assistant.".into()),
                tools: vec![test_tool()],
                ..Default::default()
            },
        )?;

        // Add exchanges with tool calls mixed in
        for i in 1..=n_messages {
            worker.add_user_message(format!("User message {}. What is {} * {}?", i, i, i));

            // Add a tool call every other message
            // Pattern: User -> Assistant (with tool call) -> Tool response -> Assistant
            if i % 2 == 0 {
                worker.add_tool_calls(
                    "",
                    vec![ToolCall {
                        name: "get_current_temperature".into(),
                        arguments: serde_json::json!({"location": "Copenhagen"}),
                    }],
                );
                worker.add_tool_resp("get_current_temperature".into(), "13.37°C".into());
                worker.add_assistant_message(format!(
                    "The temperature is 13.37°C and {} * {} = {}.",
                    i,
                    i,
                    i * i
                ));
            } else {
                worker.add_assistant_message(format!("The answer is {}.", i * i));
            }
        }

        worker.add_user_message("Final question!".to_string());

        worker.context_shift(0)?;

        assert!(render(&worker, &worker.messages)?.n_tokens() <= (n_ctx / 2) as usize);
        assert_valid_message_structure(&worker.messages);

        Ok(())
    }

    #[test]
    fn test_context_shift_on_say() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();

        let n_messages = 14;
        // n_messages is chosen by trial and error. This exactly fills up the
        // the context so much that the next user message cannot be read and a context shift happens.
        let mut worker = Chat::new(
            model,
            ChatConfig {
                system_prompt: Some("You are a helpful assistant.".into()),
                n_ctx: 512, // Use a small context size to force shifting
                ..Default::default()
            },
        )?;

        // Fill up the context until it's almost full
        for i in 1..=n_messages {
            worker.add_user_message(format!(
                "This is user message number {}. What is {} * {}?",
                i, i, i
            ));
            worker.add_assistant_message(format!("The answer is {}.", i * i));
        }

        let messages_before_shift = worker.messages.len();
        println!("Messages before shift: {}", messages_before_shift);

        // This should trigger context shift internally because there's not enough space
        let _response = worker
            .ask("This is a new question that will not fit in the context! What is 10 * 10?")?
            .response(&mut worker, None, true)?
            .content;

        let messages_after = worker.messages.clone();

        println!("Messages after operation: {}", messages_after.len());

        // Verify context shift occurred
        assert!(
            messages_after.len() < messages_before_shift,
            "Context shift should have reduced message count"
        );

        // Verify essential messages are preserved
        // 1. Should have first user message
        let first_user_idx = messages_after.iter().position(|m| m.is_user());
        assert!(
            first_user_idx.is_some(),
            "First user message should be preserved"
        );

        // 2. Verify the last user message is there (the one that triggered the shift)
        let last_user = messages_after.iter().rev().find(|m| m.is_user());

        if let Some(Message::User { content, .. }) = last_user {
            assert!(
                content.to_string().contains("new question"),
                "Last user message should be preserved"
            );
        }

        // 3. Message structure should still be valid
        assert_valid_message_structure(&messages_after);

        Ok(())
    }

    #[test]
    fn test_context_while_writing() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();

        let n_messages = 19;
        // n_messages is chosen by trial and error. This exactly fills up the
        // the context so much that the next assistant message cannot be fully written.
        // The same is true for n_ctx. It needs to be large enough to where n_ctx/2 is large enough
        // to contain the response but also small enough to fill easily and test wihtout being to slow.
        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 768, // Use a small context size to force shifting
                system_prompt: Some("You are a helpful assistant.".into()),
                ..Default::default()
            },
        )?;

        // Fill up the context until it's almost full
        for i in 1..=n_messages {
            worker.add_user_message(format!(
                "This is user message number {}. What is {} * {}?",
                i, i, i
            ));
            worker.add_assistant_message(format!("The answer is {}.", i * i));
        }

        let messages_before_shift = worker.messages.len();
        println!("Messages before shift: {}", messages_before_shift);

        // This should trigger context shift internally because there's not enough space
        let _response = worker
            .ask("What is 10 * 10?")?
            .response(&mut worker, None, true)?
            .content;

        let messages_after = worker.messages.clone();

        println!("Messages after operation: {}", messages_after.len());

        // Verify context shift occurred
        assert!(
            messages_after.len() < messages_before_shift,
            "Context shift should have reduced message count"
        );

        // Verify essential messages are preserved
        // 1. Should have first user message
        let first_user_idx = messages_after.iter().position(|m| m.is_user());
        assert!(
            first_user_idx.is_some(),
            "First user message should be preserved"
        );

        // 2. Verify the last user message is there (the one that triggered the shift)
        let last_user = messages_after.iter().rev().find(|m| m.is_user());

        if let Some(Message::User { content, .. }) = last_user {
            assert!(
                content.to_string().contains("What is"),
                "Last user message should be preserved"
            );
        }

        // 3. Message structure should still be valid
        assert_valid_message_structure(&messages_after);

        Ok(())
    }

    #[test]
    fn test_chat_worker_multiple_contexts() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();

        // Create two separate chat handles that will run in parallel
        let model_clone = Arc::clone(&model);

        // Start Denmark chat thread
        let dk_handle = std::thread::spawn(move || {
            let chat = ChatBuilder::new(model_clone)
                .with_context_size(4096)
                .with_template_variable("enable_thinking".to_string(), false)
                .build()
                .expect("chat build failed in test");

            chat.ask("What is the capital of Denmark?")
                .blocking_completed()
        });

        // Start Germany chat thread
        let de_handle = std::thread::spawn(move || {
            let chat = ChatBuilder::new(model)
                .with_context_size(4096)
                .with_template_variable("enable_thinking".to_string(), false)
                .build()
                .expect("chat build failed in test");

            chat.ask("What is the capital of Germany?")
                .blocking_completed()
        });

        // Wait for both threads to complete and get responses
        let dk_resp = dk_handle.join().unwrap()?;
        let de_resp = de_handle.join().unwrap()?;

        println!("Denmark response: {}", dk_resp);
        println!("Germany response: {}", de_resp);

        assert!(
            dk_resp.to_lowercase().contains("copenhagen"),
            "Expected completion to contain 'Copenhagen', got: {dk_resp}"
        );
        assert!(
            de_resp.to_lowercase().contains("berlin"),
            "Expected completion to contain 'Berlin', got: {de_resp}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_enable_thinking() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .build()
            .expect("chat build_async failed in test");

        let res1: String = chat
            .ask("What is the capital of Denmark?".to_string())
            .completed()
            .await?;

        assert!(
            res1.contains("<think>"),
            "Expected the model to initialize with thinking mode, but it did not"
        );

        chat.lock()
            .await
            .set_template_variable("enable_thinking".to_string(), false);

        let res2: String = chat
            .ask("What is the capital of the Czech Republic?".to_string())
            .completed()
            .await?;

        assert!(
            !res2.contains("<think>"),
            "Expected the model to not think, but it did"
        );

        Ok(())
    }

    #[test]
    fn test_greedy_sampler_produces_deterministic_output() {
        let model = test_utils::load_test_model();

        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        chat.blocking_lock()
            .set_sampler_config(SamplerPresets::greedy())
            .unwrap();

        // Also test if get_sampler followed by set_sampler is no op
        let sampler_config = chat.blocking_lock().get_sampler_config();
        chat.blocking_lock()
            .set_sampler_config(sampler_config)
            .unwrap();

        let response1 = chat
            .ask("Say exactly: 'Hello'")
            .blocking_completed()
            .unwrap();
        chat.blocking_lock().reset_history().unwrap();
        let response2 = chat
            .ask("Say exactly: 'Hello'")
            .blocking_completed()
            .unwrap();

        assert_eq!(
            response1, response2,
            "Greedy sampler should produce identical output for the same prompt"
        );
    }

    #[test]
    fn test_reset_chat_with_no_system_prompt() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");
        let _ = chat.blocking_lock().reset_history();
        let resp = chat
            .ask("What is the capital of Denmark?")
            .blocking_completed()
            .unwrap();
        assert!(
            resp.contains("Copenhagen"),
            "Model failed to answer after reset"
        );
    }

    fn user(content: &str) -> Message {
        Message::new_user(content.to_string())
    }

    fn assistant(content: &str) -> Message {
        Message::new_assistant(content.to_string())
    }

    /// The supplied messages become the history, and the reply is appended to them.
    #[test]
    fn test_complete_replaces_history() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        chat.ask("My favorite color is teal.")
            .blocking_completed()
            .unwrap();

        let messages = vec![
            user("Who was the first person to walk on the moon?"),
            assistant("Neil Armstrong."),
            user("Which year did he do it? Answer with only the year."),
        ];
        let resp = chat
            .complete(messages.clone(), Options::new())
            .unwrap()
            .blocking_completed()
            .unwrap();
        assert!(
            resp.contains("1969"),
            "Model did not read the supplied history: {resp}"
        );

        let history = chat.blocking_lock().get_chat_history();
        assert_eq!(
            serde_json::to_value(&history[..messages.len()]).unwrap(),
            serde_json::to_value(&messages).unwrap(),
            "the supplied messages should be the history, and the teal turn gone"
        );
        assert_eq!(history.len(), messages.len() + 1);
        assert!(history[messages.len()].is_assistant());
        assert_eq!(history[messages.len()].content(), resp);
    }

    /// Options follow the system-prompt rule: what they set stays set, what they
    /// leave out is kept — so a later `complete` need not repeat them.
    #[test]
    fn test_complete_options_stick() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), true)
            .build()
            .expect("chat build failed in test");

        let greedy = SamplerPresets::greedy();
        chat.complete(
            vec![user("Say hi.")],
            Options::new()
                .with_sampler(greedy.clone())
                .with_template_variables([("enable_thinking".to_string(), false)].into()),
        )
        .unwrap()
        .blocking_completed()
        .unwrap();

        // Observable through the ordinary getters, like any other setter.
        assert_eq!(
            chat.blocking_lock().get_template_variables(),
            [("enable_thinking".to_string(), false)].into()
        );
        let after_first = chat.blocking_lock().get_sampler_config();

        // A turn that carries no options changes neither of them.
        chat.complete(vec![user("Say hi again.")], Options::new())
            .unwrap()
            .blocking_completed()
            .unwrap();
        assert_eq!(
            chat.blocking_lock().get_template_variables(),
            [("enable_thinking".to_string(), false)].into(),
            "an empty Options should leave the template variables alone"
        );
        assert_eq!(
            serde_json::to_value(chat.blocking_lock().get_sampler_config()).unwrap(),
            serde_json::to_value(&after_first).unwrap(),
            "an empty Options should leave the sampler alone"
        );
    }

    /// A system message in the list becomes the chat's system prompt; a list
    /// without one keeps the prompt the chat already had.
    #[test]
    fn test_complete_replaces_system_prompt() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_system_prompt(Some("You are a dog. End all responses with woof."))
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        let dog = chat.ask("Hello!").blocking_completed().unwrap();
        assert!(dog.to_lowercase().contains("woof"), "{dog}");

        let cat = chat
            .complete(
                vec![
                    Message::new_system("You are a cat. End all responses with meow.".to_string()),
                    user("Hello!"),
                ],
                Options::new(),
            )
            .unwrap()
            .blocking_completed()
            .unwrap();
        assert!(cat.to_lowercase().contains("meow"), "{cat}");
        assert_eq!(
            chat.blocking_lock().get_system_prompt().as_deref(),
            Some("You are a cat. End all responses with meow.")
        );

        let cat_again = chat.ask("Hello again!").blocking_completed().unwrap();
        assert!(cat_again.to_lowercase().contains("meow"), "{cat_again}");

        let still_cat = chat
            .complete(vec![user("Hello!")], Options::new())
            .unwrap()
            .blocking_completed()
            .unwrap();
        assert_eq!(
            chat.blocking_lock().get_system_prompt().as_deref(),
            Some("You are a cat. End all responses with meow."),
            "a complete() without a system message should keep the current one"
        );
        assert!(still_cat.to_lowercase().contains("meow"), "{still_cat}");
    }

    /// A system message that is not the first one stays in the history, so the
    /// chat template renders it where the caller put it. The test model's
    /// template has to support the system role mid-conversation for this — see
    /// `test_render_inline_system_message` in `template.rs` for the plain render.
    #[test]
    fn test_complete_keeps_inline_system_message() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        let messages = vec![
            Message::new_system("You are a dog. End all responses with woof.".to_string()),
            user("Hello!"),
            assistant("Hello there! Woof."),
            Message::new_system("You are a cat now. End all responses with meow.".to_string()),
            user("Hello again!"),
        ];

        let response = chat
            .complete(messages.clone(), Options::new())
            .unwrap()
            .blocking_completed()
            .unwrap();
        assert!(response.to_lowercase().contains("meow"), "{response}");

        // The leading one became the prompt; the later one is history, followed
        // by the turn that was just generated.
        assert_eq!(
            chat.blocking_lock().get_system_prompt().as_deref(),
            Some("You are a dog. End all responses with woof.")
        );
        let history = chat.blocking_lock().get_chat_history();
        assert_eq!(
            serde_json::to_value(&history[..messages.len() - 1]).unwrap(),
            serde_json::to_value(&messages[1..]).unwrap(),
        );
    }

    #[test]
    fn test_ask_after_complete_continues_completion() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        chat.ask("My favorite color is teal. Remember it.")
            .blocking_completed()
            .unwrap();

        chat.complete(
            vec![
                user("Who was the first person to walk on the moon?"),
                assistant("Neil Armstrong."),
                user("Which year did he do it? Answer with only the year."),
            ],
            Options::new(),
        )
        .unwrap()
        .blocking_completed()
        .unwrap();

        let resp = chat
            .ask("Who are we talking about? Answer with only the name.")
            .blocking_completed()
            .unwrap();
        assert!(
            resp.contains("Armstrong"),
            "ask() did not continue from the completion: {resp}"
        );
        assert!(
            !resp.to_lowercase().contains("teal"),
            "The replaced history is still in context: {resp}"
        );
    }

    #[test]
    fn test_complete_with_tools() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(4096)
            .with_tool(test_tool())
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        let resp = chat
            .complete(
                vec![user("What is the temperature in Copenhagen?")],
                Options::new(),
            )
            .unwrap()
            .blocking_completed()
            .unwrap();

        assert!(resp.contains("13.37"), "Tool was not called: {resp}");

        let history = chat.blocking_lock().get_chat_history();
        assert!(
            history.iter().any(Message::has_tool_calls) && history.iter().any(Message::is_tool),
            "the tool exchange should be part of the history: {history:?}"
        );
    }

    /// `apply_options` applies the sampler and the tools together, so a tool set
    /// that cannot produce a grammar leaves the sampler config alone too.
    #[test]
    fn apply_options_is_atomic_on_failure() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(512)
            .build()
            .expect("chat build failed in test");

        let before = chat.blocking_lock().get_sampler_config();

        // An unresolvable `$ref`: the lark is generated fine and then fails to
        // compile, after the base sampler was already built. (A bogus `type` or a
        // broken regex would not do — tool schemas are compiled leniently.)
        let bad_tool = Tool {
            name: "bad".into(),
            description: "a tool whose schema cannot compile".into(),
            json_schema: serde_json::json!({"$ref": "#/definitions/nope"}),
            function: Arc::new(|_| String::new()),
        };

        // The history is valid, so the rejection comes back through the stream.
        let err = chat
            .complete(
                vec![user("Say hi.")],
                Options::new()
                    .with_sampler(SamplerPresets::greedy())
                    .with_tools(vec![bad_tool]),
            )
            .expect("the history itself is fine")
            .blocking_completed()
            .expect_err("an uncompilable tool schema should be rejected");
        eprintln!("rejected as expected: {err}");

        // The sampler config is the observable half: it is applied first, so
        // before the coalescing it survived a later tool failure.
        assert_eq!(
            serde_json::to_value(chat.blocking_lock().get_sampler_config()).unwrap(),
            serde_json::to_value(&before).unwrap(),
            "a failed Options should leave the sampler config alone"
        );
    }

    #[test]
    fn test_complete_rejects_invalid_history() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(512)
            .build()
            .expect("chat build failed in test");

        assert!(matches!(
            chat.complete(vec![], Options::new()),
            Err(InvalidHistoryError::Empty)
        ));

        assert!(matches!(
            chat.complete(vec![user("Hi"), assistant("Aye, ")], Options::new()),
            Err(InvalidHistoryError::DoesNotEndInUserOrTool { role: "assistant" })
        ));

        // The system prompt is stored as text, so media in it would flatten to a
        // marker with no bitmap behind it.
        assert!(matches!(
            chat.complete(
                vec![
                    Message::new_system(vec![
                        ContentPart::text("Describe like this:"),
                        ContentPart::image("example.png"),
                    ]),
                    user("Hi"),
                ],
                Options::new()
            ),
            Err(InvalidHistoryError::MediaInSystemMessage)
        ));

        // Media is rejected in a mid-conversation system message too: that one
        // stays in the history rather than becoming the prompt, but no chat
        // template renders media in the system role either.
        assert!(matches!(
            chat.complete(
                vec![
                    user("Hi"),
                    assistant("Hello!"),
                    Message::new_system(vec![
                        ContentPart::text("Describe like this:"),
                        ContentPart::image("example.png"),
                    ]),
                    user("What is this?"),
                ],
                Options::new()
            ),
            Err(InvalidHistoryError::MediaInSystemMessage)
        ));
    }

    /// Nothing trial-renders a history before accepting it, so a mid-conversation
    /// system message on a template without a system role is only caught when the
    /// turn renders. Check that what comes back then explains itself.
    #[test]
    fn test_inline_system_message_reports_a_useful_error() -> Result<(), Box<dyn std::error::Error>>
    {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 512,
                ..Default::default()
            },
        )?;
        // Gemma 2 style: raises as soon as it sees the system role, so only a
        // leading system message can be rendered.
        worker.chat_template = ChatTemplate::new(
            "{% for message in messages %}{% if message['role'] == 'system' %}\
             {{ raise_exception('System role not supported') }}{% endif %}\
             {{ message['role'] }}: {{ message['content'] }}\n{% endfor %}",
            "",
            "",
        )?;

        let unrenderable = History::new(vec![
            user("Hi"),
            assistant("Hello!"),
            Message::new_system("Now answer in French.".to_string()),
            user("How are you?"),
        ])?;

        let err = worker.complete(unrenderable, Options::new()).unwrap_err();

        // The help has to survive every wrapper between the template and the
        // binding that renders it.
        let rendered = crate::render_miette(&err);
        assert!(
            rendered.contains("only supports a system message at the front"),
            "{rendered}"
        );
        assert!(rendered.contains("Move the instruction"), "{rendered}");

        Ok(())
    }

    /// `set_chat_history` hoists a leading system message the same way
    /// `complete` does, so it rejects the same histories — and leaves the
    /// worker alive to say so. A mid-conversation system message is not one of
    /// them: it belongs to the history.
    #[test]
    fn test_set_chat_history_rejects_invalid_history() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(512)
            .build()
            .expect("chat build failed in test");
        let mut chat = chat.blocking_lock();

        let err = chat
            .set_chat_history(vec![
                Message::new_system(vec![
                    ContentPart::text("Describe like this:"),
                    ContentPart::image("example.png"),
                ]),
                user("Hi"),
            ])
            .unwrap_err();
        assert!(
            matches!(
                err,
                crate::errors::SetterError::InvalidHistory(
                    InvalidHistoryError::MediaInSystemMessage
                )
            ),
            "{err:?}"
        );

        // The worker survived that, so a valid history still goes through.
        chat.set_chat_history(vec![
            Message::new_system("Be terse.".to_string()),
            user("Hi"),
        ])
        .expect("a leading system message is fine");
        assert_eq!(chat.get_chat_history().len(), 1);

        // A mid-conversation system message is accepted and kept in place, while
        // the leading one is still hoisted out to the system prompt.
        let messages = vec![
            Message::new_system("Be terse.".to_string()),
            user("Hi"),
            assistant("Hello!"),
            Message::new_system("Now answer in French.".to_string()),
            user("How are you?"),
        ];
        chat.set_chat_history(messages.clone())
            .expect("a mid-conversation system message is fine");
        assert_eq!(chat.get_system_prompt().as_deref(), Some("Be terse."));
        assert_eq!(
            serde_json::to_value(chat.get_chat_history()).unwrap(),
            serde_json::to_value(&messages[1..]).unwrap(),
        );

        // An unreadable media file is reported, not fatal.
        let err = chat
            .set_chat_history(vec![Message::new_user(vec![
                ContentPart::text("What is this?"),
                ContentPart::image("/nonexistent/nope.png"),
            ])])
            .unwrap_err();
        assert!(
            matches!(err, crate::errors::SetterError::Multimodal(_)),
            "{err:?}"
        );
        assert_eq!(
            serde_json::to_value(chat.get_chat_history()).unwrap(),
            serde_json::to_value(&messages[1..]).unwrap(),
            "a failed set_chat_history should leave the history alone"
        );
    }

    /// A setter the worker rejects reports the reason and leaves the chat usable.
    /// Before, the error propagated out of `process_worker_msg` and ended the
    /// worker thread, so every later call saw only "worker terminated".
    #[test]
    fn test_rejected_setter_keeps_worker_alive() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(512)
            .build()
            .expect("chat build failed in test");

        let before = chat.blocking_lock().get_sampler_config();

        let err = chat
            .blocking_lock()
            .set_sampler_config(SamplerPresets::constrain_with_json_schema(
                "this is not a json schema".to_string(),
            ))
            .unwrap_err();
        assert!(
            matches!(err, SetterError::Sampler(_)),
            "the sampler error should reach the caller, got {err:?}"
        );

        // The worker survived, and the rejected config was not committed.
        assert_eq!(
            format!("{:?}", chat.blocking_lock().get_sampler_config()),
            format!("{before:?}"),
            "a failed set_sampler_config should leave the sampler alone"
        );

        // A valid config still goes through, and generation still works.
        chat.blocking_lock()
            .set_sampler_config(SamplerPresets::greedy())
            .expect("a valid sampler config is fine");
        chat.ask("Say hi")
            .blocking_completed()
            .expect("worker still answers");
    }

    /// Bitmap ids issued by another worker mean nothing here, so the media has
    /// to be re-registered from the part paths.
    #[test]
    fn test_set_chat_history_reloads_media() -> Result<(), Box<dyn std::error::Error>> {
        let Some(model) = test_utils::load_mtmd_models() else {
            eprintln!(
                "skipping test_set_chat_history_reloads_media: \
                 set TEST_VISION_MODEL and TEST_MMPROJ_MODEL to enable"
            );
            return Ok(());
        };

        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 4096,
                ..Default::default()
            },
        )?;

        let image = test_utils::test_image();
        let mut content = MessageContent::parts([
            ContentPart::text("What is in this image?"),
            ContentPart::image(image),
        ]);
        for part in content.media_parts_mut() {
            part.set_id("id-from-another-session".to_string());
        }

        worker.set_chat_history(vec![Message::User { content }])?;

        let history = worker.get_chat_history();
        let registered = history[0].content_ref().media_parts()[0]
            .id()
            .expect("the part should carry a freshly registered id");
        assert_ne!(registered, "id-from-another-session");
        assert!(worker.media.bitmaps.contains_key(registered));

        Ok(())
    }

    /// The wire format the API promises: a user message whose content is a list
    /// of parts, with an image interleaved between two runs of text.
    #[test]
    fn test_complete_with_content_parts_from_json() -> Result<(), Box<dyn std::error::Error>> {
        let Some(model) = test_utils::load_mtmd_models() else {
            eprintln!(
                "skipping test_complete_with_content_parts_from_json: \
                 set TEST_VISION_MODEL and TEST_MMPROJ_MODEL to enable"
            );
            return Ok(());
        };

        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 4096,
                ..Default::default()
            },
        )?;

        let image = test_utils::test_image();
        let messages: Vec<Message> = serde_json::from_value(serde_json::json!([{
            "role": "user",
            "content": [
                {"type": "text", "text": "Here is an image:"},
                {"type": "image", "path": image},
                {"type": "text", "text": "What animal is in it? Answer in one word."},
            ],
        }]))?;

        let resp = worker
            .complete(History::new(messages)?, Options::new())?
            .response(&mut worker, None, true)?
            .content
            .to_lowercase();

        assert!(
            ["dog", "retriever", "puppy"]
                .iter()
                .any(|word| resp.contains(word)),
            "the interleaved image did not reach the model: {resp}"
        );
        assert_eq!(
            worker.media.bitmaps.len(),
            1,
            "the image part should have been registered"
        );

        // The history keeps the parts, with the bitmap id filled in, and the
        // text between them intact.
        let history = worker.get_chat_history();
        let Message::User { content } = &history[0] else {
            panic!("expected a user message: {:?}", history[0]);
        };
        let MessageContent::Parts(parts) = content else {
            panic!("expected the content to stay parts: {content:?}");
        };
        assert_eq!(parts.len(), 3);
        assert_eq!(parts[0], ContentPart::text("Here is an image:"));
        assert_eq!(content.media_parts().len(), 1);
        assert!(
            content.media_parts()[0].id().is_some(),
            "the registered bitmap id should be on the part"
        );

        Ok(())
    }

    /// Replacing the history releases the media it referenced, and a history that
    /// arrives with media is loaded from the part paths — the bitmap ids in it
    /// mean nothing to this worker.
    #[test]
    fn test_complete_reloads_media_from_part_paths() -> Result<(), Box<dyn std::error::Error>> {
        let Some(model) = test_utils::load_mtmd_models() else {
            eprintln!(
                "skipping test_complete_reloads_media_from_part_paths: \
                 set TEST_VISION_MODEL and TEST_MMPROJ_MODEL to enable"
            );
            return Ok(());
        };

        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 4096,
                ..Default::default()
            },
        )?;

        let image = std::path::PathBuf::from(test_utils::test_image());
        worker
            .ask(Prompt::parts([
                ContentPart::text("What is in this image?"),
                ContentPart::image(image),
            ]))?
            .response(&mut worker, None, true)?;
        assert_eq!(
            worker.media.bitmaps.len(),
            1,
            "expected the image to be registered"
        );

        // Keep the message the image arrived in: its content holds the image part,
        // path and all, which is what a saved conversation stores.
        let stored = worker.get_chat_history()[0].clone();

        // Handing it back while its bitmap is still registered is a no-op: the id
        // is a content hash, so there is nothing to re-read.
        let mut unchanged = stored.clone();
        worker.register_media(unchanged.content_mut())?;
        assert_eq!(
            serde_json::to_value(&unchanged)?,
            serde_json::to_value(&stored)?,
            "an already-registered part should keep its id rather than be reloaded"
        );
        assert_eq!(worker.media.bitmaps.len(), 1);

        worker
            .complete(
                History::new(vec![user("Say the word 'banana'.")])?,
                Options::new(),
            )?
            .response(&mut worker, None, true)?;
        assert_eq!(
            worker.media.bitmaps.len(),
            0,
            "the replaced history's image bitmap should have been released"
        );

        // Replay it with a bitmap id from nowhere — ids are per-worker, so this is
        // what the same history looks like coming from another session.
        let Message::User { content } = &stored else {
            panic!("expected the image to be on a user message: {stored:?}");
        };
        let mut content = content.clone();
        for part in content.media_parts_mut() {
            part.set_id("id-from-another-session".to_string());
        }
        let replayed = Message::User { content };

        let resp = worker
            .complete(History::new(vec![replayed])?, Options::new())?
            .response(&mut worker, None, true)?
            .content
            .to_lowercase();

        assert!(
            ["dog", "retriever", "puppy"]
                .iter()
                .any(|w| resp.contains(w)),
            "the image was not reloaded from its path: {resp}"
        );
        assert_eq!(
            worker.media.bitmaps.len(),
            1,
            "the reloaded bitmap should be registered"
        );

        Ok(())
    }

    /// Media is not a user-only thing — a tool can answer with a screenshot.
    /// An unregistered part would flatten to a marker with no bitmap behind it.
    #[test]
    fn test_reload_media_covers_every_non_system_role() -> Result<(), Box<dyn std::error::Error>> {
        let Some(model) = test_utils::load_mtmd_models() else {
            eprintln!(
                "skipping test_reload_media_covers_every_non_system_role: \
                 set TEST_VISION_MODEL and TEST_MMPROJ_MODEL to enable"
            );
            return Ok(());
        };

        let mut worker = Chat::new(
            model,
            ChatConfig {
                n_ctx: 4096,
                ..Default::default()
            },
        )?;

        let image = test_utils::test_image();
        let screenshot = || {
            MessageContent::parts([ContentPart::text("Here it is:"), ContentPart::image(&image)])
        };
        let mut messages = History::new(vec![
            user("Take a screenshot."),
            Message::new_tool("screenshot".to_string(), screenshot()),
        ])?;

        worker.reload_media(&mut messages)?;

        for message in messages.iter() {
            for part in message.content_ref().media_parts() {
                assert!(
                    part.id()
                        .is_some_and(|id| worker.media.bitmaps.contains_key(id)),
                    "media on a {} message was not registered: {part:?}",
                    message.role(),
                );
            }
        }
        assert_eq!(worker.media.bitmaps.len(), 1);

        Ok(())
    }

    #[tokio::test]
    async fn test_complete_async() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build_async failed in test");

        let resp = chat
            .complete(
                vec![user("What is the capital of Denmark?")],
                Options::new(),
            )?
            .completed()
            .await?;

        assert!(resp.contains("Copenhagen"), "{resp}");
        assert_eq!(chat.lock().await.get_chat_history().len(), 2);

        Ok(())
    }

    /// Prompt tokens llama.cpp decodes during `turn`; generated tokens aren't counted.
    fn prompt_tokens_decoded(chat: &mut Chat, turn: impl FnOnce(&mut Chat)) -> i32 {
        // llama.cpp counts a decode once it finishes, which reading the logits waits for.
        let _ = chat.engine().ctx.get_logits();
        chat.engine_mut(|e| e.ctx.reset_timings());
        turn(chat);
        let _ = chat.engine().ctx.get_logits();
        chat.engine_mut(|e| e.ctx.timings()).n_p_eval()
    }

    /// Before each turn the KV cache must hold exactly the rendered chat, by
    /// position and by token count, and later turns must not re-read all of it.
    fn assert_cache_matches_render(
        model: Arc<llm::Model>,
        config: ChatConfig,
        first: MessageContent,
    ) {
        let mut chat = Chat::new(
            model,
            ChatConfig {
                n_ctx: 4096,
                sampler_config: Some(SamplerPresets::greedy()),
                ..config
            },
        )
        .unwrap();
        for (turn, mut prompt) in [
            first,
            "Say bye in one word.".into(),
            "Count to three.".into(),
        ]
        .into_iter()
        .enumerate()
        {
            chat.register_media(&mut prompt).unwrap();
            chat.add_user_message(prompt);
            let render = render(&chat, &chat.messages).unwrap();
            let decoded = prompt_tokens_decoded(&mut chat, |chat| {
                chat.sync_context_with_render().unwrap();
                assert_eq!(
                    chat.engine().ctx.kv_cache_seq_pos_max(0) + 1,
                    render.n_positions() as i32
                );
                assert_eq!(
                    chat.engine().actual_context_size(),
                    render.n_tokens() as i32
                );
            });
            if turn > 0 {
                assert!(
                    decoded < render.n_tokens() as i32,
                    "turn {turn} re-read the whole chat"
                );
            }
            // MTP verifies its drafts in batches, which llama.cpp counts as prompt tokens.
            let _response = TurnGenerator::new(&mut chat)
                .unwrap()
                .response(&mut chat, Some(64), true)
                .unwrap();
        }
    }

    /// Gemma 3 ends a turn with a newline after the end-of-turn token the model generates.
    #[test]
    fn test_cache_matches_render_gemma() {
        let Some(model) = test_utils::load_model_from_env("TEST_VISION_MODEL", None) else {
            return;
        };
        assert_cache_matches_render(model, ChatConfig::default(), "Say hi in one word.".into());
    }

    /// Qwen3.5 gives an image fewer KV positions than tokens (M-RoPE).
    #[test]
    fn test_cache_matches_render_mrope_image() {
        let Some(model) = test_utils::load_model_from_env(
            "TEST_RECURRENT_MODEL",
            Some("TEST_RECURRENT_MMPROJ_MODEL"),
        ) else {
            return;
        };
        let image = test_utils::test_image();
        let first = MessageContent::parts([
            ContentPart::image(image),
            ContentPart::text("What animal is this? One word."),
        ]);
        assert_cache_matches_render(model, ChatConfig::default(), first);
    }

    /// Qwen3.5 drops an answer's thinking from the history, so each turn rewinds
    /// its recurrent state to the checkpoint at the end of the last user message.
    #[test]
    fn test_recurrent_thinking_restores_checkpoint() {
        let Some(model) = test_utils::load_model_from_env("TEST_RECURRENT_MODEL", None) else {
            return;
        };
        let config = ChatConfig {
            template_variables: [("enable_thinking".to_string(), true)].into(),
            ..Default::default()
        };
        assert_cache_matches_render(model, config, "Say hi in one word.".into());
    }

    /// MTP on a recurrent model rolls rejected drafts back natively, and the draft
    /// context's own cache follows each rewind of the target's.
    #[test]
    fn test_recurrent_mtp_restores_checkpoint() {
        test_utils::init_test_tracing();
        let Ok(path) = std::env::var("TEST_RECURRENT_MTP_MODEL") else {
            eprintln!("skipping: set TEST_RECURRENT_MTP_MODEL to run this test");
            return;
        };
        // Qwen3.5 keeps its MTP head in the same file.
        let model = Arc::new(llm::get_model(&path, true, None, Some(&path), None).unwrap());
        let config = ChatConfig {
            mtp: Some(MtpConfig::default()),
            template_variables: [("enable_thinking".to_string(), true)].into(),
            ..Default::default()
        };
        assert_cache_matches_render(model, config, "Say hi in one word.".into());
    }

    /// A tool call adds a sync at the tool response; the checkpoint has to stay at
    /// the user message, which the next turn rewinds to.
    #[test]
    fn test_recurrent_tool_calls_restore_checkpoint() {
        let Some(model) = test_utils::load_model_from_env("TEST_RECURRENT_MODEL", None) else {
            return;
        };
        let mut chat = Chat::new(
            model,
            ChatConfig {
                tools: vec![test_tool()],
                template_variables: [("enable_thinking".to_string(), false)].into(),
                sampler_config: Some(SamplerPresets::greedy()),
                ..Default::default()
            },
        )
        .unwrap();
        for (prompt, expected) in [
            ("What is the temperature in Copenhagen?", "13.37"),
            ("What is the temperature in Beijing?", "42.69"),
            ("And Copenhagen again?", "13.37"),
        ] {
            let cached = chat.engine_mut(|e| e.kv_mirror().n_tokens()) as i32;
            let mut answer = String::new();
            let decoded = prompt_tokens_decoded(&mut chat, |chat| {
                answer = chat
                    .ask(prompt)
                    .unwrap()
                    .response(chat, None, true)
                    .unwrap()
                    .content;
            });
            assert!(answer.contains(expected), "expected {expected} in {answer}");
            if cached > 0 {
                assert!(decoded < cached, "{prompt:?} re-read the whole chat");
            }
        }
    }

    // Template rendering tests have been moved to template.rs module
}
