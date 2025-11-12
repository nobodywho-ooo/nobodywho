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
//! let response = chat.say_complete("Hello!").await?;
//! # Ok(())
//! # }
//! ```

use crate::chat_state::{self, Role};
use crate::chat_state::{ChatState, Message};
use crate::errors::{
    ChatWorkerError, DecodingError, FromModelError, GenerateResponseError, InferenceError,
    InitWorkerError, RenderError, SayError, ShiftError, WrappedResponseError,
};
use crate::llm::{self};
use crate::llm::{GlobalInferenceLockToken, GLOBAL_INFERENCE_LOCK};
use crate::llm::{Worker, WriteOutput};
use crate::sampler_config::{SamplerConfig, SamplerPresets, ShiftStep};
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use llama_cpp_2::{context::params::LlamaPoolingType, model::LlamaModel};
use std::cmp::min;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, MutexGuard};
use tracing::{debug, error, info, trace, trace_span, warn};

// PARALLELISM

/// A handle to interact with a chat session running in a background thread.
///
/// Use [`ChatBuilder`] to create a new instance with a fluent API.
pub struct ChatHandle {
    msg_tx: std::sync::mpsc::Sender<ChatMsg>,
    should_stop: Arc<AtomicBool>,
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
    n_ctx: u32,
    system_prompt: String,
    tools: Vec<Tool>,
}

impl ChatBuilder {
    /// Create a new chat builder with a model.
    pub fn new(model: Arc<LlamaModel>) -> Self {
        Self {
            model,
            n_ctx: 2048,
            system_prompt: String::new(),
            tools: Vec::new(),
        }
    }

    /// Set the context size for the chat session.
    pub fn with_context_size(mut self, n_ctx: u32) -> Self {
        self.n_ctx = n_ctx;
        self
    }

    /// Set the system prompt for the chat session.
    pub fn with_system_prompt<S: Into<String>>(mut self, prompt: S) -> Self {
        self.system_prompt = prompt.into();
        self
    }

    /// Add a tool that the model can use.
    pub fn with_tool(mut self, tool: Tool) -> Self {
        self.tools.push(tool);
        self
    }

    /// Add multiple tools that the model can use.
    pub fn with_tools(mut self, tools: Vec<Tool>) -> Self {
        self.tools.extend(tools);
        self
    }

    /// Build the chat handle and start the background worker.
    pub fn build(self) -> ChatHandle {
        ChatHandle::new(self.model, self.n_ctx, self.system_prompt, self.tools)
    }
}

