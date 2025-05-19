use crate::chat_state;
use crate::chat_state::ChatState;
use crate::llm;
use crate::llm::Worker;
use crate::sampler_config::SamplerConfig;
use llama_cpp_2::model::LlamaModel;
use std::sync::Arc;
use tracing::debug;

pub struct Tool {
    name: String,
    description: String,
    json_schema: serde_json::Value,
    function: Box<dyn Fn(serde_json::Value) -> String>,
}

impl Tool {
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

struct ToolChatWorker {
    chat_state: ChatState,
    tools: Vec<Tool>,
}

impl llm::GenerationCapability for ToolChatWorker {}

#[derive(Debug, thiserror::Error)]
pub enum SayError {
    #[error("Error generating text: {0}")]
    WriteError(#[from] llm::WriteError),

    #[error("Error reading string: {0}")]
    ReadError(#[from] llm::ReadError),

    #[error("Error getting response: {0}")]
    ResponseError(#[from] std::sync::mpsc::RecvError),

    #[error("Error rendering chat template: {0}")]
    ChatTemplateRenderError(#[from] minijinja::Error),
}

impl<'a> Worker<'_, ToolChatWorker> {
    fn new_tool_chat_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
        system_prompt: String,
        tools: Vec<Tool>,
    ) -> Result<Worker<'_, ToolChatWorker>, llm::InitWorkerError> {
        // initialize chat state with system prompt
        let mut chat_state = ChatState::from_model_and_tools(
            model,
            tools.iter().map(|t| t.to_chat_state_tool()).collect(),
        )?;
        chat_state.add_system_message(system_prompt);

        Ok(Worker::new_with_type(
            model,
            n_ctx,
            false,
            ToolChatWorker { chat_state, tools },
        )?)
    }

    pub fn say<F>(
        &mut self,
        text: String,
        sampler: SamplerConfig,
        stop_words: Vec<String>,
        respond: F,
    ) -> Result<&mut Self, SayError>
    where
        F: Fn(llm::WriteOutput) + Clone,
    {
        // TODO: this is the token used by qwen3
        //       but e.g. deepseek uses "<｜tool▁calls▁begin｜><｜tool▁call▁begin｜>" instead.
        //       we need to support multiple different tool call begin tokens
        let tool_call_begin = "<tool_call>";

        self.extra.chat_state.add_user_message(text);
        let diff = self.extra.chat_state.render_diff()?;

        // TODO: don't emit stuff after tool_call_begin
        // todo!();

        // wrap the response callback to keep a copy of the completed response
        let (wrapped_respond, resp_receiver) =
            wrap_respond(respond.clone(), tool_call_begin.into());

        // llm go brrr
        self.read_string(diff)?.write_until_done(
            sampler.clone(),
            stop_words.clone(),
            wrapped_respond,
        )?;

        // get the finished response
        let response: String = resp_receiver.recv()?;

        let Some(tool_calls) = extract_tool_calls(&response) else {
            // no tool call. all good. return here
            debug_assert!(!response.contains(tool_call_begin));
            return Ok(self);
        };

        debug!("Got tool calls! {tool_calls:?}");
        self.extra.chat_state.add_tool_calls(tool_calls.clone());
        let _ = self.extra.chat_state.render_diff();
        // render diff just to keep up with context.
        // discard result, because the llm context has already seen these tokens

        for tool_call in tool_calls {
            // XXX: do the tool call
            // find the tool
            let tool = self
                .extra
                .tools
                .iter()
                .find(|t| t.name == tool_call.name)
                .expect("TODO: handle bad tool name");
            // TODO: how to handle the llm selecting an invalid tool?
            //       should we put an error message in the chat history?
            //       or crash hard?
            //       or prevent it from ever happening with GBNF?

            // call the tool
            let response = (tool.function)(tool_call.arguments);

            // add to chat history
            self.extra
                .chat_state
                .add_tool_resp(tool_call.name, response);
        }

        let diff = self.extra.chat_state.render_diff()?;

        let (wrapped_respond, resp_receiver) = wrap_respond(respond, tool_call_begin.into());
        self.read_string(diff)?
            .write_until_done(sampler, stop_words, wrapped_respond)?;

        // get the finished response
        // TODO? should we allow multiple tool calls in sequence?
        let response: String = resp_receiver.recv()?;

        Ok(self)
    }
}

/// wraps a response function in a closure to do two things:
/// 1. save a copy of the response (using a channel) before sending it out
/// 2. skip emitting once a tool call begin token has been seen
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
    let start_tag = "<tool_call>";
    let end_tag = "</tool_call>";

    let re = regex::Regex::new(r"<tool_call>([\s\S]*?)</tool_call>").expect("Invalid regex");

    let tool_calls: Vec<chat_state::ToolCall> = re
        .captures_iter(input)
        .filter_map(|cap| {
            let tool_call: Option<chat_state::ToolCall> = serde_json::from_str(cap[1].trim()).ok();
            tool_call
        })
        .collect();

    if tool_calls.len() > 0 {
        Some(tool_calls)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;

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
            function: Box::new(|args| {
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

    #[test]
    fn test_tool_chat() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Worker::new_tool_chat_worker(
            &model,
            4096,
            "You're a helpful assistant.".into(),
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
                crate::sampler_config::SamplerConfig::default(),
                vec![],
                f,
            )
            .expect("fuck");

        let result = receiver.recv().unwrap();
        assert!(result.contains("13.37"));
        assert!(result.contains("42.69"));
    }
}
