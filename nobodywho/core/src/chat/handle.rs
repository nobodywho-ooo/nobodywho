use crate::llm::{self, Worker};
use crate::tool_calling::Tool;
use llama_cpp_2::model::LlamaModel;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::error;

use super::config::ChatConfig;
use super::stream::{TokenStream, TokenStreamAsync};
use super::worker::messaging::{process_worker_msg, ChatMsg};

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
    ) -> Result<Vec<super::types::Message>, crate::errors::GetterError> {
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
        messages: Vec<super::types::Message>,
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
    ) -> Result<Vec<super::types::Message>, crate::errors::GetterError> {
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
        messages: Vec<super::types::Message>,
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