impl ChatHandle {
    /// Create a new chat handle directly. Consider using [`ChatBuilder`] for a more ergonomic API.
    pub fn new(
        model: Arc<LlamaModel>,
        n_ctx: u32,
        system_prompt: String,
        tools: Vec<Tool>,
    ) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);
        std::thread::spawn(move || {
            if let Err(e) = run_worker(
                model,
                n_ctx,
                system_prompt,
                tools,
                msg_rx,
                should_stop_clone,
            ) {
                error!("Worker crashed: {}", e)
            }
        });

        Self {
            msg_tx,
            should_stop,
        }
    }

    /// Send a message to the model and get a stream of tokens back.
    pub fn say(
        &self,
        text: String,
        sampler: SamplerConfig,
        stop_words: Vec<String>,
    ) -> tokio::sync::mpsc::Receiver<llm::WriteOutput> {
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(4096);
        let _ = self.msg_tx.send(ChatMsg::Say {
            text,
            sampler,
            stop_words,
            output_tx,
        });
        output_rx
    }

    /// Send a message and wait for the complete response.
    ///
    /// # Example
    /// ```
    /// # use nobodywho::chat::ChatHandle;
    /// # async fn example(chat: &ChatHandle) -> Result<(), nobodywho::errors::SayError> {
    /// let response = chat.say_complete("What is the capital of France?").await?;
    /// println!("{}", response);
    /// # Ok(())
    /// # }
    /// ```
    pub async fn say_complete(&self, text: impl Into<String>) -> Result<String, SayError> {
        self.say_complete_with_config(text, SamplerPresets::default(), vec![])
            .await
    }

    /// Send a message with custom configuration and wait for the complete response.
    pub async fn say_complete_with_config(
        &self,
        text: impl Into<String>,
        sampler: SamplerConfig,
        stop_words: Vec<String>,
    ) -> Result<String, SayError> {
        let mut rx = self.say(text.into(), sampler, stop_words);

        let mut tokens = Vec::new();
        while let Some(output) = rx.recv().await {
            match output {
                llm::WriteOutput::Token(token) => tokens.push(token),
                llm::WriteOutput::Done(response) => return Ok(response),
            }
        }

        // If we got here, the channel closed without sending Done
        Ok(tokens.join(""))
    }

    /// Send a message and collect tokens as they arrive.
    ///
    /// # Example
    /// ```
    /// # use nobodywho::chat::ChatHandle;
    /// # async fn example(chat: &ChatHandle) {
    /// let mut stream = chat.say_stream("Tell me a story");
    /// while let Some(token) = stream.next_token().await {
    ///     print!("{}", token);
    /// }
    /// # }
    /// ```
    pub fn say_stream(&self, text: impl Into<String>) -> TokenStream {
        TokenStream::new(self.say(text.into(), SamplerPresets::default(), vec![]))
    }

    /// Send a message with custom configuration and collect tokens as they arrive.
    pub fn say_stream_with_config(
        &self,
        text: impl Into<String>,
        sampler: SamplerConfig,
        stop_words: Vec<String>,
    ) -> TokenStream {
        TokenStream::new(self.say(text.into(), sampler, stop_words))
    }

    /// Reset the chat conversation with a new system prompt and tools.
    pub fn reset_chat(&self, system_prompt: String, tools: Vec<Tool>) {
        let _ = self.msg_tx.send(ChatMsg::ResetChat {
            system_prompt,
            tools,
        });
    }

    /// Update the available tools for the model to use.
    pub fn set_tools(&self, tools: Vec<Tool>) {
        let _ = self.msg_tx.send(ChatMsg::SetTools { tools });
    }

    /// Stop the current generation if one is in progress.
    pub fn stop_generation(&self) {
        self.should_stop
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    /// Get the current chat history.
    pub async fn get_chat_history_async(&self) -> Vec<crate::chat_state::Message> {
        let mut rx = self.get_chat_history();
        rx.recv().await.unwrap_or_default()
    }

    /// Get a receiver for the chat history (lower-level API).
    pub fn get_chat_history(&self) -> tokio::sync::mpsc::Receiver<Vec<crate::chat_state::Message>> {
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(1);
        let _ = self.msg_tx.send(ChatMsg::GetChatHistory { output_tx });
        output_rx
    }

    /// Set the chat history.
    pub async fn set_chat_history_async(&self, messages: Vec<crate::chat_state::Message>) {
        let mut rx = self.set_chat_history(messages);
        let _ = rx.recv().await;
    }

    /// Set the chat history (lower-level API).
    pub fn set_chat_history(
        &self,
        messages: Vec<crate::chat_state::Message>,
    ) -> tokio::sync::mpsc::Receiver<()> {
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(1);
        let _ = self.msg_tx.send(ChatMsg::SetChatHistory {
            output_tx,
            messages,
        });
        output_rx
    }
}

/// A stream of tokens from the model.
pub struct TokenStream {
    rx: tokio::sync::mpsc::Receiver<llm::WriteOutput>,
    done: bool,
}

impl TokenStream {
    fn new(rx: tokio::sync::mpsc::Receiver<llm::WriteOutput>) -> Self {
        Self { rx, done: false }
    }

    /// Get the next token from the stream.
    pub async fn next_token(&mut self) -> Option<String> {
        if self.done {
            return None;
        }

        if let Some(output) = self.rx.recv().await {
            match output {
                llm::WriteOutput::Token(token) => return Some(token),
                llm::WriteOutput::Done(_) => {
                    self.done = true;
                    return None;
                }
            }
        }

        None
    }

    /// Collect all remaining tokens into a single string.
    pub async fn collect(mut self) -> String {
        let mut result = Vec::new();
        while let Some(token) = self.next_token().await {
            result.push(token);
        }
        result.join("")
    }
}

