use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::{AddBos, LlamaModel, Special};
use llama_cpp_2::token::LlamaToken;
use std::sync::Arc;
use std::sync::{
    mpsc::{channel, Receiver, Sender},
    LazyLock,
};
use std::thread;

const SEQ_ID: u32 = 0;

static LLAMA_BACKEND: LazyLock<LlamaBackend> =
    LazyLock::new(|| LlamaBackend::init().expect("Failed to initialize llama backend"));

enum ActorMessage {
    Reset {},
    Advance { prompt: String },
    GetCompletion { respond_to: Sender<Option<String>> },
    GetEmbedding { respond_to: Sender<Vec<f32>> },
}

struct Actor {
    receiver: Receiver<ActorMessage>,
    pos: u32,
}

impl Actor {
    fn new(receiver: Receiver<ActorMessage>) -> Self {
        Self { receiver, pos: 0 }
    }

    fn handle_message(
        &mut self,
        msg: ActorMessage,
        ctx: &mut LlamaContext,
        batch: &mut LlamaBatch,
    ) {
        match msg {
            ActorMessage::Reset {} => {
                ctx.clear_kv_cache_seq(Some(SEQ_ID), None, None)
                    .expect("Failed to clear kv cache");

                self.pos = 0;
            }
            ActorMessage::Advance { prompt } => {
                // Get the current position
                let tokens = ctx.model.str_to_token(&prompt, AddBos::Always).unwrap();

                // We want to output logits for the last token in the prompt
                let last_index = (tokens.len() - 1) as i32;

                for (i, token) in (0..).zip(tokens.into_iter()) {
                    let output_logits = i == last_index;
                    batch
                        .add(token, (self.pos as i32) + i, &[0], output_logits)
                        .unwrap();
                }
            }
            ActorMessage::GetCompletion { respond_to } => {
                let mut utf8decoder = encoding_rs::UTF_8.new_decoder();

                loop {
                    {
                        let new_token_id = LlamaToken(0);

                        if ctx.model.is_eog_token(new_token_id) {
                            batch.clear();
                            batch
                                .add(new_token_id, self.pos as i32, &[SEQ_ID as i32], true)
                                .unwrap();
                            respond_to.send(None).unwrap();
                            break;
                        }

                        let output_bytes = ctx
                            .model
                            .token_to_bytes(new_token_id, Special::Tokenize)
                            .unwrap();

                        // use `Decoder.decode_to_string()` to avoid the intermediate buffer
                        let mut output_string = String::with_capacity(32);
                        let _decode_result =
                            utf8decoder.decode_to_string(&output_bytes, &mut output_string, false);

                        // send new token string back to user
                        // Append the token to the result
                        respond_to.send(Some(output_string)).unwrap();

                        // prepare batch or the next decode
                        batch.clear();

                        batch
                            .add(new_token_id, self.pos as i32, &[0], true)
                            .unwrap();
                    }

                    self.pos += 1;

                    ctx.decode(batch).unwrap();
                }
            }
            ActorMessage::GetEmbedding { respond_to } => {
                respond_to.send(vec![0.0; 1024]).unwrap();
            }
        }
    }

    fn run(&mut self, model: Arc<LlamaModel>) {
        let ctx_params = LlamaContextParams::default();
        let mut ctx = model.new_context(&LLAMA_BACKEND, ctx_params).unwrap();

        let mut batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);

        while let Ok(msg) = self.receiver.try_recv() {
            self.handle_message(msg, &mut ctx, &mut batch);
        }
    }
}

#[derive(Clone)]
pub struct ActorHandle {
    sender: Sender<ActorMessage>,
}

impl ActorHandle {
    pub fn new(model: Arc<LlamaModel>) -> Self {
        let (sender, receiver) = channel();

        let mut actor = Actor::new(receiver);

        thread::spawn(move || actor.run(model));

        Self { sender }
    }

    fn reset_context(&self) {
        let msg = ActorMessage::Reset {};
        self.sender.send(msg).unwrap();
    }

    fn advance(&self, prompt: String) {
        let msg = ActorMessage::Advance { prompt };
        self.sender.send(msg).unwrap();
    }

    fn get_completion(&self) -> Option<String> {
        let (send, recv) = channel();
        let msg = ActorMessage::GetCompletion { respond_to: send };
        self.sender.send(msg).unwrap();
        recv.recv().unwrap()
    }

    fn get_embedding(&self) -> Vec<f32> {
        let (send, recv) = channel();
        let msg = ActorMessage::GetEmbedding { respond_to: send };
        self.sender.send(msg).unwrap();
        recv.recv().unwrap()
    }

    pub fn prompt(&self, prompt: String) -> Option<String> {
        self.advance(prompt);
        self.get_completion()
    }

    pub fn embed(&self, prompt: String) -> Vec<f32> {
        self.advance(prompt);
        self.get_embedding()
    }
}
