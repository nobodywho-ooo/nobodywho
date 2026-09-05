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
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let model = Arc::new(llm::get_model("model.gguf", true, None, None, None)?);
//!
//! let chat = ChatBuilder::new(model)
//!     .with_system_prompt(Some("You are a helpful assistant"))
//!     .build();
//!
//! let response = chat.ask("Hello!").completed()?;
//! # Ok(())
//! # }
//! ```
//!

pub use crate::content::{ContentPart, MessageContent};
use crate::errors::{
    ChatWorkerError, CompleteError, ContextSyncError, GenerateResponseError, InitWorkerError,
    InvalidHistoryError, MultimodalError, RenderError, SayError, SetterError, ShiftError,
    TokenizeError, ToolCallingSetupError, WrappedResponseError,
};
use crate::inference::{acquire_inference_lock, InferenceEngine};
use crate::llm;
use crate::llm::{GlobalInferenceLockToken, Worker, WorkerGuard, WriteOutput};
use crate::sampler::read_sampler_from_metadata;
use crate::sampler::GrammarFactory;
use crate::sampler::SamplerConfig;
use crate::template::{select_template, ChatTemplate, ChatTemplateContext};
use crate::tokenizer::{ChunkId, Prompt, Promptable, TokenizerChunk, TokenizerChunks};
use crate::tool_calling::{detect_tool_format, Tool, ToolCall, ToolFormat, ToolFormatError};
use ahash::AHasher;
use indexmap::IndexMap;
use llama_cpp_2::mtmd::MtmdBitmap;
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::collections::HashSet;
use std::hash::Hasher;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, MutexGuard};
use tracing::{debug, error, info, trace, warn};

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User {
        content: MessageContent,
    },
    // The optional tool_calls field distinguishes a plain assistant response
    // from one that includes tool calls. When tool_calls is Some, the content
    // field is typically empty (required by qwen3 chat templates).
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

/// Check that a message list describes a conversation the model can answer:
/// non-empty, ending in a user or tool message, with a system message only in
/// front. A trailing assistant message would render without a generation prompt,
/// making the model continue that message instead of replying to the user.
pub fn validate_completion_messages(messages: &[Message]) -> Result<(), InvalidHistoryError> {
    let Some(last) = messages.last() else {
        return Err(InvalidHistoryError::Empty);
    };

    if !(last.is_user() || last.is_tool()) {
        return Err(InvalidHistoryError::DoesNotEndInUserOrTool { role: last.role() });
    }

    if let Some(index) = messages[1..].iter().position(Message::is_system) {
        return Err(InvalidHistoryError::MisplacedSystemMessage { index: index + 1 });
    }

    // A leading system message becomes the plain-text system prompt, so media
    // in it has nowhere to go.
    match messages.first() {
        Some(first) if first.is_system() && !first.content_ref().media_parts().is_empty() => {
            Err(InvalidHistoryError::MediaInSystemMessage)
        }
        _ => Ok(()),
    }
}

/// Turns kept at the end of the history during a context shift; the first turn
/// is always kept too.
const PRESERVED_RECENT_TURNS: usize = 2;

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

/// Settings that apply only to one structured completion.
#[derive(Clone, Debug, Default)]
pub struct TurnOptions {
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub seed: Option<u32>,
    pub max_tokens: Option<usize>,
}

impl TurnOptions {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn with_sampling_overrides(
        mut self,
        temperature: Option<f32>,
        top_p: Option<f32>,
        seed: Option<u32>,
    ) -> Result<Self, crate::errors::SamplingOverrideError> {
        crate::sampler::validate_sampling_overrides(temperature, top_p)?;
        self.temperature = temperature;
        self.top_p = top_p;
        self.seed = seed;
        Ok(self)
    }

    pub fn with_max_tokens(mut self, max_tokens: usize) -> Self {
        self.max_tokens = Some(max_tokens);
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

    /// Build a blocking chat handle and start the background worker.
    pub fn build(self) -> Result<ChatHandle, InitWorkerError> {
        ChatHandle::new(self.model, self.config)
    }

    /// Build an async chat handle and start the background worker.
    pub fn build_async(self) -> Result<ChatHandleAsync, InitWorkerError> {
        ChatHandleAsync::new(self.model, self.config)
    }
}

/// Interact with a ChatWorker in a blocking manner.
///
/// Use [`ChatBuilder`] to create a new instance with a fluent API.
pub struct ChatHandle {
    guard: Arc<WorkerGuard<ChatMsg>>,
}

impl ChatHandle {
    /// Create a new chat handle directly. Consider using [`ChatBuilder`] for a more ergonomic API.
    pub fn new(model: Arc<llm::Model>, config: ChatConfig) -> Result<Self, InitWorkerError> {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();
        let (init_tx, init_rx) = std::sync::mpsc::channel::<Result<(), InitWorkerError>>();

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        let join_handle = std::thread::spawn(move || {
            let worker = Chat::new_chat_worker(&model, config, should_stop_clone);
            let mut worker_state = match worker {
                Ok(w) => {
                    let _ = init_tx.send(Ok(()));
                    w
                }
                Err(e) => {
                    let _ = init_tx.send(Err(e));
                    return;
                }
            };

            while let Ok(msg) = msg_rx.recv() {
                process_worker_msg(&mut worker_state, msg);
            }
        });

        init_rx.recv().map_err(|_| InitWorkerError::NoResponse)??;

        Ok(Self {
            guard: Arc::new(WorkerGuard::new(msg_tx, join_handle, Some(should_stop))),
        })
    }

    /// Send a message and get a tokio channel
    /// TODO: deprecate this in favor of plain `ask` once integrations are updated
    pub fn ask_channel(
        &self,
        prompt: Prompt,
    ) -> tokio::sync::mpsc::UnboundedReceiver<llm::WriteOutput> {
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        self.guard.send(ChatMsg::Ask { prompt, output_tx });
        output_rx
    }

    /// Send a message and collect tokens as they arrive.
    ///
    /// # Example
    /// ```
    /// # use nobodywho::chat::ChatHandleAsync;
    /// # async fn example(chat: &ChatHandleAsync) {
    /// let mut stream = chat.ask("Tell me a story");
    /// while let Some(token) = stream.next_token().await {
    ///     print!("{}", token);
    /// }
    /// # }
    /// ```
    pub fn ask(&self, prompt: impl Promptable) -> TokenStream {
        TokenStream::new(forward_write_output(self.ask_channel(prompt.to_prompt())))
    }

    /// Answer a full message list and get a tokio channel.
    pub fn complete_channel(
        &self,
        messages: Vec<Message>,
        options: Options,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<llm::WriteOutput>, InvalidHistoryError> {
        validate_completion_messages(&messages)?;
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        self.guard.send(ChatMsg::Complete {
            messages,
            options,
            output_tx,
        });
        Ok(output_rx)
    }

    /// Answer a full message list, which replaces the chat history.
    ///
    /// The list is the whole conversation: it must be non-empty, end in a user or
    /// tool message, and carry a system message only in front. A leading system
    /// message becomes the chat's system prompt; a list without one keeps the
    /// prompt the chat already had. The response is appended, and a following
    /// `ask` continues from there.
    ///
    /// [`Options`] follows the same rule for the chat's other settings.
    ///
    /// # Example
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
    /// println!("{}", stream.completed()?);
    /// # Ok(())
    /// # }
    /// ```
    pub fn complete(
        &self,
        messages: Vec<Message>,
        options: Options,
    ) -> Result<TokenStream, InvalidHistoryError> {
        Ok(TokenStream::new(forward_write_output(
            self.complete_channel(messages, options)?,
        )))
    }

    /// Generate one assistant response with completion metadata.
    ///
    /// Tool callbacks run as they do for [`complete`](Self::complete). Settings in
    /// [`TurnOptions`] apply only to this request and do not change the chat defaults.
    pub fn complete_with_metadata(
        &self,
        messages: Vec<Message>,
        options: Options,
        turn_options: TurnOptions,
    ) -> Result<StructuredCompletionStream, InvalidHistoryError> {
        self.structured_completion_stream(messages, options, turn_options, true)
    }

    /// Generate one assistant response and return tool calls to the caller.
    ///
    /// Unlike [`complete`](Self::complete), this never executes tool callbacks.
    /// The caller can execute returned calls and submit the full conversation,
    /// including the tool results, in a following request. Settings in
    /// [`TurnOptions`] apply only to this request.
    pub fn complete_with_external_tools(
        &self,
        messages: Vec<Message>,
        options: Options,
        turn_options: TurnOptions,
    ) -> Result<StructuredCompletionStream, InvalidHistoryError> {
        self.structured_completion_stream(messages, options, turn_options, false)
    }

    fn structured_completion_stream(
        &self,
        messages: Vec<Message>,
        options: Options,
        turn_options: TurnOptions,
        execute_tools: bool,
    ) -> Result<StructuredCompletionStream, InvalidHistoryError> {
        Ok(StructuredCompletionStream(completion_receiver(
            &self.guard,
            messages,
            options,
            turn_options,
            execute_tools,
        )?))
    }

    /// Send a setter message and block until the worker answers. The outer error
    /// is the worker being gone, the inner one is the worker rejecting the value.
    fn set_and_wait_blocking<F>(
        &self,
        make_msg: F,
        label: &str,
    ) -> Result<(), crate::errors::SetterError>
    where
        F: FnOnce(SetterReply) -> ChatMsg,
    {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        let msg = make_msg(output_tx);
        self.guard.send(msg);
        // block until processed
        output_rx
            .blocking_recv()
            .ok_or_else(|| crate::errors::SetterError::WorkerTerminated(label.into()))?
    }

    /// Reset the chat conversation with a new system prompt and tools.
    pub fn reset_chat(
        &self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(
            |output_tx| ChatMsg::ResetChat {
                system_prompt,
                tools,
                output_tx,
            },
            "reset_chat",
        )
    }

    /// Reset the chat conversation history.
    pub fn reset_history(&self) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(
            |output_tx| ChatMsg::SetChatHistory {
                messages: vec![],
                output_tx,
            },
            "reset_history",
        )
    }

    /// Update the available tools for the model to use.
    pub fn set_tools(&self, tools: Vec<Tool>) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(
            |output_tx| ChatMsg::SetTools { tools, output_tx },
            "set_tools",
        )
    }

    /// DEPRECATED: Use set_template_variable("enable_thinking", value) instead.
    #[deprecated(note = "Use set_template_variable(\"enable_thinking\", value) instead")]
    pub fn set_allow_thinking(
        &self,
        allow_thinking: bool,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(
            |output_tx| ChatMsg::SetThinking {
                allow_thinking,
                output_tx,
            },
            "set_allow_thinking",
        )
    }

    /// Set a single template variable.
    pub fn set_template_variable(
        &self,
        name: String,
        value: bool,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(
            |output_tx| ChatMsg::SetTemplateVariable {
                name,
                value,
                output_tx,
            },
            "set_template_variable",
        )
    }

    /// Set all template variables, replacing any existing ones.
    pub fn set_template_variables(
        &self,
        variables: std::collections::HashMap<String, bool>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(
            |output_tx| ChatMsg::SetTemplateVariables {
                variables,
                output_tx,
            },
            "set_template_variables",
        )
    }

    /// Get all template variables.
    pub fn get_template_variables(
        &self,
    ) -> Result<std::collections::HashMap<String, bool>, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetTemplateVariables { output_tx });
        output_rx
            .blocking_recv()
            .ok_or(crate::errors::GetterError::GetterError(
                "get_template_variables".into(),
            ))
    }

    /// Update the sampler configuration for inference.
    pub fn set_sampler_config(
        &self,
        sampler_config: SamplerConfig,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(
            |output_tx| ChatMsg::SetSamplerConfig {
                sampler_config,
                output_tx,
            },
            "set_sampler_config",
        )
    }

    /// Stop the current generation if one is in progress.
    pub fn stop_generation(&self) {
        self.guard.stop();
    }

    /// Get the chat history (lower-level API).
    ///
    /// The system prompt is a separate setting, so it is never part of this
    /// list. See [`get_system_prompt`](Self::get_system_prompt).
    pub fn get_chat_history(&self) -> Result<Vec<Message>, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetChatHistory { output_tx });
        output_rx
            .blocking_recv()
            .ok_or(crate::errors::GetterError::GetterError(
                "get_chat_history".into(),
            ))
    }

    /// Set the chat history (lower-level API).
    ///
    /// A leading system message becomes the chat's system prompt.
    pub fn set_chat_history(
        &self,
        messages: Vec<Message>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(
            |output_tx| ChatMsg::SetChatHistory {
                messages,
                output_tx,
            },
            "set_chat_history",
        )
    }
    /// Get the sampler config
    pub fn get_sampler_config(&self) -> Result<SamplerConfig, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetSamplerConfig { output_tx });
        output_rx
            .blocking_recv()
            .ok_or(crate::errors::GetterError::GetterError(
                "get_sampler_config".into(),
            ))
    }

    /// Get context usage statistics.
    pub fn get_stats(&self) -> Result<ChatStats, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetStats { output_tx });
        output_rx
            .blocking_recv()
            .ok_or(crate::errors::GetterError::GetterError("get_stats".into()))
    }

    /// MTP draft acceptance rate for the most recent generation, in `[0.0, 1.0]`.
    ///
    /// The counters reset at the start of each generation.
    /// Returns `None` when no drafts were proposed in the last generation.
    pub fn mtp_acceptance_rate(&self) -> Result<Option<f32>, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetMtpAcceptanceRate { output_tx });
        output_rx
            .blocking_recv()
            .ok_or(crate::errors::GetterError::GetterError(
                "mtp_acceptance_rate".into(),
            ))
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
    /// # let chat = ChatBuilder::new(model).build();
    /// chat.set_system_prompt(Some("You are a helpful coding assistant.".to_string()))?;
    /// # Ok::<(), nobodywho::errors::SetterError>(())
    /// ```
    pub fn set_system_prompt(
        &self,
        system_prompt: Option<String>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(
            |output_tx| ChatMsg::SetSystemPrompt {
                system_prompt,
                output_tx,
            },
            "set_system_prompt",
        )
    }

    /// Get the system prompt
    pub fn get_system_prompt(&self) -> Result<Option<String>, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetSystemPrompt { output_tx });
        output_rx
            .blocking_recv()
            .ok_or(crate::errors::GetterError::GetterError(
                "get_system_prompt".into(),
            ))
    }

    /// Tokenize a prompt and return token IDs. Text tokens are `Some(id)`, media embedding
    /// slots are `None` (one per slot consumed in the context window).
    pub fn tokenize(&self, prompt: impl Promptable) -> Result<Vec<Option<i32>>, TokenizeError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::Tokenize {
            prompt: prompt.to_prompt(),
            output_tx,
        });
        output_rx
            .blocking_recv()
            .ok_or(TokenizeError::WorkerTerminated)?
    }
}

