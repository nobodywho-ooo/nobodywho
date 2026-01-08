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
//!     .with_system_prompt("You are a helpful assistant")
//!     .build();
//!
//! let response = chat.ask("Hello!").completed()?;
//! # Ok(())
//! # }
//! ```
//!

use std::sync::LazyLock;

use crate::errors::{
    ChatWorkerError, DecodingError, FromModelError, GenerateResponseError, InferenceError,
    InitWorkerError, RenderError, SayError, ShiftError, WrappedResponseError,
};
use crate::llm::{self};
use crate::llm::{GlobalInferenceLockToken, GLOBAL_INFERENCE_LOCK};
use crate::llm::{Worker, WriteOutput};
use crate::sampler_config::{SamplerConfig, ShiftStep};
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::{context::params::LlamaPoolingType, model::LlamaModel};
use minijinja::{context, Environment};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::cmp::min;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, MutexGuard};
use tracing::{debug, error, info, trace, trace_span, warn};

static MINIJINJA_ENV: LazyLock<Environment> = LazyLock::new(|| {
    let mut env = Environment::new();
    env.add_function(
        "raise_exception",
        |msg: String| -> Result<(), minijinja::Error> {
            Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                msg,
            ))
        },
    );
    env.add_function("strftime_now", strftime_now);

    // add a bunch of python-isms, like str.split() or dict.get()
    // was introduced in #106 to fix the deepseek chat template
    env.set_unknown_method_callback(minijinja_contrib::pycompat::unknown_method_callback);
    env
});

fn strftime_now(format_str: &str) -> String {
    chrono::Local::now().format(format_str).to_string()
}

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
    // length of the content field, which is otherwise undefiend
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

    pub fn content(&self) -> &String {
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
    pub system_prompt: String,
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
            system_prompt: String::new(),
            tools: Vec::new(),
            sampler_config: SamplerConfig::default(),
        }
    }
}

/// Builder for creating a [`ChatHandle`] with a fluent API.
///
/// # Example
/// ```
/// use nobodywho::chat::{ChatBuilder, Tool};
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
///     .with_system_prompt("You're a helpful assistant")
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
    pub fn with_system_prompt<S: Into<String>>(mut self, prompt: S) -> Self {
        self.config.system_prompt = prompt.into();
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
    msg_tx: std::sync::mpsc::Sender<ChatMsg>,
    should_stop: Arc<AtomicBool>,
}

impl ChatHandle {
    /// Create a new chat handle directly. Consider using [`ChatBuilder`] for a more ergonomic API.
    pub fn new(model: Arc<LlamaModel>, config: ChatConfig) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        std::thread::spawn(move || {
            let Ok(mut worker_state) = Worker::new_chat_worker(&model, config, should_stop_clone)
            else {
                return error!("Could not set up the worker initial state");
            };

            while let Ok(msg) = msg_rx.recv() {
                if let Err(e) = process_worker_msg(&mut worker_state, msg) {
                    return error!("Worker crashed: {e}");
                }
            }
        });

