//! Stateless one-shot translation via TranslateGemma (or any model that accepts the same
//! structured content format).

use std::collections::HashMap;
use std::sync::MutexGuard;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use llama_cpp_2::token::LlamaToken;
use serde::Serialize;
use tracing::info;

use crate::chat::{TokenStream, TokenStreamAsync};
use crate::errors::{GenerateResponseError, InitWorkerError, TranslateWorkerError};
use crate::inference::{acquire_inference_lock, InferenceEngine};
use crate::llm::{self, GlobalInferenceLockToken, Worker, WorkerGuard, WriteOutput};
use crate::sampler::{read_sampler_from_metadata, SamplerConfig};
use crate::template::{select_template, ChatTemplate, ChatTemplateContext};
use crate::tokenizer::TokenizerChunks;

// ---------------------------------------------------------------------------
// Private serialisation types for the TranslateGemma template
// ---------------------------------------------------------------------------

#[derive(Serialize)]
struct TranslateMessage<'a> {
    role: &'static str,
    content: [TranslatePart<'a>; 1],
}

#[derive(Serialize)]
struct TranslatePart<'a> {
    r#type: &'static str,
    source_lang_code: &'a str,
    target_lang_code: &'a str,
    text: &'a str,
}

// ---------------------------------------------------------------------------
// Worker state
// ---------------------------------------------------------------------------

struct Translate<'a> {
    engine: InferenceEngine<'a>,
    should_stop: Arc<AtomicBool>,
    source_lang_code: String,
    target_lang_code: String,
    template: ChatTemplate,
    sampler_config: SamplerConfig,
}

impl<'a> Translate<'a> {
    fn new(
        model: &'a llm::Model,
        source_lang_code: String,
        target_lang_code: String,
        should_stop: Arc<AtomicBool>,
        n_ctx: u32,
    ) -> Result<Self, InitWorkerError> {
        let template = select_template(&model.language_model, false)?;
        let sampler_config = read_sampler_from_metadata(&model.language_model).unwrap_or_default();

        let Worker { engine, extra: () } = Worker::new_with_type(model, n_ctx, false, ())?;

        Ok(Self {
            engine,
            should_stop,
            source_lang_code,
            target_lang_code,
            template,
            sampler_config,
        })
    }

    fn translate<F>(&mut self, text: &str, mut respond: F) -> Result<(), TranslateWorkerError>
    where
        F: FnMut(WriteOutput),
    {
        self.should_stop.store(false, Ordering::Relaxed);
        self.engine.reset_context();

        let msg = TranslateMessage {
            role: "user",
            content: [TranslatePart {
                r#type: "text",
                source_lang_code: &self.source_lang_code,
                target_lang_code: &self.target_lang_code,
                text,
            }],
        };

        let ctx = ChatTemplateContext::new(HashMap::new(), None);
        let rendered = self.template.render_raw(&[msg], &ctx, true)?;
        let chunks = self.engine.tokenize(rendered, vec![])?;

        let lock = acquire_inference_lock();
        let empty = TokenizerChunks::new();
        self.engine.sync_context(chunks, &empty, &lock)?;

        self.generate_until_done(&mut respond, &lock)?;
        Ok(())
    }