/// Interact with a ChatWorker in an asynchronous manner.
///
/// Use [`ChatBuilder`] to create a new instance with a fluent API.
#[derive(Clone)]
pub struct ChatHandleAsync {
    guard: Arc<WorkerGuard<ChatMsg>>,
}

impl ChatHandleAsync {
    /// Create a new chat handle directly. Consider using [`ChatBuilder`] for a more ergonomic API.
    pub fn new(model: Arc<llm::Model>, config: ChatConfig) -> Result<Self, InitWorkerError> {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();
        let (init_tx, init_rx) = std::sync::mpsc::channel::<Result<(), InitWorkerError>>();

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        let join_handle = std::thread::spawn(move || {
            let worker = Chat::new_chat_worker(&model, config, should_stop_clone);
            let mut worker_state = match worker {
                Ok(w) => {
                    let _ = init_tx.send(Ok(()));
                    w
                }
                Err(e) => {
                    let _ = init_tx.send(Err(e));
                    return;
                }
            };

            while let Ok(msg) = msg_rx.recv() {
                process_worker_msg(&mut worker_state, msg);
            }
        });

        init_rx.recv().map_err(|_| InitWorkerError::NoResponse)??;

        Ok(Self {
            guard: Arc::new(WorkerGuard::new(msg_tx, join_handle, Some(should_stop))),
        })
    }

    /// Send a message and get a tokio channel
    /// TODO: deprecate this in favor of plain `ask` once integrations are updated
    pub fn ask_channel(
        &self,
        prompt: Prompt,
    ) -> tokio::sync::mpsc::UnboundedReceiver<llm::WriteOutput> {
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        self.guard.send(ChatMsg::Ask { prompt, output_tx });
        output_rx
    }

    /// Send a message and collect tokens as they arrive.
    ///
    /// # Example
    /// ```
    /// # use nobodywho::chat::ChatHandleAsync;
    /// # async fn example(chat: &ChatHandleAsync) {
    /// let mut stream = chat.ask("Tell me a story");
    /// while let Some(token) = stream.next_token().await {
    ///     print!("{}", token);
    /// }
    /// # }
    /// ```
    pub fn ask(&self, prompt: impl Promptable) -> TokenStreamAsync {
        TokenStreamAsync::new(forward_write_output(self.ask_channel(prompt.to_prompt())))
    }

    /// Answer a full message list and get a tokio channel.
    pub fn complete_channel(
        &self,
        messages: Vec<Message>,
        options: Options,
    ) -> Result<tokio::sync::mpsc::UnboundedReceiver<llm::WriteOutput>, InvalidHistoryError> {
        validate_completion_messages(&messages)?;
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        self.guard.send(ChatMsg::Complete {
            messages,
            options,
            output_tx,
        });
        Ok(output_rx)
    }

    /// Answer a full message list, which replaces the chat history.
    ///
    /// The list is the whole conversation: it must be non-empty, end in a user or
    /// tool message, and carry a system message only in front. A leading system
    /// message becomes the chat's system prompt; a list without one keeps the
    /// prompt the chat already had. The response is appended, and a following
    /// `ask` continues from there.
    ///
    /// [`Options`] follows the same rule for the chat's other settings.
    ///
    /// # Example
    /// ```
    /// # use nobodywho::chat::{ChatHandleAsync, Message, Options};
    /// # async fn example(chat: &ChatHandleAsync) -> Result<(), Box<dyn std::error::Error>> {
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
    ) -> Result<TokenStreamAsync, InvalidHistoryError> {
        Ok(TokenStreamAsync::new(forward_write_output(
            self.complete_channel(messages, options)?,
        )))
    }

    /// Generate one assistant response and return tool calls to the caller.
    ///
    /// Unlike [`complete`](Self::complete), this never executes tool callbacks.
    /// The caller can execute returned calls and submit the full conversation,
    /// including the tool results, in a following request.
    pub fn complete_with_external_tools(
        &self,
        messages: Vec<Message>,
        options: Options,
        max_tokens: Option<usize>,
    ) -> Result<CompletionStreamAsync, InvalidHistoryError> {
        Ok(CompletionStreamAsync(completion_receiver(
            &self.guard,
            messages,
            options,
            TurnOptions {
                max_tokens,
                ..Default::default()
            },
            false,
        )?))
    }

    pub fn complete_with_external_tools_and_metadata(
        &self,
        messages: Vec<Message>,
        options: Options,
        turn_options: TurnOptions,
    ) -> Result<StructuredCompletionStreamAsync, InvalidHistoryError> {
        Ok(StructuredCompletionStreamAsync(completion_receiver(
            &self.guard,
            messages,
            options,
            turn_options,
            false,
        )?))
    }

    /// Send a setter message and wait for the worker's answer. The outer error
    /// is the worker being gone, the inner one is the worker rejecting the value.
    async fn set_and_wait_async<F>(
        &self,
        make_msg: F,
        label: &str,
    ) -> Result<(), crate::errors::SetterError>
    where
        F: FnOnce(SetterReply) -> ChatMsg,
    {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        let msg = make_msg(output_tx);
        self.guard.send(msg);
        // wait until processed
        output_rx
            .recv()
            .await
            .ok_or_else(|| crate::errors::SetterError::WorkerTerminated(label.into()))?
    }

    /// Reset the chat conversation with a new system prompt and tools.
    pub async fn reset_chat(
        &self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(
            |output_tx| ChatMsg::ResetChat {
                system_prompt,
                tools,
                output_tx,
            },
            "reset_chat",
        )
        .await
    }

    /// Reset the chat conversation history.
    pub async fn reset_history(&self) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(
            |output_tx| ChatMsg::SetChatHistory {
                messages: vec![],
                output_tx,
            },
            "reset_history",
        )
        .await
    }

    /// Update the available tools for the model to use.
    pub async fn set_tools(&self, tools: Vec<Tool>) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(
            |output_tx| ChatMsg::SetTools { tools, output_tx },
            "set_tools",
        )
        .await
    }

    /// DEPRECATED: Use set_template_variable("enable_thinking", value) instead.
    #[deprecated(note = "Use set_template_variable(\"enable_thinking\", value) instead")]
    pub async fn set_allow_thinking(
        &self,
        allow_thinking: bool,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(
            |output_tx| ChatMsg::SetThinking {
                allow_thinking,
                output_tx,
            },
            "set_allow_thinking",
        )
        .await
    }

    /// Set a single template variable.
    pub async fn set_template_variable(
        &self,
        name: String,
        value: bool,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(
            |output_tx| ChatMsg::SetTemplateVariable {
                name,
                value,
                output_tx,
            },
            "set_template_variable",
        )
        .await
    }

    /// Set all template variables, replacing any existing ones.
    pub async fn set_template_variables(
        &self,
        variables: std::collections::HashMap<String, bool>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(
            |output_tx| ChatMsg::SetTemplateVariables {
                variables,
                output_tx,
            },
            "set_template_variables",
        )
        .await
    }

    /// Get all template variables.
    pub async fn get_template_variables(
        &self,
    ) -> Result<std::collections::HashMap<String, bool>, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetTemplateVariables { output_tx });
        output_rx
            .recv()
            .await
            .ok_or(crate::errors::GetterError::GetterError(
                "get_template_variables".into(),
            ))
    }

    /// Update the sampler configuration for inference.
    pub async fn set_sampler_config(
        &self,
        sampler_config: SamplerConfig,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(
            |output_tx| ChatMsg::SetSamplerConfig {
                sampler_config,
                output_tx,
            },
            "set_sampler_config",
        )
        .await
    }

    /// Stop the current generation if one is in progress.
    pub fn stop_generation(&self) {
        self.guard.stop();
    }

    /// Get the chat history (lower-level API).
    ///
    /// The system prompt is a separate setting, so it is never part of this
    /// list. See [`get_system_prompt`](Self::get_system_prompt).
    pub async fn get_chat_history(&self) -> Result<Vec<Message>, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetChatHistory { output_tx });
        output_rx
            .recv()
            .await
            .ok_or(crate::errors::GetterError::GetterError(
                "get_chat_history".into(),
            ))
    }

    /// Set the chat history (lower-level API).
    ///
    /// A leading system message becomes the chat's system prompt.
    pub async fn set_chat_history(
        &self,
        messages: Vec<Message>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(
            |output_tx| ChatMsg::SetChatHistory {
                messages,
                output_tx,
            },
            "set_chat_history",
        )
        .await
    }

    /// Get the sampler config.
    pub async fn get_sampler_config(&self) -> Result<SamplerConfig, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetSamplerConfig { output_tx });
        output_rx
            .recv()
            .await
            .ok_or(crate::errors::GetterError::GetterError(
                "get_sampler_config".into(),
            ))
    }

    /// Get context usage statistics.
    pub async fn get_stats(&self) -> Result<ChatStats, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetStats { output_tx });
        output_rx
            .recv()
            .await
            .ok_or(crate::errors::GetterError::GetterError("get_stats".into()))
    }

    /// MTP draft acceptance rate for the most recent generation, in `[0.0, 1.0]`.
    ///
    /// The counters reset at the start of each generation.
    /// Returns `None` when no drafts were proposed in the last generation.
    pub async fn mtp_acceptance_rate(&self) -> Result<Option<f32>, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetMtpAcceptanceRate { output_tx });
        output_rx
            .recv()
            .await
            .ok_or(crate::errors::GetterError::GetterError(
                "mtp_acceptance_rate".into(),
            ))
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
    /// ```ignore
    /// # use nobodywho::chat::ChatBuilder;
    /// # use nobodywho::llm::get_model;
    /// # use std::sync::Arc;
    /// # let model = Arc::new(get_model("model.gguf", true, None, None, None).unwrap());
    /// # let chat = ChatBuilder::new(model).build_async();
    /// # chat.set_system_prompt(Some("You are a helpful coding assistant.".to_string())).await?;
    /// # Ok::<(), nobodywho::errors::SetterError>(())
    /// ```
    pub async fn set_system_prompt(
        &self,
        system_prompt: Option<String>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(
            |output_tx| ChatMsg::SetSystemPrompt {
                system_prompt,
                output_tx,
            },
            "set_system_prompt",
        )
        .await
    }

    /// Get the system prompt
    pub async fn get_system_prompt(&self) -> Result<Option<String>, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::GetSystemPrompt { output_tx });
        output_rx
            .recv()
            .await
            .ok_or(crate::errors::GetterError::GetterError(
                "get_system_prompt".into(),
            ))
    }

    /// Tokenize a prompt and return token IDs. Text tokens are `Some(id)`, media embedding
    /// slots are `None` (one per slot consumed in the context window).
    pub async fn tokenize(
        &self,
        prompt: impl Promptable,
    ) -> Result<Vec<Option<i32>>, TokenizeError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(ChatMsg::Tokenize {
            prompt: prompt.to_prompt(),
            output_tx,
        });
        output_rx
            .recv()
            .await
            .ok_or(TokenizeError::WorkerTerminated)?
    }
}

/// A stream of tokens from the model.
pub type TokenStream = crate::stream::TokenStream<crate::errors::CompletionError>;
/// A stream of tokens from the model, async version.
pub type TokenStreamAsync = crate::stream::TokenStreamAsync<crate::errors::CompletionError>;

/// Why a chat completion stopped generating.
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

/// Token counts for one chat completion.
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
pub struct AssistantResponse {
    pub content: String,
    pub tool_calls: Vec<ToolCall>,
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
    Done(AssistantResponse),
}

#[derive(Clone, Debug, PartialEq)]
pub enum StructuredCompletionChunk {
    Token(String),
    Done(CompletionResponse),
}

enum ExternalCompletionOutput {
    Token(String),
    Done(CompletionResponse),
    Error(crate::errors::CompletionError),
}

enum CompletionOutput {
    Token(String),
    Done(CompletionResponse),
}

struct CompletionReceiver {
    rx: tokio::sync::mpsc::Receiver<ExternalCompletionOutput>,
    _guard: Arc<WorkerGuard<ChatMsg>>,
    done: bool,
}

impl CompletionReceiver {
    fn new(
        rx: tokio::sync::mpsc::Receiver<ExternalCompletionOutput>,
        guard: Arc<WorkerGuard<ChatMsg>>,
    ) -> Self {
        Self {
            rx,
            _guard: guard,
            done: false,
        }
    }

    fn handle(
        &mut self,
        output: Option<ExternalCompletionOutput>,
    ) -> Result<Option<CompletionOutput>, crate::errors::CompletionError> {
        match output {
            Some(ExternalCompletionOutput::Token(token)) => {
                Ok(Some(CompletionOutput::Token(token)))
            }
            Some(ExternalCompletionOutput::Done(response)) => {
                self.done = true;
                Ok(Some(CompletionOutput::Done(response)))
            }
            Some(ExternalCompletionOutput::Error(error)) => {
                self.done = true;
                Err(error)
            }
            None => {
                self.done = true;
                Ok(None)
            }
        }
    }