enum ChatMsg {
    Say {
        text: String,
        sampler: SamplerConfig,
        stop_words: Vec<String>,
        output_tx: tokio::sync::mpsc::Sender<llm::WriteOutput>,
    },
    ResetChat {
        system_prompt: String,
        tools: Vec<Tool>,
    },
    SetTools {
        tools: Vec<Tool>,
    },
    GetChatHistory {
        output_tx: tokio::sync::mpsc::Sender<Vec<crate::chat_state::Message>>,
    },
    SetChatHistory {
        messages: Vec<crate::chat_state::Message>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
}

fn run_worker(
    model: Arc<LlamaModel>,
    n_ctx: u32,
    system_prompt: String,
    tools: Vec<Tool>,
    msg_rx: std::sync::mpsc::Receiver<ChatMsg>,
    should_stop: Arc<AtomicBool>,
) -> Result<(), ChatWorkerError> {
    let mut worker_state =
        Worker::new_chat_worker(&model, n_ctx, system_prompt, should_stop, tools)?;
    while let Ok(msg) = msg_rx.recv() {
        match msg {
            ChatMsg::Say {
                text,
                sampler,
                stop_words,
                output_tx,
            } => {
                let callback = move |out| {
                    let _ = output_tx.blocking_send(out);
                };
                worker_state.say(text, sampler, stop_words, callback)?;
            }
            ChatMsg::ResetChat {
                system_prompt,
                tools,
            } => {
                worker_state.reset_chat(system_prompt, tools)?;
            }
            ChatMsg::SetTools { tools } => {
                worker_state.set_tools(tools)?;
            }
            ChatMsg::GetChatHistory { output_tx } => {
                let _ =
                    output_tx.blocking_send(worker_state.extra.chat_state.get_messages().to_vec());
            }
            ChatMsg::SetChatHistory {
                messages,
                output_tx,
            } => {
                worker_state.set_chat_history(messages)?;
                let _ = output_tx.blocking_send(());
            }
        }
    }
    Ok(())
}

// TOOLS TYPE STUFF

// the callback closure isn't normally Send
// but we just cheat a little here
// so far it has been fine...
unsafe impl Send for Tool {}

/// A tool that the model can call during conversation.
#[derive(Clone)]
pub struct Tool {
    pub name: String,
    description: String,
    json_schema: serde_json::Value,
    function: Arc<dyn Fn(serde_json::Value) -> String>,
}

impl Tool {
    /// Create a new tool directly. Consider using [`ToolBuilder`] for a more ergonomic API.
    pub fn new(
        name: String,
        description: String,
        json_schema: serde_json::Value,
        function: Arc<dyn Fn(serde_json::Value) -> String>,
    ) -> Self {
        Self {
            name,
            description,
            json_schema,
            function,
        }
    }

    /// Create a new tool builder.
    pub fn builder<S: Into<String>>(name: S) -> ToolBuilder {
        ToolBuilder::new(name)
    }

    fn to_chat_state_tool(&self) -> chat_state::Tool {
        chat_state::Tool {
            r#type: chat_state::ToolType::Function,
            function: chat_state::Function {
                name: self.name.clone(),
                description: self.description.clone(),
                parameters: self.json_schema.clone(),
            },
        }
    }
}

/// Builder for creating tools with a fluent API.
///
/// # Example
/// ```
/// use nobodywho::chat::{Tool};
/// let tool = Tool::builder("get_weather")
///     .description("Get the current weather for a location")
///     .param("location", "string", "The city to get weather for")
///     .required("location")
///     .handler(|args| {
///         let location = args["location"].as_str().unwrap();
///         format!("Weather in {}: Sunny, 22°C", location)
///     })
///     .build();
/// ```
pub struct ToolBuilder {
    name: String,
    description: String,
    properties: serde_json::Map<String, serde_json::Value>,
    required: Vec<String>,
    handler: Option<Arc<dyn Fn(serde_json::Value) -> String>>,
}

impl ToolBuilder {
    /// Create a new tool builder with a name.
    pub fn new<S: Into<String>>(name: S) -> Self {
        Self {
            name: name.into(),
            description: String::new(),
            properties: serde_json::Map::new(),
            required: Vec::new(),
            handler: None,
        }
    }

    /// Set the description of the tool.
    pub fn description<S: Into<String>>(mut self, desc: S) -> Self {
        self.description = desc.into();
        self
    }

    /// Add a parameter to the tool.
    pub fn param<S: Into<String>>(mut self, name: S, param_type: &str, description: S) -> Self {
        let name = name.into();
        self.properties.insert(
            name,
            serde_json::json!({
                "type": param_type,
                "description": description.into(),
            }),
        );
        self
    }

