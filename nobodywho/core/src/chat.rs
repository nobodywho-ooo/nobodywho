use crate::chat_state::ChatState;
use crate::llm;
use crate::llm::Worker;
use crate::sampler_config::SamplerConfig;
use std::sync::Arc;
use tracing::error;

use llama_cpp_2::model::LlamaModel;

// ChatHandle - for parallelism

pub struct ChatHandle {
    msg_tx: std::sync::mpsc::Sender<ChatMsg>,
}

impl ChatHandle {
    pub fn new(model: Arc<LlamaModel>, n_ctx: u32) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            if let Err(e) = run_worker(model, n_ctx, msg_rx) {
                error!("Worker crashed: {}", e)
            }
        });

        Self { msg_tx }
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
            respond: output_tx,
        });
        output_rx
    }

    pub fn reset_chat(&self) {
        let _ = self.msg_tx.send(ChatMsg::ResetChat);
    }
}

enum ChatMsg {
    Say {
        text: String,
        sampler: SamplerConfig,
        stop_words: Vec<String>,
        respond: tokio::sync::mpsc::Sender<llm::WriteOutput>,
    },
    ResetChat,
}

#[derive(Debug, thiserror::Error)]
enum ChatWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorkerError(#[from] llm::InitWorkerError),

    #[error("Error reading string: {0}")]
    SayError(#[from] SayError),
}

fn run_worker(
    model: Arc<LlamaModel>,
    n_ctx: u32,
    msg_rx: std::sync::mpsc::Receiver<ChatMsg>,
) -> Result<(), ChatWorkerError> {
    let mut worker_state = Worker::new_chat_worker(&model, n_ctx)?;
    while let Ok(msg) = msg_rx.recv() {
        match msg {
            ChatMsg::Say {
                text,
                sampler,
                stop_words,
                respond,
            } => {
                let callback = move |out| {
                    let _ = respond.blocking_send(out);
                };
                worker_state.say(text, sampler, stop_words, callback)?;
            }
            ChatMsg::ResetChat => {
                worker_state.reset_chat();
            }
        }
    }
    Ok(())
}

// ChatWorker - for synchronous, blocking work

struct ChatWorker {
    chat_state: ChatState,
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

impl<'a> Worker<'_, ChatWorker> {
    fn new_chat_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
    ) -> Result<Worker<'_, ChatWorker>, llm::InitWorkerError> {
        let chat_state = ChatState::from_model(model)?;
        Ok(Worker::new_with_type(
            model,
            n_ctx,
            false,
            ChatWorker { chat_state },
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
        self.extra.chat_state.add_message("user".to_string(), text);
        let diff = self.extra.chat_state.render_diff()?;

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
        self.read_string(diff)?
            .write_until_done(sampler, stop_words, wrapped_respond)?;

        // get the finished response and add it to our chat history
        let response = resp_receiver.recv()?;
        self.extra
            .chat_state
            .add_message("assistant".to_string(), response);
        // render diff again, because this response is already in the context
        // next time we generate a diff, we want it to be of everything after this message
        let _ = self.extra.chat_state.render_diff()?;

        Ok(self)
    }

    pub fn reset_chat(&mut self) {
        self.reset_context();
        self.extra.chat_state.reset();
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

        let mut worker = Worker::new_chat_worker(&model, 1024)?;

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
}
