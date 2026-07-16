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
//! let model = Arc::new(llm::get_model("model.gguf", true, None, None)?);
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

use crate::errors::{
    ChatWorkerError, ContextSyncError, GenerateResponseError, InitWorkerError, MultimodalError,
    RenderError, SayError, SelectTemplateError, SetToolsError, ShiftError, TokenizeError,
    WrappedResponseError,
};
use crate::inference::{acquire_inference_lock, InferenceEngine};
use crate::llm;
use crate::llm::{GlobalInferenceLockToken, Worker, WorkerGuard, WriteOutput};
use crate::sampler::read_sampler_from_metadata;
use crate::sampler::{SamplerConfig, ShiftStep};
use crate::template::{select_template, ChatTemplate, ChatTemplateContext};
use crate::tokenizer::{ChunkId, Prompt, PromptPart, Promptable, TokenizerChunk, TokenizerChunks};
use crate::tool_calling::{detect_tool_format, Tool, ToolCall, ToolFormat};
use ahash::AHasher;
use indexmap::IndexMap;
use llama_cpp_2::mtmd::MtmdBitmap;
use llama_cpp_2::token::LlamaToken;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use std::cmp::min;
use std::collections::HashSet;
use std::fmt;
use std::hash::Hasher;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, MutexGuard};
use tracing::{debug, error, info, trace};

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug, Hash)]
pub struct Asset {
    pub id: String,
    pub path: PathBuf,
}

/// The content of a user message — either plain text or a raw JSON value.
///
/// Serializes transparently: `Text` becomes a JSON string, `Json` becomes the
/// raw JSON value (array, object, etc.). This lets chat templates that expect
/// `content: [{"type": "translate", ...}]` receive the actual array rather than
/// a stringified version of it.
#[derive(Clone, Debug)]
pub enum MessageContent {
    Text(String),
    Json(serde_json::Value),
}

impl Serialize for MessageContent {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        match self {
            MessageContent::Text(t) => t.serialize(s),
            MessageContent::Json(v) => v.serialize(s),
        }
    }
}

impl<'de> Deserialize<'de> for MessageContent {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let v = serde_json::Value::deserialize(d)?;
        Ok(match v {
            serde_json::Value::String(s) => MessageContent::Text(s),
            other => MessageContent::Json(other),
        })
    }
}

impl fmt::Display for MessageContent {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            MessageContent::Text(t) => write!(f, "{t}"),
            MessageContent::Json(v) => write!(f, "{v}"),
        }
    }
}