    fn next(&mut self) -> Result<Option<CompletionOutput>, crate::errors::CompletionError> {
        if self.done {
            return Ok(None);
        }
        let output = self.rx.blocking_recv();
        self.handle(output)
    }

    async fn next_async(
        &mut self,
    ) -> Result<Option<CompletionOutput>, crate::errors::CompletionError> {
        if self.done {
            return Ok(None);
        }
        let output = self.rx.recv().await;
        self.handle(output)
    }
}

fn completion_receiver(
    guard: &Arc<WorkerGuard<ChatMsg>>,
    messages: Vec<Message>,
    options: Options,
    turn_options: TurnOptions,
    execute_tools: bool,
) -> Result<CompletionReceiver, InvalidHistoryError> {
    validate_completion_messages(&messages)?;
    let (output_tx, output_rx) = tokio::sync::mpsc::channel(32);
    guard.send(ChatMsg::StructuredComplete {
        messages,
        options,
        turn_options,
        execute_tools,
        output_tx,
    });
    Ok(CompletionReceiver::new(output_rx, Arc::clone(guard)))
}

pub struct StructuredCompletionStream(CompletionReceiver);

impl StructuredCompletionStream {
    #[allow(
        clippy::should_implement_trait,
        reason = "Matches the async stream API; errors are separate from end-of-stream"
    )]
    pub fn next(
        &mut self,
    ) -> Result<Option<StructuredCompletionChunk>, crate::errors::CompletionError> {
        self.0.next().map(|output| {
            output.map(|output| match output {
                CompletionOutput::Token(token) => StructuredCompletionChunk::Token(token),
                CompletionOutput::Done(response) => StructuredCompletionChunk::Done(response),
            })
        })
    }
}

pub struct CompletionStreamAsync(CompletionReceiver);

impl CompletionStreamAsync {
    pub async fn next(
        &mut self,
    ) -> Result<Option<CompletionChunk>, crate::errors::CompletionError> {
        self.0.next_async().await.map(|output| {
            output.map(|output| match output {
                CompletionOutput::Token(token) => CompletionChunk::Token(token),
                CompletionOutput::Done(response) => CompletionChunk::Done(AssistantResponse {
                    content: response.content,
                    tool_calls: response.tool_calls,
                }),
            })
        })
    }
}

pub struct StructuredCompletionStreamAsync(CompletionReceiver);

impl StructuredCompletionStreamAsync {
    pub async fn next(
        &mut self,
    ) -> Result<Option<StructuredCompletionChunk>, crate::errors::CompletionError> {
        self.0.next_async().await.map(|output| {
            output.map(|output| match output {
                CompletionOutput::Token(token) => StructuredCompletionChunk::Token(token),
                CompletionOutput::Done(response) => StructuredCompletionChunk::Done(response),
            })
        })
    }
}

/// Convert a raw `WriteOutput` channel into a typed `StreamOutput<CompletionError>` channel.
///
/// `ask_channel` intentionally stays as `WriteOutput` so the Godot binding
/// (which pattern-matches on it directly) is not broken. `ask` uses this
/// forwarder to serve the generic `TokenStream`.
fn forward_write_output(
    rx: tokio::sync::mpsc::UnboundedReceiver<llm::WriteOutput>,
) -> tokio::sync::mpsc::UnboundedReceiver<crate::stream::StreamOutput<crate::errors::CompletionError>>
{
    let (tx, new_rx) = tokio::sync::mpsc::unbounded_channel();
    // Use std::thread::spawn so this is callable from non-Tokio threads (e.g. the
    // Flutter Rust Bridge sync dispatcher).  blocking_recv() is safe here because
    // this thread is not inside any async executor.
    std::thread::spawn(move || {
        let mut rx = rx;
        while let Some(output) = rx.blocking_recv() {
            let item = match output {
                llm::WriteOutput::Token(t) => crate::stream::StreamOutput::Token(t),
                llm::WriteOutput::Done(s) => crate::stream::StreamOutput::Done(s),
                llm::WriteOutput::Error(e) => crate::stream::StreamOutput::Error(
                    crate::errors::CompletionError::WorkerError(e),
                ),
            };
            if tx.send(item).is_err() {
                break;
            }
        }
    });
    new_rx
}

pub struct ChatStats {
    pub context_size: u32,
    pub context_used: u32,
}

/// Every setter answers with a result, so a rejected value is reported to the
/// caller instead of ending the worker. See [`process_worker_msg`].
type SetterReply = tokio::sync::mpsc::Sender<Result<(), SetterError>>;

enum ChatMsg {
    Ask {
        prompt: Prompt,
        output_tx: tokio::sync::mpsc::UnboundedSender<llm::WriteOutput>,
    },
    Complete {
        messages: Vec<Message>,
        options: Options,
        output_tx: tokio::sync::mpsc::UnboundedSender<llm::WriteOutput>,
    },
    StructuredComplete {
        messages: Vec<Message>,
        options: Options,
        turn_options: TurnOptions,
        execute_tools: bool,
        output_tx: tokio::sync::mpsc::Sender<ExternalCompletionOutput>,
    },
    ResetChat {
        system_prompt: Option<String>,
        tools: Vec<Tool>,
        output_tx: SetterReply,
    },
    SetTools {
        tools: Vec<Tool>,
        output_tx: SetterReply,
    },
    SetSystemPrompt {
        system_prompt: Option<String>,
        output_tx: SetterReply,
    },
    GetSystemPrompt {
        output_tx: tokio::sync::mpsc::Sender<Option<String>>,
    },
    SetThinking {
        allow_thinking: bool,
        output_tx: SetterReply,
    },
    SetTemplateVariable {
        name: String,
        value: bool,
        output_tx: SetterReply,
    },
    SetTemplateVariables {
        variables: std::collections::HashMap<String, bool>,
        output_tx: SetterReply,
    },
    GetTemplateVariables {
        output_tx: tokio::sync::mpsc::Sender<std::collections::HashMap<String, bool>>,
    },
    SetSamplerConfig {
        sampler_config: SamplerConfig,
        output_tx: SetterReply,
    },
    GetChatHistory {
        output_tx: tokio::sync::mpsc::Sender<Vec<Message>>,
    },
    GetSamplerConfig {
        output_tx: tokio::sync::mpsc::Sender<SamplerConfig>,
    },
    SetChatHistory {
        messages: Vec<Message>,
        output_tx: SetterReply,
    },
    GetStats {
        output_tx: tokio::sync::mpsc::Sender<ChatStats>,
    },
    GetMtpAcceptanceRate {
        output_tx: tokio::sync::mpsc::Sender<Option<f32>>,
    },
    Tokenize {
        prompt: Prompt,
        output_tx: tokio::sync::mpsc::Sender<Result<Vec<Option<i32>>, TokenizeError>>,
    },
}

impl std::fmt::Debug for ChatMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMsg::Ask { prompt, .. } => f.debug_struct("Ask").field("text", prompt).finish(),
            ChatMsg::Complete { messages, .. } => f
                .debug_struct("Complete")
                .field("messages", &format!("[{} messages]", messages.len()))
                .finish(),
            ChatMsg::StructuredComplete { messages, .. } => f
                .debug_struct("StructuredComplete")
                .field("messages", &format!("[{} messages]", messages.len()))
                .finish(),
            ChatMsg::ResetChat {
                system_prompt,
                tools,
                ..
            } => f
                .debug_struct("ResetChat")
                .field("system_prompt", system_prompt)
                .field("tools", &format!("[{} tools]", tools.len()))
                .finish(),
            ChatMsg::SetTools { tools, .. } => f
                .debug_struct("SetTools")
                .field("tools", &format!("[{} tools]", tools.len()))
                .finish(),
            ChatMsg::SetSystemPrompt { system_prompt, .. } => f
                .debug_struct("SetSystemPrompt")
                .field("system_prompt", system_prompt)
                .finish(),
            ChatMsg::GetSystemPrompt { .. } => f.debug_struct("GetSystemPrompt").finish(),
            ChatMsg::SetThinking { allow_thinking, .. } => f
                .debug_struct("SetThinking")
                .field("allow_thinking", allow_thinking)
                .finish(),
            ChatMsg::SetTemplateVariable { name, value, .. } => f
                .debug_struct("SetTemplateVariable")
                .field("name", name)
                .field("value", value)
                .finish(),
            ChatMsg::SetTemplateVariables { variables, .. } => f
                .debug_struct("SetTemplateVariables")
                .field("variables", &format!("[{} variables]", variables.len()))
                .finish(),
            ChatMsg::GetTemplateVariables { .. } => f.debug_struct("GetTemplateVariables").finish(),
            ChatMsg::SetSamplerConfig { sampler_config, .. } => f
                .debug_struct("SetSamplerConfig")
                .field("sampler_config", sampler_config)
                .finish(),
            ChatMsg::GetChatHistory { .. } => f.debug_struct("GetChatHistory").finish(),
            ChatMsg::SetChatHistory { messages, .. } => f
                .debug_struct("SetChatHistory")
                .field("messages", &format!("[{} messages]", messages.len()))
                .finish(),
            ChatMsg::GetSamplerConfig { .. } => f.debug_struct("GetSamplerConfig").finish(),
            ChatMsg::GetStats { .. } => f.debug_struct("GetStats").finish(),
            ChatMsg::GetMtpAcceptanceRate { .. } => f.debug_struct("GetMtpAcceptanceRate").finish(),
            ChatMsg::Tokenize { prompt, .. } => f
                .debug_struct("Tokenize")
                .field(
                    "prompt",
                    &prompt.to_string().chars().take(50).collect::<String>(),
                )
                .finish(),
        }
    }
}

