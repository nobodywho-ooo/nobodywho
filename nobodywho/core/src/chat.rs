use crate::chat_state;
use crate::chat_state::ChatState;
use crate::llm;
use crate::llm::Worker;
use crate::sampler_config::SamplerConfig;
use llama_cpp_2::model::LlamaModel;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use tracing::{debug, error, warn};

// PARALLELISM

pub struct ChatHandle {
    msg_tx: std::sync::mpsc::Sender<ChatMsg>,
    should_stop: Arc<AtomicBool>,
}

impl ChatHandle {
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

    pub fn reset_chat(&self, system_prompt: String, tools: Vec<Tool>) {
        let _ = self.msg_tx.send(ChatMsg::ResetChat {
            system_prompt,
            tools,
        });
    }

    pub fn stop_generation(&self) {
        self.should_stop
            .store(true, std::sync::atomic::Ordering::Relaxed);
    }

    pub fn get_chat_history(&self) -> tokio::sync::mpsc::Receiver<Vec<crate::chat_state::Message>> {
        let (output_tx, output_rx) = tokio::sync::mpsc::channel(1);
        let _ = self.msg_tx.send(ChatMsg::GetChatHistory { output_tx });
        output_rx
    }

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
    GetChatHistory {
        output_tx: tokio::sync::mpsc::Sender<Vec<crate::chat_state::Message>>,
    },
    SetChatHistory {
        messages: Vec<crate::chat_state::Message>,
        output_tx: tokio::sync::mpsc::Sender<()>,
    },
}

#[derive(thiserror::Error, Debug)]
enum ChatWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorkerError(#[from] llm::InitWorkerError),

    #[error("Error reading string: {0}")]
    SayError(#[from] SayError),

    #[error("Init template error: {0}")]
    TemplateError(#[from] chat_state::FromModelError),
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
            ChatMsg::GetChatHistory { output_tx } => {
                let _ =
                    output_tx.blocking_send(worker_state.extra.chat_state.get_messages().to_vec());
            }
            ChatMsg::SetChatHistory {
                messages,
                output_tx,
            } => {
                worker_state.set_chat_history(messages);
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

#[derive(Clone)]
pub struct Tool {
    name: String,
    description: String,
    json_schema: serde_json::Value,
    function: Arc<dyn Fn(serde_json::Value) -> String>,
}

impl Tool {
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

fn grammar_from_tools(tools: &[Tool]) -> Result<gbnf::Grammar, gbnf::JsonSchemaParseError> {
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

impl llm::GenerationCapability for ChatWorker {}
impl llm::Stoppable for ChatWorker {
    fn stop(&self) -> bool {
        self.should_stop.load(std::sync::atomic::Ordering::Relaxed)
    }
}

impl<'a> Worker<'_, ChatWorker> {
    fn new_chat_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
        system_prompt: String,
        should_stop: Arc<AtomicBool>,
        tools: Vec<Tool>,
    ) -> Result<Worker<'_, ChatWorker>, llm::InitWorkerError> {
        // initialize chat state with system prompt
        let mut chat_state = ChatState::from_model_and_tools(
            model,
            tools.iter().map(|t| t.to_chat_state_tool()).collect(),
        )?;
        chat_state.add_system_message(system_prompt);

        let grammar = if tools.len() > 0 {
            grammar_from_tools(&tools).ok()
        } else {
            None
        };

        Ok(Worker::new_with_type(
            model,
            n_ctx,
            false,
            ChatWorker {
                chat_state,
                tools,
                tool_grammar: grammar,
                should_stop,
            },
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
        // reset the stop flag
        self.extra
            .should_stop
            .store(false, std::sync::atomic::Ordering::Relaxed);

        // TODO: this is the token used by qwen3
        //       but e.g. deepseek uses "<｜tool▁calls▁begin｜><｜tool▁call▁begin｜>" instead.
        //       we need to support multiple different tool call begin tokens
        let tool_call_begin = "<tool_call>";

        self.extra.chat_state.add_user_message(text);
        let diff = self.extra.chat_state.render_diff()?;

        // wrap the response callback to keep a copy of the completed response
        // and to avoid emitting tool calls
        let (wrapped_respond, resp_receiver) =
            wrap_respond(respond.clone(), tool_call_begin.into());

        let mut sampler = sampler;
        if let Some(ref tool_grammar) = self.extra.tool_grammar {
            sampler.use_grammar = true;
            sampler.grammar_root = "superroot".into();
            sampler.lazy_grammar_trigger = "<tool_call>".into(); // TODO: multiple tool call tokens
            sampler.gbnf_grammar = tool_grammar.to_string();
        }

        // llm go brrr
        self.read_string(diff)?.write_until_done(
            sampler.clone(),
            stop_words.clone(),
            wrapped_respond,
        )?;

        // get the finished response
        let mut response: String = resp_receiver.recv()?;

        while let Some(tool_calls) = extract_tool_calls(&response) {
            debug!("Got tool calls! {tool_calls:?}");

            self.extra.chat_state.add_tool_calls(tool_calls.clone());
            let _ = self.extra.chat_state.render_diff()?;
            // render diff just to keep up with context.
            // discard result, because the llm context has already seen these tokens

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

            let diff = self.extra.chat_state.render_diff()?;

            let (wrapped_respond, resp_receiver) =
                wrap_respond(respond.clone(), tool_call_begin.into());
            self.read_string(diff)?.write_until_done(
                sampler.clone(),
                stop_words.clone(),
                wrapped_respond,
            )?;

            // get the finished response
            response = resp_receiver.recv()?;
        }
        debug_assert!(!response.contains(tool_call_begin));
        self.extra.chat_state.add_assistant_message(response);
        let _ = self.extra.chat_state.render_diff()?;

        Ok(self)
    }

    pub fn reset_chat(
        &mut self,
        system_prompt: String,
        tools: Vec<Tool>,
    ) -> Result<(), chat_state::FromModelError> {
        self.reset_context();
        self.extra.chat_state = ChatState::from_model_and_tools(
            self.ctx.model,
            tools.iter().map(|t| t.to_chat_state_tool()).collect(),
        )?;
        self.extra.tool_grammar = if tools.len() > 0 {
            grammar_from_tools(&tools).ok()
        } else {
            None
        };
        self.extra.tools = tools;
        self.extra.chat_state.add_system_message(system_prompt);
        Ok(())
    }

    pub fn set_chat_history(&mut self, messages: Vec<crate::chat_state::Message>) {
        self.reset_context();
        self.extra.chat_state.set_messages(messages);
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

    #[test]
    fn test_chat_worker() -> Result<(), Box<dyn std::error::Error>> {
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let sampler = SamplerConfig::default();
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
        let sampler = SamplerConfig::default();

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
        worker.reset_chat("You're a cat. End all responses with 'meow'".into(), vec![]);

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

        let sampler = SamplerConfig::default();

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
                crate::sampler_config::SamplerConfig::default(),
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
            crate::sampler_config::SamplerConfig::default(),
            vec![],
            f,
        )
        .expect("dammit");

        let result = receiver.recv().unwrap();
        println!("{}", result);
        assert!(result.contains("13.37"));
        assert!(result.contains("0.15"));
    }
}
