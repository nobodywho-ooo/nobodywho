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
        F: Fn(llm::WriteOutput),
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
        let (resp_sender, resp_receiver) = std::sync::mpsc::channel();
        let wrapped_respond = |x| {
            if let llm::WriteOutput::Done(resp) = &x {
                resp_sender
                    .send(resp.clone())
                    .expect("Failed sending response");
            }
            respond(x)
        };

        // brrr
        self.read_string(diff)?.write_until_done(
            sampler.clone(),
            stop_words.clone(),
            wrapped_respond,
        )?;

        // get the finished response
        let response: String = resp_receiver.recv()?;

        let Ok(tool_call) = extract_and_parse_tool_call(&response) else {
            // no tool call. all good. return here
            return Ok(self);
        };

        debug!("Got tool call! {tool_call:?}");
        self.extra.chat_state.add_tool_call(tool_call.clone());
        let _ = self.extra.chat_state.render_diff();
        // render diff just to keep up with context. discard result

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

        // render the templ
        self.extra
            .chat_state
            .add_tool_resp(tool_call.name, response);
        let diff = self.extra.chat_state.render_diff()?;

        self.read_string(diff)?
            .write_until_done(sampler, stop_words, wrapped_respond)?;

        // get the finished response
        let response: String = dbg!(resp_receiver.recv()?);

        Ok(self)
    }
}

fn extract_and_parse_tool_call(
    input: &str,
) -> Result<
    chat_state::ToolCall,
    // TODO: use proper error here
    Box<dyn std::error::Error>,
> {
    // Find the start and end tags
    // TODO: these are the tokens used by qwen3
    //       but e.g. deepseek uses "<｜tool▁calls▁begin｜><｜tool▁call▁begin｜>" instead.
    //       we need to support multiple different tool call begin tokens
    let start_tag = "<tool_call>";
    let end_tag = "</tool_call>";

    let start_idx = input.find(start_tag).ok_or("Start tag not found")? + start_tag.len();
    let end_idx = input.rfind(end_tag).ok_or("End tag not found")?;

    if start_idx >= end_idx {
        return Err("Invalid tag positions".into());
    }

    let json_str = &input[start_idx..end_idx].trim();
    debug!(json_str = ?json_str);

    // Parse the JSON
    let tool_call: chat_state::ToolCall = serde_json::from_str(json_str)?;
    debug!(tool_call = ?tool_call);

    Ok(tool_call)
}

// fn test_tool() -> chat_state::Tool {
//     chat_state::Tool {
//         r#type: chat_state::ToolType::Function,
//         function: chat_state::Function {
//             name: "get_current_temperature".to_string(),
//             description: "Gets the temperature at a given location".to_string(),
//             parameters: serde_json::json!({
//                 "type": "object",
//                 "properties": {
//                     "location": {
//                         "type": "string",
//                         "description": "The location to get the temperature for"
//                     }
//                 },
//                 "required": [
//                     "location"
//                 ]
//             }),
//         },
//     }
// }

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
                        "description": "The location to get the temperature for"
                    }
                },
                "required": [
                    "location"
                ]
            }),
            function: Box::new(|_| "13.37".into()),
        }
    }

    #[test]
    fn test_tool_chat() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let mut worker = Worker::new_tool_chat_worker(
            &model,
            4096,
            "beep boop you're a snoot".into(),
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
                "What is the temperature in Copenhagen, Denmark?".into(),
                crate::sampler_config::SamplerConfig::default(),
                vec![],
                f,
            )
            .expect("fuck");

        let result = receiver.recv();
        println!("{result:?}");
    }
}
