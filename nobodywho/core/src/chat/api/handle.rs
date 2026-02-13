use crate::errors::ChatWorkerError;
use crate::llm::{self, Worker};
use crate::sampler_config::SamplerConfig;
use crate::tool_calling::Tool;
use llama_cpp_2::model::LlamaModel;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::{error, info};

use super::super::stream::{TokenStream, TokenStreamAsync};
use super::super::worker::ChatWorker;

// Configuration types

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

// Messaging types and functions

pub(crate) enum ChatMsg {
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
    SetSystemPrompt {
        system_prompt: String,
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
        output_tx: tokio::sync::mpsc::Sender<Vec<crate::chat::Message>>,
    },
    SetChatHistory {
        messages: Vec<crate::chat::Message>,
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

pub(crate) fn process_worker_msg(
    worker_state: &mut Worker<'_, ChatWorker>,
    msg: ChatMsg,
) -> Result<(), ChatWorkerError> {
    info!(?msg, "Worker processing:");
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

// Handle types and implementations

/// Interact with a ChatWorker in a blocking manner.
///
/// Use [`super::config::ChatBuilder`] to create a new instance with a fluent API.
pub struct ChatHandle {
    msg_tx: std::sync::mpsc::Sender<ChatMsg>,
    should_stop: Arc<AtomicBool>,
}

impl ChatHandle {
    /// Create a new chat handle directly. Consider using [`super::config::ChatBuilder`] for a more ergonomic API.
    pub fn new(model: Arc<LlamaModel>, config: ChatConfig) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        std::thread::spawn(move || {
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

    /// Reset the chat conversation with a new system prompt and tools.
    pub fn reset_chat(
        &self,
        system_prompt: String,
        tools: Vec<Tool>,
    ) -> Result<(), crate::errors::SetterError> {
        set_and_wait_blocking(&self.msg_tx, |output_tx| ChatMsg::ResetChat {
            system_prompt,
            tools,
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError("reset_chat".into()))
    }

    /// Reset the chat conversation history.
    pub fn reset_history(&self) -> Result<(), crate::errors::SetterError> {
        set_and_wait_blocking(&self.msg_tx, |output_tx| ChatMsg::SetChatHistory {
            messages: vec![],
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError(
            "reset_history".into(),
        ))
    }

    /// Update the available tools for the model to use.
    pub fn set_tools(&self, tools: Vec<Tool>) -> Result<(), crate::errors::SetterError> {
        set_and_wait_blocking(&self.msg_tx, |output_tx| ChatMsg::SetTools {
            tools,
            output_tx,
        })
        .ok_or(crate::errors::SetterError::SetterError("set_tools".into()))
    }

    /// Update whether the model should use thinking mode during inference.
    pub fn set_allow_thinking(
        &self,
        allow_thinking: bool,
    ) -> Result<(), crate::errors::SetterError> {
        set_and_wait_blocking(&self.msg_tx, |output_tx| ChatMsg::SetThinking {
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
        sampler_config: crate::sampler_config::SamplerConfig,
    ) -> Result<(), crate::errors::SetterError> {
        set_and_wait_blocking(&self.msg_tx, |output_tx| ChatMsg::SetSamplerConfig {
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

    /// Get the chat history without the system prompt (lower-level API).
    pub fn get_chat_history(
        &self,
    ) -> Result<Vec<crate::chat::Message>, crate::errors::GetterError> {
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
        messages: Vec<crate::chat::Message>,
    ) -> Result<(), crate::errors::SetterError> {
        set_and_wait_blocking(&self.msg_tx, |output_tx| ChatMsg::SetChatHistory {
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
    /// chat.set_system_prompt("You are a helpful coding assistant.".to_string())?;
    /// # Ok::<(), nobodywho::errors::SetterError>(())
    /// ```
    pub fn set_system_prompt(
        &self,
        system_prompt: String,
    ) -> Result<(), crate::errors::SetterError> {
        set_and_wait_blocking(&self.msg_tx, |output_tx| ChatMsg::SetSystemPrompt {
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
/// Use [`super::config::ChatBuilder`] to create a new instance with a fluent API.
#[derive(Clone)]
pub struct ChatHandleAsync {
    msg_tx: std::sync::mpsc::Sender<ChatMsg>,
    should_stop: Arc<AtomicBool>,
}

impl ChatHandleAsync {
    /// Create a new chat handle directly. Consider using [`super::config::ChatBuilder`] for a more ergonomic API.
    pub fn new(model: Arc<LlamaModel>, config: ChatConfig) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        std::thread::spawn(move || {
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

    /// Reset the chat conversation with a new system prompt and tools.
    pub async fn reset_chat(
        &self,
        system_prompt: String,
        tools: Vec<Tool>,
    ) -> Result<(), crate::errors::SetterError> {
        set_and_wait_async(&self.msg_tx, |output_tx| ChatMsg::ResetChat {
            system_prompt,
            tools,
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError("reset_chat".into()))
    }

    /// Reset the chat conversation history.
    pub async fn reset_history(&self) -> Result<(), crate::errors::SetterError> {
        set_and_wait_async(&self.msg_tx, |output_tx| ChatMsg::SetChatHistory {
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
        set_and_wait_async(&self.msg_tx, |output_tx| ChatMsg::SetTools {
            tools,
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError("set_tools".into()))
    }

    /// Update whether the model should use thinking mode during inference.
    pub async fn set_allow_thinking(
        &self,
        allow_thinking: bool,
    ) -> Result<(), crate::errors::SetterError> {
        set_and_wait_async(&self.msg_tx, |output_tx| ChatMsg::SetThinking {
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
        sampler_config: crate::sampler_config::SamplerConfig,
    ) -> Result<(), crate::errors::SetterError> {
        set_and_wait_async(&self.msg_tx, |output_tx| ChatMsg::SetSamplerConfig {
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

    /// Get the chat history without the system prompt (lower-level API).
    pub async fn get_chat_history(
        &self,
    ) -> Result<Vec<crate::chat::Message>, crate::errors::GetterError> {
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
        messages: Vec<crate::chat::Message>,
    ) -> Result<(), crate::errors::SetterError> {
        set_and_wait_async(&self.msg_tx, |output_tx| ChatMsg::SetChatHistory {
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
    /// # chat.set_system_prompt("You are a helpful coding assistant.".to_string()).await?;
    /// # Ok::<(), nobodywho::errors::SetterError>(())
    /// ```
    pub async fn set_system_prompt(
        &self,
        system_prompt: String,
    ) -> Result<(), crate::errors::SetterError> {
        set_and_wait_async(&self.msg_tx, |output_tx| ChatMsg::SetSystemPrompt {
            system_prompt,
            output_tx,
        })
        .await
        .ok_or(crate::errors::SetterError::SetterError(
            "set_system_prompt".into(),
        ))
    }
}

// Helper functions

fn set_and_wait_blocking<F>(msg_tx: &std::sync::mpsc::Sender<ChatMsg>, make_msg: F) -> Option<()>
where
    F: FnOnce(tokio::sync::mpsc::Sender<()>) -> ChatMsg,
{
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
    let msg = make_msg(output_tx);
    let _ = msg_tx.send(msg);
    // block until processed
    output_rx.blocking_recv()
}

async fn set_and_wait_async<F>(msg_tx: &std::sync::mpsc::Sender<ChatMsg>, make_msg: F) -> Option<()>
where
    F: FnOnce(tokio::sync::mpsc::Sender<()>) -> ChatMsg,
{
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(1);
    let msg = make_msg(output_tx);
    let _ = msg_tx.send(msg);
    // wait until processed
    output_rx.recv().await
}
