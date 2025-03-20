use crate::chat_state;
use crate::llm;
use serde_json;
use std::error::Error;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, trace};

// XXX: random tool calling types

fn extract_and_parse_tool_call(input: &str) -> Result<chat_state::ToolCall, Box<dyn Error>> {
    // Find the start and end tags
    let start_tag = "<tool_call>";
    let end_tag = "</tool_call>";

    let start_idx = input.find(start_tag).ok_or("Start tag not found")? + start_tag.len();
    let end_idx = input.rfind(end_tag).ok_or("End tag not found")?;

    if start_idx >= end_idx {
        return Err("Invalid tag positions".into());
    }

    let json_str = &input[start_idx..end_idx].trim();

    // Parse the JSON
    let tool_call: chat_state::ToolCall = serde_json::from_str(json_str)?;

    Ok(tool_call)
}

#[derive(Debug, thiserror::Error)]
pub enum ChatLoopError {
    // see Issue #104 for why this error message is so long.
    // https://github.com/nobodywho-ooo/nobodywho/issues/96
    #[error(
        "Lama.cpp failed fetching chat template from the model file. \
        This is likely because you're using an older GGUF file, \
        which might not include a chat template. \
        For example, this is the case for most LLaMA2-based GGUF files. \
        Try using a more recent GGUF model file. \
        If you want to check if a given model includes a chat template, \
        you can use the gguf-dump script from llama.cpp. \
        Here is a more technical detailed error: {0}"
    )]
    ChatTemplateError(#[from] chat_state::FromModelError),

    #[error("Failed initializing the LLM worker: {0}")]
    InitWorkerError(#[from] llm::InitWorkerError),

    #[error("Worker died while generating response: {0}")]
    GenerateResponseError(#[from] llm::GenerateResponseError),

    #[error("Worker finished stream without a complete response")]
    NoResponseError,
}

pub trait ChatOutput {
    fn emit_token(&self, token: String);
    fn emit_response(&self, resp: String);
    fn emit_error(&self, err: String);
    fn call_tool(&self, name: String, args: String) -> String;
}

pub enum ChatMsg {
    Say(String),
    ResetContext,
}

fn test_tool() -> chat_state::Tool {
    chat_state::Tool {
        r#type: chat_state::ToolType::Function,
        function: chat_state::Function {
            name: "get_current_temperature".to_string(),
            description: "Gets the temperature at a given location".to_string(),
            parameters: serde_json::json!({
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
        },
    }
}

pub async fn emit_until_done(
    stream: tokio_stream::wrappers::ReceiverStream<
        Result<llm::WriteOutput, llm::GenerateResponseError>,
    >,
    output: &Box<dyn ChatOutput>,
    blacklist: &Vec<String>,
) -> Result<String, ChatLoopError> {
    let resp = stream
        .fold(None, |_, out| {
            debug!("Streamed out: {out:?}");
            match out {
                Ok(llm::WriteOutput::Token(token)) => {
                    trace!("Got new token: {token:?}");
                    if !blacklist.iter().any(|bt| token.contains(bt)) {
                        output.emit_token(token);
                    }
                    None
                }
                Err(err) => {
                    error!("Got error from worker: {err:?}");
                    output.emit_error(format!("{err:?}"));
                    Some(Err(err))
                }
                Ok(llm::WriteOutput::Done(resp)) => Some(Ok(resp)),
            }
        })
        .await
        .ok_or(ChatLoopError::NoResponseError)?;
    Ok(resp?)
}

pub async fn simple_chat_loop(
    params: llm::LLMActorParams,
    system_prompt: String,
    stop_words: Vec<String>,
    mut msg_rx: mpsc::Receiver<ChatMsg>,
    output: Box<dyn ChatOutput>,
) -> Result<(), ChatLoopError> {
    info!("Entering simple chat loop");

    // init chat state
    let mut chat_state = chat_state::ChatState::from_model(&params.model, vec![test_tool()])?;
    chat_state.add_message("system".to_string(), system_prompt.clone());
    info!("Initialized chat state.");

    // init actor
    let actor = llm::LLMActorHandle::new(params).await?;
    info!("Initialized actor.");

    let tool_control_tokens = vec!["<tool_call>".to_string()];

    // wait for message from user
    while let Some(msg) = msg_rx.recv().await {
        match msg {
            ChatMsg::Say(message) => {
                chat_state.add_message("user".to_string(), message);
                let diff = chat_state.render_diff().expect("TODO: handle err");

                // stream out the response
                let stream = actor
                    .generate_response(
                        diff,
                        [stop_words.clone(), tool_control_tokens.clone()].concat(),
                    )
                    .await;

                // TODO: don't emit <tool_call>
                let full_response = emit_until_done(stream, &output, &tool_control_tokens).await?;
                if full_response.contains("<tool_call>") {
                    todo!()
                }

                // TODO: stop on <tool_call> control token,
                //       and swap sampler for a json-schema compliant one

                match extract_and_parse_tool_call(&full_response) {
                    Ok(tool_call) => {
                        debug!("Performing tool call: {:?}", tool_call);
                        // TODO: support multiple tool calls in one message

                        // do the tool call
                        let resp = output
                            .call_tool(tool_call.name.clone(), tool_call.arguments.to_string());

                        // put tool call and results in chat_state
                        let _ = chat_state.render_diff();
                        chat_state.add_tool_calls(vec![tool_call.clone()]);
                        chat_state.add_tool_result(tool_call.name, resp);
                        let diff = chat_state.render_diff().expect("TODO: handle err");

                        // generate text
                        let stream = actor.generate_response(diff, stop_words.clone()).await;
                        let full_response = emit_until_done(stream, &output).await?;
                    }
                    Err(_) => {
                        // we have a full response. send it out.
                        output.emit_response(full_response.clone());
                        chat_state.add_message("assistant".to_string(), full_response);
                    }
                }

                // render diff just to update the internal length state
                let _ = chat_state.render_diff();
            }
            ChatMsg::ResetContext => {
                chat_state.reset();
                chat_state.add_message("system".to_string(), system_prompt.clone());
                actor.reset_context().await.expect("TODO: handle err");
            }
        }
    }

    // XXX: we only arrive here when the sender-part of the say channel is dropped
    // and in that case, we don't have anything to send our error to anyway
    Ok(()) // accept our fate
}

#[derive(Debug, thiserror::Error)]
pub enum EmbeddingLoopError {
    #[error("Failed initializing the LLM worker: {0}")]
    InitWorkerError(#[from] llm::InitWorkerError),

    #[error("Failed generating embedding: {0}")]
    GenerateEmbeddingError(#[from] llm::GenerateEmbeddingError),
}

pub trait EmbeddingOutput {
    fn emit_embedding(&self, embd: Vec<f32>);
}

pub async fn simple_embedding_loop(
    params: llm::LLMActorParams,
    mut text_rx: mpsc::Receiver<String>,
    output: Box<dyn EmbeddingOutput>,
) -> Result<(), EmbeddingLoopError> {
    let actor = llm::LLMActorHandle::new(params).await?;
    while let Some(text) = text_rx.recv().await {
        let embd = actor.generate_embedding(text).await?;
        output.emit_embedding(embd);
    }
    Ok(()) // we dead
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::sampler_config::SamplerConfig;
    use crate::test_utils;

    struct MockOutput {
        response_tx: mpsc::Sender<String>,
    }

    impl MockOutput {
        fn new() -> (Self, mpsc::Receiver<String>) {
            let (response_tx, response_rx) = mpsc::channel(1024);
            (Self { response_tx }, response_rx)
        }
    }

    impl ChatOutput for MockOutput {
        fn emit_response(&self, resp: String) {
            self.response_tx.try_send(resp).expect("send failed!");
        }
        fn emit_token(&self, token: String) {
            debug!("MockEngine: {token}");
        }
        fn emit_error(&self, err: String) {
            error!("MockEngine: {err}");
            panic!()
        }
        fn call_tool(&self, name: String, args: String) -> String {
            if name == "get_current_temperature" {
                "42.0".to_string()
            } else {
                panic!("unknown tool! {name:?}")
            }
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_chat_loop() {
        test_utils::init_test_tracing();

        // Setup
        let model = test_utils::load_test_model();
        let system_prompt =
            "You are a helpful assistant. The user asks you a question, and you provide an answer."
                .to_string();
        let params = llm::LLMActorParams {
            model,
            sampler_config: SamplerConfig::default(),
            n_ctx: 4096,
            stop_tokens: vec![],
            use_embeddings: false,
        };

        let (mock_output, mut response_rx) = MockOutput::new();
        let (say_tx, say_rx) = mpsc::channel(2);

        let local = tokio::task::LocalSet::new();
        local.spawn_local(simple_chat_loop(
            params,
            system_prompt,
            say_rx,
            Box::new(mock_output),
        ));

        let check_results = async move {
            let _ = say_tx
                .send(ChatMsg::Say("What is the capital of Denmark?".to_string()))
                .await;
            let response = response_rx.recv().await.unwrap();
            assert!(
                response.contains("Copenhagen"),
                "Expected completion to contain 'Copenhagen', got: {response}"
            );

            let _ = say_tx
                .send(ChatMsg::Say(
                    "What language do they speak there?".to_string(),
                ))
                .await;
            let response = response_rx.recv().await.unwrap();

            assert!(
                response.contains("Danish"),
                "Expected completion to contain 'Danish', got: {response}"
            );
        };

        // run stuff
        local.run_until(check_results).await;
    }

    #[test]
    fn test_extract_tool_call() {
        let toolcall_str = r#"<tool_call>{"name": "get_current_temperature", "arguments": {"location": "Copenhagen, Denmark"}}</tool_call>"#;
        let toolcall = extract_and_parse_tool_call(toolcall_str).expect("failed parsing tool call");
        assert_eq!(toolcall.name, "get_current_temperature");
        assert_eq!(
            toolcall.arguments,
            serde_json::json!({"location": "Copenhagen, Denmark"})
        );
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_tool_calling() {
        test_utils::init_test_tracing();

        // Setup
        let model = test_utils::load_test_model();
        let system_prompt =
            "You are a helpful assistant. The user asks you a question, and you provide an answer."
                .to_string();
        let params = llm::LLMActorParams {
            model,
            sampler_config: SamplerConfig::default(),
            n_ctx: 4096,
            stop_tokens: vec![],
            use_embeddings: false,
        };

        let (mock_output, mut response_rx) = MockOutput::new();
        let (say_tx, say_rx) = mpsc::channel(2);

        let local = tokio::task::LocalSet::new();
        local.spawn_local(simple_chat_loop(
            params,
            system_prompt,
            say_rx,
            Box::new(mock_output),
        ));

        let check_results = async move {
            let _ = say_tx
                .send(ChatMsg::Say(
                    "What is the temperature in Copenhagen, Denmark?".to_string(),
                ))
                .await;
            let response = response_rx.recv().await.unwrap();
            assert!(
                response.contains("42"),
                "Expected completion to contain 'Copenhagen', got: {response}"
            );
        };

        // run stuff
        local.run_until(check_results).await;
    }
}