/// Handle one message. Nothing here propagates: a failure is answered on the
/// message's own reply channel, so no argument a caller passes can end the
/// worker thread.
fn process_worker_msg(worker_state: &mut Chat<'_>, msg: ChatMsg) {
    info!(?msg, "Worker processing:");
    match msg {
        ChatMsg::Ask { prompt, output_tx } => {
            let should_stop = Arc::clone(&worker_state.should_stop);
            let error_tx = output_tx.clone();
            let callback = move |out| {
                if output_tx.send(out).is_err() {
                    // Receiver was dropped or the buffer is full with nobody consuming.
                    // Either way, stop generating immediately.
                    should_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            };
            if let Err(e) = worker_state.ask(prompt, callback) {
                let _ = error_tx.send(llm::WriteOutput::Error(Box::new(e)));
                // Return Ok — error is communicated through the channel, worker stays alive.
            }
        }
        ChatMsg::Complete {
            messages,
            options,
            output_tx,
        } => {
            let should_stop = Arc::clone(&worker_state.should_stop);
            let error_tx = output_tx.clone();
            let callback = move |out| {
                if output_tx.send(out).is_err() {
                    should_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            };
            if let Err(e) = worker_state.complete(messages, options, callback) {
                let _ = error_tx.send(llm::WriteOutput::Error(Box::new(e)));
            }
        }
        ChatMsg::StructuredComplete {
            messages,
            options,
            turn_options,
            execute_tools,
            output_tx,
        } => {
            let should_stop = Arc::clone(&worker_state.should_stop);
            let event_tx = output_tx.clone();
            let callback = move |out| {
                let result = match out {
                    llm::WriteOutput::Token(token) => {
                        event_tx.blocking_send(ExternalCompletionOutput::Token(token))
                    }
                    llm::WriteOutput::Done(_) => Ok(()),
                    llm::WriteOutput::Error(error) => {
                        event_tx.blocking_send(ExternalCompletionOutput::Error(
                            crate::errors::CompletionError::WorkerError(error),
                        ))
                    }
                };
                if result.is_err() {
                    should_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            };
            match worker_state.complete_once(
                messages,
                options,
                Some(turn_options),
                !execute_tools,
                callback,
            ) {
                Ok(response) => {
                    let _ = output_tx.blocking_send(ExternalCompletionOutput::Done(response));
                }
                Err(error) => {
                    let _ = output_tx.blocking_send(ExternalCompletionOutput::Error(
                        crate::errors::CompletionError::WorkerError(Box::new(error)),
                    ));
                }
            }
        }
        ChatMsg::ResetChat {
            system_prompt,
            tools,
            output_tx,
        } => {
            let _ = output_tx.blocking_send(worker_state.reset_chat(system_prompt, tools));
        }
        ChatMsg::SetTools { tools, output_tx } => {
            let _ = output_tx.blocking_send(worker_state.set_tools(tools));
        }
        ChatMsg::SetSystemPrompt {
            system_prompt,
            output_tx,
        } => {
            worker_state.set_system_prompt(system_prompt);
            let _ = output_tx.blocking_send(Ok(()));
        }
        ChatMsg::GetSystemPrompt { output_tx } => {
            let system_prompt = worker_state.get_system_prompt();
            let _ = output_tx.blocking_send(system_prompt);
        }
        ChatMsg::SetThinking {
            allow_thinking,
            output_tx,
        } => {
            worker_state.set_template_variable("enable_thinking".to_string(), allow_thinking);
            let _ = output_tx.blocking_send(Ok(()));
        }
        ChatMsg::SetTemplateVariable {
            name,
            value,
            output_tx,
        } => {
            worker_state.set_template_variable(name, value);
            let _ = output_tx.blocking_send(Ok(()));
        }
        ChatMsg::SetTemplateVariables {
            variables,
            output_tx,
        } => {
            worker_state.set_template_variables(variables);
            let _ = output_tx.blocking_send(Ok(()));
        }
        ChatMsg::GetTemplateVariables { output_tx } => {
            let vars = worker_state.get_template_variables();
            let _ = output_tx.blocking_send(vars);
        }
        ChatMsg::SetSamplerConfig {
            sampler_config,
            output_tx,
        } => {
            let _ = output_tx.blocking_send(worker_state.set_sampler_config(sampler_config));
        }
        ChatMsg::GetChatHistory { output_tx } => {
            let msgs = worker_state.get_chat_history();
            let _ = output_tx.blocking_send(msgs);
        }
        ChatMsg::SetChatHistory {
            messages,
            output_tx,
        } => {
            let _ = output_tx.blocking_send(
                worker_state
                    .set_chat_history(messages)
                    .map_err(SetterError::from),
            );
        }
        ChatMsg::GetSamplerConfig { output_tx } => {
            let sampler_config = worker_state.get_sampler_config();
            let _ = output_tx.blocking_send(sampler_config);
        }
        ChatMsg::GetStats { output_tx } => {
            let stats = ChatStats {
                context_size: worker_state.engine.ctx.n_ctx(),
                context_used: worker_state.engine.n_past(),
            };
            let _ = output_tx.blocking_send(stats);
        }
        ChatMsg::GetMtpAcceptanceRate { output_tx } => {
            let proposed = worker_state.engine.mtp_drafts_proposed;
            let rate = if proposed > 0 {
                Some(worker_state.engine.mtp_drafts_accepted as f32 / proposed as f32)
            } else {
                None
            };
            let _ = output_tx.blocking_send(rate);
        }
        ChatMsg::Tokenize { prompt, output_tx } => {
            let result = worker_state.tokenize(prompt);
            let _ = output_tx.blocking_send(result);
        }
    }
}

// TOOLS TYPE STUFF

// the callback closure isn't normally Send
// but we just cheat a little here
// so far it has been fine...
// unsafe impl Send for Tool {}

// TOOL CHAT WORKER

struct ChatContext {
    /// Here we keep the current tokens + media embeddings, which are in the KV cache.
    chunks: TokenizerChunks,
    /// Here we keep a list of the media bitmaps, which are needed for tokenization.
    bitmaps: IndexMap<ChunkId, MtmdBitmap>,
}

impl ChatContext {
    fn new() -> Self {
        Self {
            chunks: TokenizerChunks::new(),
            bitmaps: IndexMap::new(),
        }
    }

    pub fn add_bitmaps(
        &mut self,
        bitmaps: Vec<MtmdBitmap>,
    ) -> Result<Vec<String>, MultimodalError> {
        let mut bitmap_ids = Vec::with_capacity(bitmaps.len());
        for bitmap in bitmaps {
            let id = self.create_bitmap_id(&bitmap);
            bitmap.set_id(&id)?;
            bitmap_ids.push(id.clone());
            self.bitmaps.entry(id).or_insert(bitmap);
        }
        Ok(bitmap_ids)
    }

    /// Whether this worker has the bitmap an id names. Ids are worker-local, so
    /// content from elsewhere carries ids this answers `false` for.
    fn has_bitmap(&self, id: &str) -> bool {
        self.bitmaps.contains_key(id)
    }

    pub fn garbage_collect_bitmaps(&mut self, messages: &[Message]) {
        // Garbage collection for the bitmaps.
        let referenced_bitmaps: HashSet<String> = messages
            .iter()
            .flat_map(|msg| msg.media_ids())
            .map(str::to_string)
            .collect();

        let unreferenced_bitmap_ids: Vec<_> = self
            .bitmaps
            .keys()
            .filter(|id| !referenced_bitmaps.contains(id.as_str()))
            .cloned()
            .collect();

        self.remove_bitmaps(unreferenced_bitmap_ids);
    }

    fn create_bitmap_id(&self, bitmap: &MtmdBitmap) -> String {
        let mut hasher = AHasher::default();
        hasher.write(bitmap.data());
        hasher.finish().to_string()
    }

    fn remove_bitmaps(&mut self, bitmap_ids: Vec<String>) {
        for id in bitmap_ids {
            if let Some(bitmap) = self.bitmaps.shift_remove(&id) {
                drop(bitmap);
            }
        }
    }
}

/// Builds the tool-call grammar sampler for an already-detected `tool_format`
/// (detection happens once, in `new_chat_worker` — see the `tool_format`
/// field doc), along with the begin-token sequence that triggers the switch to
/// it. `Ok(None)` if `tools` is empty. `Err(DetectionFailed)` if tools are
/// requested but no format was ever detected for this model.
///
/// `grammar_factory` carries the llguidance state between calls: the ~400ms init
/// is paid on the first build and every later one only compiles the grammar.
fn build_tool_sampler(
    model: &llama_cpp_2::model::LlamaModel,
    grammar_factory: &mut Option<GrammarFactory>,
    tools: &[Tool],
    sampler_config: &SamplerConfig,
    tool_format: Option<&ToolFormat>,
) -> Result<Option<(LlamaSampler, Vec<LlamaToken>)>, ToolCallingSetupError> {
    if tools.is_empty() {
        return Ok(None);
    }

    let tool_format = tool_format.ok_or(ToolFormatError::DetectionFailed)?;

    let lark = tool_format.to_lark(tools, Some(model))?;
    debug!(grammar = %lark, "Generated tool calling grammar (Lark)");

    // A chat's slice set never changes in practice (`tool_format` is detected once
    // and `slice_regexes` is a constant per format), so this builds at most once.
    let slices = tool_format.slice_regexes();
    let rebuilt = GrammarFactory::build_if_stale(grammar_factory.as_ref(), model, slices)?;
    let factory = rebuilt
        .as_ref()
        .or(grammar_factory.as_ref())
        .expect("`build_if_stale` returns None only when the held factory serves us");

    let grammar_step = factory.grammar_step("lark", &lark)?;
    let tool_sampler =
        sampler_config.build_sampler_with_prepended_step(model, Some(grammar_step))?;

    let begin_tokens =
        model.str_to_token(tool_format.begin_token(), llama_cpp_2::model::AddBos::Never)?;

    // Every fallible function has run, so the rebuilt factory can be committed.
    if rebuilt.is_some() {
        *grammar_factory = rebuilt;
    }

    Ok(Some((tool_sampler, begin_tokens)))
}

/// The samplers a chat response can draw from: `base` for free generation,
/// `tool` (grammar-constrained) once the model emits the tool-call begin token.
/// The switch is driven token-by-token via [`ChatSampler::observe`].
pub(crate) struct ChatSampler {
    base: LlamaSampler,
    tool: Option<LlamaSampler>,
    /// Sequence whose completion switches to `tool`. Empty when `tool` is None.
    begin_tokens: Vec<LlamaToken>,
    /// How many leading `begin_tokens` the emitted stream has matched so far.
    begin_match_len: usize,
    grammar_activated: bool,
}

impl ChatSampler {
    fn new(base: LlamaSampler, tool: Option<(LlamaSampler, Vec<LlamaToken>)>) -> Self {
        let mut sampler = Self {
            base,
            tool: None,
            begin_tokens: Vec::new(),
            begin_match_len: 0,
            grammar_activated: false,
        };
        sampler.set_tool(tool);
        sampler
    }

    /// The sampler that should produce the next token.
    pub(crate) fn active(&mut self) -> &mut LlamaSampler {
        if self.grammar_activated {
            self.tool
                .as_mut()
                .expect("tool sampler must exist once the grammar is activated")
        } else {
            &mut self.base
        }
    }

    /// Feed an emitted token back in so the begin sequence can be tracked, and
    /// switch to the tool sampler once it completes. Must be called on each
    /// emitted token, in order, before its successor is sampled. Returns whether
    /// the switch just happened.
    pub(crate) fn observe(&mut self, token: LlamaToken) -> bool {
        if self.grammar_activated || self.begin_tokens.is_empty() {
            return false;
        }

        // Rolling match: extend on a hit, else restart from this token.
        self.begin_match_len = if token == self.begin_tokens[self.begin_match_len] {
            self.begin_match_len + 1
        } else if token == self.begin_tokens[0] {
            1
        } else {
            0
        };

        if self.begin_match_len < self.begin_tokens.len() {
            return false;
        }
        self.begin_match_len = 0;

        // Fast-forward the grammar matcher past the begin tokens.
        let ts = self
            .tool
            .as_mut()
            .expect("begin_tokens is non-empty only when a tool sampler exists");
        ts.accept_many(self.begin_tokens.iter());
        self.grammar_activated = true;
        true
    }

    fn set_base(&mut self, base: LlamaSampler) {
        self.base = base;
    }

    fn set_tool(&mut self, tool: Option<(LlamaSampler, Vec<LlamaToken>)>) {
        match tool {
            Some((sampler, begin_tokens)) => {
                self.tool = Some(sampler);
                self.begin_tokens = begin_tokens;
            }
            None => {
                self.tool = None;
                self.begin_tokens = Vec::new();
            }
        }
        self.begin_match_len = 0;
    }

    /// Reset per-response state (RNG, penalty/DRY history, grammar matcher) and
    /// return to free generation.
    fn reset(&mut self) {
        self.base.reset();
        if let Some(ts) = self.tool.as_mut() {
            ts.reset();
        }
        self.grammar_activated = false;
        self.begin_match_len = 0;
    }
}

struct GeneratedResponse {
    content: String,
    prompt_tokens: usize,
    completion_tokens: usize,
    hit_token_limit: bool,
}

/// A chat session: owns an [`InferenceEngine`] plus all the conversational state
/// (messages, tools, template, sampler config).
struct Chat<'a> {
    engine: InferenceEngine<'a>,
    should_stop: Arc<AtomicBool>,
    tool_format: Option<ToolFormat>,
    sampler: ChatSampler,
    grammar_factory: Option<GrammarFactory>,
    sampler_config: SamplerConfig,
    messages: Vec<Message>,
    system_prompt: Option<String>,
    template_variables: std::collections::HashMap<String, bool>,
    tools: Vec<Tool>,
    chat_template: ChatTemplate,
    context: ChatContext,
}

impl<'a> Chat<'a> {
    fn new_chat_worker(
        model: &'a llm::Model,
        config: ChatConfig,
        should_stop: Arc<AtomicBool>,
    ) -> Result<Chat<'a>, InitWorkerError> {
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

        // Pre-build the base sampler for `sampler_config`, reused (via
        // `ChatSampler`) for every response.
        let base_sampler = sampler_config.build_sampler(&model.language_model)?;

        // Depends only on the model's chat template, not on `config.tools`,
        // so detect it once here regardless (cheap — see `tool_format` field
        // doc). Only a hard error if tools were actually requested.
        let tool_format = match detect_tool_format(&model.language_model) {
            Ok(format) => {
                debug!(?format, "Detected tool calling format");
                Some(format)
            }
            Err(e) if config.tools.is_empty() => {
                debug!(error = %e, "Failed to detect tool calling format");
                None
            }
            Err(e) => return Err(InitWorkerError::ToolCallingSetup(e.into())),
        };

        let mut grammar_factory = None;
        let tool_sampler = build_tool_sampler(
            &model.language_model,
            &mut grammar_factory,
            &config.tools,
            &sampler_config,
            tool_format.as_ref(),
        )?;

        // Build the low-level inference engine via the shared Worker constructor,
        // then take ownership of just the engine for the chat session.
        let Worker { engine, extra: () } =
            Worker::new_with_type(model, config.n_ctx, false, config.mtp, config.n_threads, ())?;

        Ok(Chat {
            engine,
            should_stop,
            tool_format,
            sampler: ChatSampler::new(base_sampler, tool_sampler),
            grammar_factory,
            sampler_config,
            messages: vec![],
            system_prompt: config.system_prompt,
            chat_template: template,
            template_variables: config.template_variables,
            tools: config.tools,
            context: ChatContext::new(),
        })
    }

    fn should_stop(&self) -> bool {
        self.should_stop.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.messages.push(Message::new_assistant(content));
    }

    pub fn add_user_message(&mut self, content: impl Into<MessageContent>) {
        self.messages.push(Message::User {
            content: content.into(),
        });
    }

    pub fn add_tool_calls(&mut self, tool_calls: Vec<ToolCall>) {
        self.messages.push(Message::Assistant {
            content: "".into(),
            tool_calls: Some(tool_calls),
        });
    }

    pub fn add_tool_resp(&mut self, name: String, content: String) {
        self.messages.push(Message::new_tool(name, content));
    }

    /// Compare tokens from a template-rendered chat history with the tokens in the LLM's context,
    /// and perform the LLM 'reading' to make the LLM's context match the rendered tokens exactly.
    /// Because this invokes the model, this is potentially an expensive method to call.
    #[tracing::instrument(level = "debug", skip_all)]
    fn sync_context_with_render(
        &mut self,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<(), ContextSyncError> {
        let mut chunks = self.render_as_chunks(&self.messages, true)?;
        if chunks.n_tokens() > self.engine.ctx.n_ctx() as usize {
            self.context_shift()?;
            chunks = self.render_as_chunks(&self.messages, true)?;
        }

        // We should never try to sync with an empty render
        debug_assert!(!chunks.is_empty());

        // Diff against the chunks currently in the KV cache and load only the new tail.
        let prev = std::mem::take(&mut self.context.chunks);
        let new_chunks = self
            .engine
            .sync_context(chunks, &prev, inference_lock_token)?;
        self.context.chunks = new_chunks;
        self.context.garbage_collect_bitmaps(&self.messages);

        Ok(())
    }

    /// Drop whole turns from the middle of the history until the render fits
    /// `n_ctx / 2`. A turn starts at a user message and runs until just before
    /// the next one; the first turn, the last [`PRESERVED_RECENT_TURNS`] turns,
    /// and any messages preceding the first user message are always kept.
    ///
    /// With three or fewer turns there is nothing deletable, so the history
    /// comes back as-is even if it is still too large.
    fn context_shift(&mut self) -> Result<(), ShiftError> {
        info!("Context shift happens!");
        let target_token_size = (self.engine.ctx.n_ctx() / 2) as usize;
        let mut messages = self.messages.clone();

        match user_message_indices(&self.messages).len() {
            0 => return Err(ShiftError::NoUserMessages),
            1 => return Err(ShiftError::TooFewMessages),
            _ => {}
        }

        // Delete messages until context is small enough or only essential messages are left.
        // Double the number of messages to delete each iteration. This is a simple and kind of stupid solution, as it might overshoot by a lot.
        // Plenty of optimization options here.
        let mut turns_to_delete = 1;

        loop {
            let turn_starts = user_message_indices(&messages);
            // Everything between the first turn and the preserved recent ones.
            // Zero means there is nothing left this may take.
            let deletable = turn_starts.len().saturating_sub(PRESERVED_RECENT_TURNS + 1);
            if deletable == 0 {
                break;
            }
            if self.render_as_chunks(&messages, false)?.n_tokens() <= target_token_size {
                break;
            }

            // 1 <= n <= turn_starts.len() - 3, so the drain is never empty and
            // never reaches the preserved turns.
            let n = min(turns_to_delete, deletable);
            messages.drain(turn_starts[1]..turn_starts[1 + n]);
            turns_to_delete = turns_to_delete.saturating_mul(2);
        }

        self.messages = messages;
        Ok(())
    }

    // ---------- IMPORTANT ----------
    // Should only be used under a global inference lock
    // This is a safety meassure to prevent bugs from multiple
    // contexts with the same model. It might not be necessary
    // but assume it is.
    fn generate_response_until_done_with_limit<F>(
        &mut self,
        mut respond: F,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
        max_tokens: Option<usize>,
    ) -> Result<(usize, bool), GenerateResponseError>
    where
        F: FnMut(WriteOutput),
    {
        // Token generation loop
        info!("Worker writing until done");

        self.engine.reset_mtp_stats();

        // pre-allocating 4096 bytes for the response string
        // 4096 is a very randomly chosen number. how does this affect performance?
        let mut full_response: String = String::with_capacity(4096);
        let mut tokens_written_until_now = Vec::new();
        let mut new_tokens = Vec::new();
        let mut generated_tokens = 0;
        let mut hit_token_limit = max_tokens == Some(0);

        self.sampler.reset();

        // init statefull decoder for split up tokens like emojis
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        while !self.should_stop()
            && max_tokens.is_none_or(|max_tokens| generated_tokens < max_tokens)
        {
            // Check if the context is full
            if self.engine.is_context_full() {
                // pending should be preserved during context shift
                let deferred_pending = self.engine.take_pending();
                self.context_shift()?;
                self.sync_context_with_render(inference_lock_token)?;
                if !tokens_written_until_now.is_empty() {
                    let mut generated_chunks = TokenizerChunks::new();
                    generated_chunks
                        .append(TokenizerChunk::new_text(tokens_written_until_now.clone()));
                    self.engine
                        .read_chunks(generated_chunks, inference_lock_token)?;
                }
                self.engine.restore_pending(deferred_pending);
                // do not update tokens_in_context as this is done later by ask
            }

            // Sample next token(s), no need to use sampler.accept as sample already accepts the token.
            // using sampler.accept() will cause the sampler to crash when using grammar sampling.
            // https://github.com/utilityai/llama-cpp-rs/issues/604
            let remaining_tokens =
                max_tokens.map(|max_tokens| max_tokens.saturating_sub(generated_tokens));
            self.engine.sample_and_decode_next_tokens(
                &mut self.sampler,
                &mut new_tokens,
                remaining_tokens,
            )?;

            tokens_written_until_now.extend_from_slice(&new_tokens);

            let mut should_finish = false;
            for &new_token in &new_tokens {
                // Attempt to convert token(s) to bytes
                let token_bytes = match self
                    .engine
                    .ctx
                    .model
                    .token_to_piece_bytes(new_token, 64, true, None)
                {
                    Err(llama_cpp_2::TokenToStringError::InsufficientBufferSpace(i)) => {
                        self.engine.ctx.model.token_to_piece_bytes(
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

                // HACK (gemma4): some gemma4 models emit token id 1 (which renders as the
                // literal "<eos>") as a stop token after tool calls. llama.cpp's `is_eog_token`
                // does not flag it, which causes a runaway generation loop, so match it
                // explicitly. vllm handles the same case:
                // https://docs.vllm.ai/en/stable/api/vllm/model_executor/models/gemma4_utils/#vllm.model_executor.models.gemma4_utils.has_tool_response_tag
                let gemma4_eog_hotfix = token_str == "<eos>" && new_token == LlamaToken::new(1);

                let has_eog = self.engine.ctx.model.is_eog_token(new_token) || gemma4_eog_hotfix;
                trace!(?new_token, ?token_str, ?has_eog);

                if has_eog {
                    should_finish = true;
                    break;
                }

                full_response.push_str(&token_str);
                generated_tokens += 1;
                trace!(?token_str, "Sending out token:");
                respond(WriteOutput::Token(token_str));

                if max_tokens.is_some_and(|max_tokens| generated_tokens >= max_tokens) {
                    hit_token_limit = true;
                    should_finish = true;
                    break;
                }
            }

            if should_finish {
                break;
            }
        }

        // we're done!
        debug!(%full_response, "Sending out");
        respond(WriteOutput::Done(full_response));
        Ok((generated_tokens, hit_token_limit))
    }

    pub fn ask<F>(&mut self, prompt: Prompt, respond: F) -> Result<&mut Self, SayError>
    where
        F: Fn(llm::WriteOutput) + Clone,
    {
        // reset the stop flag
        self.should_stop
            .store(false, std::sync::atomic::Ordering::Relaxed);

        // A prompt is message content, so it becomes the user turn as-is —
        // interleaving and all. Flattening to `<__media__>` markers happens at
        // render time.
        let mut content = prompt;
        self.register_media(&mut content)?;
        self.add_user_message(content);

        self.run_turn(respond, None, false)?;

        Ok(self)
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
                if part.id().is_some_and(|id| self.context.has_bitmap(id)) {
                    return Ok(None);
                }
                match part {
                    ContentPart::Image { path, .. } => self.engine.load_image(path).map(Some),
                    ContentPart::Audio { path, .. } => self.engine.load_audio(path).map(Some),
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

        let bitmap_ids = self.context.add_bitmaps(loaded)?;
        let mut parts = content.media_parts_mut();
        for (position, id) in positions.into_iter().zip(bitmap_ids) {
            parts[position].set_id(id);
        }
        Ok(())
    }

    /// Generate assistant output from the current messages.
    ///
    /// Tool callbacks run until the model stops calling tools unless
    /// `skip_calling_tools` is set, in which case the first calls are returned.
    fn run_turn<F>(
        &mut self,
        respond: F,
        max_tokens: Option<usize>,
        skip_calling_tools: bool,
    ) -> Result<CompletionResponse, SayError>
    where
        F: Fn(llm::WriteOutput) + Clone,
    {
        // The tool-call grammar is NOT pre-injected into the chain. Lark/
        // llguidance has no "trigger word" mechanism, so an always-on grammar
        // would block EOS when the model just wants to chat. Instead the
        // grammar is added dynamically inside `generate_response_until_done_with_limit`
        // the moment the begin token appears in the streamed output.

        let mut generated =
            self.wrapped_update_context_and_generate_response(respond.clone(), max_tokens)?;
        let mut usage = CompletionUsage {
            prompt_tokens: generated.prompt_tokens,
            completion_tokens: generated.completion_tokens,
        };
        let mut hit_token_limit = generated.hit_token_limit;
        let mut response = generated.content;

        if let Some(tool_format) = self.tool_format.clone() {
            while let Some(tool_calls) = tool_format.extract_tool_calls(&response) {
                debug!(?tool_calls, "Got tool calls:");
                self.add_tool_calls(tool_calls.clone());

                if skip_calling_tools {
                    let content = response
                        .split_once(tool_format.begin_token())
                        .map(|(content, _)| content.to_string())
                        .unwrap_or_default();
                    self.context.chunks = self.render_as_chunks(&self.messages, true)?;
                    return Ok(CompletionResponse {
                        content,
                        tool_calls,
                        finish_reason: FinishReason::ToolCalls,
                        usage,
                    });
                }

                for tool_call in tool_calls {
                    let Some(tool) = self.tools.iter().find(|t| t.name == tool_call.name) else {
                        error!(
                            tool_name = tool_call.name,
                            "Model triggered tool call for invalid tool name:",
                        );
                        let errmsg = format!("ERROR - Invalid tool name: {}", tool_call.name);
                        self.add_tool_resp(tool_call.name, errmsg);
                        continue;
                    };

                    let response = (tool.function)(tool_call.arguments);
                    debug!(%tool_call.name, %response, "Tool call result:");

                    self.add_tool_resp(tool_call.name, response);
                }

                let remaining_tokens =
                    max_tokens.map(|limit| limit.saturating_sub(usage.completion_tokens));
                generated = self.wrapped_update_context_and_generate_response(
                    respond.clone(),
                    remaining_tokens,
                )?;
                usage.prompt_tokens += generated.prompt_tokens;
                usage.completion_tokens += generated.completion_tokens;
                hit_token_limit = generated.hit_token_limit;
                response = generated.content;
            }
        }

        debug_assert!(self
            .tool_format
            .as_ref()
            .is_none_or(|fmt| !response.contains(fmt.begin_token())));
        self.add_assistant_message(response.clone());
        self.context.chunks = self.render_as_chunks(&self.messages, true)?;

        Ok(CompletionResponse {
            content: response,
            tool_calls: Vec::new(),
            finish_reason: if hit_token_limit {
                FinishReason::Length
            } else {
                FinishReason::Stop
            },
            usage,
        })
    }

    /// Answer a full message list, which replaces the chat history.
    ///
    /// A leading system message sets the system prompt; without one the current
    /// system prompt is kept. `options` follows the same rule for the settings
    /// it carries. The rest of `messages` becomes the history, and the turn's
    /// output is appended as usual, so a following [`ask`](Self::ask) continues
    /// that conversation.
    pub fn complete<F>(
        &mut self,
        messages: Vec<Message>,
        options: Options,
        respond: F,
    ) -> Result<&mut Self, CompleteError>
    where
        F: Fn(llm::WriteOutput) + Clone,
    {
        self.complete_once(messages, options, None, false, respond)?;
        Ok(self)
    }

    fn complete_once<F>(
        &mut self,
        mut messages: Vec<Message>,
        options: Options,
        turn_options: Option<TurnOptions>,
        skip_calling_tools: bool,
        respond: F,
    ) -> Result<CompletionResponse, CompleteError>
    where
        F: Fn(llm::WriteOutput) + Clone,
    {
        validate_completion_messages(&messages)?;
        self.should_stop
            .store(false, std::sync::atomic::Ordering::Relaxed);
        self.reload_media(&mut messages)?;

        let has_turn_options = turn_options.is_some();
        let TurnOptions {
            temperature,
            top_p,
            seed,
            max_tokens,
        } = turn_options.unwrap_or_default();
        let has_overrides = temperature.is_some() || top_p.is_some() || seed.is_some();
        let temporary_sampler = if has_overrides {
            let config = options
                .sampler
                .as_ref()
                .unwrap_or(&self.sampler_config)
                .clone()
                .with_sampling_overrides(temperature, top_p, seed)
                .map_err(|error| CompleteError::Options(error.to_string()))?;
            let model = self.engine.ctx.model;
            let tools = options.tools.as_deref().unwrap_or(&self.tools);
            let base_sampler = config
                .build_sampler(model)
                .map_err(|error| CompleteError::Options(error.to_string()))?;
            let tool_sampler = build_tool_sampler(
                model,
                &mut self.grammar_factory,
                tools,
                &config,
                self.tool_format.as_ref(),
            )
            .map_err(|error| CompleteError::Options(error.to_string()))?;
            Some(ChatSampler::new(base_sampler, tool_sampler))
        } else {
            None
        };
        self.apply_options(options)
            .map_err(|error| CompleteError::Options(error.to_string()))?;

        self.hoist_system_message(&mut messages)?;
        self.messages = messages;
        let max_tokens =
            has_turn_options.then(|| max_tokens.unwrap_or(self.engine.ctx.n_ctx() as usize));
        let persistent_sampler =
            temporary_sampler.map(|sampler| std::mem::replace(&mut self.sampler, sampler));
        let result = self.run_turn(respond, max_tokens, skip_calling_tools);
        if let Some(sampler) = persistent_sampler {
            self.sampler = sampler;
        }
        Ok(result?)
    }

    /// Re-read the media files referenced by `messages` and relink the parts to
    /// the freshly registered bitmaps. Covers every role but system, whose
    /// content is flattened to plain text by `hoist_system_message`.
    ///
    /// A bitmap id identifies a bitmap within one worker, so a message that came
    /// from another session — a saved conversation, say — carries ids this worker
    /// knows nothing about. The path is the part that keeps its meaning, so the
    /// bitmap is loaded from it again and the part is pointed at the new id.
    ///
    /// Media already registered here is not re-read — see [`register_media`](Self::register_media).
    /// Handing the same conversation back therefore does not re-decode its
    /// images, at the cost of not noticing a file that changed on disk since.
    ///
    /// Runs before the history is replaced, so an unreadable file leaves the
    /// conversation as it was. Bitmaps registered before that point stay in the
    /// context unreferenced until the next `garbage_collect_bitmaps` clears them,
    /// which is the same path any replaced history takes.
    fn reload_media(&mut self, messages: &mut [Message]) -> Result<(), MultimodalError> {
        for (index, message) in messages.iter_mut().enumerate() {
            // Only a leading system message is legal, and it is about to become
            // the plain-text system prompt, so it cannot hold media.
            if index == 0 && message.is_system() {
                continue;
            }
            self.register_media(message.content_mut())?;
        }
        Ok(())
    }

    /// `messages` as the chat template expects to see them: the system prompt,
    /// which we hold as a setting rather than a turn, put back at index 0.
    fn with_system_prompt(&self, messages: &[Message]) -> Vec<Message> {
        self.system_prompt
            .iter()
            .map(|content| Message::new_system(content.clone()))
            .chain(messages.iter().cloned())
            .collect()
    }

    /// Go for the unhandled mode when you are context shifting.
    /// That is for avoiding the render will concat system message with the first user message.
    /// Otherwise please handle stuff.
    fn render_as_chunks(
        &self,
        messages: &[Message],
        handled: bool,
    ) -> Result<TokenizerChunks, RenderError> {
        // Callers pass the conversation they want rendered — which may be a
        // shortened one, during a context shift. The system prompt is not part
        // of that, so it is added here.
        let messages = &self.with_system_prompt(messages);
        let template_context = ChatTemplateContext::new(
            self.template_variables.clone(),
            if self.tools.is_empty() {
                None
            } else {
                Some(self.tools.clone())
            },
        );

        let rendered_chat = if handled {
            self.chat_template.render(messages, &template_context)?
        } else {
            self.chat_template
                .render_unhandled(messages, &template_context)?
        };

        let bitmaps: Vec<&MtmdBitmap> = messages
            .iter()
            .flat_map(|msg| msg.media_ids())
            .filter_map(|id| self.context.bitmaps.get(id))
            .collect();
        Ok(self.engine.tokenize(rendered_chat, bitmaps)?)
    }

    fn wrapped_update_context_and_generate_response<F>(
        &mut self,
        respond: F,
        max_tokens: Option<usize>,
    ) -> Result<GeneratedResponse, WrappedResponseError>
    where
        F: Fn(llm::WriteOutput) + Clone,
    {
        let inference_lock_token = acquire_inference_lock();
        self.sync_context_with_render(&inference_lock_token)?;

        let tool_call_begin_token = self
            .tool_format
            .as_ref()
            .map(|format| format.begin_token().to_string());
        let (wrapped_respond, resp_receiver) =
            crate::inference::wrap_respond(respond, tool_call_begin_token);

        let prompt_tokens = self.context.chunks.n_tokens();
        let available_tokens = (self.engine.ctx.n_ctx() as usize).saturating_sub(prompt_tokens);
        let generation_limit = max_tokens.map(|limit| min(limit, available_tokens));
        let (completion_tokens, hit_token_limit) = self.generate_response_until_done_with_limit(
            wrapped_respond,
            &inference_lock_token,
            generation_limit,
        )?;

        Ok(GeneratedResponse {
            content: resp_receiver.recv()?,
            prompt_tokens,
            completion_tokens,
            hit_token_limit,
        })
    }

    pub fn reset_chat(
        &mut self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
    ) -> Result<(), SetterError> {
        // Run fallible functions before committing to state.
        let tool_sampler = build_tool_sampler(
            self.engine.ctx.model,
            &mut self.grammar_factory,
            &tools,
            &self.sampler_config,
            self.tool_format.as_ref(),
        )?;

        self.engine.reset_context();
        self.sampler.set_tool(tool_sampler);
        self.tools = tools;
        self.messages = Vec::new();
        self.system_prompt = system_prompt;
        self.context = ChatContext::new();
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

    /// Get all template variables.
    pub fn get_template_variables(&self) -> std::collections::HashMap<String, bool> {
        self.template_variables.clone()
    }

    pub fn set_sampler_config(&mut self, sampler_config: SamplerConfig) -> Result<(), SetterError> {
        self.apply_sampler_and_tools(Some(sampler_config), None)
    }

    /// Apply a turn's [`Options`]. The sampler and tools are applied together, so
    /// setting both builds the tool grammar once, and a failure in either leaves
    /// both alone. Template variables cannot fail.
    fn apply_options(&mut self, options: Options) -> Result<(), ChatWorkerError> {
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

        let model = self.engine.ctx.model;
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
            self.tool_format.as_ref(),
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
            self.sampler.set_base(base_sampler);
        }
        self.sampler.set_tool(tool_sampler);
        if let Some(tools) = tools {
            self.tools = tools;
        }
        if let Some(chat_template) = chat_template {
            self.chat_template = chat_template;
        }
        Ok(())
    }

    pub fn set_system_prompt(&mut self, system_prompt: Option<String>) {
        self.system_prompt = system_prompt;
    }

    pub fn get_system_prompt(&self) -> Option<String> {
        self.system_prompt.clone()
    }

    pub fn set_tools(&mut self, tools: Vec<Tool>) -> Result<(), SetterError> {
        self.apply_sampler_and_tools(None, Some(tools))
    }

    /// Take a leading system message out of `messages` and make it the system
    /// prompt, so that an OpenAI-shaped array can be passed in as-is.
    ///
    /// Fails if a system message appears anywhere else, since only the leading
    /// one has somewhere to go, or if it carries media, since the prompt is
    /// stored as plain text.
    fn hoist_system_message(
        &mut self,
        messages: &mut Vec<Message>,
    ) -> Result<(), InvalidHistoryError> {
        if let Some(index) = messages.iter().skip(1).position(Message::is_system) {
            return Err(InvalidHistoryError::MisplacedSystemMessage { index: index + 1 });
        }
        let Some(Message::System { content }) = messages.first() else {
            return Ok(());
        };
        if !content.media_parts().is_empty() {
            return Err(InvalidHistoryError::MediaInSystemMessage);
        }
        self.system_prompt = Some(content.to_string());
        messages.remove(0);
        Ok(())
    }

    /// Media is re-registered from the part paths, since bitmap ids issued by
    /// another worker mean nothing here. Nothing is committed unless both that
    /// and the hoist succeed.
    pub fn set_chat_history(&mut self, mut messages: Vec<Message>) -> Result<(), ContextSyncError> {
        self.reload_media(&mut messages)?;
        self.hoist_system_message(&mut messages)?;
        self.messages = messages;

        // We used to call sync_context_with_render here but this can
        // crash as some chat templates will attempt to access fields on
        // messages[0], which will result in an error. So now we never
        // sync with an empty render and we only render when there are
        // messages present in the history.

        self.context.garbage_collect_bitmaps(&self.messages);

        Ok(())
    }

    pub fn get_chat_history(&self) -> Vec<Message> {
        self.messages.clone()
    }

    pub fn get_sampler_config(&self) -> SamplerConfig {
        self.sampler_config.clone()
    }

    pub fn tokenize(&mut self, prompt: Prompt) -> Result<Vec<Option<i32>>, TokenizeError> {
        let bitmaps = prompt
            .media_parts()
            .into_iter()
            .map(|part| match part {
                ContentPart::Image { path, .. } => self.engine.load_image(path),
                ContentPart::Audio { path, .. } => self.engine.load_audio(path),
                ContentPart::Text { .. } => unreachable!("media_parts filters out text"),
            })
            .collect::<Result<Vec<MtmdBitmap>, MultimodalError>>()?;

        let bitmap_refs: Vec<&MtmdBitmap> = bitmaps.iter().collect();
        let chunks = self.engine.tokenize(prompt.to_string(), bitmap_refs)?;
        Ok(chunks.to_token_ids())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampler::SamplerPresets;
    use crate::test_utils;

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
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 1024,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| {
            if let llm::WriteOutput::Done(resp) = x {
                sender.send(resp).unwrap();
            }
        };

        worker.ask("What is the capital of Denmark?".into(), f.clone())?;

        let resp = receiver.recv()?;
        println!("{}", resp);

        assert!(resp.contains("Copenhagen"));

        worker.ask("What language do they speak there?".into(), f)?;
        let resp = receiver.recv()?;
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
        // test_utils::init_test_tracing();
        let (Some(target_path), Some(draft_path)) = (
            test_utils::test_mtp_target_model_path(),
            test_utils::test_mtp_draft_model_path(),
        ) else {
            eprintln!(
                "skipping test_mtp_gemma4_smoke: \
                 set TEST_MTP_TARGET_MODEL and TEST_MTP_DRAFT_MODEL to enable"
            );
            return Ok(());
        };
        if !std::path::Path::new(&target_path).exists()
            || !std::path::Path::new(&draft_path).exists()
        {
            eprintln!(
                "skipping test_mtp_gemma4_smoke: file missing at {} or {}",
                target_path, draft_path
            );
            return Ok(());
        }

        let model = Arc::new(crate::llm::get_model(
            &target_path,
            true,
            None,
            Some(&draft_path),
            None,
        )?);

        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 1024,
                mtp: Some(MtpConfig::default()),
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| {
            if let llm::WriteOutput::Done(resp) = x {
                sender.send(resp).unwrap();
            }
        };

        worker.ask("What is the capital of Denmark?".into(), f)?;
        let resp = receiver.recv()?;
        println!("MTP response: {}", resp);
        assert!(resp.contains("Copenhagen"));

        Ok(())
    }

    #[test]
    fn test_reset_chat() -> Result<(), Box<dyn std::error::Error>> {
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                system_prompt: Some("You're a dog. End all responses with 'woof'".into()),
                ..ChatConfig::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;

        // just a hack to get a channel back
        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| {
            if let llm::WriteOutput::Done(resp) = x {
                sender.send(resp).unwrap();
            }
        };

        // do it once
        worker.ask("What is the capital of Denmark?".into(), f.clone())?;
        let resp1 = receiver.recv()?;
        println!("{}", resp1);
        assert!(resp1.to_lowercase().contains("woof"));

        // reset
        let _ = worker.reset_chat(
            Some("You're a cat. End all responses with 'meow'".into()),
            vec![],
        );

        // do it again
        worker.ask("What is the capital of Denmark?".into(), f.clone())?;
        let resp2 = receiver.recv()?;
        println!("{}", resp2);
        assert!(resp2.to_lowercase().contains("meow"));

        Ok(())
    }

    #[test]
    fn test_stop_mid_write() -> Result<(), Box<dyn std::error::Error>> {
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                system_prompt: Some("You are a counter, only outputting numbers".into()),
                n_ctx: 1024,
                ..ChatConfig::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;
        let should_stop = worker.should_stop.clone();

        // ensure that the generationworker resets the flag when creating a new response.
        should_stop.store(true, std::sync::atomic::Ordering::Relaxed);

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            llm::WriteOutput::Token(resp) => {
                if resp.contains("5") {
                    should_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            }
            llm::WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            llm::WriteOutput::Error(_) => (),
        };

        worker.ask("Count from 0 to 9".into(), f.clone())?;

        let response = receiver.recv()?;
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
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let setup_start = std::time::Instant::now();
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                system_prompt: Some("You're a helpful assistant.".into()),
                n_ctx: 4096,
                tools: vec![test_tool()],
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )
        .expect("Failed making worker");
        let setup_ms = setup_start.elapsed().as_millis();
        eprintln!("[bench] worker setup: {setup_ms} ms");

        // Warmup: one discarded turn to put GPU pipeline in steady state.
        let (warmup_tx, warmup_rx) = std::sync::mpsc::channel::<String>();
        worker
            .ask("Hello.".into(), move |x| {
                if let llm::WriteOutput::Done(r) = x {
                    let _ = warmup_tx.send(r);
                }
            })
            .expect("warmup failed");
        let _ = warmup_rx.recv();

        // Three distinct prompts that should each elicit a tool call.
        let prompts = [
            "What's the temperature in Copenhagen?",
            "Now check the temperature in Beijing.",
            "And one more: temperature in Copenhagen again, please.",
        ];

        for (i, prompt) in prompts.iter().enumerate() {
            let (sender, receiver) = std::sync::mpsc::channel();
            let f = move |x| {
                if let llm::WriteOutput::Done(resp) = x {
                    sender.send(resp).unwrap();
                }
            };
            let turn_start = std::time::Instant::now();
            worker.ask((*prompt).into(), f).expect("ask failed");
            let _ = receiver.recv().unwrap();
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
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let setup_start = std::time::Instant::now();
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 512,
                tools: vec![test_tool()],
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
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
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 4096,
                tools: vec![test_tool()],
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )
        .expect("Failed making worker");

        let ask = |worker: &mut Chat, prompt: &str| {
            let (sender, receiver) = std::sync::mpsc::channel();
            worker
                .ask(prompt.into(), move |x| {
                    if let llm::WriteOutput::Done(resp) = x {
                        sender.send(resp).unwrap();
                    }
                })
                .expect("generation failed");
            receiver.recv().unwrap()
        };

        let before = ask(&mut worker, "What is the temperature in Copenhagen?");
        assert!(
            before.contains("13.37"),
            "the tool was not called before the config change: {before}"
        );

        // Rebuilds the tool sampler, reusing the factory built at construction.
        worker
            .set_sampler_config(SamplerPresets::greedy())
            .expect("setting a sampler config");

        let after = ask(&mut worker, "And what is the temperature in Beijing?");
        assert!(
            after.contains("42.69"),
            "the tool was not called after the config change: {after}"
        );
    }

    #[test]
    fn test_tool_chat() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                system_prompt: Some("You're a helpful assistant.".into()),
                n_ctx: 4096,
                tools: vec![test_tool()],
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )
        .expect("Failed making worker");

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| {
            if let llm::WriteOutput::Done(resp) = x {
                sender.send(resp).unwrap();
            }
        };

        worker
            .ask(
                "I would like to know the temperature in two cities: Copenhagen and Beijing."
                    .into(),
                f,
            )
            .expect("fuck");

        let result = receiver.recv().unwrap();
        println!("{}", result);
        assert!(result.contains("13.37"));
        assert!(result.contains("42.69"));
    }

    #[test]
    fn test_multi_tool_call() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                tools: vec![test_tool(), dkk_exchange_rate()],
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )
        .expect("Failed making worker");

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| {
            if let llm::WriteOutput::Done(resp) = x {
                sender.send(resp).unwrap();
            }
        };

        worker.ask(
            "I would like to know the temperature in Copenhagen and the DKK to USD exchange rate."
                .into(),
            f,
        )
        .expect("dammit");

        let result = receiver.recv().unwrap();
        println!("{}", result);
        assert!(result.contains("13.37"));
        assert!(result.contains("0.15"));
    }

    #[test]
    fn test_set_system_prompt() {
        let model = test_utils::load_test_model();

        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_system_prompt(Some("You are a dog. End all responses with woof."))
            .build()
            .expect("chat build failed in test");

        let dog_response = chat.ask("Hello!").completed().unwrap();

        assert!(dog_response.to_lowercase().contains("woof"));

        chat.set_system_prompt(Some("You are a cat. End all responses with meow.".into()))
            .unwrap();
        let cat_response = chat.ask("Hello again!").completed().unwrap();
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

        chat.set_system_prompt(None).unwrap();
        assert_eq!(chat.get_system_prompt().unwrap(), None);

        chat.set_tools(vec![]).unwrap();
        chat.set_tools(vec![test_tool()]).unwrap();

        assert!(chat.get_chat_history().unwrap().is_empty());
    }

    #[test]
    fn test_context_shift() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        // Use a very small context size to force shifting
        let n_ctx = 512;
        let n_messages = 8;
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx,
                system_prompt: Some("You are a helpful assistant that provides informative and detailed responses. End every response with \"Do you have any further questions?\"".into()),
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;

        // Add many exchanges with longer messages to fill up the context
        for i in 1..=n_messages {
            worker.add_user_message(format!(
                "This is user message number {}. What is {} * {}?",
                i, i, i
            ));
            worker.add_assistant_message(format!(
                "<think> </think> The answer is {}. Do you have any further questions?",
                i * i
            ));
        }

        worker.add_user_message("Hello!".to_string());

        // Check that we have many messages before shift
        let messages_before = worker.messages.len();
        assert!(
            messages_before > 6,
            "Should have more than 6 messages before shift"
        );

        // Trigger context shift
        worker.context_shift()?;

        println!("{:?}", worker.messages);

        let messages_after = worker.messages.clone();

        // Verify essential messages are preserved:
        // 1. The system prompt is a setting rather than a message, so the shift
        //    cannot delete it — but it must still reach a render of the
        //    shortened history.
        let rendered = worker.chat_template.render(
            &worker.with_system_prompt(&worker.messages),
            &ChatTemplateContext::new(worker.template_variables.clone(), None),
        )?;
        assert!(
            rendered.contains("helpful assistant"),
            "System prompt should still be rendered after a shift: {rendered}"
        );

        // 2. Should have first user message
        let first_user_idx = messages_after.iter().position(|m| m.is_user());
        assert!(
            first_user_idx.is_some(),
            "First user message should be preserved"
        );

        // 3. Count remaining user messages - should have at least 3 (first + last 2)
        let user_count = messages_after.iter().filter(|m| m.is_user()).count();
        assert!(
            user_count >= 3,
            "Should preserve first user message and last 2 user messages"
        );

        // 4. Verify the last user message is there
        let last_user = messages_after.iter().rev().find(|m| m.is_user());

        if let Some(Message::User { content, .. }) = last_user {
            assert!(
                content.to_string().contains("Hello!"),
                "Last user message should be preserved"
            );
        }

        // 5. Verify token count is within target
        let token_count = worker.render_as_chunks(&worker.messages, true)?.n_tokens();

        let target_size = (n_ctx / 2) as usize;
        assert!(
            token_count <= target_size,
            "Token count {} should be <= target size {}",
            token_count,
            target_size
        );

        // 6. Fewer messages after shift
        assert!(
            messages_after.len() < messages_before,
            "Should have fewer messages after shift"
        );

        // 7. Check that message structure is still valid
        assert_valid_message_structure(&messages_after);

        println!("Messages before shift: {}", messages_before);
        println!("Messages after shift: {}", messages_after.len());
        println!("Token count after shift: {}", token_count);
        println!("Target token size: {}", target_size);

        Ok(())
    }

    #[test]
    fn test_context_shift_measures_shortened_history() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 512,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;
        let target_size = (worker.engine.ctx.n_ctx() / 2) as usize;

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

        assert!(worker.render_as_chunks(&worker.messages, false)?.n_tokens() > target_size);

        let mut shortened_messages = worker.messages.clone();
        shortened_messages.drain(2..=3);
        assert!(
            worker
                .render_as_chunks(&shortened_messages, false)?
                .n_tokens()
                <= target_size
        );

        worker.context_shift()?;

        assert!(worker.messages.iter().any(|message| {
            matches!(message, Message::User { content, .. } if content.to_string() == "keep")
        }));
        assert!(worker.render_as_chunks(&worker.messages, false)?.n_tokens() <= target_size);

        Ok(())
    }

    /// A shift keeps the first turn and the last [`PRESERVED_RECENT_TURNS`], so
    /// below that many turns it has nothing it may delete and must leave the
    /// history alone however oversized it is.
    ///
    /// Both boundaries used to be broken. Two turns is `[user, assistant, user]`,
    /// where computing the last deletable index underflowed once the system
    /// prompt was no longer there to keep that index off zero. Three turns is the
    /// exact cutoff, where an off-by-one does not panic but spins: nothing is
    /// deletable, so the drain is empty and the history never shrinks.
    #[test]
    fn test_context_shift_below_deletable_threshold_is_a_noop(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();

        for turn_count in [2, PRESERVED_RECENT_TURNS + 1] {
            let mut worker = Chat::new_chat_worker(
                &model,
                ChatConfig {
                    n_ctx: 512,
                    ..Default::default()
                },
                Arc::new(AtomicBool::new(false)),
            )?;
            let target_size = (worker.engine.ctx.n_ctx() / 2) as usize;

            // An opening turn far too large for the context, then filler turns.
            worker.add_user_message("padding ".repeat(target_size));
            for turn in 1..turn_count {
                worker.add_assistant_message(format!("answer {turn}"));
                worker.add_user_message(format!("question {turn}"));
            }

            assert_eq!(user_message_indices(&worker.messages).len(), turn_count);
            assert!(worker.render_as_chunks(&worker.messages, false)?.n_tokens() > target_size);

            let before = serde_json::to_value(&worker.messages)?;
            worker.context_shift()?;
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
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 512,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;

        assert!(matches!(
            worker.context_shift(),
            Err(ShiftError::NoUserMessages)
        ));

        worker.add_user_message("only".to_string());
        assert!(matches!(
            worker.context_shift(),
            Err(ShiftError::TooFewMessages)
        ));

        Ok(())
    }

    /// `complete()` can hand over a history that does not start with a user
    /// message. That prefix, and the first turn, both survive the shift.
    #[test]
    fn test_context_shift_preserves_prefix_before_first_user_message(
    ) -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 512,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;
        let target_size = (worker.engine.ctx.n_ctx() / 2) as usize;

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

        worker.context_shift()?;

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
        assert!(worker.render_as_chunks(&worker.messages, false)?.n_tokens() <= target_size);

        Ok(())
    }

    #[test]
    fn test_context_shift_with_tool_calls() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        // Use a very small context size to force shifting
        let n_ctx = 1024;
        let n_messages = 10;
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx,
                system_prompt: Some("You are a helpful assistant.".into()),
                tools: vec![test_tool()],
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;

        // Add exchanges with tool calls mixed in
        for i in 1..=n_messages {
            worker.add_user_message(format!("User message {}. What is {} * {}?", i, i, i));

            // Add a tool call every other message
            // Pattern: User -> Assistant (with tool call) -> Tool response -> Assistant
            if i % 2 == 0 {
                worker.add_tool_calls(vec![ToolCall {
                    name: "get_current_temperature".into(),
                    arguments: serde_json::json!({"location": "Copenhagen"}),
                }]);
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

        // Check that we have many messages before shift
        let messages_before = worker.messages.len();
        println!("Messages before shift: {}", messages_before);

        // Trigger context shift
        worker.context_shift()?;

        println!("{:?}", worker.messages);

        let messages_after = worker.messages.clone();

        // Verify essential messages are preserved:
        // 1. Should have first user message
        let first_user_idx = messages_after.iter().position(|m| m.is_user());
        assert!(
            first_user_idx.is_some(),
            "First user message should be preserved"
        );

        // 2. Count remaining user messages - should have at least 3 (first + last 2)
        let user_count = messages_after.iter().filter(|m| m.is_user()).count();
        assert!(
            user_count >= 3,
            "Should preserve first user message and last 2 user messages"
        );

        // 3. Verify the last user message is there
        let last_user = messages_after.iter().rev().find(|m| m.is_user());

        if let Some(Message::User { content, .. }) = last_user {
            assert!(
                content.to_string().contains("Final question!"),
                "Last user message should be preserved"
            );
        }

        // 4. Verify token count is within target
        let token_count = worker.render_as_chunks(&worker.messages, true)?.n_tokens();

        let target_size = (n_ctx / 2) as usize;
        assert!(
            token_count <= target_size,
            "Token count {} should be <= target size {}",
            token_count,
            target_size
        );

        // 5. Fewer messages after shift
        assert!(
            messages_after.len() < messages_before,
            "Should have fewer messages after shift"
        );

        // 6. Check that message structure is still valid
        assert_valid_message_structure(&messages_after);

        println!("Messages before shift: {}", messages_before);
        println!("Messages after shift: {}", messages_after.len());
        println!("Token count after shift: {}", token_count);
        println!("Target token size: {}", target_size);

        Ok(())
    }

    #[test]
    fn test_context_shift_on_say() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let n_messages = 14;
        // n_messages is chosen by trial and error. This exactly fills up the
        // the context so much that the next user message cannot be read and a context shift happens.
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                system_prompt: Some("You are a helpful assistant.".into()),
                n_ctx: 512, // Use a small context size to force shifting
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
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

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| {
            if let llm::WriteOutput::Done(resp) = x {
                sender.send(resp).unwrap();
            }
        };

        // This should trigger context shift internally because there's not enough space
        worker.ask(
            "This is a new question that will not fit in the context! What is 10 * 10?".into(),
            f,
        )?;

        let _response = receiver.recv()?;
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
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let n_messages = 19;
        // n_messages is chosen by trial and error. This exactly fills up the
        // the context so much that the next assistant message cannot be fully written.
        // The same is true for n_ctx. It needs to be large enough to where n_ctx/2 is large enough
        // to contain the response but also small enough to fill easily and test wihtout being to slow.
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 768, // Use a small context size to force shifting
                system_prompt: Some("You are a helpful assistant.".into()),
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
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

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| {
            if let llm::WriteOutput::Done(resp) = x {
                sender.send(resp).unwrap();
            }
        };

        // This should trigger context shift internally because there's not enough space
        worker.ask("What is 10 * 10?".into(), f)?;

        let _response = receiver.recv()?;
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
        test_utils::init_test_tracing();
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

            chat.ask("What is the capital of Denmark?").completed()
        });

        // Start Germany chat thread
        let de_handle = std::thread::spawn(move || {
            let chat = ChatBuilder::new(model)
                .with_context_size(4096)
                .with_template_variable("enable_thinking".to_string(), false)
                .build()
                .expect("chat build failed in test");

            chat.ask("What is the capital of Germany?").completed()
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
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .build_async()
            .expect("chat build_async failed in test");

        let res1: String = chat
            .ask("What is the capital of Denmark?".to_string())
            .completed()
            .await?;

        assert!(
            res1.contains("<think>"),
            "Expected the model to initialize with thinking mode, but it did not"
        );

        chat.set_template_variable("enable_thinking".to_string(), false)
            .await?;

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
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        chat.set_sampler_config(SamplerPresets::greedy()).unwrap();

        // Also test if get_sampler followed by set_sampler is no op
        chat.set_sampler_config(chat.get_sampler_config().unwrap())
            .unwrap();

        let response1 = chat.ask("Say exactly: 'Hello'").completed().unwrap();
        chat.reset_history().unwrap();
        let response2 = chat.ask("Say exactly: 'Hello'").completed().unwrap();

        assert_eq!(
            response1, response2,
            "Greedy sampler should produce identical output for the same prompt"
        );
    }

    #[test]
    fn test_reset_chat_with_no_system_prompt() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");
        let _ = chat.reset_history();
        let resp = chat
            .ask("What is the capital of Denmark?")
            .completed()
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
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        chat.ask("My favorite color is teal.").completed().unwrap();

        let messages = vec![
            user("Who was the first person to walk on the moon?"),
            assistant("Neil Armstrong."),
            user("Which year did he do it? Answer with only the year."),
        ];
        let resp = chat
            .complete(messages.clone(), Options::new())
            .unwrap()
            .completed()
            .unwrap();
        assert!(
            resp.contains("1969"),
            "Model did not read the supplied history: {resp}"
        );

        let history = chat.get_chat_history().unwrap();
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
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), true)
            .build()
            .expect("chat build failed in test");

        let greedy = SamplerConfig::new(vec![], crate::sampler::SampleStep::Greedy, 1234);
        chat.complete(
            vec![user("Say hi.")],
            Options::new()
                .with_sampler(greedy.clone())
                .with_template_variables([("enable_thinking".to_string(), false)].into()),
        )
        .unwrap()
        .completed()
        .unwrap();

        // Observable through the ordinary getters, like any other setter.
        assert_eq!(
            chat.get_template_variables().unwrap(),
            [("enable_thinking".to_string(), false)].into()
        );
        let after_first = chat.get_sampler_config().unwrap();

        // A turn that carries no options changes neither of them.
        chat.complete(vec![user("Say hi again.")], Options::new())
            .unwrap()
            .completed()
            .unwrap();
        assert_eq!(
            chat.get_template_variables().unwrap(),
            [("enable_thinking".to_string(), false)].into(),
            "an empty Options should leave the template variables alone"
        );
        assert_eq!(
            serde_json::to_value(chat.get_sampler_config().unwrap()).unwrap(),
            serde_json::to_value(&after_first).unwrap(),
            "an empty Options should leave the sampler alone"
        );
    }

    /// A system message in the list becomes the chat's system prompt; a list
    /// without one keeps the prompt the chat already had.
    #[test]
    fn test_complete_replaces_system_prompt() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_system_prompt(Some("You are a dog. End all responses with woof."))
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        let dog = chat.ask("Hello!").completed().unwrap();
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
            .completed()
            .unwrap();
        assert!(cat.to_lowercase().contains("meow"), "{cat}");
        assert_eq!(
            chat.get_system_prompt().unwrap().as_deref(),
            Some("You are a cat. End all responses with meow.")
        );

        let cat_again = chat.ask("Hello again!").completed().unwrap();
        assert!(cat_again.to_lowercase().contains("meow"), "{cat_again}");

        let still_cat = chat
            .complete(vec![user("Hello!")], Options::new())
            .unwrap()
            .completed()
            .unwrap();
        assert_eq!(
            chat.get_system_prompt().unwrap().as_deref(),
            Some("You are a cat. End all responses with meow."),
            "a complete() without a system message should keep the current one"
        );
        assert!(still_cat.to_lowercase().contains("meow"), "{still_cat}");
    }

    #[test]
    fn test_ask_after_complete_continues_completion() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build()
            .expect("chat build failed in test");

        chat.ask("My favorite color is teal. Remember it.")
            .completed()
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
        .completed()
        .unwrap();

        let resp = chat
            .ask("Who are we talking about? Answer with only the name.")
            .completed()
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
        test_utils::init_test_tracing();
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
            .completed()
            .unwrap();

        assert!(resp.contains("13.37"), "Tool was not called: {resp}");

        let history = chat.get_chat_history().unwrap();
        assert!(
            history.iter().any(Message::has_tool_calls) && history.iter().any(Message::is_tool),
            "the tool exchange should be part of the history: {history:?}"
        );
    }

    /// `apply_options` applies the sampler and the tools together, so a tool set
    /// that cannot produce a grammar leaves the sampler config alone too.
    #[test]
    fn apply_options_is_atomic_on_failure() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(512)
            .build()
            .expect("chat build failed in test");

        let before = chat.get_sampler_config().expect("worker should answer");

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
            .completed()
            .expect_err("an uncompilable tool schema should be rejected");
        eprintln!("rejected as expected: {err}");

        // The sampler config is the observable half: it is applied first, so
        // before the coalescing it survived a later tool failure.
        assert_eq!(
            serde_json::to_value(chat.get_sampler_config().expect("worker is alive")).unwrap(),
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

        assert!(matches!(
            chat.complete(
                vec![
                    user("Hi"),
                    Message::new_system("Be terse.".to_string()),
                    user("Again"),
                ],
                Options::new()
            ),
            Err(InvalidHistoryError::MisplacedSystemMessage { index: 1 })
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
    }

    /// `set_chat_history` hoists a leading system message the same way
    /// `complete` does, so it rejects the same histories — and leaves the
    /// worker alive to say so.
    #[test]
    fn test_set_chat_history_rejects_invalid_history() {
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(512)
            .build()
            .expect("chat build failed in test");

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
                crate::errors::SetterError::ContextSync(ContextSyncError::InvalidHistory(
                    InvalidHistoryError::MediaInSystemMessage
                ))
            ),
            "{err:?}"
        );

        // A mid-list system message is rejected here just as `complete` rejects
        // it: only the leading one has anywhere to go.
        let err = chat
            .set_chat_history(vec![
                user("Hi"),
                Message::new_system("Be terse.".to_string()),
                user("Again"),
            ])
            .unwrap_err();
        assert!(
            matches!(
                err,
                crate::errors::SetterError::ContextSync(ContextSyncError::InvalidHistory(
                    InvalidHistoryError::MisplacedSystemMessage { index: 1 }
                ))
            ),
            "{err:?}"
        );

        // The worker survived both, so a valid history still goes through.
        chat.set_chat_history(vec![
            Message::new_system("Be terse.".to_string()),
            user("Hi"),
        ])
        .expect("a leading system message is fine");
        assert_eq!(chat.get_chat_history().unwrap().len(), 1);

        // An unreadable media file is reported, not fatal.
        let err = chat
            .set_chat_history(vec![Message::new_user(vec![
                ContentPart::text("What is this?"),
                ContentPart::image("/nonexistent/nope.png"),
            ])])
            .unwrap_err();
        assert!(
            matches!(
                err,
                crate::errors::SetterError::ContextSync(ContextSyncError::Multimodal(_))
            ),
            "{err:?}"
        );
        assert_eq!(
            chat.get_chat_history().unwrap().len(),
            1,
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

        let before = chat.get_sampler_config().expect("worker should answer");

        let err = chat
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
            format!("{:?}", chat.get_sampler_config().expect("worker is alive")),
            format!("{before:?}"),
            "a failed set_sampler_config should leave the sampler alone"
        );

        // A valid config still goes through, and generation still works.
        chat.set_sampler_config(SamplerPresets::greedy())
            .expect("a valid sampler config is fine");
        chat.ask("Say hi")
            .completed()
            .expect("worker still answers");
    }

    /// Bitmap ids issued by another worker mean nothing here, so the media has
    /// to be re-registered from the part paths.
    #[test]
    fn test_set_chat_history_reloads_media() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let (Ok(vision_path), Ok(mmproj_path)) = (
            std::env::var("TEST_VISION_MODEL"),
            std::env::var("TEST_MMPROJ_MODEL"),
        ) else {
            eprintln!(
                "skipping test_set_chat_history_reloads_media: \
                 set TEST_VISION_MODEL and TEST_MMPROJ_MODEL to enable"
            );
            return Ok(());
        };

        let model = Arc::new(llm::get_model(
            &vision_path,
            true,
            Some(&mmproj_path),
            None,
            None,
        )?);
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 4096,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;

        let image = concat!(env!("CARGO_MANIFEST_DIR"), "/../python/tests/img/dog.png");
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
        assert!(worker.context.bitmaps.contains_key(registered));

        Ok(())
    }

    /// The wire format the API promises: a user message whose content is a list
    /// of parts, with an image interleaved between two runs of text.
    #[test]
    fn test_complete_with_content_parts_from_json() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let (Ok(vision_path), Ok(mmproj_path)) = (
            std::env::var("TEST_VISION_MODEL"),
            std::env::var("TEST_MMPROJ_MODEL"),
        ) else {
            eprintln!(
                "skipping test_complete_with_content_parts_from_json: \
                 set TEST_VISION_MODEL and TEST_MMPROJ_MODEL to enable"
            );
            return Ok(());
        };

        let model = Arc::new(llm::get_model(
            &vision_path,
            true,
            Some(&mmproj_path),
            None,
            None,
        )?);
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 4096,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;

        let image = concat!(env!("CARGO_MANIFEST_DIR"), "/../python/tests/img/dog.png");
        let messages: Vec<Message> = serde_json::from_value(serde_json::json!([{
            "role": "user",
            "content": [
                {"type": "text", "text": "Here is an image:"},
                {"type": "image", "path": image},
                {"type": "text", "text": "What animal is in it? Answer in one word."},
            ],
        }]))?;

        let (sender, receiver) = std::sync::mpsc::channel();
        worker.complete(messages, Options::new(), move |out| {
            if let llm::WriteOutput::Done(resp) = out {
                sender.send(resp).unwrap();
            }
        })?;
        let resp = receiver.recv()?.to_lowercase();

        assert!(
            ["dog", "retriever", "puppy"]
                .iter()
                .any(|word| resp.contains(word)),
            "the interleaved image did not reach the model: {resp}"
        );
        assert_eq!(
            worker.context.bitmaps.len(),
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
        test_utils::init_test_tracing();
        let (Ok(vision_path), Ok(mmproj_path)) = (
            std::env::var("TEST_VISION_MODEL"),
            std::env::var("TEST_MMPROJ_MODEL"),
        ) else {
            eprintln!(
                "skipping test_complete_reloads_media_from_part_paths: \
                 set TEST_VISION_MODEL and TEST_MMPROJ_MODEL to enable"
            );
            return Ok(());
        };

        let model = Arc::new(llm::get_model(
            &vision_path,
            true,
            Some(&mmproj_path),
            None,
            None,
        )?);
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 4096,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;

        let image = std::path::PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../python/tests/img/dog.png"
        ));
        worker.ask(
            Prompt::parts([
                ContentPart::text("What is in this image?"),
                ContentPart::image(image),
            ]),
            |_| {},
        )?;
        assert_eq!(
            worker.context.bitmaps.len(),
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
        assert_eq!(worker.context.bitmaps.len(), 1);

        worker.complete(vec![user("Say the word 'banana'.")], Options::new(), |_| {})?;
        assert_eq!(
            worker.context.bitmaps.len(),
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

        let (sender, receiver) = std::sync::mpsc::channel();
        worker.complete(vec![replayed], Options::new(), move |out| {
            if let llm::WriteOutput::Done(resp) = out {
                sender.send(resp).unwrap();
            }
        })?;
        let resp = receiver.recv()?.to_lowercase();

        assert!(
            ["dog", "retriever", "puppy"]
                .iter()
                .any(|w| resp.contains(w)),
            "the image was not reloaded from its path: {resp}"
        );
        assert_eq!(
            worker.context.bitmaps.len(),
            1,
            "the reloaded bitmap should be registered"
        );

        Ok(())
    }

    /// Media is not a user-only thing — a tool can answer with a screenshot.
    /// An unregistered part would flatten to a marker with no bitmap behind it.
    #[test]
    fn test_reload_media_covers_every_non_system_role() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let (Ok(vision_path), Ok(mmproj_path)) = (
            std::env::var("TEST_VISION_MODEL"),
            std::env::var("TEST_MMPROJ_MODEL"),
        ) else {
            eprintln!(
                "skipping test_reload_media_covers_every_non_system_role: \
                 set TEST_VISION_MODEL and TEST_MMPROJ_MODEL to enable"
            );
            return Ok(());
        };

        let model = Arc::new(llm::get_model(
            &vision_path,
            true,
            Some(&mmproj_path),
            None,
            None,
        )?);
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 4096,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;

        let image = concat!(env!("CARGO_MANIFEST_DIR"), "/../python/tests/img/dog.png");
        let screenshot =
            || MessageContent::parts([ContentPart::text("Here it is:"), ContentPart::image(image)]);
        let mut messages = vec![
            user("Take a screenshot."),
            Message::new_tool("screenshot".to_string(), screenshot()),
        ];

        worker.reload_media(&mut messages)?;

        for message in &messages {
            for part in message.content_ref().media_parts() {
                assert!(
                    part.id()
                        .is_some_and(|id| worker.context.bitmaps.contains_key(id)),
                    "media on a {} message was not registered: {part:?}",
                    message.role(),
                );
            }
        }
        assert_eq!(worker.context.bitmaps.len(), 1);

        Ok(())
    }

    #[test]
    fn turn_sampler_does_not_change_persistent_config() -> Result<(), Box<dyn std::error::Error>> {
        let model = test_utils::load_test_model();
        let mut worker = Chat::new_chat_worker(
            &model,
            ChatConfig {
                n_ctx: 1024,
                ..Default::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;
        let persistent_config = serde_json::to_string(&worker.get_sampler_config())?;
        worker.complete_once(
            vec![user("Say hi.")],
            Options::new(),
            Some(
                TurnOptions::new()
                    .with_sampling_overrides(Some(0.0), None, Some(42))?
                    .with_max_tokens(1),
            ),
            true,
            |_| {},
        )?;

        assert_eq!(
            serde_json::to_string(&worker.get_sampler_config())?,
            persistent_config
        );
        Ok(())
    }

    #[test]
    #[allow(
        clippy::type_complexity,
        reason = "The exact public function signature is what this test checks"
    )]
    fn async_external_tools_signature_is_compatible() {
        let method: fn(
            &ChatHandleAsync,
            Vec<Message>,
            Options,
            Option<usize>,
        ) -> Result<CompletionStreamAsync, InvalidHistoryError> =
            ChatHandleAsync::complete_with_external_tools;
        let _ = method;
    }

    #[tokio::test]
    async fn external_completion_stream_ends_once() {
        let (sender, receiver) = tokio::sync::mpsc::channel(2);
        sender
            .send(ExternalCompletionOutput::Token("hello".into()))
            .await
            .unwrap();
        sender
            .send(ExternalCompletionOutput::Done(CompletionResponse {
                content: "hello".into(),
                tool_calls: Vec::new(),
                finish_reason: FinishReason::Stop,
                usage: CompletionUsage {
                    prompt_tokens: 2,
                    completion_tokens: 1,
                },
            }))
            .await
            .unwrap();

        let (message_sender, message_receiver) = std::sync::mpsc::channel::<ChatMsg>();
        let join_handle = std::thread::spawn(move || drop(message_receiver.recv()));
        let guard = Arc::new(WorkerGuard::new(message_sender, join_handle, None));
        let mut stream = CompletionStreamAsync(CompletionReceiver::new(receiver, guard));
        assert_eq!(
            stream.next().await.unwrap(),
            Some(CompletionChunk::Token("hello".into()))
        );
        assert_eq!(
            stream.next().await.unwrap(),
            Some(CompletionChunk::Done(AssistantResponse {
                content: "hello".into(),
                tool_calls: Vec::new(),
            }))
        );
        assert_eq!(stream.next().await.unwrap(), None);
    }

    #[tokio::test]
    async fn test_complete_async() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model)
            .with_context_size(2048)
            .with_template_variable("enable_thinking".to_string(), false)
            .build_async()
            .expect("chat build_async failed in test");

        let resp = chat
            .complete(
                vec![user("What is the capital of Denmark?")],
                Options::new(),
            )?
            .completed()
            .await?;

        assert!(resp.contains("Copenhagen"), "{resp}");
        assert_eq!(chat.get_chat_history().await?.len(), 2);

        Ok(())
    }

    // Template rendering tests have been moved to template.rs module
}