    fn generate_until_done<F>(
        &mut self,
        respond: &mut F,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<(), GenerateResponseError>
    where
        F: FnMut(WriteOutput),
    {
        info!("Translate worker generating response");

        let mut full_response = String::with_capacity(4096);
        let mut sampler = self
            .sampler_config
            .clone()
            .to_stateful(self.engine.ctx.model)?;
        let mut decoder = encoding_rs::UTF_8.new_decoder();

        while !self.should_stop.load(Ordering::Relaxed) {
            if self.engine.is_context_full() {
                return Err(GenerateResponseError::ContextSize);
            }

            let new_token = self.engine.sample_and_decode_next_token(&mut sampler)?;

            let token_bytes = match self
                .engine
                .ctx
                .model
                .token_to_piece_bytes(new_token, 8, true, None)
            {
                Err(llama_cpp_2::TokenToStringError::InsufficientBufferSpace(i)) => {
                    self.engine.ctx.model.token_to_piece_bytes(
                        new_token,
                        (-i).try_into().expect("buffer size is positive"),
                        true,
                        None,
                    )
                }
                x => x,
            }?;

            let max_len = decoder
                .max_utf8_buffer_length(token_bytes.len())
                .unwrap_or(32);
            let mut token_str = String::with_capacity(max_len);
            let _ = decoder.decode_to_string(&token_bytes, &mut token_str, false);

            let gemma4_eog_hotfix = token_str == "<eos>" && new_token == LlamaToken::new(1);
            let has_eog = self.engine.ctx.model.is_eog_token(new_token) || gemma4_eog_hotfix;

            if !has_eog {
                full_response.push_str(&token_str);
                respond(WriteOutput::Token(token_str));
            }

            if has_eog {
                break;
            }
        }

        let _ = inference_lock_token; // held for the entire loop
        respond(WriteOutput::Done(full_response));
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Message enum for the worker thread
// ---------------------------------------------------------------------------

enum TranslateMsg {
    Translate {
        text: String,
        output_tx: tokio::sync::mpsc::UnboundedSender<WriteOutput>,
    },
}

// ---------------------------------------------------------------------------
// TranslateHandle — synchronous API
// ---------------------------------------------------------------------------

pub struct TranslateHandle {
    guard: WorkerGuard<TranslateMsg>,
}

impl TranslateHandle {
    pub fn new(
        model: Arc<llm::Model>,
        source: String,
        target: String,
        n_ctx: u32,
    ) -> Result<Self, InitWorkerError> {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();
        let (init_tx, init_rx) = std::sync::mpsc::channel::<Result<(), InitWorkerError>>();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = Arc::clone(&should_stop);

        let join_handle = std::thread::spawn(move || {
            let result = Translate::new(&model, source, target, should_stop_clone, n_ctx);
            let mut worker = match result {
                Ok(w) => {
                    let _ = init_tx.send(Ok(()));
                    w
                }
                Err(e) => {
                    let _ = init_tx.send(Err(e));
                    return;
                }
            };

            while let Ok(msg) = msg_rx.recv() {
                match msg {
                    TranslateMsg::Translate { text, output_tx } => {
                        let should_stop = Arc::clone(&worker.should_stop);
                        let error_tx = output_tx.clone();
                        let callback = move |out| {
                            if output_tx.send(out).is_err() {
                                should_stop.store(true, Ordering::Relaxed);
                            }
                        };
                        if let Err(e) = worker.translate(&text, callback) {
                            let _ = error_tx.send(WriteOutput::Error(Box::new(e)));
                        }
                    }
                }
            }
        });

        init_rx.recv().map_err(|_| InitWorkerError::NoResponse)??;

        Ok(Self {
            guard: WorkerGuard::new(msg_tx, join_handle, Some(should_stop)),
        })
    }

    pub fn translate(&self, text: impl Into<String>) -> TokenStream {
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        self.guard.send(TranslateMsg::Translate {
            text: text.into(),
            output_tx,
        });
        TokenStream::new_from_channel(output_rx)
    }
}

// ---------------------------------------------------------------------------
// TranslateHandleAsync — async API
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct TranslateHandleAsync {
    guard: Arc<WorkerGuard<TranslateMsg>>,
}

impl TranslateHandleAsync {
    pub fn new(
        model: Arc<llm::Model>,
        source: String,
        target: String,
        n_ctx: u32,
    ) -> Result<Self, InitWorkerError> {
        let handle = TranslateHandle::new(model, source, target, n_ctx)?;
        Ok(Self {
            guard: Arc::new(handle.guard),
        })
    }

    pub fn translate(&self, text: impl Into<String>) -> TokenStreamAsync {
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        self.guard.send(TranslateMsg::Translate {
            text: text.into(),
            output_tx,
        });
        TokenStreamAsync::new(output_rx)
    }

    /// Like `translate`, but exposes the raw `WriteOutput` channel instead of a
    /// `TokenStreamAsync`. Useful for integrations (e.g. Godot) that consume
    /// `Token` / `Done` / `Error` variants directly.
    pub fn translate_channel(
        &self,
        text: impl Into<String>,
    ) -> tokio::sync::mpsc::UnboundedReceiver<WriteOutput> {
        let (output_tx, output_rx) = tokio::sync::mpsc::unbounded_channel();
        self.guard.send(TranslateMsg::Translate {
            text: text.into(),
            output_tx,
        });
        output_rx
    }
}
