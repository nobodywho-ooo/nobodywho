pub(crate) mod context;
pub(crate) mod generation;
pub(crate) mod messaging;

use crate::errors::{
    ChatWorkerError, ContextSyncError, InitWorkerError, SayError, SelectTemplateError,
    SetToolsError,
};
use crate::llm::{self, Worker, GLOBAL_INFERENCE_LOCK};
use crate::sampler_config::{SamplerConfig, ShiftStep};
use crate::template::{select_template, ChatTemplate};
use crate::tool_calling::{detect_tool_format, Tool, ToolCall, ToolFormat};
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::token::LlamaToken;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::{debug, error};

use super::types::{Message, Role};

pub(crate) struct ChatWorker {
    pub(crate) should_stop: Arc<AtomicBool>,
    pub(crate) tool_grammar: Option<gbnf::GbnfGrammar>,
    pub(crate) tool_format: Option<ToolFormat>,
    pub(crate) sampler_config: SamplerConfig,
    pub(crate) messages: Vec<Message>,
    pub(crate) tokens_in_context: Vec<LlamaToken>,
    pub(crate) allow_thinking: bool,
    pub(crate) tools: Vec<Tool>,
    pub(crate) chat_template: ChatTemplate,
}

impl llm::PoolingType for ChatWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::None
    }
}

impl Worker<'_, ChatWorker> {
    pub(crate) fn new_chat_worker(
        model: &Arc<LlamaModel>,
        config: super::config::ChatConfig,
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
                messages: vec![Message::Message {
                    role: Role::System,
                    content: config.system_prompt,
                }],
                chat_template: template,
                allow_thinking: config.allow_thinking,
                tools: config.tools,
                tokens_in_context: Vec::new(),
            },
        )
    }

    pub(crate) fn should_stop(&self) -> bool {
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

    pub fn reset_chat(
        &mut self,
        system_prompt: String,
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

    pub fn set_system_prompt(&mut self, system_prompt: String) -> Result<(), ContextSyncError> {
        let system_message = Message::Message {
            role: Role::System,
            content: system_prompt,
        };
        if self.extra.messages.is_empty() {
            self.extra.messages.push(system_message);
        } else if *self.extra.messages[0].role() == Role::System {
            self.extra.messages[0] = system_message;
        } else {
            self.extra.messages.insert(0, system_message);
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