    /// Add a parameter with a custom JSON schema.
    pub fn param_with_schema<S: Into<String>>(
        mut self,
        name: S,
        schema: serde_json::Value,
    ) -> Self {
        self.properties.insert(name.into(), schema);
        self
    }

    /// Mark a parameter as required.
    pub fn required<S: Into<String>>(mut self, name: S) -> Self {
        self.required.push(name.into());
        self
    }

    /// Set the handler function for the tool.
    pub fn handler<F>(mut self, f: F) -> Self
    where
        F: Fn(serde_json::Value) -> String + 'static,
    {
        self.handler = Some(Arc::new(f));
        self
    }

    /// Build the tool.
    pub fn build(self) -> Tool {
        let json_schema = serde_json::json!({
            "type": "object",
            "properties": self.properties,
            "required": self.required,
        });

        Tool {
            name: self.name,
            description: self.description,
            json_schema,
            function: self
                .handler
                .unwrap_or_else(|| Arc::new(|_| "Tool handler not implemented".to_string())),
        }
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

// TOOL CHAT WORKER

struct ChatWorker {
    chat_state: ChatState,
    should_stop: Arc<AtomicBool>,
    tools: Vec<Tool>,
    tool_grammar: Option<gbnf::Grammar>,
}

impl llm::PoolingType for ChatWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::None
    }
}

impl Worker<'_, ChatWorker> {
    fn new_chat_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
        system_prompt: String,
        should_stop: Arc<AtomicBool>,
        tools: Vec<Tool>,
    ) -> Result<Worker<'_, ChatWorker>, InitWorkerError> {
        // initialize chat state with system prompt
        let mut chat_state = ChatState::from_model_and_tools(
            model,
            tools.iter().map(|t| t.to_chat_state_tool()).collect(),
        )?;
        chat_state.add_system_message(system_prompt);

        let grammar = if !tools.is_empty() {
            grammar_from_tools(&tools).ok()
        } else {
            None
        };

        Worker::new_with_type(
            model,
            n_ctx,
            false,
            ChatWorker {
                chat_state,
                tools,
                tool_grammar: grammar,
                should_stop,
            },
        )
    }

    fn should_stop(&self) -> bool {
        self.extra
            .should_stop
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    // TODO //.
    fn context_shift(&mut self) -> Result<(), ShiftError> {
        info!("Context shift happens!");
        let target_token_size = (self.ctx.n_ctx() / 2) as usize;
        let mut messages = self.extra.chat_state.get_messages().to_vec();

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
                        &self.extra.chat_state.naive_render_message_vec(&messages)?,
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
        self.extra.chat_state.set_messages(messages);
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
        stop_words: Vec<String>,
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

                let (prefix_index, token_difference) = self
                    .extra
                    .chat_state
                    .find_prefix_index_and_difference_with_tokens_in_context(&render_as_tokens);

                self.remove_all_tokens_after_index_from_ctx(prefix_index)?;
                self.read_tokens(token_difference, inference_lock_token)?;
                self.read_tokens(tokens_written_until_now.clone(), inference_lock_token)?;
                // do not update tokens_in_context as this is done later by say
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

            let has_stop_words = stop_words
                .iter()
                .any(|stop_word| full_response.contains(stop_word));
            if has_eog || has_stop_words {
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

    pub fn say<F>(
        &mut self,
        text: String,
        user_sampler: SamplerConfig,
        stop_words: Vec<String>,
        respond: F,
    ) -> Result<&mut Self, SayError>
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

        self.extra.chat_state.add_user_message(text);

        let sampler =
            self.extra
                .tool_grammar
                .as_ref()
                .map_or(user_sampler.clone(), |tool_grammar| {
                    user_sampler.shift(ShiftStep::Grammar {
                        trigger_on: Some(tool_call_begin.into()),
                        root: "superoot".into(),
                        grammar: tool_grammar.to_string(),
                    })
                });

        // get the finished response
        let mut response: String = self.wrapped_update_context_and_generate_response(
            sampler.clone(),
            stop_words.clone(),
            respond.clone(),
            tool_call_begin.into(),
        )?;

        while let Some(tool_calls) = extract_tool_calls(&response) {
            debug!("Got tool calls! {tool_calls:?}");

            self.extra.chat_state.add_tool_calls(tool_calls.clone());

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
                    self.extra.chat_state.add_tool_resp(tool_call.name, errmsg);
                    continue;
                };

                // call the tool
                let response = (tool.function)(tool_call.arguments);
                debug!(?tool_call.name, ?response);

                // add to chat history
                self.extra
                    .chat_state
                    .add_tool_resp(tool_call.name, response);
            }

            // get the finished response
            response = self.wrapped_update_context_and_generate_response(
                sampler.clone(),
                stop_words.clone(),
                respond.clone(),
                tool_call_begin.into(),
            )?;
        }
        debug_assert!(!response.contains(tool_call_begin));
        self.extra.chat_state.add_assistant_message(response);

        // Update tokens_in_context as the model already has seen this respone
        let render_as_tokens = self.get_render_as_tokens()?;

        self.extra
            .chat_state
            .set_tokens_in_context(render_as_tokens);

        Ok(self)
    }

    fn get_render_as_tokens(&mut self) -> Result<Vec<LlamaToken>, RenderError> {
        let render_as_string = self.extra.chat_state.render_string()?;
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
        stop_words: Vec<String>,
        wrapped_respond: impl FnMut(WriteOutput),
    ) -> Result<&mut Self, InferenceError> {
        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();

        Ok(self
            .read_tokens(tokens, &inference_lock_token)?
            .generate_response_until_done(
                sampler,
                stop_words.clone(),
                wrapped_respond,
                &inference_lock_token,
            )?)
    }

    fn wrapped_update_context_and_generate_response<F>(
        &mut self,
        sampler: SamplerConfig,
        stop_words: Vec<String>,
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

        let (prefix_index, token_difference) = self
            .extra
            .chat_state
            .find_prefix_index_and_difference_with_tokens_in_context(&render_as_tokens);

        self.remove_all_tokens_after_index_from_ctx(prefix_index)?;

        // wrap the response callback to keep a copy of the completed response
        // and to avoid emitting tool calls
        let (wrapped_respond, resp_receiver) = wrap_respond(respond.clone(), tool_call_begin_token);

        // llm go brrr
        self.read_tokens_and_generate_response(
            token_difference,
            sampler,
            stop_words.clone(),
            wrapped_respond,
        )?;

        // update the chat_state to match the tokens in the context.
        self.extra
            .chat_state
            .set_tokens_in_context(render_as_tokens);

        Ok(resp_receiver.recv()?)
    }

    pub fn reset_chat(
        &mut self,
        system_prompt: String,
        tools: Vec<Tool>,
    ) -> Result<(), FromModelError> {
        self.reset_context();
        self.extra.chat_state = ChatState::from_model_and_tools(
            self.ctx.model,
            tools.iter().map(|t| t.to_chat_state_tool()).collect(),
        )?;
        self.extra.tool_grammar = if !tools.is_empty() {
            grammar_from_tools(&tools).ok()
        } else {
            None
        };
        self.extra.tools = tools;
        self.extra.chat_state.add_system_message(system_prompt);
        Ok(())
    }

    pub fn set_tools(&mut self, tools: Vec<Tool>) -> Result<(), ChatWorkerError> {
        let current_messages = self.extra.chat_state.get_messages().to_vec();
        self.extra.chat_state = ChatState::from_model_and_tools(
            self.ctx.model,
            tools.iter().map(|t| t.to_chat_state_tool()).collect(),
        )?;
        self.extra.tool_grammar = if !tools.is_empty() {
            grammar_from_tools(&tools).ok()
        } else {
            None
        };
        self.extra.tools = tools;
        self.extra.chat_state.set_messages(current_messages);
        // Reuse cached prefix
        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();
        let render_as_tokens = self.get_render_as_tokens()?;
        let (prefix_index, token_difference) = self
            .extra
            .chat_state
            .find_prefix_index_and_difference_with_tokens_in_context(&render_as_tokens);

        self.remove_all_tokens_after_index_from_ctx(prefix_index)?;
        self.read_tokens(token_difference, &inference_lock_token)?;
        self.extra
            .chat_state
            .set_tokens_in_context(render_as_tokens);

        Ok(())
    }

    pub fn set_chat_history(
        &mut self,
        messages: Vec<crate::chat_state::Message>,
    ) -> Result<(), ChatWorkerError> {
        self.reset_context();
        self.extra.chat_state.set_messages(messages);

        // Reuse cached prefix

        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();
        let render_as_tokens = self.get_render_as_tokens()?;
        let (prefix_index, token_difference) = self
            .extra
            .chat_state
            .find_prefix_index_and_difference_with_tokens_in_context(&render_as_tokens);

        self.remove_all_tokens_after_index_from_ctx(prefix_index)?;
        self.read_tokens(token_difference, &inference_lock_token)?;
        self.extra
            .chat_state
            .set_tokens_in_context(render_as_tokens);
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

fn extract_tool_calls(input: &str) -> Option<Vec<chat_state::ToolCall>> {
    // Find the start and end tags
    // TODO: these are the tokens used by qwen3
    //       but e.g. deepseek uses "<｜tool▁calls▁begin｜><｜tool▁call▁begin｜>" instead.
    //       we need to support multiple different tool call begin tokens
    let re = regex::Regex::new(r"<tool_call>([\s\S]*?)</tool_call>").expect("Invalid regex");

    let tool_calls: Vec<chat_state::ToolCall> = re
        .captures_iter(input)
        .filter_map(|cap| {
            let tool_call: Option<chat_state::ToolCall> = serde_json::from_str(cap[1].trim()).ok();
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
        let sampler = SamplerPresets::default();
        let mut worker = Worker::new_chat_worker(
            &model,
            1024,
            "".into(),
            Arc::new(AtomicBool::new(false)),
            vec![],
        )?;

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            llm::WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        worker.say(
            "What is the capital of Denmark?".to_string(),
            sampler.clone(),
            vec![],
            f.clone(),
        )?;

        let resp = receiver.recv()?;
        println!("{}", resp);

        assert!(resp.contains("Copenhagen"));

        worker.say(
            "What language do they speak there?".to_string(),
            sampler.clone(),
            vec![],
            f,
        )?;
        let resp = receiver.recv()?;
        println!("{}", resp);

        assert!(resp.contains("Danish"));

        Ok(())
    }

    #[test]
    fn test_reset_chat() -> Result<(), Box<dyn std::error::Error>> {
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let system_prompt = "You're a dog. End all responses with 'woof'";
        let mut worker = Worker::new_chat_worker(
            &model,
            1024,
            system_prompt.into(),
            Arc::new(AtomicBool::new(false)),
            vec![],
        )?;
        let sampler = SamplerPresets::default();

        // just a hack to get a channel back
        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            llm::WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        // do it once
        worker.say(
            "What is the capital of Denmark?".to_string(),
            sampler.clone(),
            vec![],
            f.clone(),
        )?;
        let resp1 = receiver.recv()?;
        println!("{}", resp1);
        assert!(resp1.to_lowercase().contains("woof"));

        // reset
        let _ = worker.reset_chat("You're a cat. End all responses with 'meow'".into(), vec![]);

        // do it again
        worker.say(
            "What is the capital of Denmark?".to_string(),
            sampler.clone(),
            vec![],
            f.clone(),
        )?;
        let resp2 = receiver.recv()?;
        println!("{}", resp2);
        assert!(resp2.to_lowercase().contains("meow"));

        Ok(())
    }

    #[test]
    fn test_stop_mid_write() -> Result<(), Box<dyn std::error::Error>> {
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let system_prompt = "You are a counter, only outputting numbers";
        let mut worker = Worker::new_chat_worker(
            &model,
            1024,
            system_prompt.into(),
            Arc::new(AtomicBool::new(false)),
            vec![],
        )?;
        let should_stop = worker.extra.should_stop.clone();

        // ensure that the generationworker resets the flag when creating a new response.
        should_stop.store(true, std::sync::atomic::Ordering::Relaxed);

        let sampler = SamplerPresets::default();

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

        worker.say(
            "Count from 0 to 9".to_string(),
            sampler.clone(),
            vec![],
            f.clone(),
        )?;

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
            function: Arc::new(|args| {
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
            function: Arc::new(|args| {
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
            4096,
            "You're a helpful assistant.".into(),
            Arc::new(AtomicBool::new(false)),
            vec![test_tool()],
        )
        .expect("Failed making worker");

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            llm::WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        worker
            .say(
                "I would like to know the temperature in two cities: Copenhagen and Beijing."
                    .into(),
                crate::sampler_config::SamplerPresets::default(),
                vec![],
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
            1024,
            "".into(),
            Arc::new(AtomicBool::new(false)),
            vec![test_tool(), dkk_exchange_rate()],
        )
        .expect("Failed making worker");

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            llm::WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        worker.say(
            "I would like to know the temperature in Copenhagen and the DKK to USD exchange rate."
                .into(),
            crate::sampler_config::SamplerPresets::default(),
            vec![],
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
            n_ctx,
            "You are a helpful assistant that provides informative and detailed responses. End every response with \"Do you have any further questions?\"".into(),
            Arc::new(AtomicBool::new(false)),
            vec![],
        )?;

        // Add many exchanges with longer messages to fill up the context
        for i in 1..=n_messages {
            worker.extra.chat_state.add_user_message(format!(
                "This is user message number {}. What is {} * {}?",
                i, i, i
            ));
            worker.extra.chat_state.add_assistant_message(format!(
                "<think> </think> The answer is {}. Do you have any further questions?",
                i * i
            ));
        }

        worker.extra.chat_state.add_user_message("Hello!".into());

        // Check that we have many messages before shift
        let messages_before = worker.extra.chat_state.get_messages().len();
        assert!(
            messages_before > 6,
            "Should have more than 6 messages before shift"
        );

        // Trigger context shift
        worker.context_shift()?;

        println!("{:?}", worker.extra.chat_state.get_messages());

        let messages_after = worker.extra.chat_state.get_messages().to_vec();

        // Verify essential messages are preserved:
        // 1. System prompt should be first
        assert_eq!(messages_after[0].role(), &Role::System);

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
        let token_count = model
            .str_to_token(&worker.extra.chat_state.render_string()?, AddBos::Never)?
            .len();

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
            n_ctx,
            "You are a helpful assistant.".into(),
            Arc::new(AtomicBool::new(false)),
            vec![test_tool()],
        )?;

        // Add exchanges with tool calls mixed in
        for i in 1..=n_messages {
            worker
                .extra
                .chat_state
                .add_user_message(format!("User message {}. What is {} * {}?", i, i, i));

            // Add a tool call every other message
            // Pattern: User -> Assistant (with tool call) -> Tool response -> Assistant
            if i % 2 == 0 {
                worker
                    .extra
                    .chat_state
                    .add_tool_calls(vec![chat_state::ToolCall {
                        name: "get_current_temperature".into(),
                        arguments: serde_json::json!({"location": "Copenhagen"}),
                    }]);
                worker
                    .extra
                    .chat_state
                    .add_tool_resp("get_current_temperature".into(), "13.37°C".into());
                worker.extra.chat_state.add_assistant_message(format!(
                    "The temperature is 13.37°C and {} * {} = {}.",
                    i,
                    i,
                    i * i
                ));
            } else {
                worker
                    .extra
                    .chat_state
                    .add_assistant_message(format!("The answer is {}.", i * i));
            }
        }

        worker
            .extra
            .chat_state
            .add_user_message("Final question!".into());

        // Check that we have many messages before shift
        let messages_before = worker.extra.chat_state.get_messages().len();
        println!("Messages before shift: {}", messages_before);

        // Trigger context shift
        worker.context_shift()?;

        println!("{:?}", worker.extra.chat_state.get_messages());

        let messages_after = worker.extra.chat_state.get_messages().to_vec();

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
        let token_count = model
            .str_to_token(&worker.extra.chat_state.render_string()?, AddBos::Never)?
            .len();

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
        let sampler = SamplerPresets::default();

        // Use a small context size to force shifting
        let n_ctx = 512;
        let n_messages = 14;
        // n_messages is chosen by trial and error. This exactly fills up the
        // the context so much that the next user message cannot be read and a context shift happens.
        let mut worker = Worker::new_chat_worker(
            &model,
            n_ctx,
            "You are a helpful assistant.".into(),
            Arc::new(AtomicBool::new(false)),
            vec![],
        )?;

        // Fill up the context until it's almost full
        for i in 1..=n_messages {
            worker.extra.chat_state.add_user_message(format!(
                "This is user message number {}. What is {} * {}?",
                i, i, i
            ));
            worker
                .extra
                .chat_state
                .add_assistant_message(format!("The answer is {}.", i * i));
        }

        let messages_before_shift = worker.extra.chat_state.get_messages().len();
        println!("Messages before shift: {}", messages_before_shift);

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            llm::WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        // This should trigger context shift internally because there's not enough space
        worker.say(
            "This is a new question that will not fit in the context! What is 10 * 10?".to_string(),
            sampler,
            vec![],
            f,
        )?;

        let _response = receiver.recv()?;
        let messages_after = worker.extra.chat_state.get_messages().to_vec();

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
        let sampler = SamplerPresets::default();

        // Use a small context size to force shifting
        let n_ctx = 768;
        let n_messages = 19;
        // n_messages is chosen by trial and error. This exactly fills up the
        // the context so much that the next assistant message cannot be fully written.
        // The same is true for n_ctx. It needs to be large enough to where n_ctx/2 is large enough
        // to contain the response but also small enough to fill easily and test wihtout being to slow.
        let mut worker = Worker::new_chat_worker(
            &model,
            n_ctx,
            "You are a helpful assistant.".into(),
            Arc::new(AtomicBool::new(false)),
            vec![],
        )?;

        // Fill up the context until it's almost full
        for i in 1..=n_messages {
            worker.extra.chat_state.add_user_message(format!(
                "This is user message number {}. What is {} * {}?",
                i, i, i
            ));
            worker
                .extra
                .chat_state
                .add_assistant_message(format!("The answer is {}.", i * i));
        }

        let messages_before_shift = worker.extra.chat_state.get_messages().len();
        println!("Messages before shift: {}", messages_before_shift);

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            llm::WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        // This should trigger context shift internally because there's not enough space
        worker.say("What is 10 * 10?".to_string(), sampler, vec![], f)?;

        let _response = receiver.recv()?;
        let messages_after = worker.extra.chat_state.get_messages().to_vec();

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
    fn test_chat_worker_simple_completion() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let sampler = SamplerPresets::default();
        let mut worker = Worker::new_chat_worker(
            &model,
            4096,
            "".into(),
            Arc::new(AtomicBool::new(false)),
            vec![],
        )?;

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            llm::WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        worker.read_tokens_and_generate_response(
            worker
                .ctx
                .model
                .str_to_token("I'm going to count to 10: 1, 2, 3", AddBos::Never)?,
            sampler,
            vec!["10".to_string()],
            f,
        )?;

        let response = receiver.recv()?;
        println!("Response: {}", response);
        assert!(response.contains("4, 5, 6, 7, 8, 9, 10"));

        Ok(())
    }

    #[test]
    fn test_chat_worker_stop_tokens() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let sampler = SamplerPresets::default();
        let mut worker = Worker::new_chat_worker(
            &model,
            1024,
            "".into(),
            Arc::new(AtomicBool::new(false)),
            vec![],
        )?;

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            llm::WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        worker.read_tokens_and_generate_response(
            worker
                .ctx
                .model
                .str_to_token("I'm going to count to 10: 1, 2, 3, 4,", AddBos::Never)?,
            sampler,
            vec!["7".to_string()],
            f,
        )?;

        let response = receiver.recv()?;
        println!("Response: {}", response);

        assert!(
            response.to_lowercase().contains("5, 6, "),
            "Expected output to contain text before stop token. Got: {response}"
        );
        assert!(
            response.to_lowercase().ends_with("7"),
            "Expected output to stop at stop token, but continued. Got: {response}"
        );
        assert!(
            !response.to_lowercase().contains("8"),
            "Expected output to stop at stop token, but continued. Got: {response}"
        );

        Ok(())
    }

    #[test]
    fn test_chat_worker_multiple_contexts() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let sampler = SamplerPresets::default();
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
                n_ctx,
                "".into(),
                Arc::new(AtomicBool::new(false)),
                vec![],
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
                    vec!["Copenhagen".to_string()],
                    f,
                )
                .unwrap();
        });

        // Start Germany worker thread
        let de_handle = std::thread::spawn(move || {
            let mut worker = Worker::new_chat_worker(
                &model,
                n_ctx,
                "".into(),
                Arc::new(AtomicBool::new(false)),
                vec![],
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
                    vec!["Berlin".to_string()],
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
}