        Self {
            msg_tx,
            should_stop,
        }
    }

    /// Send a message and get a tokio channel
    /// TODO: deprecate this in favor of plain `ask` once integrations are updated
    pub fn ask_channel(
        &self,
        text: impl Into<String>,
    ) -> tokio::sync::mpsc::Receiver<llm::WriteOutput> {
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(4096);
        let _ = self.msg_tx.send(ChatMsg::Ask {
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
        let _ = self.msg_tx.send(msg);
        // block until processed
        output_rx.blocking_recv()
    }

    /// Reset the chat conversation with a new system prompt and tools.
    pub fn reset_chat(
        &self,
        system_prompt: String,
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
        self.should_stop
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get a receiver for the chat history (lower-level API).
    pub fn get_chat_history(&self) -> Result<Vec<Message>, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        let _ = self.msg_tx.send(ChatMsg::GetChatHistory { output_tx });
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
}

/// Interact with a ChatWorker in an asynchronous manner.
///
/// Use [`ChatBuilder`] to create a new instance with a fluent API.
#[derive(Clone)]
pub struct ChatHandleAsync {
    msg_tx: std::sync::mpsc::Sender<ChatMsg>,
    should_stop: Arc<AtomicBool>,
}

impl ChatHandleAsync {
    /// Create a new chat handle directly. Consider using [`ChatBuilder`] for a more ergonomic API.
    pub fn new(model: Arc<LlamaModel>, config: ChatConfig) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        std::thread::spawn(move || {
            let Ok(mut worker_state) = Worker::new_chat_worker(&model, config, should_stop_clone)
            else {
                return error!("Could not set up the worker initial state");
            };

            while let Ok(msg) = msg_rx.recv() {
                if let Err(e) = process_worker_msg(&mut worker_state, msg) {
                    return error!("Worker crashed: {e}");
                }
            }
        });

        Self {
            msg_tx,
            should_stop,
        }
    }

    /// Send a message and get a tokio channel
    /// TODO: deprecate this in favor of plain `ask` once integrations are updated
    pub fn ask_channel(
        &self,
        text: impl Into<String>,
    ) -> tokio::sync::mpsc::Receiver<llm::WriteOutput> {
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(4096);
        let _ = self.msg_tx.send(ChatMsg::Ask {
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
        let _ = self.msg_tx.send(msg);
        // wait until processed
        output_rx.recv().await
    }

    /// Reset the chat conversation with a new system prompt and tools.
    pub async fn reset_chat(
        &self,
        system_prompt: String,
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
        self.should_stop
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get a receiver for the chat history (lower-level API).
    pub async fn get_chat_history(&self) -> Result<Vec<Message>, crate::errors::GetterError> {
        let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
        let _ = self.msg_tx.send(ChatMsg::GetChatHistory { output_tx });
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

#[derive(Debug)]
enum ChatMsg {
    Ask {
        text: String,
        output_tx: tokio::sync::mpsc::Sender<llm::WriteOutput>,
    },
    ResetChat {
        system_prompt: String,
        tools: Vec<Tool>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
    SetTools {
        tools: Vec<Tool>,
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

fn process_worker_msg(
    worker_state: &mut Worker<'_, ChatWorker>,
    msg: ChatMsg,
) -> Result<(), ChatWorkerError> {
    debug!("Worker processing msg: {:?}", msg);
    match msg {
        ChatMsg::Ask { text, output_tx } => {
            let callback = move |out| {
                let _ = output_tx.blocking_send(out);
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
            let _ = output_tx.blocking_send(worker_state.extra.messages.clone());
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

/// A tool that the model can call during conversation.
#[derive(Clone)]
pub struct Tool {
    pub name: String,
    description: String,
    json_schema: serde_json::Value,
    function: Arc<dyn Fn(serde_json::Value) -> String + Send + Sync>,
}

impl std::fmt::Debug for Tool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Tool")
            .field("name", &self.name)
            .field("description", &self.description)
            .field("json_schema", &self.json_schema)
            .field("function", &"<function>")
            .finish()
    }
}

impl Tool {
    /// Create a new tool directly. Consider using [`ToolBuilder`] for a more ergonomic API.
    pub fn new<S: Into<String>>(
        name: S,
        description: S,
        json_schema: serde_json::Value,
        function: Arc<dyn Fn(serde_json::Value) -> String + Send + Sync>,
    ) -> Self {
        Self {
            name: name.into(),
            description: description.into(),
            json_schema,
            function,
        }
    }
}

impl Serialize for Tool {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;

        let mut state = serializer.serialize_struct("Tool", 2)?;
        state.serialize_field("type", "function")?;
        state.serialize_field(
            "function",
            &json!({
                "name": self.name,
                "description": self.description,
                "parameters": self.json_schema,
            }),
        )?;
        state.end()
    }
}

fn grammar_from_tools(tools: &[Tool]) -> Result<gbnf::Grammar, gbnf::json::JsonSchemaParseError> {
    // get a json schema that describes the tool call for each tool
    let tool_call_schemas: serde_json::Value = tools
        .iter()
        .map(|tool| {
            serde_json::json!(
                {
                    "type": "object",
                    "properties": {
                        "name": { "const": tool.name, },
                        "arguments": tool.json_schema
                    },
                    "required": ["name", "arguments"]
                }
            )
        })
        .collect();

    // a json schema that describes any of the tool calls
    let tool_call_schema = serde_json::json!(
        { "oneOf": tool_call_schemas }
    );

    // a GBNF grammar for the above
    let mut json_grammar = match gbnf::Grammar::from_json_schema(&tool_call_schema.to_string()) {
        Ok(jg) => jg,
        Err(e) => {
            warn!("Failed generating grammar for tools. Probably because of a bad json schema: {e:?}.");
            return Err(e);
        }
    };

    // optional whitespace
    let ws = gbnf::ProductionItem::NonTerminal(
        gbnf::NonTerminalSymbol { name: "ws".into() },
        gbnf::RepetitionType::One,
    );

    // wrap the newly generated grammar's root in tool calling tokens
    // e.g. <tool_call> json_grammar </tool_call>
    let tool_call_rule = gbnf::GrammarItem::Rule(gbnf::Rule {
        lhs: gbnf::NonTerminalSymbol {
            name: "toolcall".into(),
        },
        rhs: gbnf::Production {
            items: vec![
                // tool call begin
                gbnf::ProductionItem::Terminal(
                    gbnf::TerminalSymbol {
                        value: "<tool_call>".into(),
                    },
                    gbnf::RepetitionType::One,
                ),
                // optional whitespace
                ws.clone(),
                // tool call json, just refer to the grammar we made from json schema
                gbnf::ProductionItem::NonTerminal(
                    gbnf::NonTerminalSymbol {
                        name: "root".into(),
                    },
                    gbnf::RepetitionType::One,
                ),
                // optional whitespace
                ws.clone(),
                // </tool_call>
                gbnf::ProductionItem::Terminal(
                    gbnf::TerminalSymbol {
                        value: "</tool_call>".into(),
                    },
                    gbnf::RepetitionType::One,
                ),
                // optional whitespace
                ws.clone(),
            ],
        },
    });

    // one or more tool calls
    let new_root_rule = gbnf::GrammarItem::Rule(gbnf::Rule {
        lhs: gbnf::NonTerminalSymbol {
            name: "superroot".into(),
        },
        rhs: gbnf::Production {
            items: vec![gbnf::ProductionItem::NonTerminal(
                gbnf::NonTerminalSymbol {
                    name: "toolcall".into(),
                },
                gbnf::RepetitionType::OneOrMore,
            )],
        },
    });

    json_grammar.items.push(tool_call_rule);
    json_grammar.items.push(new_root_rule);

    Ok(json_grammar)
}

#[derive(Debug, Deserialize, Serialize, Clone, PartialEq)]
pub struct ToolCall {
    pub name: String,
    pub arguments: serde_json::Value, // Flexible structure for arbitrary arguments
}

// TOOL CHAT WORKER

/// CHAT TEMPLATE SELECTION & RENDERING
fn select_template(
    model: &llama_cpp_2::model::LlamaModel,
    with_tools: bool,
) -> Result<String, FromModelError> {
    let default_template = model.chat_template(None)?.to_string()?;
    let tool_template = model.chat_template(Some("tool_use"));

    let template = if !with_tools {
        // no tools. use default template.
        default_template
    } else if let Ok(tool_template) = tool_template {
        // tools provided, and we have a tool template, use that.
        debug_assert!(tool_template.to_string()?.contains("tools"));
        tool_template.to_string()?
    } else if default_template.contains("tools") {
        // tools provided, but no tool template, but the default template seems to mention tools
        default_template
    } else {
        // tools provided, but we have no tool-capable template
        return Err(FromModelError::NoToolTemplate);
    };
    trace!(template);

    Ok(template)
}

/// given a chat history where the first two messages are from system and user
/// return a history where the first message is from user, and contains the system prompt as well.
/// (this is what llama.cpp does for the gemma template too)
fn concat_system_and_first_user_messages(
    messages: &[Message],
) -> Result<Vec<Message>, minijinja::Error> {
    warn!("System role not supported by this chat template. Concatenating first user message and system prompt.");
    match messages {
        [Message::Message {
            role: Role::System,
            content: first_content,
        }, Message::Message {
            role: Role::User,
            content: second_content,
        }, rest @ ..] => {
            let new_first_message = Message::Message {
                role: Role::User,
                content: format!("{}\n\n{}", first_content, second_content),
            };
            let new_messages = vec![new_first_message]
                .into_iter()
                .chain(rest.iter().cloned())
                .collect();
            Ok(new_messages)
        }
        _ => {
            // HACK: this should probably be a custom ChatStateError, and not a minijinja error
            //       but this was quick and easy rn, and we "abuse" the minijinja errors for
            //       `raise_exception` anyway...
            Err(minijinja::Error::new(
                minijinja::ErrorKind::InvalidOperation,
                "Cannot replace system prompt unless the first two messages are from system and user roles."
            ))
        }
    }
}

pub fn naive_render_message_vec(
    messages: &[Message],
    chat_template: &str,
    allow_thinking: bool,
    bos_token: &str,
    eos_token: &str,
    tools: &[Tool],
) -> Result<String, minijinja::Error> {
    let tmpl = MINIJINJA_ENV.template_from_str(chat_template)?;
    let add_generation_prompt = messages.last().is_some_and(|msg| {
        matches!(
            msg,
            Message::Message {
                role: Role::User,
                ..
            } | Message::ToolResp { .. }
        )
    });

    let ctx = context! {
        messages => messages,
        add_generation_prompt => add_generation_prompt,
        // we call it allow thinking, because not every model has thinking mode,
        // and 'enable' could then cause confusion
        enable_thinking => allow_thinking,
        bos_token => bos_token,
        eos_token => eos_token,
        tools => tools,
    };

    tmpl.render(ctx)
}

pub fn render_string(
    messages: &[Message],
    chat_template: &str,
    allow_thinking: bool,
    bos_token: &str,
    eos_token: &str,
    tools: &[Tool],
) -> Result<String, minijinja::Error> {
    let rendered_template = naive_render_message_vec(
        messages,
        chat_template,
        allow_thinking,
        bos_token,
        eos_token,
        tools,
    );
    let result = match rendered_template {
        Ok(rendered) => Ok(rendered),
        Err(err) => match err.kind() {
            minijinja::ErrorKind::InvalidOperation => {
                if err.to_string().contains("System role not supported") {
                    // this is the error message we get when rendering the gemma2 template
                    // concat the first two messages and try again
                    naive_render_message_vec(
                        &concat_system_and_first_user_messages(messages)?,
                        chat_template,
                        allow_thinking,
                        bos_token,
                        eos_token,
                        tools,
                    )
                } else if err
                    .to_string()
                    .contains("Conversation roles must alternate user/assistant/user/assistant/...")
                {
                    // this is the error we get when rendering the mistral 7b v0.3 template,
                    // which, like gemma2, does not support the system role
                    // concat the first two messages and try again
                    naive_render_message_vec(
                        &concat_system_and_first_user_messages(messages)?,
                        chat_template,
                        allow_thinking,
                        bos_token,
                        eos_token,
                        tools,
                    )
                } else {
                    Err(err)
                }
            }
            _ => Err(err),
        },
    };

    let text = result?;
    trace!(text);

    Ok(text)
}

struct ChatWorker {
    should_stop: Arc<AtomicBool>,
    tools: Vec<Tool>,
    tool_grammar: Option<gbnf::Grammar>,
    sampler_config: SamplerConfig,
    messages: Vec<Message>,
    chat_template: String,
    tokens_in_context: Vec<LlamaToken>,
    allow_thinking: bool,
    bos_token: String,
    eos_token: String,
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

        let tokenize = llama_cpp_2::model::Special::Tokenize;
        let bos = model.token_to_str(model.token_bos(), tokenize)?;
        let eos = model.token_to_str(model.token_eos(), tokenize)?;

        let grammar = if !config.tools.is_empty() {
            grammar_from_tools(&config.tools).ok()
        } else {
            None
        };

        Worker::new_with_type(
            model,
            config.n_ctx,
            false,
            ChatWorker {
                should_stop,
                tools: config.tools,
                tool_grammar: grammar,
                sampler_config: config.sampler_config,
                messages: vec![Message::Message {
                    role: Role::System,
                    content: config.system_prompt,
                }],
                chat_template: template,
                tokens_in_context: Vec::new(),
                allow_thinking: config.allow_thinking,
                bos_token: bos,
                eos_token: eos,
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

    // Context shifting

    pub fn find_prefix_index_and_difference_with_tokens_in_context(
        &self,
        tokens: &[LlamaToken],
    ) -> (u32, Vec<LlamaToken>) {
        if self.extra.tokens_in_context.is_empty() {
            return (0, tokens.to_owned());
        }

        let longest_common_prefix_index = self
            .extra
            .tokens_in_context
            .iter()
            .zip(tokens.iter())
            .position(|(a, b)| a != b);

        let (index, difference): (u32, Vec<LlamaToken>) = match longest_common_prefix_index {
            Some(i) => (i as u32, tokens[i..].to_vec()),
            None => (
                self.extra.tokens_in_context.len() as u32,
                tokens[(self.extra.tokens_in_context.len())..].to_vec(),
            ),
        };

        (index, difference)
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
                        &naive_render_message_vec(
                            &messages,
                            &self.extra.chat_template,
                            self.extra.allow_thinking,
                            &self.extra.bos_token,
                            &self.extra.eos_token,
                            &self.extra.tools,
                        )?,
                        AddBos::Never,
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

        // update the messages in chat_state
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
        let mut token_bytes_vec = Vec::new();

        while !self.should_stop() {
            // Check if the context is full
            if self.n_past as u32 == self.ctx.n_ctx() {
                self.context_shift()?;
                let render_as_tokens = self.get_render_as_tokens()?;

                let (prefix_index, token_difference) =
                    self.find_prefix_index_and_difference_with_tokens_in_context(&render_as_tokens);

                self.remove_all_tokens_after_index_from_ctx(prefix_index)?;
                self.read_tokens(token_difference, inference_lock_token)?;
                self.read_tokens(tokens_written_until_now.clone(), inference_lock_token)?;
                // do not update tokens_in_context as this is done later by ask
            }

            // Sample next token, no need to use sampler.accept as sample already accepts the token.
            // using sampler.accept() will cause the sampler to crash when using grammar sampling.
            // https://github.com/utilityai/llama-cpp-rs/issues/604
            trace!("Applying sampler...");
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
                trace!("Sending out token: {token_str}");
                respond(WriteOutput::Token(token_str.to_string()));
            }

            // done using token_str, so now we can clear token_bytes_vec
            token_bytes_vec.clear();

            if has_eog {
                break;
            }
        }

        // we're done!
        debug!("Sending out response: {full_response}");
        respond(WriteOutput::Done(full_response));
        Ok(self)
    }

    fn sample_and_decode_next_token(
        &mut self,
        sampler: &mut LlamaSampler,
    ) -> Result<LlamaToken, DecodingError> {
        trace!("Applying sampler...");
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

        // TODO: this is the token used by qwen3
        //       but e.g. deepseek uses "<｜tool▁calls▁begin｜><｜tool▁call▁begin｜>" instead.
        //       we need to support multiple different tool call begin tokens
        let tool_call_begin = "<tool_call>";

        self.add_user_message(text);

        // Modify sampler with tool grammar if we have tools
        let sampler = self.extra.tool_grammar.as_ref().map_or(
            self.extra.sampler_config.clone(),
            |tool_grammar| {
                self.extra.sampler_config.clone().shift(ShiftStep::Grammar {
                    trigger_on: Some(tool_call_begin.into()),
                    root: "superroot".into(),
                    grammar: tool_grammar.to_string(),
                })
            },
        );

        // get the finished response
        let mut response: String = self.wrapped_update_context_and_generate_response(
            sampler.clone(),
            respond.clone(),
            tool_call_begin.into(),
        )?;

        while let Some(tool_calls) = extract_tool_calls(&response) {
            debug!("Got tool calls! {tool_calls:?}");

            self.add_tool_calls(tool_calls.clone());

            for tool_call in tool_calls {
                // find the tool
                // this is just a stupid linear search
                // but I think it's probably faster than something fancy as long as we have few tools
                // /shrug I'm happy to be wrong
                let Some(tool) = self.extra.tools.iter().find(|t| t.name == tool_call.name) else {
                    // in case the tool isn't found.
                    // I *think* this should be impossible, as long as the tool calling grammar
                    // works.
                    error!(
                        "Model triggered tool call for invalid tool name: {}",
                        tool_call.name
                    );
                    let errmsg = format!("ERROR - Invalid tool name: {}", tool_call.name);
                    self.add_tool_resp(tool_call.name, errmsg);
                    continue;
                };

                // call the tool
                let response = (tool.function)(tool_call.arguments);
                debug!(?tool_call.name, ?response);

                // add to chat history
                self.add_tool_resp(tool_call.name, response);
            }

            // get the finished response
            response = self.wrapped_update_context_and_generate_response(
                sampler.clone(),
                respond.clone(),
                tool_call_begin.into(),
            )?;
        }
        debug_assert!(!response.contains(tool_call_begin));
        self.add_assistant_message(response);

        // Update tokens_in_context as the model already has seen this respone
        let render_as_tokens = self.get_render_as_tokens()?;

        self.extra.tokens_in_context = render_as_tokens;

        Ok(self)
    }

    fn get_render_as_tokens(&mut self) -> Result<Vec<LlamaToken>, RenderError> {
        let render_as_string = render_string(
            &self.extra.messages,
            &self.extra.chat_template,
            self.extra.allow_thinking,
            &self.extra.bos_token,
            &self.extra.eos_token,
            &self.extra.tools,
        )?;
        let render_as_tokens = self
            .ctx
            .model
            .str_to_token(&render_as_string, AddBos::Never)?;
        Ok(render_as_tokens)
    }

    fn read_tokens_and_generate_response(
        &mut self,
        tokens: Vec<LlamaToken>,
        sampler: SamplerConfig,
        wrapped_respond: impl FnMut(WriteOutput),
    ) -> Result<&mut Self, InferenceError> {
        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();

        Ok(self
            .read_tokens(tokens, &inference_lock_token)?
            .generate_response_until_done(sampler, wrapped_respond, &inference_lock_token)?)
    }

    fn wrapped_update_context_and_generate_response<F>(
        &mut self,
        sampler: SamplerConfig,
        respond: F,
        tool_call_begin_token: String,
    ) -> Result<String, WrappedResponseError>
    where
        F: Fn(llm::WriteOutput) + Clone,
    {
        // Check how much of the current KVCache we can keep
        let mut render_as_tokens = self.get_render_as_tokens()?;
        if render_as_tokens.len() > self.ctx.n_ctx() as usize {
            self.context_shift()?;
            render_as_tokens = self.get_render_as_tokens()?;
        }

        let (prefix_index, token_difference) =
            self.find_prefix_index_and_difference_with_tokens_in_context(&render_as_tokens);

        self.remove_all_tokens_after_index_from_ctx(prefix_index)?;

        // wrap the response callback to keep a copy of the completed response
        // and to avoid emitting tool calls
        let (wrapped_respond, resp_receiver) = wrap_respond(respond.clone(), tool_call_begin_token);

        // llm go brrr
        self.read_tokens_and_generate_response(token_difference, sampler, wrapped_respond)?;

        // update the chat_state to match the tokens in the context.
        self.extra.tokens_in_context = render_as_tokens;

        Ok(resp_receiver.recv()?)
    }

    pub fn reset_chat(
        &mut self,
        system_prompt: String,
        tools: Vec<Tool>,
    ) -> Result<(), FromModelError> {
        self.reset_context();
        self.extra.tool_grammar = if !tools.is_empty() {
            grammar_from_tools(&tools).ok()
        } else {
            None
        };
        self.extra.tools = tools;
        self.extra.messages = Vec::new();
        self.extra.tokens_in_context = Vec::new();
        self.add_system_message(system_prompt);
        Ok(())
    }

    pub fn set_allow_thinking(&mut self, allow_thinking: bool) -> Result<(), ChatWorkerError> {
        self.extra.allow_thinking = allow_thinking;
        Ok(())
    }

    pub fn set_sampler_config(&mut self, sampler_config: SamplerConfig) {
        self.extra.sampler_config = sampler_config;
    }

    pub fn set_tools(&mut self, tools: Vec<Tool>) -> Result<(), ChatWorkerError> {
        self.extra.tool_grammar = if !tools.is_empty() {
            grammar_from_tools(&tools).ok()
        } else {
            None
        };
        self.extra.tools = tools;

        // Reuse cached prefix
        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();
        let render_as_tokens = self.get_render_as_tokens()?;
        let (prefix_index, token_difference) =
            self.find_prefix_index_and_difference_with_tokens_in_context(&render_as_tokens);

        self.remove_all_tokens_after_index_from_ctx(prefix_index)?;
        self.read_tokens(token_difference, &inference_lock_token)?;
        self.extra.tokens_in_context = render_as_tokens;

        Ok(())
    }

    pub fn set_chat_history(&mut self, messages: Vec<Message>) -> Result<(), ChatWorkerError> {
        self.reset_context();
        self.extra.tokens_in_context = Vec::new();
        self.extra.messages = messages;

        // Reuse cached prefix

        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();
        let render_as_tokens = self.get_render_as_tokens()?;
        let (prefix_index, token_difference) =
            self.find_prefix_index_and_difference_with_tokens_in_context(&render_as_tokens);

        self.remove_all_tokens_after_index_from_ctx(prefix_index)?;
        self.read_tokens(token_difference, &inference_lock_token)?;
        self.extra.tokens_in_context = render_as_tokens;

        Ok(())
    }
}

/// wraps a response function in a closure to do two things:
/// 1. save a copy of the response (using a channel) before sending it out
/// 2. skip emitting once a tool_call_begin_token has been seen
fn wrap_respond<F>(
    respond: F,
    tool_call_begin_token: String,
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
            llm::WriteOutput::Token(tok) if tok == &tool_call_begin_token => {
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

fn extract_tool_calls(input: &str) -> Option<Vec<ToolCall>> {
    // Find the start and end tags
    // TODO: these are the tokens used by qwen3
    //       but e.g. deepseek uses "<｜tool▁calls▁begin｜><｜tool▁call▁begin｜>" instead.
    //       we need to support multiple different tool call begin tokens
    let re = regex::Regex::new(r"<tool_call>([\s\S]*?)</tool_call>").expect("Invalid regex");

    let tool_calls: Vec<ToolCall> = re
        .captures_iter(input)
        .filter_map(|cap| {
            let tool_call: Option<ToolCall> = serde_json::from_str(cap[1].trim()).ok();
            tool_call
        })
        .collect();

    if !tool_calls.is_empty() {
        Some(tool_calls)
    } else {
        None
    }
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
                system_prompt: "You're a dog. End all responses with 'woof'".into(),
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
        let _ = worker.reset_chat("You're a cat. End all responses with 'meow'".into(), vec![]);

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
                system_prompt: "You are a counter, only outputting numbers".into(),
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
                system_prompt: "You're a helpful assistant.".into(),
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
                system_prompt: "You are a helpful assistant that provides informative and detailed responses. End every response with \"Do you have any further questions?\"".into(),
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
                system_prompt: "You are a helpful assistant.".into(),
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
                system_prompt: "You are a helpful assistant.".into(),
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
                system_prompt: "You are a helpful assistant.".into(),
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
        let sampler = SamplerConfig::default();
        let n_ctx = 4096;

        // Use two separate response containers for thread safety
        let dk_response = Arc::new(std::sync::Mutex::new(None));
        let de_response = Arc::new(std::sync::Mutex::new(None));

        // Clone references for thread use
        let model_clone = Arc::clone(&model);
        let dk_response_clone = Arc::clone(&dk_response);
        let de_response_clone = Arc::clone(&de_response);
        let dk_sampler = sampler.clone();

        // Start Denmark worker thread
        let dk_handle = std::thread::spawn(move || {
            let mut worker = Worker::new_chat_worker(
                &model_clone,
                ChatConfig {
                    n_ctx,
                    ..Default::default()
                },
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();

            let f = move |x| {
                if let WriteOutput::Done(resp) = x {
                    let mut response = dk_response_clone.lock().unwrap();
                    *response = Some(resp);
                }
            };

            worker
                .read_tokens_and_generate_response(
                    worker.ctx.model.str_to_token("<think>\nCopenhagen is the capital of Denmark\n</think>\nThe name of the capital city of Denmark is \"", AddBos::Never).unwrap(),
                    dk_sampler,
                    f,
                )
                .unwrap();
        });

        // Start Germany worker thread
        let de_handle = std::thread::spawn(move || {
            let mut worker = Worker::new_chat_worker(
                &model,
                ChatConfig {
                    n_ctx,
                    ..Default::default()
                },
                Arc::new(AtomicBool::new(false)),
            )
            .unwrap();

            let f = move |x| {
                if let WriteOutput::Done(resp) = x {
                    let mut response = de_response_clone.lock().unwrap();
                    *response = Some(resp);
                }
            };
            worker
                .read_tokens_and_generate_response(
                    worker.ctx.model.str_to_token("<think>\nBerlin is the capital of Germany\n</think>\nThe capital of germany is called ", AddBos::Never).unwrap(),
                    sampler,
                    f,
                )
                .unwrap();
        });

        // Wait for threads to complete
        dk_handle.join().unwrap();
        de_handle.join().unwrap();

        // Retrieve and verify responses
        let dk_resp = dk_response
            .lock()
            .unwrap()
            .clone()
            .expect("No response from dk_worker");
        let de_resp = de_response
            .lock()
            .unwrap()
            .clone()
            .expect("No response from de_worker");

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

    #[test]
    fn test_strftime_now() {
        // huggingface chat template docs say that `strftime_now(format_str)` should be equivalent to `datetime.now().strftime(format_str)`
        // https://huggingface.co/docs/transformers/main/chat_templating#callable-functions

        let result = strftime_now("%Y-%m-%d");
        assert!(
            result.len() == 10,
            "Expected format YYYY-MM-DD to be 10 chars"
        );

        let result = strftime_now("%H:%M:%S");
        assert!(result.len() == 8, "Expected format HH:MM:SS to be 8 chars");
    }

    #[test]
    fn test_render_string_llama3_template() {
        // Llama 3.1 template from the existing test
        let template = "{% set loop_messages = messages %}{% for message in loop_messages %}{% set content = '<|start_header_id|>' + message['role'] + '<|end_header_id|>\n\n'+ message['content'] | trim + '<|eot_id|>' %}{% if loop.index0 == 0 %}{% set content = bos_token + content %}{% endif %}{{ content }}{% endfor %}{{ '<|start_header_id|>assistant<|end_header_id|>\n\n' }}";

        let allow_thinking = true;
        let bos = "<|begin_of_text|>";
        let eos = "<|end_of_text|>";
        let tools = vec![];

        // Test 1: Single user message
        let mut messages = vec![Message::Message {
            role: Role::User,
            content: "Hello, world!".into(),
        }];
        let rendered =
            render_string(&messages, template, allow_thinking, &bos, &eos, &tools).unwrap();

        let expected = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nHello, world!<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";
        assert_eq!(rendered, expected);

        // Test 2: Add assistant response
        messages.push(Message::Message {
            role: Role::Assistant,
            content: "Hi there! How can I help?".into(),
        });
        let rendered2 =
            render_string(&messages, template, allow_thinking, &bos, &eos, &tools).unwrap();

        let expected2 = "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nHello, world!<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\nHi there! How can I help?<|eot_id|><|start_header_id|>assistant<|end_header_id|>\n\n";
        assert_eq!(rendered2, expected2);

        // Test 3: Multi-turn conversation
        messages.push(Message::Message {
            role: Role::User,
            content: "What's the weather like?".into(),
        });
        messages.push(Message::Message {
            role: Role::Assistant,
            content: "I don't have access to weather data.".into(),
        });
        let rendered3 =
            render_string(&messages, template, allow_thinking, &bos, &eos, &tools).unwrap();

        assert!(rendered3.starts_with(
            "<|begin_of_text|><|start_header_id|>user<|end_header_id|>\n\nHello, world!<|eot_id|>"
        ));
        assert!(rendered3.contains(
            "<|start_header_id|>user<|end_header_id|>\n\nWhat's the weather like?<|eot_id|>"
        ));
        assert!(rendered3.contains("<|start_header_id|>assistant<|end_header_id|>\n\nI don't have access to weather data.<|eot_id|>"));
        assert!(rendered3.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));

        // Test 4: System message (if added first)
        let messages = vec![
            Message::Message {
                role: Role::System,
                content: "You are a helpful assistant.".into(),
            },
            Message::Message {
                role: Role::User,
                content: "Hi".into(),
            },
        ];
        let rendered4 =
            render_string(&messages, template, allow_thinking, &bos, &eos, &tools).unwrap();

        println!("{:?}", rendered4);

        assert!(rendered4.starts_with("<|begin_of_text|><|start_header_id|>system<|end_header_id|>\n\nYou are a helpful assistant.<|eot_id|>"));
        assert!(rendered4.contains("<|start_header_id|>user<|end_header_id|>\n\nHi<|eot_id|>"));
        assert!(rendered4.ends_with("<|start_header_id|>assistant<|end_header_id|>\n\n"));
    }

    #[test]
    fn test_render_string_deepseek_template() {
        // DeepSeek template from the existing test
        let template = "{% if not add_generation_prompt is defined %}{% set add_generation_prompt = false %}{% endif %}{% set ns = namespace(is_first=false, is_tool=false, is_output_first=true, system_prompt='') %}{%- for message in messages %}{%- if message['role'] == 'system' %}{% set ns.system_prompt = message['content'] %}{%- endif %}{%- endfor %}{{bos_token}}{{ns.system_prompt}}{%- for message in messages %}{%- if message['role'] == 'user' %}{%- set ns.is_tool = false -%}{{'<｜User｜>' + message['content']}}{%- endif %}{%- if message['role'] == 'assistant' and message['content'] is none %}{%- set ns.is_tool = false -%}{%- for tool in message['tool_calls']%}{%- if not ns.is_first %}{{'<｜Assistant｜><｜tool▁calls▁begin｜><｜tool▁call▁begin｜>' + tool['type'] + '<｜tool▁sep｜>' + tool['function']['name'] + '\\n' + '```json' + '\\n' + tool['function']['arguments'] + '\\n' + '```' + '<｜tool▁call▁end｜>'}}{%- set ns.is_first = true -%}{%- else %}{{'\\n' + '<｜tool▁call▁begin｜>' + tool['type'] + '<｜tool▁sep｜>' + tool['function']['name'] + '\\n' + '```json' + '\\n' + tool['function']['arguments'] + '\\n' + '```' + '<｜tool▁call▁end｜>'}}{{'<｜tool▁calls▁end｜><｜end▁of▁sentence｜>'}}{%- endif %}{%- endfor %}{%- endif %}{%- if message['role'] == 'assistant' and message['content'] is not none %}{%- if ns.is_tool %}{{'<｜tool▁outputs▁end｜>' + message['content'] + '<｜end▁of▁sentence｜>'}}{%- set ns.is_tool = false -%}{%- else %}{% set content = message['content'] %}{% if '</think>' in content %}{% set content = content.split('</think>')[-1] %}{% endif %}{{'<｜Assistant｜>' + content + '<｜end▁of▁sentence｜>'}}{%- endif %}{%- endif %}{%- if message['role'] == 'tool' %}{%- set ns.is_tool = true -%}{%- if ns.is_output_first %}{{'<｜tool▁outputs▁begin｜><｜tool▁output▁begin｜>' + message['content'] + '<｜tool▁output▁end｜>'}}{%- set ns.is_output_first = false %}{%- else %}{{'\\n<｜tool▁output▁begin｜>' + message['content'] + '<｜tool▁output▁end｜>'}}{%- endif %}{%- endif %}{%- endfor -%}{% if ns.is_tool %}{{'<｜tool▁outputs▁end｜>'}}{% endif %}{% if add_generation_prompt and not ns.is_tool %}{{'<｜Assistant｜>'}}{% endif %}";

        let allow_thinking = true;
        let bos = "<|bos|>";
        let eos = "<|eos|>";
        let tools = vec![];

        // Test 1: Single user message
        let mut messages = vec![Message::Message {
            role: Role::User,
            content: "Hello, world!".into(),
        }];
        let rendered =
            render_string(&messages, template, allow_thinking, bos, eos, &tools).unwrap();

        // render_string sets add_generation_prompt to true for user messages, so <｜Assistant｜> is added
        let expected = "<|bos|><｜User｜>Hello, world!<｜Assistant｜>";
        assert_eq!(rendered, expected);

        // Test 2: Add assistant response
        messages.push(Message::Message {
            role: Role::Assistant,
            content: "Hi there! How can I help?".into(),
        });
        let rendered2 =
            render_string(&messages, template, allow_thinking, bos, eos, &tools).unwrap();

        let expected2 = "<|bos|><｜User｜>Hello, world!<｜Assistant｜>Hi there! How can I help?<｜end▁of▁sentence｜>";
        assert_eq!(rendered2, expected2);

        // Test 3: Assistant message with thinking block
        messages.push(Message::Message {
            role: Role::User,
            content: "Can you help me?".into(),
        });
        messages.push(Message::Message {
            role: Role::Assistant,
            content: "<think>The user is asking for help</think>I'd be happy to assist you!".into(),
        });
        let rendered3 =
            render_string(&messages, template, allow_thinking, bos, eos, &tools).unwrap();

        // The thinking block should be stripped out, only the content after </think> should remain
        assert!(
            rendered3.contains("<｜Assistant｜>I'd be happy to assist you!<｜end▁of▁sentence｜>")
        );
        assert!(!rendered3.contains("<think>"));
        assert!(!rendered3.contains("</think>"));

        // Test 4: System message
        let messages = vec![
            Message::Message {
                role: Role::System,
                content: "You are a helpful assistant.".into(),
            },
            Message::Message {
                role: Role::User,
                content: "Hi".into(),
            },
        ];
        let rendered4 =
            render_string(&messages, template, allow_thinking, bos, eos, &tools).unwrap();

        let expected4 = "<|bos|>You are a helpful assistant.<｜User｜>Hi<｜Assistant｜>";
        assert_eq!(rendered4, expected4);

        // Test 5: Multi-turn conversation
        let messages = vec![
            Message::Message {
                role: Role::User,
                content: "What's 2+2?".into(),
            },
            Message::Message {
                role: Role::Assistant,
                content: "4".into(),
            },
            Message::Message {
                role: Role::User,
                content: "Thanks!".into(),
            },
        ];
        let rendered5 =
            render_string(&messages, template, allow_thinking, bos, eos, &tools).unwrap();

        let expected5 =
            "<|bos|><｜User｜>What's 2+2?<｜Assistant｜>4<｜end▁of▁sentence｜><｜User｜>Thanks!<｜Assistant｜>";
        assert_eq!(rendered5, expected5);

        // Test 6: Empty messages (no generation prompt by default)
        let messages: Vec<Message> = vec![];
        let rendered6 =
            render_string(&messages, template, allow_thinking, bos, eos, &tools).unwrap();

        let expected6 = "<|bos|>";
        assert_eq!(rendered6, expected6);
    }

    #[test]
    fn test_render_string_qwen3_template() {
        // Qwen3 template from the existing test
        let template = "{%- if tools %}\n    {{- '<|im_start|>system\\n' }}\n    {%- if messages[0].role == 'system' %}\n        {{- messages[0].content + '\\n\\n' }}\n    {%- endif %}\n    {{- \"# Tools\\n\\nYou may call one or more functions to assist with the user query.\\n\\nYou are provided with function signatures within <tools></tools> XML tags:\\n<tools>\" }}\n    {%- for tool in tools %}\n        {{- \"\\n\" }}\n        {{- tool | tojson }}\n    {%- endfor %}\n    {{- \"\\n</tools>\\n\\nFor each function call, return a json object with function name and arguments within <tool_call></tool_call> XML tags:\\n<tool_call>\\n{\\\"name\\\": <function-name>, \\\"arguments\\\": <args-json-object>}\\n</tool_call><|im_end|>\\n\" }}\n{%- else %}\n    {%- if messages[0].role == 'system' %}\n        {{- '<|im_start|>system\\n' + messages[0].content + '<|im_end|>\\n' }}\n    {%- endif %}\n{%- endif %}\n{%- set ns = namespace(multi_step_tool=true, last_query_index=messages|length - 1) %}\n{%- for message in messages[::-1] %}\n    {%- set index = (messages|length - 1) - loop.index0 %}\n    {%- if ns.multi_step_tool and message.role == \"user\" and not(message.content.startswith('<tool_response>') and message.content.endswith('</tool_response>')) %}\n        {%- set ns.multi_step_tool = false %}\n        {%- set ns.last_query_index = index %}\n    {%- endif %}\n{%- endfor %}\n{%- for message in messages %}\n    {%- if (message.role == \"user\") or (message.role == \"system\" and not loop.first) %}\n        {{- '<|im_start|>' + message.role + '\\n' + message.content + '<|im_end|>' + '\\n' }}\n    {%- elif message.role == \"assistant\" %}\n        {%- set content = message.content %}\n        {%- set reasoning_content = '' %}\n        {%- if message.reasoning_content is defined and message.reasoning_content is not none %}\n            {%- set reasoning_content = message.reasoning_content %}\n        {%- else %}\n            {%- if '</think>' in message.content %}\n                {%- set content = message.content.split('</think>')[-1].lstrip('\\n') %}\n                {%- set reasoning_content = message.content.split('</think>')[0].rstrip('\\n').split('<think>')[-1].lstrip('\\n') %}\n            {%- endif %}\n        {%- endif %}\n        {%- if loop.index0 > ns.last_query_index %}\n            {%- if loop.last or (not loop.last and reasoning_content) %}\n                {{- '<|im_start|>' + message.role + '\\n<think>\\n' + reasoning_content.strip('\\n') + '\\n</think>\\n\\n' + content.lstrip('\\n') }}\n            {%- else %}\n                {{- '<|im_start|>' + message.role + '\\n' + content }}\n            {%- endif %}\n        {%- else %}\n            {{- '<|im_start|>' + message.role + '\\n' + content }}\n        {%- endif %}\n        {%- if message.tool_calls %}\n            {%- for tool_call in message.tool_calls %}\n                {%- if (loop.first and content) or (not loop.first) %}\n                    {{- '\\n' }}\n                {%- endif %}\n                {%- if tool_call.function %}\n                    {%- set tool_call = tool_call.function %}\n                {%- endif %}\n                {{- '<tool_call>\\n{\"name\": \"' }}\n                {{- tool_call.name }}\n                {{- '\", \"arguments\": ' }}\n                {%- if tool_call.arguments is string %}\n                    {{- tool_call.arguments }}\n                {%- else %}\n                    {{- tool_call.arguments | tojson }}\n                {%- endif %}\n                {{- '}\\n</tool_call>' }}\n            {%- endfor %}\n        {%- endif %}\n        {{- '<|im_end|>\\n' }}\n    {%- elif message.role == \"tool\" %}\n        {%- if loop.first or (messages[loop.index0 - 1].role != \"tool\") %}\n            {{- '<|im_start|>user' }}\n        {%- endif %}\n        {{- '\\n<tool_response>\\n' }}\n        {{- message.content }}\n        {{- '\\n</tool_response>' }}\n        {%- if loop.last or (messages[loop.index0 + 1].role != \"tool\") %}\n            {{- '<|im_end|>\\n' }}\n        {%- endif %}\n    {%- endif %}\n{%- endfor %}\n{%- if add_generation_prompt %}\n    {{- '<|im_start|>assistant\\n' }}\n    {%- if enable_thinking is defined and enable_thinking is false %}\n        {{- '<think>\\n\\n</think>\\n\\n' }}\n    {%- endif %}\n{%- endif %}";

        let allow_thinking = true;
        let bos = "";
        let eos = "";
        let tools = vec![];

        // Test 1: Single user message
        let mut messages = vec![Message::Message {
            role: Role::User,
            content: "Hi, robot!".into(),
        }];
        let rendered =
            render_string(&messages, template, allow_thinking, bos, eos, &tools).unwrap();

        let expected = "<|im_start|>user\nHi, robot!<|im_end|>\n<|im_start|>assistant\n";
        assert_eq!(rendered, expected);

        // Test 2: Add assistant response with thinking
        messages.push(Message::Message {
            role: Role::Assistant,
            content: "<think>\nHm... That's a tough cookie. I think the answer is probably 42.\nCould it be something else?\nNah... It's 42!\n</think>\nThe answer is 42!".into(),
        });
        let rendered2 =
            render_string(&messages, template, allow_thinking, bos, eos, &tools).unwrap();

        // The thinking block should be included in the output for Qwen3
        let expected2 = "<|im_start|>user\nHi, robot!<|im_end|>\n<|im_start|>assistant\n<think>\nHm... That's a tough cookie. I think the answer is probably 42.\nCould it be something else?\nNah... It's 42!\n</think>\n\nThe answer is 42!<|im_end|>\n";
        assert_eq!(rendered2, expected2);

        // Test 3: System message
        let messages = vec![
            Message::Message {
                role: Role::System,
                content: "You are a helpful assistant.".into(),
            },
            Message::Message {
                role: Role::User,
                content: "Hello".into(),
            },
        ];
        let rendered3 =
            render_string(&messages, template, allow_thinking, bos, eos, &tools).unwrap();

        let expected3 = "<|im_start|>system\nYou are a helpful assistant.<|im_end|>\n<|im_start|>user\nHello<|im_end|>\n<|im_start|>assistant\n";
        assert_eq!(rendered3, expected3);

        // Test 4: Multi-turn conversation
        let messages = vec![
            Message::Message {
                role: Role::User,
                content: "What's 2+2?".into(),
            },
            Message::Message {
                role: Role::Assistant,
                content: "4".into(),
            },
            Message::Message {
                role: Role::User,
                content: "Thanks!".into(),
            },
        ];
        let rendered4 =
            render_string(&messages, template, allow_thinking, bos, eos, &tools).unwrap();

        let expected4 = "<|im_start|>user\nWhat's 2+2?<|im_end|>\n<|im_start|>assistant\n4<|im_end|>\n<|im_start|>user\nThanks!<|im_end|>\n<|im_start|>assistant\n";
        assert_eq!(rendered4, expected4);

        // Test 5: Assistant message without thinking
        let messages = vec![
            Message::Message {
                role: Role::User,
                content: "Hello".into(),
            },
            Message::Message {
                role: Role::Assistant,
                content: "Hi there!".into(),
            },
        ];
        let rendered5 = render_string(&messages, template, false, bos, eos, &tools).unwrap();

        // The template now includes empty thinking blocks for assistant messages
        let expected5 = "<|im_start|>user\nHello<|im_end|>\n<|im_start|>assistant\n<think>\n\n</think>\n\nHi there!<|im_end|>\n";
        assert_eq!(rendered5, expected5);
    }
}