impl From<String> for MessageContent {
    fn from(s: String) -> Self {
        MessageContent::Text(s)
    }
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum Message {
    User {
        content: MessageContent,
        #[serde(default, skip_serializing_if = "Vec::is_empty")]
        assets: Vec<Asset>,
    },
    // The optional tool_calls field distinguishes a plain assistant response
    // from one that includes tool calls. When tool_calls is Some, the content
    // field is typically empty (required by qwen3 chat templates).
    // https://github.com/QwenLM/Qwen3/blob/e5a1d326/docs/source/framework/function_call.md
    Assistant {
        content: String,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        tool_calls: Option<Vec<ToolCall>>,
    },
    System {
        content: String,
    },
    Tool {
        name: String,
        content: String,
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

    pub fn content(&self) -> String {
        match self {
            Message::User { content, .. } => content.to_string(),
            Message::Assistant { content, .. }
            | Message::System { content, .. }
            | Message::Tool { content, .. } => content.clone(),
        }
    }

    pub fn assets(&self) -> Vec<Asset> {
        match self {
            Message::User { assets, .. } => assets.clone(),
            _ => vec![],
        }
    }

    pub fn new_user(content: String) -> Self {
        Self::User {
            content: MessageContent::Text(content),
            assets: vec![],
        }
    }

    pub fn new_assistant(content: String) -> Self {
        Self::Assistant {
            content,
            tool_calls: None,
        }
    }

    pub fn new_system(content: String) -> Self {
        Self::System { content }
    }
}

// PARALLELISM

///
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
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            n_ctx: 4096,
            template_variables: std::collections::HashMap::new(),
            system_prompt: None,
            tools: Vec::new(),
            sampler_config: None,
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
/// let model = Arc::new(llm::get_model("model.gguf", true, None, None)?);
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
    guard: WorkerGuard<ChatMsg>,
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
                if let Err(e) = process_worker_msg(&mut worker_state, msg) {
                    return error!("Worker crashed: {e}");
                }
            }
        });

        init_rx.recv().map_err(|_| InitWorkerError::NoResponse)??;

        Ok(Self {
            guard: WorkerGuard::new(msg_tx, join_handle, Some(should_stop)),
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

    fn set_and_wait_blocking<F>(&self, make_msg: F) -> Option<()>
    where
        F: FnOnce(tokio::sync::mpsc::Sender<()>) -> ChatMsg,
    {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        let msg = make_msg(output_tx);
        self.guard.send(msg);
        // block until processed
        output_rx.blocking_recv()
    }

    /// Reset the chat conversation with a new system prompt and tools.
    pub fn reset_chat(
        &self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(|output_tx| ChatMsg::ResetChat {
            system_prompt,
            tools,
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError("reset_chat".into()))
    }

    /// Reset the chat conversation history.
    pub fn reset_history(&self) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(|output_tx| ChatMsg::SetChatHistory {
            messages: vec![],
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError(
            "reset_history".into(),
        ))
    }

    /// Update the available tools for the model to use.
    pub fn set_tools(&self, tools: Vec<Tool>) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(|output_tx| ChatMsg::SetTools { tools, output_tx })
            .ok_or(crate::errors::SetterError::SetterError("set_tools".into()))
    }

    /// DEPRECATED: Use set_template_variable("enable_thinking", value) instead.
    #[deprecated(note = "Use set_template_variable(\"enable_thinking\", value) instead")]
    pub fn set_allow_thinking(
        &self,
        allow_thinking: bool,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(|output_tx| ChatMsg::SetThinking {
            allow_thinking,
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError(
            "set_allow_thinking".into(),
        ))
    }

    /// Set a single template variable.
    pub fn set_template_variable(
        &self,
        name: String,
        value: bool,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(|output_tx| ChatMsg::SetTemplateVariable {
            name,
            value,
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError(
            "set_template_variable".into(),
        ))
    }

    /// Set all template variables, replacing any existing ones.
    pub fn set_template_variables(
        &self,
        variables: std::collections::HashMap<String, bool>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(|output_tx| ChatMsg::SetTemplateVariables {
            variables,
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError(
            "set_template_variables".into(),
        ))
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
        self.set_and_wait_blocking(|output_tx| ChatMsg::SetSamplerConfig {
            sampler_config,
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError(
            "set_sampler_config".into(),
        ))
    }

    /// Stop the current generation if one is in progress.
    pub fn stop_generation(&self) {
        self.guard.stop();
    }

    /// Get the chat history without the system prompt (lower-level API).
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
    pub fn set_chat_history(
        &self,
        messages: Vec<Message>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(|output_tx| ChatMsg::SetChatHistory {
            messages,
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError(
            "set_chat_history".into(),
        ))
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

    /// Update the system prompt without resetting chat history.
    ///
    /// This modifies the system message while preserving the conversation history.
    /// If no system prompt exists, it will be added. If one exists, it will be replaced.
    /// The model context is re-synchronized after the change, reusing the KV cache where possible.
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
    /// # let model = Arc::new(get_model("model.gguf", true, None, None).unwrap());
    /// # let chat = ChatBuilder::new(model).build();
    /// chat.set_system_prompt(Some("You are a helpful coding assistant.".to_string()))?;
    /// # Ok::<(), nobodywho::errors::SetterError>(())
    /// ```
    pub fn set_system_prompt(
        &self,
        system_prompt: Option<String>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_blocking(|output_tx| ChatMsg::SetSystemPrompt {
            system_prompt,
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError(
            "set_system_prompt".into(),
        ))
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
                if let Err(e) = process_worker_msg(&mut worker_state, msg) {
                    return error!("Worker crashed: {e}");
                }
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

    // internal helper function for async setters
    async fn set_and_wait_async<F>(&self, make_msg: F) -> Option<()>
    where
        F: FnOnce(tokio::sync::mpsc::Sender<()>) -> ChatMsg,
    {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        let msg = make_msg(output_tx);
        self.guard.send(msg);
        // wait until processed
        output_rx.recv().await
    }

    /// Reset the chat conversation with a new system prompt and tools.
    pub async fn reset_chat(
        &self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(|output_tx| ChatMsg::ResetChat {
            system_prompt,
            tools,
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError("reset_chat".into()))
    }

    /// Reset the chat conversation history.
    pub async fn reset_history(&self) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(|output_tx| ChatMsg::SetChatHistory {
            messages: vec![],
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError(
            "reset_history".into(),
        ))
    }

    /// Update the available tools for the model to use.
    pub async fn set_tools(&self, tools: Vec<Tool>) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(|output_tx| ChatMsg::SetTools { tools, output_tx })
            .await
            .ok_or(crate::errors::SetterError::SetterError("set_tools".into()))
    }

    /// DEPRECATED: Use set_template_variable("enable_thinking", value) instead.
    #[deprecated(note = "Use set_template_variable(\"enable_thinking\", value) instead")]
    pub async fn set_allow_thinking(
        &self,
        allow_thinking: bool,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(|output_tx| ChatMsg::SetThinking {
            allow_thinking,
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError(
            "set_allow_thinking".into(),
        ))
    }

    /// Set a single template variable.
    pub async fn set_template_variable(
        &self,
        name: String,
        value: bool,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(|output_tx| ChatMsg::SetTemplateVariable {
            name,
            value,
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError(
            "set_template_variable".into(),
        ))
    }

    /// Set all template variables, replacing any existing ones.
    pub async fn set_template_variables(
        &self,
        variables: std::collections::HashMap<String, bool>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(|output_tx| ChatMsg::SetTemplateVariables {
            variables,
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError(
            "set_template_variables".into(),
        ))
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
        self.set_and_wait_async(|output_tx| ChatMsg::SetSamplerConfig {
            sampler_config,
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError(
            "set_sampler_config".into(),
        ))
    }

    /// Stop the current generation if one is in progress.
    pub fn stop_generation(&self) {
        self.guard.stop();
    }

    /// Get the chat history without the system prompt (lower-level API).
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
    pub async fn set_chat_history(
        &self,
        messages: Vec<Message>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(|output_tx| ChatMsg::SetChatHistory {
            messages,
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError(
            "set_chat_history".into(),
        ))
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

    /// Update the system prompt without resetting chat history.
    ///
    /// This modifies the system message while preserving the conversation history.
    /// If no system prompt exists, it will be added. If one exists, it will be replaced.
    /// The model context is re-synchronized after the change, reusing the KV cache where possible.
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
    /// # let model = Arc::new(get_model("model.gguf", true, None, None).unwrap());
    /// # let chat = ChatBuilder::new(model).build_async();
    /// # chat.set_system_prompt(Some("You are a helpful coding assistant.".to_string())).await?;
    /// # Ok::<(), nobodywho::errors::SetterError>(())
    /// ```
    pub async fn set_system_prompt(
        &self,
        system_prompt: Option<String>,
    ) -> Result<(), crate::errors::SetterError> {
        self.set_and_wait_async(|output_tx| ChatMsg::SetSystemPrompt {
            system_prompt,
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError(
            "set_system_prompt".into(),
        ))
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

enum ChatMsg {
    Ask {
        prompt: Prompt,
        output_tx: tokio::sync::mpsc::UnboundedSender<llm::WriteOutput>,
    },
    ResetChat {
        system_prompt: Option<String>,
        tools: Vec<Tool>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    SetTools {
        tools: Vec<Tool>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    SetSystemPrompt {
        system_prompt: Option<String>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    GetSystemPrompt {
        output_tx: tokio::sync::mpsc::Sender<Option<String>>,
    },
    SetThinking {
        allow_thinking: bool,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    SetTemplateVariable {
        name: String,
        value: bool,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    SetTemplateVariables {
        variables: std::collections::HashMap<String, bool>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    GetTemplateVariables {
        output_tx: tokio::sync::mpsc::Sender<std::collections::HashMap<String, bool>>,
    },
    SetSamplerConfig {
        sampler_config: SamplerConfig,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    GetChatHistory {
        output_tx: tokio::sync::mpsc::Sender<Vec<Message>>,
    },
    GetSamplerConfig {
        output_tx: tokio::sync::mpsc::Sender<SamplerConfig>,
    },
    SetChatHistory {
        messages: Vec<Message>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    GetStats {
        output_tx: tokio::sync::mpsc::Sender<ChatStats>,
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

fn process_worker_msg(worker_state: &mut Chat<'_>, msg: ChatMsg) -> Result<(), ChatWorkerError> {
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
        ChatMsg::ResetChat {
            system_prompt,
            tools,
            output_tx,
        } => {
            worker_state.reset_chat(system_prompt, tools)?;
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::SetTools { tools, output_tx } => {
            worker_state.set_tools(tools)?;
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::SetSystemPrompt {
            system_prompt,
            output_tx,
        } => {
            worker_state.set_system_prompt(system_prompt)?;
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::GetSystemPrompt { output_tx } => {
            let system_prompt = worker_state.get_system_prompt();
            let _ = output_tx.blocking_send(system_prompt);
        }
        ChatMsg::SetThinking {
            allow_thinking,
            output_tx,
        } => {
            worker_state.set_template_variable("enable_thinking".to_string(), allow_thinking)?;
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::SetTemplateVariable {
            name,
            value,
            output_tx,
        } => {
            worker_state.set_template_variable(name, value)?;
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::SetTemplateVariables {
            variables,
            output_tx,
        } => {
            worker_state.set_template_variables(variables)?;
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::GetTemplateVariables { output_tx } => {
            let vars = worker_state.get_template_variables();
            let _ = output_tx.blocking_send(vars);
        }
        ChatMsg::SetSamplerConfig {
            sampler_config,
            output_tx,
        } => {
            worker_state.set_sampler_config(sampler_config);
            let _ = output_tx.blocking_send(());
        }
        ChatMsg::GetChatHistory { output_tx } => {
            let msgs = worker_state.get_chat_history();
            let _ = output_tx.blocking_send(msgs);
        }
        ChatMsg::SetChatHistory {
            messages,
            output_tx,
        } => {
            worker_state.set_chat_history(messages)?;
            let _ = output_tx.blocking_send(());
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
        ChatMsg::Tokenize { prompt, output_tx } => {
            let result = worker_state.tokenize(prompt);
            let _ = output_tx.blocking_send(result);
        }
    };

    Ok(())
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

    pub fn garbage_collect_bitmaps(&mut self, messages: &[Message]) {
        // Garbage collection for the bitmaps.
        let referenced_bitmaps: HashSet<String> = messages
            .iter()
            .flat_map(|msg| msg.assets())
            .map(|asset| asset.id)
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

/// Build a stateful sampler chain that starts with a Lark grammar step.
///
/// Prepends a `ShiftStep::LarkWithSlices` to the sampler config so that every
/// token sampled through this chain is constrained by the grammar. The
/// `slices` argument is a list of regex patterns pre-computed into vocabulary
/// bitmasks at construction time; when every valid token at the current grammar
/// position matches a slice, llguidance uses the bitmask instead of a full
/// vocabulary walk, cutting per-token constraint cost significantly.
///
/// This function is called once at session init (and again when config
/// changes) so the ~400ms sampler-construction cost is paid upfront rather
/// than on the first tool-calling token. See [`Chat::tool_sampler`].
fn build_tool_sampler(
    config: &SamplerConfig,
    lark: &str,
    slices: Vec<String>,
    model: &llama_cpp_2::model::LlamaModel,
) -> Result<llama_cpp_2::sampling::LlamaSampler, crate::errors::SamplerError> {
    let mut steps = config.steps.clone();
    steps.insert(0, ShiftStep::LarkWithSlices(lark.to_string(), slices));
    let with_grammar = SamplerConfig::new(steps, config.sample_step.clone(), config.seed);
    with_grammar.to_stateful(model)
}

/// A chat session: owns an [`InferenceEngine`] plus all the conversational state
/// (messages, tools, template, sampler config).
struct Chat<'a> {
    engine: InferenceEngine<'a>,
    should_stop: Arc<AtomicBool>,
    /// Tool-calling grammar in Lark format (with a lazy preamble that lets
    /// the model produce free-form text up to the begin token, then
    /// constrains the tool call). Fed to llguidance via `ShiftStep::Lark`.
    /// `None` when no tools are configured or grammar generation failed; in
    /// that case the model generates freely and we fall back to text-level
    /// extraction. Built by `ToolFormatHandler::to_lark`.
    tool_grammar: Option<String>,
    tool_format: Option<ToolFormat>,
    /// Pre-built sampler enforcing the tool-call grammar. Held idle until the
    /// begin token appears mid-stream, then swapped in for the rest of
    /// generation (see [`build_tool_sampler`] for why it's built eagerly).
    /// `None` when no tools are configured or grammar generation failed;
    /// falls back to text-level extraction.
    tool_sampler: Option<llama_cpp_2::sampling::LlamaSampler>,
    sampler_config: SamplerConfig,
    messages: Vec<Message>,
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

        // Only detect tool calling format if tools are provided
        let (tool_format, tool_grammar) = if !config.tools.is_empty() {
            match detect_tool_format(&model.language_model) {
                Ok(format) => {
                    debug!(format = ?format, "Detected tool calling format");

                    let grammar = match format.to_lark(&config.tools) {
                        Ok(g) => {
                            debug!(grammar = %g, "Generated tool calling grammar (Lark)");
                            Some(g)
                        }
                        Err(e) => {
                            debug!(error = %e, "Failed to generate grammar from tools");
                            None
                        }
                    };

                    (Some(format), grammar)
                }
                Err(e) => {
                    debug!(error = %e, "Failed to detect tool format, tools will not work");
                    (None, None)
                }
            }
        } else {
            (None, None)
        };
        let sampler_config = match config.sampler_config {
            Some(sc) => sc,
            None => read_sampler_from_metadata(&model.language_model).unwrap_or_default(),
        };

        // Pre-build the with-grammar sampler. We accept the ~400ms
        // `llguidance_tok_env` + `LlamaSampler::from(Matcher)` cost here (during
        // session setup) rather than per activation — the tokenizer env and
        // ParserFactory are rebuilt from scratch on every call.
        let tool_sampler = tool_grammar.as_ref().and_then(|lark| {
            let slices = tool_format
                .as_ref()
                .map_or_else(Vec::new, |f| f.slice_regexes());
            build_tool_sampler(&sampler_config, lark, slices, &model.language_model)
                .inspect_err(|e| debug!(error = %e, "Failed to pre-build tool sampler"))
                .ok()
        });

        // Build the low-level inference engine via the shared Worker constructor,
        // then take ownership of just the engine for the chat session.
        let Worker { engine, extra: () } = Worker::new_with_type(model, config.n_ctx, false, ())?;

        Ok(Chat {
            engine,
            should_stop,
            tool_grammar,
            tool_format,
            tool_sampler,
            sampler_config,
            messages: match config.system_prompt {
                Some(msg) => vec![Message::System { content: msg }],
                None => vec![],
            },
            chat_template: template,
            template_variables: config.template_variables,
            tools: config.tools,
            context: ChatContext::new(),
        })
    }

    fn should_stop(&self) -> bool {
        self.should_stop.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn add_system_message(&mut self, content: String) {
        self.messages.push(Message::System { content });
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.messages.push(Message::new_assistant(content));
    }

    pub fn add_user_message(&mut self, content: impl Into<MessageContent>, assets: Vec<Asset>) {
        self.messages.push(Message::User {
            content: content.into(),
            assets,
        });
    }

    pub fn add_tool_calls(&mut self, tool_calls: Vec<ToolCall>) {
        self.messages.push(Message::Assistant {
            content: "".into(),
            tool_calls: Some(tool_calls),
        });
    }

    pub fn add_tool_resp(&mut self, name: String, content: String) {
        self.messages.push(Message::Tool { name, content });
    }

    /// Compare tokens from a template-rendered chat history with the tokens in the LLM's context,
    /// and perform the LLM 'reading' to make the LLM's context match the rendered tokens exactly.
    /// Because this invokes the model, this is potentially an expensive method to call.
    #[tracing::instrument(level = "debug", skip_all)]
    fn sync_context_with_render(
        &mut self,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<(), ContextSyncError> {
        let mut chunks = self.render_as_chunks(true)?;
        if chunks.n_tokens() > self.engine.ctx.n_ctx() as usize {
            self.context_shift()?;
            chunks = self.render_as_chunks(true)?;
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

    fn context_shift(&mut self) -> Result<(), ShiftError> {
        info!("Context shift happens!");
        let target_token_size = (self.engine.ctx.n_ctx() / 2) as usize;
        let mut messages = self.messages.clone();

        // Find indices to preserve
        let system_end = if messages[0].is_system() { 1 } else { 0 };
        let first_user_message_index = self
            .find_next_user_message(&messages, system_end)
            .ok_or(ShiftError::NoUserMessages)?;
        let first_deletable_index = self
            .find_next_user_message(&messages, first_user_message_index + 1)
            .ok_or(ShiftError::TooFewMessages)?;
        let mut last_deletable_index = self
            .find_start_of_last_n_user_messages(&messages, 2)
            .ok_or(ShiftError::TooFewMessages)?
            - 1;

        // Two is the smallest number of messages we can delete as we need to preserve the message structure.
        // There might be a better start guess here.
        let mut messages_to_delete = 2;

        // Delete messages until context is small enough or only essential messages are left.
        // Double the number of messages to delete each iteration. This is a simple and kind of stupid solution, as it might overshoot by a lot.
        // Plenty of optimization options here.

        loop {
            // No non-essential messages left to delete or the new context has reached desired size.
            if first_deletable_index > last_deletable_index {
                break;
            }

            let chunks = self.render_as_chunks(false)?;
            if chunks.n_tokens() <= target_token_size {
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
                    .ok_or(ShiftError::InternalError(
                        "Could not find user message supposed to be there".into(),
                    ))?
                    - 1,
                last_deletable_index,
            ); // should never fail
            messages.drain(first_deletable_index..=delete_index);
            messages_to_delete *= 2;

            let messages_deleted = delete_index - first_deletable_index + 1;

            last_deletable_index -= messages_deleted;
        }

        self.messages = messages;
        Ok(())
    }

    fn find_next_user_message(&self, messages: &[Message], start_index: usize) -> Option<usize> {
        messages[start_index..]
            .iter()
            .position(|msg| msg.is_user())
            .map(|pos| pos + start_index)
    }

    fn find_start_of_last_n_user_messages(&self, messages: &[Message], n: usize) -> Option<usize> {
        let user_indices: Vec<usize> = messages
            .iter()
            .enumerate()
            .filter(|(_, msg)| msg.is_user())
            .map(|(idx, _)| idx)
            .collect();

        if user_indices.len() >= n {
            Some(user_indices[user_indices.len() - n])
        } else {
            None
        }
    }

    // ---------- IMPORTANT ----------
    // Should only be used under a global inference lock
    // This is a safety meassure to prevent bugs from multiple
    // contexts with the same model. It might not be necessary
    // but assume it is.
    pub fn generate_response_until_done<F>(
        &mut self,
        sampler_config: SamplerConfig,
        mut respond: F,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, GenerateResponseError>
    where
        F: FnMut(WriteOutput),
    {
        // Token generation loop
        info!("Worker writing until done");

        // pre-allocating 4096 bytes for the response string
        // 4096 is a very randomly chosen number. how does this affect performance?
        let mut full_response: String = String::with_capacity(4096);
        let mut tokens_written_until_now = TokenizerChunks::new();

        // Initial sampler — no tool grammar. The expensive llguidance step
        // lives on the pre-built `self.tool_sampler` and is only switched in
        // when the begin token appears in the streamed output. `to_stateful`
        // without the Lark step is cheap (<1ms).
        let mut base_sampler = sampler_config.to_stateful(self.engine.ctx.model)?;

        // Reset the pre-built tool sampler so its grammar matcher and any
        // stateful sub-samplers (Dist RNG, etc.) start fresh for this run.
        if let Some(ts) = self.tool_sampler.as_mut() {
            ts.reset();
        }

        // Capture begin-token text + its token-id sequence once, so when we
        // detect the trigger we can fast-forward the pre-built sampler past
        // those tokens without doing string→token conversion mid-loop.
        let pending_tool_activation: Option<(String, Vec<LlamaToken>)> = self
            .tool_format
            .as_ref()
            .filter(|_| self.tool_sampler.is_some())
            .and_then(|format| {
                let begin_token = format.begin_token().to_string();
                let tokens = self
                    .engine
                    .ctx
                    .model
                    .str_to_token(&begin_token, llama_cpp_2::model::AddBos::Never)
                    .ok()?;
                Some((begin_token, tokens))
            });
        // Flipped to true when the begin token is detected and never reset
        // within this call. The Lark grammar encodes its own endpoint via EOS,
        // so generation stops when the tool call is complete. A future design
        // that allows free text after tool calls would need to reset this flag
        // when the end token is observed.
        let mut grammar_activated = false;

        // init statefull decoder for split up tokens like emojis
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        while !self.should_stop() {
            // Check if the context is full
            if self.engine.is_context_full() {
                self.context_shift()?;
                self.sync_context_with_render(inference_lock_token)?;
                self.engine
                    .read_chunks(tokens_written_until_now.clone(), inference_lock_token)?;
                // do not update tokens_in_context as this is done later by ask
            }

            // Sample next token, no need to use sampler.accept as sample already accepts the token.
            // using sampler.accept() will cause the sampler to crash when using grammar sampling.
            // https://github.com/utilityai/llama-cpp-rs/issues/604
            //
            // After activation we delegate to `self.tool_sampler` (pre-built
            // at session init), so we don't pay the ~400ms sampler-construction
            // cost mid-stream.
            let new_token = if grammar_activated {
                let ts = self
                    .tool_sampler
                    .as_mut()
                    .expect("tool_sampler must exist when grammar_activated is true");
                self.engine.sample_and_decode_next_token(ts)?
            } else {
                self.engine
                    .sample_and_decode_next_token(&mut base_sampler)?
            };

            tokens_written_until_now.append(TokenizerChunk::new_text(vec![new_token]));

            // Attempt to convert token(s) to bytes
            let token_bytes = match self
                .engine
                .ctx
                .model
                .token_to_piece_bytes(new_token, 8, true, None)
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

            // XXX: this literal '<eos>' token match is a fucked hotfix for gemma4. it seems like
            // some gemma4 models will emit a *wrong* eos token (doesn't match the expected format)
            // after tool calls. This doesn't trigger the is_eog_token match in llama.cpp and
            // causes a bad infinite generation loop.
            // it seems like vllm also has a codepath to handle this specific case:
            // https://docs.vllm.ai/en/stable/api/vllm/model_executor/models/gemma4_utils/#vllm.model_executor.models.gemma4_utils.has_tool_response_tag
            let gemma4_eog_hotfix = token_str == "<eos>" && new_token == LlamaToken::new(1);

            let has_eog = self.engine.ctx.model.is_eog_token(new_token) || gemma4_eog_hotfix;
            trace!(?new_token, ?token_str, ?has_eog);

            if !has_eog {
                full_response.push_str(&token_str);
                trace!(?token_str, "Sending out token:");
                respond(WriteOutput::Token(token_str.to_string()));
            }

            // Dynamic tool-grammar activation: the begin token has finished
            // emitting once `full_response` ends with it. We only fast-forward
            // the pre-built `self.tool_sampler`'s matcher past those tokens —
            // no rebuild needed.
            if !grammar_activated {
                if let Some((begin_token, begin_tokens)) = pending_tool_activation.as_ref() {
                    if full_response.ends_with(begin_token.as_str()) {
                        let activate_start = std::time::Instant::now();
                        let ts = self
                            .tool_sampler
                            .as_mut()
                            .expect("tool_sampler must exist when pending_tool_activation is Some");
                        ts.accept_many(begin_tokens.iter());
                        grammar_activated = true;
                        info!(
                            begin_token = %begin_token,
                            total_ms = activate_start.elapsed().as_millis(),
                            "Activated tool-call grammar"
                        );
                    }
                }
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

    pub fn ask<F>(&mut self, prompt: Prompt, respond: F) -> Result<&mut Self, SayError>
    where
        F: Fn(llm::WriteOutput) + Clone,
    {
        // reset the stop flag
        self.should_stop
            .store(false, std::sync::atomic::Ordering::Relaxed);

        // Get the tool call begin token from the format if tools are configured
        let tool_call_begin = self
            .tool_format
            .as_ref()
            .map(|fmt| fmt.begin_token().to_string());

        let prompt_text = prompt.to_string();

        let media_assets = prompt.extract_media_assets();
        let bitmaps = media_assets
            .iter()
            .map(|part| match part {
                PromptPart::Image(path) => self.engine.load_image(path),
                PromptPart::Audio(path) => self.engine.load_audio(path),
                PromptPart::Text(_) => unreachable!(),
            })
            .collect::<Result<Vec<MtmdBitmap>, MultimodalError>>()?;

        debug!("Detected bitmaps: {:?}", bitmaps);

        let bitmap_ids = self.context.add_bitmaps(bitmaps)?;
        let assets = bitmap_ids
            .iter()
            .zip(media_assets.iter())
            .map(|(id, part)| Asset {
                id: id.clone(),
                path: match part {
                    PromptPart::Image(path) | PromptPart::Audio(path) => path.to_path_buf(),
                    PromptPart::Text(_) => unreachable!(),
                },
            })
            .collect::<Vec<_>>();

        let content = match prompt {
            Prompt::Json(v) => MessageContent::Json(v),
            Prompt::Parts(_) => MessageContent::Text(prompt_text),
        };
        self.add_user_message(content, assets);

        // The tool-call grammar is NOT pre-injected into the chain. Lark/
        // llguidance has no "trigger word" mechanism, so an always-on grammar
        // would block EOS when the model just wants to chat. Instead the
        // grammar is added dynamically inside `generate_response_until_done`
        // the moment the begin token appears in the streamed output.
        let sampler = self.sampler_config.clone();

        // get the finished response
        let mut response: String = self.wrapped_update_context_and_generate_response(
            sampler.clone(),
            respond.clone(),
            tool_call_begin.clone(),
        )?;

        // Process tool calls if tool format is configured
        // Clone to avoid borrow issues in the loop
        if let Some(tool_format) = self.tool_format.clone() {
            while let Some(tool_calls) = tool_format.extract_tool_calls(&response) {
                debug!(?tool_calls, "Got tool calls:");

                self.add_tool_calls(tool_calls.clone());

                for tool_call in tool_calls {
                    // find the tool
                    // this is just a stupid linear search
                    // but I think it's probably faster than something fancy as long as we have few tools
                    // /shrug I'm happy to be wrong
                    let Some(tool) = self.tools.iter().find(|t| t.name == tool_call.name) else {
                        // in case the tool isn't found.
                        // I *think* this should be impossible, as long as the tool calling grammar
                        // works.
                        error!(
                            tool_name = tool_call.name,
                            "Model triggered tool call for invalid tool name:",
                        );
                        let errmsg = format!("ERROR - Invalid tool name: {}", tool_call.name);
                        self.add_tool_resp(tool_call.name, errmsg);
                        continue;
                    };

                    // call the tool
                    debug!("Calling the tool now!");
                    let response = (tool.function)(tool_call.arguments);
                    debug!(%tool_call.name, %response, "Tool call result:");

                    // add to chat history
                    self.add_tool_resp(tool_call.name, response);
                }

                // get the finished response
                response = self.wrapped_update_context_and_generate_response(
                    sampler.clone(),
                    respond.clone(),
                    tool_call_begin.clone(),
                )?;
            }
        } // Close if let Some(tool_format)

        debug_assert!(tool_call_begin
            .as_ref()
            .is_none_or(|t| !response.contains(t.as_str())));
        self.add_assistant_message(response);

        self.context.chunks = self.render_as_chunks(true)?;

        Ok(self)
    }

    /// Go for the unhandled mode when you are context shifting.
    /// That is for avoiding the render will concat system message with the first user message.
    /// Otherwise please handle stuff.
    fn render_as_chunks(&mut self, handled: bool) -> Result<TokenizerChunks, RenderError> {
        let messages = &self.messages;
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

        let bitmaps: Vec<&MtmdBitmap> = self
            .messages
            .iter()
            .flat_map(|msg| msg.assets())
            .filter_map(|asset| self.context.bitmaps.get(&asset.id))
            .collect();
        Ok(self.engine.tokenize(rendered_chat, bitmaps)?)
    }

    fn wrapped_update_context_and_generate_response<F>(
        &mut self,
        sampler: SamplerConfig,
        respond: F,
        tool_call_begin_token: Option<String>,
    ) -> Result<String, WrappedResponseError>
    where
        F: Fn(llm::WriteOutput) + Clone,
    {
        // Check how much of the current KVCache we can keep
        let inference_lock_token = acquire_inference_lock();
        self.sync_context_with_render(&inference_lock_token)?;

        // wrap the response callback to keep a copy of the completed response
        // and to avoid emitting tool calls
        let (wrapped_respond, resp_receiver) =
            crate::inference::wrap_respond(respond.clone(), tool_call_begin_token);

        // llm go brrr
        self.generate_response_until_done(sampler, wrapped_respond, &inference_lock_token)?;

        Ok(resp_receiver.recv()?)
    }

    pub fn reset_chat(
        &mut self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
    ) -> Result<(), SelectTemplateError> {
        self.engine.reset_context();

        // Detect tool format if not already detected and tools are provided
        if !tools.is_empty() && self.tool_format.is_none() {
            match detect_tool_format(self.engine.ctx.model) {
                Ok(format) => {
                    debug!(format = ?format, "Detected tool calling format");
                    self.tool_format = Some(format);
                }
                Err(e) => {
                    debug!(error = %e, "Failed to detect tool format, tools will not work");
                }
            }
        }

        self.tool_grammar = if !tools.is_empty() {
            if let Some(ref format) = self.tool_format {
                match format.to_lark(&tools) {
                    Ok(g) => Some(g),
                    Err(e) => {
                        debug!(error = %e, "Failed to generate grammar from tools");
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };
        let slices = self
            .tool_format
            .as_ref()
            .map_or_else(Vec::new, |f| f.slice_regexes());
        self.tool_sampler = self.tool_grammar.as_ref().and_then(|lark| {
            build_tool_sampler(&self.sampler_config, lark, slices, self.engine.ctx.model)
                .inspect_err(|e| debug!(error = %e, "Failed to pre-build tool sampler"))
                .ok()
        });
        self.tools = tools;
        self.messages = Vec::new();
        self.context = ChatContext::new();
        if let Some(sys_msg) = system_prompt {
            self.add_system_message(sys_msg);
        }
        Ok(())
    }

    /// Set a single template variable.
    pub fn set_template_variable(
        &mut self,
        name: String,
        value: bool,
    ) -> Result<(), ChatWorkerError> {
        self.template_variables.insert(name, value);
        Ok(())
    }

    /// Set all template variables, replacing any existing ones.
    pub fn set_template_variables(
        &mut self,
        variables: std::collections::HashMap<String, bool>,
    ) -> Result<(), ChatWorkerError> {
        self.template_variables = variables;
        Ok(())
    }

    /// Get all template variables.
    pub fn get_template_variables(&self) -> std::collections::HashMap<String, bool> {
        self.template_variables.clone()
    }

    pub fn set_sampler_config(&mut self, sampler_config: SamplerConfig) {
        self.sampler_config = sampler_config;
        // The pre-built tool sampler embeds the previous config — rebuild it.
        let slices = self
            .tool_format
            .as_ref()
            .map_or_else(Vec::new, |f| f.slice_regexes());
        self.tool_sampler = self.tool_grammar.as_ref().and_then(|lark| {
            build_tool_sampler(&self.sampler_config, lark, slices, self.engine.ctx.model)
                .inspect_err(|e| debug!(error = %e, "Failed to pre-build tool sampler"))
                .ok()
        });
    }

    pub fn set_system_prompt(
        &mut self,
        system_prompt: Option<String>,
    ) -> Result<(), ContextSyncError> {
        match system_prompt {
            Some(sys_msg) => {
                let system_message = Message::System { content: sys_msg };
                if self.messages.is_empty() {
                    self.messages.push(system_message);
                } else if self.messages[0].is_system() {
                    self.messages[0] = system_message;
                } else {
                    self.messages.insert(0, system_message);
                }
            }
            None => {
                if !self.messages.is_empty() && self.messages[0].is_system() {
                    self.messages.remove(0);
                }
            }
        }

        Ok(())
    }

    pub fn get_system_prompt(&self) -> Option<String> {
        if self.messages.is_empty() {
            return None;
        };
        match &self.messages[0] {
            Message::System { content } => Some(content.clone()),
            _ => None,
        }
    }

    pub fn set_tools(&mut self, tools: Vec<Tool>) -> Result<(), SetToolsError> {
        // Detect tool format if not already detected and tools are provided
        if !tools.is_empty() && self.tool_format.is_none() {
            match detect_tool_format(self.engine.ctx.model) {
                Ok(format) => {
                    debug!(format = ?format, "Detected tool calling format");
                    self.tool_format = Some(format);
                }
                Err(e) => {
                    debug!(error = %e, "Failed to detect tool format, tools will not work");
                }
            }
        }

        self.tool_grammar = if !tools.is_empty() {
            if let Some(ref format) = self.tool_format {
                match format.to_lark(&tools) {
                    Ok(g) => Some(g),
                    Err(e) => {
                        debug!(error = %e, "Failed to generate grammar from tools");
                        None
                    }
                }
            } else {
                None
            }
        } else {
            None
        };
        let slices = self
            .tool_format
            .as_ref()
            .map_or_else(Vec::new, |f| f.slice_regexes());
        self.tool_sampler = self.tool_grammar.as_ref().and_then(|lark| {
            build_tool_sampler(&self.sampler_config, lark, slices, self.engine.ctx.model)
                .inspect_err(|e| debug!(error = %e, "Failed to pre-build tool sampler"))
                .ok()
        });
        self.tools = tools;

        self.chat_template = select_template(self.engine.ctx.model, !self.tools.is_empty())?;

        Ok(())
    }

    pub fn set_chat_history(&mut self, messages: Vec<Message>) -> Result<(), ContextSyncError> {
        // get system prompt, if it is there
        let system_msg: Option<Message> = match self.messages.as_slice() {
            [msg @ Message::System { .. }, ..] => Some(msg.clone()),
            _ => None,
        };

        self.messages = system_msg.into_iter().chain(messages).collect();

        // We used to call sync_context_with_render here but this can
        // crash as some chat templates will attempt to access fields on
        // messages[0], which will result in an error. So now we never
        // sync with an empty render and we only render when there are
        // messages present in the history.

        self.context.garbage_collect_bitmaps(&self.messages);

        Ok(())
    }

    pub fn get_chat_history(&self) -> Vec<Message> {
        match self.messages.as_slice() {
            [Message::System { .. }, rest @ ..] => rest.to_vec(),
            _ => self.messages.clone(),
        }
    }

    pub fn get_sampler_config(&self) -> SamplerConfig {
        self.sampler_config.clone()
    }

    pub fn tokenize(&mut self, prompt: Prompt) -> Result<Vec<Option<i32>>, TokenizeError> {
        let media_assets = prompt.extract_media_assets();
        let bitmaps = media_assets
            .iter()
            .map(|part| match part {
                PromptPart::Image(path) => self.engine.load_image(path),
                PromptPart::Audio(path) => self.engine.load_audio(path),
                PromptPart::Text(_) => unreachable!(),
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

            // Skip system message
            if prev_msg.is_system() {
                assert!(curr_msg.is_user(), "After system should come user");
                continue;
            }

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
        println!("{}", worker.tool_grammar.as_deref().unwrap_or(""));
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
            worker.add_user_message(
                format!("This is user message number {}. What is {} * {}?", i, i, i),
                vec![],
            );
            worker.add_assistant_message(format!(
                "<think> </think> The answer is {}. Do you have any further questions?",
                i * i
            ));
        }

        worker.add_user_message("Hello!".to_string(), vec![]);

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
        // 1. System prompt should be first
        assert!(
            messages_after[0].is_system(),
            "System message should remain"
        );

        if let Message::System { content, .. } = &messages_after[0] {
            assert!(
                content.to_string().contains("helpful assistant"),
                "System prompt should be preserved"
            );
        }

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
        let token_count = worker.render_as_chunks(true)?.len();

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
            worker.add_user_message(
                format!("User message {}. What is {} * {}?", i, i, i),
                vec![],
            );

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

        worker.add_user_message("Final question!".to_string(), vec![]);

        // Check that we have many messages before shift
        let messages_before = worker.messages.len();
        println!("Messages before shift: {}", messages_before);

        // Trigger context shift
        worker.context_shift()?;

        println!("{:?}", worker.messages);

        let messages_after = worker.messages.clone();

        // Verify essential messages are preserved:
        // 1. System prompt should be first
        assert!(messages_after[0].is_system());

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
                content.to_string().contains("Final question!"),
                "Last user message should be preserved"
            );
        }

        // 5. Verify token count is within target
        let token_count = worker.render_as_chunks(true)?.len();

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
            worker.add_user_message(
                format!("This is user message number {}. What is {} * {}?", i, i, i),
                vec![],
            );
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
        // 1. System prompt should be first
        assert!(messages_after[0].is_system());

        // 2. Should have first user message
        let first_user_idx = messages_after.iter().position(|m| m.is_user());
        assert!(
            first_user_idx.is_some(),
            "First user message should be preserved"
        );

        // 3. Verify the last user message is there (the one that triggered the shift)
        let last_user = messages_after.iter().rev().find(|m| m.is_user());

        if let Some(Message::User { content, .. }) = last_user {
            assert!(
                content.to_string().contains("new question"),
                "Last user message should be preserved"
            );
        }

        // 4. Message structure should still be valid
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
            worker.add_user_message(
                format!("This is user message number {}. What is {} * {}?", i, i, i),
                vec![],
            );
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
        // 1. System prompt should be first
        assert!(messages_after[0].is_system());

        // 2. Should have first user message
        let first_user_idx = messages_after.iter().position(|m| m.is_user());
        assert!(
            first_user_idx.is_some(),
            "First user message should be preserved"
        );

        // 3. Verify the last user message is there (the one that triggered the shift)
        let last_user = messages_after.iter().rev().find(|m| m.is_user());

        if let Some(Message::User { content, .. }) = last_user {
            assert!(
                content.to_string().contains("What is"),
                "Last user message should be preserved"
            );
        }

        // 4. Message structure should still be valid
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

    // Template rendering tests have been moved to template.rs module
}
