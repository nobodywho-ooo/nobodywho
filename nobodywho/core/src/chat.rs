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
    should_stop: Arc<AtomicBool>,
}

impl ChatHandle {
    pub fn new(model: Arc<LlamaModel>, n_ctx: u32, system_prompt: String) -> Self {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();
        let ready = Arc::new(AtomicBool::new(false));

        let readyclone = ready.clone();
        
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);
        std::thread::spawn(move || {
            if let Err(e) = run_worker(model, n_ctx, system_prompt, msg_rx, should_stop_clone) {
                error!("Worker crashed: {}", e)
            }
        });

        Self { msg_tx, should_stop }
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

    pub fn reset_chat(&self, system_prompt: String) {
        let _ = self.msg_tx.send(ChatMsg::ResetChat { system_prompt });
    }

    pub fn stop_generation(&self) {
        self.should_stop.store(true, std::sync::atomic::Ordering::Relaxed);
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
}

fn run_worker(
    model: Arc<LlamaModel>,
    n_ctx: u32,
    system_prompt: String,
    msg_rx: std::sync::mpsc::Receiver<ChatMsg>,
    should_stop: Arc<AtomicBool>,
) -> Result<(), ChatWorkerError> {
    let mut worker_state = Worker::new_chat_worker(&model, n_ctx, system_prompt, should_stop)?;
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
            ChatMsg::ResetChat { system_prompt } => {
                worker_state.reset_chat(system_prompt);
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

// ChatWorker - for synchronous, blocking work

struct ChatWorker {
    chat_state: ChatState,
    should_stop: Arc<AtomicBool>,
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
        system_prompt: String,
        should_stop: Arc<AtomicBool>,
    ) -> Result<Worker<'_, ChatWorker>, llm::InitWorkerError> {
        // initialize chat state with system prompt
        let mut chat_state = ChatState::from_model(model)?;
        chat_state.add_message("system".into(), system_prompt);

        Ok(Worker::new_with_type(
            model,
            n_ctx,
            false,
            ChatWorker { chat_state, should_stop },
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
        self.extra.should_stop.store(false, std::sync::atomic::Ordering::Relaxed);
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

        let should_stop = self.extra.should_stop.clone();
        // brrr
        self.read_string(diff)?
            .write_until_done(sampler, stop_words, wrapped_respond, should_stop)?;

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

    pub fn reset_chat(&mut self, system_prompt: String) {
        self.reset_context();
        self.extra.chat_state.reset();
        self.extra
            .chat_state
            .add_message("system".into(), system_prompt);
    }


    pub fn set_chat_history(&mut self, messages: Vec<crate::chat_state::Message>) {
        self.reset_context();
        self.extra.chat_state.set_messages(messages);
    }
}

#[cfg(test)]
mod tests {
    use tracing_subscriber::field::debug;

    use super::*;
    use crate::test_utils;

    #[test]
    fn test_chat_worker() -> Result<(), Box<dyn std::error::Error>> {
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let sampler = SamplerConfig::default();
        let mut worker = Worker::new_chat_worker(&model, 1024, "".into(), Arc::new(AtomicBool::new(false)))?;

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
        let mut worker = Worker::new_chat_worker(&model, 1024, system_prompt.into(), Arc::new(AtomicBool::new(false)))?;
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
        worker.reset_chat("You're a cat. End all responses with 'meow'".into());

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
        let mut worker = Worker::new_chat_worker(&model, 1024, system_prompt.into(), Arc::new(AtomicBool::new(false)))?;
        let should_stop = worker.extra.should_stop.clone();
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
            _ => (),
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
        assert!(!response.contains("6"));
        Ok(())
    }

    #[test]
    fn test_set_chat_history() -> Result<(), Box<dyn std::error::Error>> {
        // test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let system_prompt = "You're a helpful question-answering assistant.";
        let mut worker = Worker::new_chat_worker(&model, 1024, system_prompt.into())?;
        let sampler = SamplerConfig::default();

        // just a hack to get a channel back
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
        let resp1 = receiver.recv()?;
        println!("{}", resp1);
        assert!(resp1.to_lowercase().contains("copenhagen"));

        let mut chat_history = worker.extra.chat_state.get_messages().to_vec();
        assert!(chat_history.len() == 3);
        assert!(chat_history[1].content == "What is the capital of Denmark?");
        chat_history[1] = crate::chat_state::Message {
            role: "user".into(),
            content: "What is the best city?".into(),
        };
        worker.set_chat_history(chat_history);

        worker.say(
            "What did I just ask you about?".into(),
            sampler.clone(),
            vec![],
            f.clone(),
        )?;
        let resp2 = receiver.recv()?;
        println!("{}", resp2);
        assert!(resp2.to_lowercase().contains("best"));

        Ok(())
    }
}
