use crate::chat_state;
use crate::llm;
use tokio::sync::mpsc;
use tokio_stream::StreamExt;
use tracing::{debug, error, info, trace};

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
    InitChatTemplateError(#[from] chat_state::FromModelError),

    #[error("Failed rendering chat template: {0}")]
    RenderChatTemplateError(#[from] minijinja::Error),

    #[error("Failed initializing the LLM worker: {0}")]
    InitWorkerError(#[from] llm::InitWorkerError),

    #[error("Worker died while generating response: {0}")]
    GenerateResponseError(#[from] llm::GenerateResponseError),

    #[error("Worker finished stream without a complete response")]
    NoResponseError,

    #[error("Couldn't get response from LLM worker. It probably died: {0}")]
    WorkerDiedError(#[from] tokio::sync::oneshot::error::RecvError),
}

pub trait ChatOutput {
    fn emit_token(&self, token: String);
    fn emit_response(&self, resp: String);
    fn emit_error(&self, err: String);
}

pub enum ChatMsg {
    Say(String),
    ResetContext(String),
}

#[tracing::instrument(level = "trace", skip(output, params))]
pub async fn simple_chat_loop(
    params: llm::LLMActorParams,
    system_prompt: String,
    mut msg_rx: mpsc::Receiver<ChatMsg>,
    output: Box<dyn ChatOutput>,
) -> Result<(), ChatLoopError> {
    // init chat state
    let mut chat_state = chat_state::ChatState::from_model(&params.model)?;
    chat_state.add_message("system".to_string(), system_prompt.clone());
    info!("Initialized chat state.");

    // init actor
    let actor = llm::LLMActorHandle::new(params).await?;
    info!("Initialized actor.");

    // wait for message from user
    while let Some(msg) = msg_rx.recv().await {
        match msg {
            ChatMsg::Say(message) => {
                chat_state.add_message("user".to_string(), message);
                let diff = chat_state.render_diff()?;

                // stream out the response
                let full_response = actor
                    .generate_response(diff)
                    .await
                    .fold(None, |_, out| match out {
                        Ok(llm::WriteOutput::Token(token)) => {
                            output.emit_token(token);
                            None
                        }
                        Err(err) => {
                            error!("Got error from worker: {err:?}");
                            output.emit_error(format!("{err:?}"));
                            Some(Err(err))
                        }
                        Ok(llm::WriteOutput::Done(resp)) => Some(Ok(resp)),
                    })
                    .await
                    .ok_or(ChatLoopError::NoResponseError)??;

                // we have a full response. send it out.
                output.emit_response(full_response.clone());
                chat_state.add_message("assistant".to_string(), full_response);

                // render diff just to update the internal length state
                let _ = chat_state.render_diff();
            }
            ChatMsg::ResetContext(system_prompt) => {
                chat_state.reset();
                chat_state.add_message("system".to_string(), system_prompt.clone());
                actor.reset_context().await?;
            }
        }
    }

    // XXX: we only arrive here when the sender-part of the say channel is dropped
    // and in that case, we don't have anything to send our error to anyway
    info!("simple_chat_loop exiting");
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

    #[tokio::test(flavor = "current_thread")]
    async fn test_reset_context() {
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
            let _ = say_tx.send(ChatMsg::Say("Hello, world.".to_string())).await;
            let response_1 = response_rx.recv().await.unwrap();

            let new_system_prompt = "You're a wizard, Harry.".to_string();
            let _ = say_tx.send(ChatMsg::ResetContext(new_system_prompt)).await;

            let _ = say_tx.send(ChatMsg::Say("Hello, world.".to_string())).await;
            let response_2 = response_rx.recv().await.unwrap();

            assert!(
                response_1 != response_2,
                "Expected responses to differ after resetting context, got {response_1} and {response_2}"
            );
        };

        // run stuff
        local.run_until(check_results).await;
    }
}
