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
//!
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let model = llm::get_model("model.gguf", true)?;
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
    ChatWorkerError, ContextSyncError, DecodingError, GenerateResponseError, InitWorkerError,
    RenderError, SayError, SelectTemplateError, SetToolsError, ShiftError, WrappedResponseError,
};
use crate::llm::{self};
use crate::llm::{GlobalInferenceLockToken, GLOBAL_INFERENCE_LOCK};
use crate::llm::{Worker, WorkerGuard, WriteOutput};
use crate::sampler_config::{SamplerConfig, ShiftStep};
use crate::template::{select_template, ChatTemplate, ChatTemplateContext};
use crate::tool_calling::{detect_tool_format, Tool, ToolCall, ToolFormat};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::{context::params::LlamaPoolingType, model::LlamaModel};
use serde::{Deserialize, Serialize};
use std::cmp::min;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, MutexGuard};
use tracing::{debug, error, info, trace, trace_span};

#[derive(Deserialize, Serialize, Clone, PartialEq, Eq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

#[derive(Deserialize, Serialize, Clone, Debug)]
#[serde(untagged)]
pub enum Message {
    Message {
        role: Role,
        content: String,
    },
    // it's kind of weird to have the content field in here
    // but according to the qwen3 docs, it should be an empty field on tool call messages
    // https://github.com/QwenLM/Qwen3/blob/e5a1d326/docs/source/framework/function_call.md
    // this also causes a crash when rendering qwen3 chat template, because it tries to get the
    // length of the content field, which is otherwise undefined
    ToolCalls {
        role: Role,
        content: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolResp {
        role: Role,
        name: String,
        content: String,
    },
}

impl Message {
    pub fn role(&self) -> &Role {
        match self {
            Message::Message { role, .. }
            | Message::ToolCalls { role, .. }
            | Message::ToolResp { role, .. } => role,
        }
    }

    pub fn content(&self) -> &str {
        match self {
            Message::Message { content, .. }
            | Message::ToolCalls { content, .. }
            | Message::ToolResp { content, .. } => content,
        }
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
    /// Whether to allow thinking mode during inference.
    pub allow_thinking: bool,
    /// Sampler configuration for inference.
    pub sampler_config: SamplerConfig,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            n_ctx: 4096,
            allow_thinking: true,
            system_prompt: None,
            tools: Vec::new(),
            sampler_config: SamplerConfig::default(),
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
/// let model = llm::get_model("model.gguf", true)?;
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
    model: Arc<LlamaModel>,
    config: ChatConfig,
}

impl ChatBuilder {
    /// Create a new chat builder with a model.
    pub fn new(model: Arc<LlamaModel>) -> Self {
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

    /// Allow thinking mode during inference.
    pub fn with_allow_thinking(mut self, allow_thinking: bool) -> Self {
        self.config.allow_thinking = allow_thinking;
        self
    }

    /// Set a custom sampler configuration
    pub fn with_sampler(mut self, sampler: SamplerConfig) -> Self {
        self.config.sampler_config = sampler;
        self
    }

    /// Build a blocking chat handle and start the background worker.
    pub fn build(self) -> ChatHandle {
        ChatHandle::new(self.model, self.config)
    }

    /// Build an async chat handle and start the background worker.
    pub fn build_async(self) -> ChatHandleAsync {
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
    pub fn new(model: Arc<LlamaModel>, config: ChatConfig) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        let join_handle = std::thread::spawn(move || {
            let worker = Worker::new_chat_worker(&model, config, should_stop_clone);
            let mut worker_state = match worker {
                Ok(worker_state) => worker_state,
                Err(errmsg) => {
                    return error!("Could not set up the worker initial state: {errmsg}")
                }
            };

            while let Ok(msg) = msg_rx.recv() {
                if let Err(e) = process_worker_msg(&mut worker_state, msg) {
                    return error!("Worker crashed: {e}");
                }
            }
        });

        Self {
            guard: WorkerGuard::new(msg_tx, join_handle, Some(should_stop)),
        }
    }

    /// Send a message and get a tokio channel
    /// TODO: deprecate this in favor of plain `ask` once integrations are updated
    pub fn ask_channel(
        &self,
        text: impl Into<String>,
    ) -> tokio::sync::mpsc::Receiver<llm::WriteOutput> {
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(4096);
        self.guard.send(ChatMsg::Ask {
            text: text.into(),
            output_tx,
        });
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
    pub fn ask(&self, text: impl Into<String>) -> TokenStream {
        TokenStream::new(self.ask_channel(text))
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

    /// Update whether the model should use thinking mode during inference.
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
    /// # let model = get_model("model.gguf", true).unwrap();
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
    pub fn new(model: Arc<LlamaModel>, config: ChatConfig) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        let join_handle = std::thread::spawn(move || {
            let worker = Worker::new_chat_worker(&model, config, should_stop_clone);
            let mut worker_state = match worker {
                Ok(worker_state) => worker_state,
                Err(errmsg) => {
                    return error!("Could not set up the worker initial state: {errmsg}")
                }
            };

            while let Ok(msg) = msg_rx.recv() {
                if let Err(e) = process_worker_msg(&mut worker_state, msg) {
                    return error!("Worker crashed: {e}");
                }
            }
        });

        Self {
            guard: Arc::new(WorkerGuard::new(msg_tx, join_handle, Some(should_stop))),
        }
    }

    /// Send a message and get a tokio channel
    /// TODO: deprecate this in favor of plain `ask` once integrations are updated
    pub fn ask_channel(
        &self,
        text: impl Into<String>,
    ) -> tokio::sync::mpsc::Receiver<llm::WriteOutput> {
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(4096);
        self.guard.send(ChatMsg::Ask {
            text: text.into(),
            output_tx,
        });
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
    pub fn ask(&self, text: impl Into<String>) -> TokenStreamAsync {
        TokenStreamAsync::new(self.ask_channel(text))
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

    /// Update whether the model should use thinking mode during inference.
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
    /// # let model = get_model("model.gguf", true).unwrap();
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
}

/// A stream of tokens from the model.
pub struct TokenStream {
    rx: tokio::sync::mpsc::Receiver<llm::WriteOutput>,
    completed_response: Option<String>,
}

impl TokenStream {
    fn new(rx: tokio::sync::mpsc::Receiver<llm::WriteOutput>) -> Self {
        Self {
            rx,
            completed_response: None,
        }
    }

    /// Get the next token from the stream.
    pub fn next_token(&mut self) -> Option<String> {
        if self.completed_response.is_some() {
            return None;
        }

        if let Some(output) = self.rx.blocking_recv() {
            match output {
                llm::WriteOutput::Token(token) => return Some(token),
                llm::WriteOutput::Done(completed_response) => {
                    self.completed_response = Some(completed_response);
                    return None;
                }
            }
        }
        None
    }

    /// Blocks until the  entire response is completed. Does not consume the response, so this
    /// method is idempotent.
    pub fn completed(&mut self) -> Result<String, crate::errors::CompletionError> {
        loop {
            match self.next_token() {
                Some(_) => {
                    continue;
                }
                None => {
                    return self
                        .completed_response
                        .clone()
                        .ok_or(crate::errors::CompletionError::WorkerCrashed);
                }
            }
        }
    }
}

/// A stream of tokens from the model, async version.
pub struct TokenStreamAsync {
    rx: tokio::sync::mpsc::Receiver<llm::WriteOutput>,
    completed_response: Option<String>,
}

impl TokenStreamAsync {
    pub fn new(rx: tokio::sync::mpsc::Receiver<llm::WriteOutput>) -> Self {
        Self {
            rx,
            completed_response: None,
        }
    }

    /// Waits for the next token in the stream. Consumes the token when emitted.
    pub async fn next_token(&mut self) -> Option<String> {
        if self.completed_response.is_some() {
            return None;
        }

        if let Some(output) = self.rx.recv().await {
            match output {
                llm::WriteOutput::Token(token) => return Some(token),
                llm::WriteOutput::Done(completed_response) => {
                    self.completed_response = Some(completed_response);
                    return None;
                }
            }
        }
        None
    }

    /// Waits for the entire response to be completed. Does not consume the response, so this
    /// method is idempotent.
    pub async fn completed(&mut self) -> Result<String, crate::errors::CompletionError> {
        loop {
            match self.next_token().await {
                Some(_) => {
                    continue;
                }
                None => {
                    return self
                        .completed_response
                        .clone()
                        .ok_or(crate::errors::CompletionError::WorkerCrashed);
                }
            }
        }
    }
}

enum ChatMsg {
    Ask {
        text: String,
        output_tx: tokio::sync::mpsc::Sender<llm::WriteOutput>,
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
    SetThinking {
        allow_thinking: bool,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    SetSamplerConfig {
        sampler_config: SamplerConfig,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    GetChatHistory {
        output_tx: tokio::sync::mpsc::Sender<Vec<Message>>,
    },
    SetChatHistory {
        messages: Vec<Message>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
}

impl std::fmt::Debug for ChatMsg {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChatMsg::Ask { text, .. } => f.debug_struct("Ask").field("text", text).finish(),
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
            ChatMsg::SetThinking { allow_thinking, .. } => f
                .debug_struct("SetThinking")
                .field("allow_thinking", allow_thinking)
                .finish(),
            ChatMsg::SetSamplerConfig { sampler_config, .. } => f
                .debug_struct("SetSamplerConfig")
                .field("sampler_config", sampler_config)
                .finish(),
            ChatMsg::GetChatHistory { .. } => f.debug_struct("GetChatHistory").finish(),
            ChatMsg::SetChatHistory { messages, .. } => f
                .debug_struct("SetChatHistory")
                .field("messages", &format!("[{} messages]", messages.len()))
                .finish(),
        }
    }
}

fn process_worker_msg(
    worker_state: &mut Worker<'_, ChatWorker>,
    msg: ChatMsg,
) -> Result<(), ChatWorkerError> {
    info!(?msg, "Worker processing:");
    match msg {
        ChatMsg::Ask { text, output_tx } => {
            let should_stop = Arc::clone(&worker_state.extra.should_stop);
            let callback = move |out| {
                if output_tx.try_send(out).is_err() {
                    // Receiver was dropped or the buffer is full with nobody consuming.
                    // Either way, stop generating immediately.
                    should_stop.store(true, std::sync::atomic::Ordering::Relaxed);
                }
            };
            worker_state.ask(text, callback)?;
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
        ChatMsg::SetThinking {
            allow_thinking,
            output_tx,
        } => {
            worker_state.set_allow_thinking(allow_thinking)?;
            let _ = output_tx.blocking_send(());
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
    };

    Ok(())
}

// TOOLS TYPE STUFF

// the callback closure isn't normally Send
// but we just cheat a little here
// so far it has been fine...
// unsafe impl Send for Tool {}

// TOOL CHAT WORKER

/// Utility function for prefix caching
/// Given a rendered chat template (intended for the LLM's context),
/// it compares with the tokens currently in the LLM's context, to find a common prefix.
/// The return value is a tuple of:
/// - the index of the first differing token
///   and
/// - the tokens that should be read into the context (starting at that index)
fn find_prefix_index_and_difference_with_tokens_in_context(
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

struct ChatWorker {
    should_stop: Arc<AtomicBool>,
    tool_grammar: Option<gbnf::GbnfGrammar>,
    tool_format: Option<ToolFormat>,
    sampler_config: SamplerConfig,
    messages: Vec<Message>,
    tokens_in_context: Vec<LlamaToken>,
    allow_thinking: bool,
    tools: Vec<Tool>,
    chat_template: ChatTemplate,
}

impl llm::PoolingType for ChatWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::None
    }
}

impl Worker<'_, ChatWorker> {
    fn new_chat_worker(
        model: &Arc<LlamaModel>,
        config: ChatConfig,
        should_stop: Arc<AtomicBool>,
    ) -> Result<Worker<'_, ChatWorker>, InitWorkerError> {
        let template = select_template(model, !config.tools.is_empty())?;

        // Only detect tool calling format if tools are provided
        let (tool_format, grammar) = if !config.tools.is_empty() {
            match detect_tool_format(model) {
                Ok(format) => {
                    debug!(format = ?format, "Detected tool calling format");

                    let grammar = match format.generate_grammar(&config.tools) {
                        Ok(g) => Some(g),
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

        Worker::new_with_type(
            model,
            config.n_ctx,
            false,
            ChatWorker {
                should_stop,
                tool_grammar: grammar,
                tool_format,
                sampler_config: config.sampler_config,
                messages: match config.system_prompt {
                    Some(msg) => vec![Message::Message {
                        role: Role::System,
                        content: msg,
                    }],
                    None => vec![],
                },
                chat_template: template,
                allow_thinking: config.allow_thinking,
                tools: config.tools,
                tokens_in_context: Vec::new(),
            },
        )
    }

    fn should_stop(&self) -> bool {
        self.extra
            .should_stop
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn add_system_message(&mut self, content: String) {
        self.add_message(Role::System, content)
    }

    pub fn add_assistant_message(&mut self, content: String) {
        self.add_message(Role::Assistant, content)
    }

    pub fn add_user_message(&mut self, content: String) {
        self.add_message(Role::User, content)
    }

    fn add_message(&mut self, role: Role, content: String) {
        self.extra.messages.push(Message::Message { role, content });
    }

    pub fn add_tool_calls(&mut self, tool_calls: Vec<ToolCall>) {
        self.extra.messages.push(Message::ToolCalls {
            role: Role::Assistant,
            content: "".into(),
            tool_calls,
        });
    }

    pub fn add_tool_resp(&mut self, name: String, content: String) {
        self.extra.messages.push(Message::ToolResp {
            role: Role::Tool,
            name,
            content,
        });
    }

    /// Compare tokens from a template-rendered chat history with the tokens in the LLM's context,
    /// and perform the LLM 'reading' to make the LLM's context match the rendered tokens exactly.
    /// Because this invokes the model, this is potentially an expensive method to call.
    fn sync_context_with_render(
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

    fn context_shift(&mut self) -> Result<(), ShiftError> {
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
                                tools: Some(self.extra.tools.clone()).filter(|t| !t.is_empty()),
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

    fn find_next_user_message(&self, messages: &[Message], start_index: usize) -> Option<usize> {
        messages[start_index..]
            .iter()
            .position(|msg| msg.role() == &Role::User)
            .map(|pos| pos + start_index)
    }

    fn find_start_of_last_n_user_messages(&self, messages: &[Message], n: usize) -> Option<usize> {
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
        let mut tokens_written_until_now = vec![];

        // initialize sampler
        // stateful samplers only live for one response
        let mut sampler = sampler_config.to_stateful(self.ctx.model)?;

        // init statefull decoder for split up tokens like emojis
        let mut decoder = encoding_rs::UTF_8.new_decoder();

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

            trace!(?new_token, ?token_str);
            let has_eog = self.ctx.model.is_eog_token(new_token);

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

    fn sample_and_decode_next_token(
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

    pub fn ask<F>(&mut self, text: String, respond: F) -> Result<&mut Self, SayError>
    where
        F: Fn(llm::WriteOutput) + Clone,
    {
        // reset the stop flag
        self.extra
            .should_stop
            .store(false, std::sync::atomic::Ordering::Relaxed);

        // Get the tool call begin token from the format if tools are configured
        let tool_call_begin = self
            .extra
            .tool_format
            .as_ref()
            .map(|fmt| fmt.begin_token().to_string());

        self.add_user_message(text);

        // Modify sampler with tool grammar if we have tools
        let sampler = self.extra.tool_grammar.as_ref().map_or(
            self.extra.sampler_config.clone(),
            |tool_grammar| {
                self.extra
                    .sampler_config
                    .clone()
                    .prepend(ShiftStep::Grammar {
                        trigger_on: tool_call_begin.clone(),
                        root: "superroot".into(),
                        grammar: tool_grammar.as_str().into(),
                    })
            },
        );

        // get the finished response
        let mut response: String = self.wrapped_update_context_and_generate_response(
            sampler.clone(),
            respond.clone(),
            tool_call_begin.clone(),
        )?;

        // Process tool calls if tool format is configured
        // Clone to avoid borrow issues in the loop
        if let Some(tool_format) = self.extra.tool_format.clone() {
            while let Some(tool_calls) = tool_format.extract_tool_calls(&response) {
                debug!(?tool_calls, "Got tool calls:");

                self.add_tool_calls(tool_calls.clone());

                for tool_call in tool_calls {
                    // find the tool
                    // this is just a stupid linear search
                    // but I think it's probably faster than something fancy as long as we have few tools
                    // /shrug I'm happy to be wrong
                    let Some(tool) = self.extra.tools.iter().find(|t| t.name == tool_call.name)
                    else {
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

        // Update tokens_in_context as the model already has seen this respone
        self.extra.tokens_in_context = self.get_render_as_tokens()?;

        Ok(self)
    }

    fn get_render_as_tokens(&mut self) -> Result<Vec<LlamaToken>, RenderError> {
        let render_as_string = self.extra.chat_template.render(
            &self.extra.messages,
            &ChatTemplateContext {
                enable_thinking: self.extra.allow_thinking,
                tools: Some(self.extra.tools.clone()).filter(|t| !t.is_empty()),
            },
        )?;

        let render_as_tokens = self
            .ctx
            .model
            .str_to_token(&render_as_string, self.add_bos)?;
        Ok(render_as_tokens)
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
        let mut rendered_tokens = self.get_render_as_tokens()?;

        if rendered_tokens.len() > self.ctx.n_ctx() as usize {
            self.context_shift()?;
            rendered_tokens = self.get_render_as_tokens()?;
        }

        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();
        self.sync_context_with_render(rendered_tokens, &inference_lock_token)?;

        // wrap the response callback to keep a copy of the completed response
        // and to avoid emitting tool calls
        let (wrapped_respond, resp_receiver) = wrap_respond(respond.clone(), tool_call_begin_token);

        // llm go brrr
        self.generate_response_until_done(sampler, wrapped_respond, &inference_lock_token)?;

        Ok(resp_receiver.recv()?)
    }

    pub fn reset_chat(
        &mut self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
    ) -> Result<(), SelectTemplateError> {
        self.reset_context();

        // Detect tool format if not already detected and tools are provided
        if !tools.is_empty() && self.extra.tool_format.is_none() {
            match detect_tool_format(self.ctx.model) {
                Ok(format) => {
                    debug!(format = ?format, "Detected tool calling format");
                    self.extra.tool_format = Some(format);
                }
                Err(e) => {
                    debug!(error = %e, "Failed to detect tool format, tools will not work");
                }
            }
        }

        self.extra.tool_grammar = if !tools.is_empty() {
            if let Some(ref format) = self.extra.tool_format {
                match format.generate_grammar(&tools) {
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
        self.extra.tools = tools;
        self.extra.messages = Vec::new();
        self.extra.tokens_in_context = Vec::new();
        if let Some(sys_msg) = system_prompt {
            self.add_system_message(sys_msg);
        }
        Ok(())
    }

    pub fn set_allow_thinking(&mut self, allow_thinking: bool) -> Result<(), ChatWorkerError> {
        self.extra.allow_thinking = allow_thinking;
        Ok(())
    }

    pub fn set_sampler_config(&mut self, sampler_config: SamplerConfig) {
        self.extra.sampler_config = sampler_config;
    }

    pub fn set_system_prompt(
        &mut self,
        system_prompt: Option<String>,
    ) -> Result<(), ContextSyncError> {
        match system_prompt {
            Some(sys_msg) => {
                let system_message = Message::Message {
                    role: Role::System,
                    content: sys_msg,
                };
                if self.extra.messages.is_empty() {
                    self.extra.messages.push(system_message);
                } else if *self.extra.messages[0].role() == Role::System {
                    self.extra.messages[0] = system_message;
                } else {
                    self.extra.messages.insert(0, system_message);
                }
            }
            None => {
                if !self.extra.messages.is_empty() && *self.extra.messages[0].role() == Role::System
                {
                    self.extra.messages.remove(0);
                }
            }
        }

        // Reuse cached prefix

        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();
        let rendered_tokens = self.get_render_as_tokens()?;
        self.sync_context_with_render(rendered_tokens, &inference_lock_token)?;

        Ok(())
    }

    pub fn set_tools(&mut self, tools: Vec<Tool>) -> Result<(), SetToolsError> {
        // Detect tool format if not already detected and tools are provided
        if !tools.is_empty() && self.extra.tool_format.is_none() {
            match detect_tool_format(self.ctx.model) {
                Ok(format) => {
                    debug!(format = ?format, "Detected tool calling format");
                    self.extra.tool_format = Some(format);
                }
                Err(e) => {
                    debug!(error = %e, "Failed to detect tool format, tools will not work");
                }
            }
        }

        self.extra.tool_grammar = if !tools.is_empty() {
            if let Some(ref format) = self.extra.tool_format {
                match format.generate_grammar(&tools) {
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
        self.extra.tools = tools;

        self.extra.chat_template = select_template(self.ctx.model, !self.extra.tools.is_empty())?;

        // Reuse cached prefix

        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();
        let rendered_tokens = self.get_render_as_tokens()?;
        self.sync_context_with_render(rendered_tokens, &inference_lock_token)?;

        Ok(())
    }

    pub fn set_chat_history(&mut self, messages: Vec<Message>) -> Result<(), ContextSyncError> {
        // get system prompt, if it is there
        let system_msg: Option<Message> = match self.extra.messages.as_slice() {
            [msg @ Message::Message {
                role: Role::System, ..
            }, ..] => Some(msg.clone()),
            _ => None,
        };

        self.extra.messages = system_msg.into_iter().chain(messages).collect();

        // Reuse cached prefix
        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();
        let rendered_tokens = self.get_render_as_tokens()?;
        self.sync_context_with_render(rendered_tokens, &inference_lock_token)?;

        Ok(())
    }

    pub fn get_chat_history(&self) -> Vec<Message> {
        match self.extra.messages.as_slice() {
            [Message::Message {
                role: Role::System, ..
            }, rest @ ..] => rest.to_vec(),
            _ => self.extra.messages.clone(),
        }
    }
}

/// wraps a response function in a closure to do two things:
/// 1. save a copy of the response (using a channel) before sending it out
/// 2. skip emitting once a tool_call_begin_token has been seen
fn wrap_respond<F>(
    respond: F,
    tool_call_begin_token: Option<String>,
) -> (
    impl FnMut(llm::WriteOutput),
    std::sync::mpsc::Receiver<String>,
)
where
    F: Fn(llm::WriteOutput),
{
    let (resp_sender, resp_receiver) = std::sync::mpsc::channel();
    let mut emitting = true;

    let wrapped_respond = move |x| {
        match &x {
            llm::WriteOutput::Token(tok) if tool_call_begin_token.as_ref() == Some(tok) => {
                emitting = false;
            }
            llm::WriteOutput::Done(resp) => {
                resp_sender
                    .send(resp.clone())
                    .expect("Failed sending response");
            }
            llm::WriteOutput::Token(_) => (),
        }
        if emitting {
            respond(x)
        }
    };
    (wrapped_respond, resp_receiver)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;

    // Helper function to verify message structure is valid
    fn assert_valid_message_structure(messages: &[Message]) {
        for i in 1..messages.len() {
            let prev_msg = &messages[i - 1];
            let curr_msg = &messages[i];
            let prev_role = prev_msg.role();
            let curr_role = curr_msg.role();

            // Skip system message
            if prev_role == &Role::System {
                assert_eq!(curr_role, &Role::User, "After system should come user");
                continue;
            }

            // User should be followed by assistant role (either tool calls or assistant message)
            if prev_role == &Role::User {
                assert_eq!(
                    curr_role,
                    &Role::Assistant,
                    "User message should be followed by assistant role"
                );
            }

            // Assistant role: check if it's tool calls or assistant message
            if prev_role == &Role::Assistant {
                if matches!(prev_msg, Message::ToolCalls { .. }) {
                    // Tool calls should be followed by tool response
                    assert_eq!(
                        curr_role,
                        &Role::Tool,
                        "Tool calls should be followed by tool response"
                    );
                } else {
                    // Assistant message should be followed by user
                    assert_eq!(
                        curr_role,
                        &Role::User,
                        "Assistant message should be followed by user"
                    );
                }
            }

            // Tool response should be followed by either another tool response or assistant
            if prev_role == &Role::Tool {
                assert!(
                    curr_role == &Role::Tool || curr_role == &Role::Assistant,
                    "Tool response should be followed by another tool response or assistant"
                );
            }
        }
    }

    #[test]
    fn test_chat_worker() -> Result<(), Box<dyn std::error::Error>> {
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let mut worker = Worker::new_chat_worker(
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

        worker.ask("What is the capital of Denmark?".to_string(), f.clone())?;

        let resp = receiver.recv()?;
        println!("{}", resp);

        assert!(resp.contains("Copenhagen"));

        worker.ask("What language do they speak there?".to_string(), f)?;
        let resp = receiver.recv()?;
        println!("{}", resp);

        assert!(resp.contains("Danish"));

        Ok(())
    }

    #[test]
    fn test_reset_chat() -> Result<(), Box<dyn std::error::Error>> {
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Worker::new_chat_worker(
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
        worker.ask("What is the capital of Denmark?".to_string(), f.clone())?;
        let resp1 = receiver.recv()?;
        println!("{}", resp1);
        assert!(resp1.to_lowercase().contains("woof"));

        // reset
        let _ = worker.reset_chat(
            Some("You're a cat. End all responses with 'meow'".into()),
            vec![],
        );

        // do it again
        worker.ask("What is the capital of Denmark?".to_string(), f.clone())?;
        let resp2 = receiver.recv()?;
        println!("{}", resp2);
        assert!(resp2.to_lowercase().contains("meow"));

        Ok(())
    }

    #[test]
    fn test_stop_mid_write() -> Result<(), Box<dyn std::error::Error>> {
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Worker::new_chat_worker(
            &model,
            ChatConfig {
                system_prompt: Some("You are a counter, only outputting numbers".into()),
                n_ctx: 1024,
                ..ChatConfig::default()
            },
            Arc::new(AtomicBool::new(false)),
        )?;
        let should_stop = worker.extra.should_stop.clone();

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
        };

        worker.ask("Count from 0 to 9".to_string(), f.clone())?;

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

    #[test]
    fn test_tool_chat() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Worker::new_chat_worker(
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
        println!("{}", worker.extra.tool_grammar.unwrap().as_str());
        assert!(result.contains("13.37"));
        assert!(result.contains("42.69"));
    }

    #[test]
    fn test_multi_tool_call() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Worker::new_chat_worker(
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
            .build();

        let dog_response = chat.ask("Hello!").completed().unwrap();

        assert!(dog_response.contains("woof"));

        chat.set_system_prompt(Some("You are a cat. End all responses with meow.".into()))
            .unwrap();
        let cat_response = chat.ask("Hello again!").completed().unwrap();

        assert!(cat_response.contains("meow"));
    }

    #[test]
    fn test_context_shift() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        // Use a very small context size to force shifting
        let n_ctx = 512;
        let n_messages = 8;
        let mut worker = Worker::new_chat_worker(
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

        worker.add_user_message("Hello!".into());

        // Check that we have many messages before shift
        let messages_before = worker.extra.messages.len();
        assert!(
            messages_before > 6,
            "Should have more than 6 messages before shift"
        );

        // Trigger context shift
        worker.context_shift()?;

        println!("{:?}", worker.extra.messages);

        let messages_after = worker.extra.messages.clone();

        // Verify essential messages are preserved:
        // 1. System prompt should be first
        assert_eq!(
            messages_after[0].role(),
            &Role::System,
            "System message should remain"
        );

        if let Message::Message { content, .. } = &messages_after[0] {
            assert!(
                content.contains("helpful assistant"),
                "System prompt should be preserved"
            );
        }

        // 2. Should have first user message
        let first_user_idx = messages_after.iter().position(|m| m.role() == &Role::User);
        assert!(
            first_user_idx.is_some(),
            "First user message should be preserved"
        );

        // 3. Count remaining user messages - should have at least 3 (first + last 2)
        let user_count = messages_after
            .iter()
            .filter(|m| m.role() == &Role::User)
            .count();
        assert!(
            user_count >= 3,
            "Should preserve first user message and last 2 user messages"
        );

        // 4. Verify the last user message is there
        let last_user = messages_after
            .iter()
            .rev()
            .find(|m| m.role() == &Role::User);

        if let Some(Message::Message { content, .. }) = last_user {
            assert!(
                content.contains("Hello!"),
                "Last user message should be preserved"
            );
        }

        // 5. Verify token count is within target
        let token_count = worker.get_render_as_tokens()?.len();

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
        let mut worker = Worker::new_chat_worker(
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

        worker.add_user_message("Final question!".into());

        // Check that we have many messages before shift
        let messages_before = worker.extra.messages.len();
        println!("Messages before shift: {}", messages_before);

        // Trigger context shift
        worker.context_shift()?;

        println!("{:?}", worker.extra.messages);

        let messages_after = worker.extra.messages.clone();

        // Verify essential messages are preserved:
        // 1. System prompt should be first
        assert_eq!(messages_after[0].role(), &Role::System);

        // 2. Should have first user message
        let first_user_idx = messages_after.iter().position(|m| m.role() == &Role::User);
        assert!(
            first_user_idx.is_some(),
            "First user message should be preserved"
        );

        // 3. Count remaining user messages - should have at least 3 (first + last 2)
        let user_count = messages_after
            .iter()
            .filter(|m| m.role() == &Role::User)
            .count();
        assert!(
            user_count >= 3,
            "Should preserve first user message and last 2 user messages"
        );

        // 4. Verify the last user message is there
        let last_user = messages_after
            .iter()
            .rev()
            .find(|m| m.role() == &Role::User);

        if let Some(Message::Message { content, .. }) = last_user {
            assert!(
                content.contains("Final question!"),
                "Last user message should be preserved"
            );
        }

        // 5. Verify token count is within target
        let token_count = worker.get_render_as_tokens()?.len();

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
        let mut worker = Worker::new_chat_worker(
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

        let messages_before_shift = worker.extra.messages.len();
        println!("Messages before shift: {}", messages_before_shift);

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| {
            if let llm::WriteOutput::Done(resp) = x {
                sender.send(resp).unwrap();
            }
        };

        // This should trigger context shift internally because there's not enough space
        worker.ask(
            "This is a new question that will not fit in the context! What is 10 * 10?".to_string(),
            f,
        )?;

        let _response = receiver.recv()?;
        let messages_after = worker.extra.messages.clone();

        println!("Messages after operation: {}", messages_after.len());

        // Verify context shift occurred
        assert!(
            messages_after.len() < messages_before_shift,
            "Context shift should have reduced message count"
        );

        // Verify essential messages are preserved
        // 1. System prompt should be first
        assert_eq!(messages_after[0].role(), &Role::System);

        // 2. Should have first user message
        let first_user_idx = messages_after.iter().position(|m| m.role() == &Role::User);
        assert!(
            first_user_idx.is_some(),
            "First user message should be preserved"
        );

        // 3. Verify the last user message is there (the one that triggered the shift)
        let last_user = messages_after
            .iter()
            .rev()
            .find(|m| m.role() == &Role::User);

        if let Some(Message::Message { content, .. }) = last_user {
            assert!(
                content.contains("new question"),
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
        let mut worker = Worker::new_chat_worker(
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

        let messages_before_shift = worker.extra.messages.len();
        println!("Messages before shift: {}", messages_before_shift);

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| {
            if let llm::WriteOutput::Done(resp) = x {
                sender.send(resp).unwrap();
            }
        };

        // This should trigger context shift internally because there's not enough space
        worker.ask("What is 10 * 10?".to_string(), f)?;

        let _response = receiver.recv()?;
        let messages_after = worker.extra.messages.clone();

        println!("Messages after operation: {}", messages_after.len());

        // Verify context shift occurred
        assert!(
            messages_after.len() < messages_before_shift,
            "Context shift should have reduced message count"
        );

        // Verify essential messages are preserved
        // 1. System prompt should be first
        assert_eq!(messages_after[0].role(), &Role::System);

        // 2. Should have first user message
        let first_user_idx = messages_after.iter().position(|m| m.role() == &Role::User);
        assert!(
            first_user_idx.is_some(),
            "First user message should be preserved"
        );

        // 3. Verify the last user message is there (the one that triggered the shift)
        let last_user = messages_after
            .iter()
            .rev()
            .find(|m| m.role() == &Role::User);

        if let Some(Message::Message { content, .. }) = last_user {
            assert!(
                content.contains("What is"),
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
                .with_allow_thinking(false)
                .build();

            chat.ask("What is the capital of Denmark?").completed()
        });

        // Start Germany chat thread
        let de_handle = std::thread::spawn(move || {
            let chat = ChatBuilder::new(model)
                .with_context_size(4096)
                .with_allow_thinking(false)
                .build();

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
    async fn test_allow_thinking() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let chat = ChatBuilder::new(model).build_async();

        let res1: String = chat
            .ask("What is the capital of Denmark?".to_string())
            .completed()
            .await?;

        assert!(
            res1.contains("<think>"),
            "Expected the model to initialize with thinking mode, but it did not"
        );

        chat.set_allow_thinking(false).await?;

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

    // Template rendering tests have been moved to template.rs module
}
