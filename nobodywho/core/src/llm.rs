use crate::sampler_config::{make_sampler, SamplerConfig};
use lazy_static::lazy_static;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;
use std::pin::pin;
use std::sync::{Arc, LazyLock, Mutex};
use tokio;
use tokio::sync::{mpsc, oneshot};
use tracing::{debug, debug_span, error, info, trace, trace_span, warn};

const MAX_TOKEN_STR_LEN: usize = 128;

const CHANNEL_SIZE: usize = 4096; // this number is very arbitrary

lazy_static! {
    static ref GLOBAL_INFERENCE_LOCK: Mutex<()> = Mutex::new(());
}

static LLAMA_BACKEND: LazyLock<LlamaBackend> =
    LazyLock::new(|| LlamaBackend::init().expect("Failed to initialize llama backend"));

#[derive(Debug)]
pub enum LLMOutput {
    Token(String),
    Done(String),
    Embedding(Vec<f32>),
    FatalErr(WorkerError),
}

pub type Model = Arc<LlamaModel>;

pub fn has_discrete_gpu() -> bool {
    // TODO: Upstream a safe API for accessing the ggml backend API
    unsafe {
        for i in 0..llama_cpp_sys_2::ggml_backend_dev_count() {
            let dev = llama_cpp_sys_2::ggml_backend_dev_get(i);

            if llama_cpp_sys_2::ggml_backend_dev_type(dev)
                == llama_cpp_sys_2::GGML_BACKEND_DEVICE_TYPE_GPU
            {
                return true;
            }
        }
    }

    false
}

#[derive(Debug, thiserror::Error)]
pub enum LoadModelError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Invalid or unsupported GGUF model: {0}")]
    InvalidModel(String),
}

pub fn get_model(
    model_path: &str,
    use_gpu_if_available: bool,
) -> Result<Arc<LlamaModel>, LoadModelError> {
    if !std::path::Path::new(model_path).exists() {
        let e = LoadModelError::ModelNotFound(model_path.into());
        error!("{e:?}");
        return Err(e);
    }

    // TODO: `LlamaModelParams` uses all devices by default. Set it to an empty list once an upstream device API is available.
    let model_params = LlamaModelParams::default().with_n_gpu_layers(
        if use_gpu_if_available && has_discrete_gpu() {
            u32::MAX
        } else {
            0
        },
    );

    let model_params = pin!(model_params);
    let model =
        LlamaModel::load_from_file(&LLAMA_BACKEND, model_path, &model_params).map_err(|e| {
            LoadModelError::InvalidModel(format!(
                "Bad model path: {} - Llama.cpp error: {}",
                model_path, e
            ))
        })?;
    Ok(Arc::new(model))
}

#[allow(dead_code)]
fn print_kv_cache(ctx: &mut LlamaContext) {
    let mut kv_cache_view = ctx.new_kv_cache_view(1);
    kv_cache_view.update();
    for cell in kv_cache_view.cells() {
        println!("cell: {:?}", cell);
    }
}

/// Performs context window shifting by discarding old tokens and shifting remaining ones left.
/// This prevents context overflow by removing older tokens when nearing context length limits.
/// As implemented in <https://github.com/ggerganov/llama.cpp/blob/3b4f2e33e2cbfca621e623c4b92b88da57a8c2f4/examples/main/main.cpp#L528>
///
/// # Arguments
/// * `ctx` - LLaMA context to perform shifting on
/// * `pos` - Current position in context window
///
/// # Returns
/// * `Ok(n_discard)` - Number of tokens discarded from start of context
/// * `Err(WorkerError)` - If cache operations fail
fn apply_context_shifting(
    ctx: &mut LlamaContext,
    n_past: i32,
) -> Result<i32, llama_cpp_2::context::kv_cache::KvCacheConversionError> {
    warn!("Applying context shifting.");
    let n_keep = 0;
    let n_left = n_past - n_keep;
    let n_discard = n_left / 2;

    debug_assert!(n_past == ctx.get_kv_cache_token_count());

    // Delete the first `n_discard` tokens
    ctx.clear_kv_cache_seq(
        Some(0),
        Some(n_keep as u32),
        Some((n_keep + n_discard) as u32),
    )?;

    debug_assert!(n_past - n_discard == ctx.get_kv_cache_token_count());

    // Shift the context left with `n_discard` tokens
    ctx.kv_cache_seq_add(
        0,
        Some((n_keep + n_discard) as u32),
        Some(n_past as u32),
        -n_discard,
    )?;

    ctx.kv_cache_update();

    debug!(target: "Context shifted", ?n_discard);

    Ok(n_discard)
}

/// Parameters for configuring an LLM actor instance.
///
/// This struct contains the configuration needed to create a new LLM actor,
/// including the model, sampling parameters, context size, and stop tokens.
///
/// # Fields
/// * `model` - The LLaMA model to use for inference, wrapped in an Arc for thread-safe sharing
/// * `sampler_config` - Configuration for the token sampling strategy
/// * `n_ctx` - Maximum context length in tokens
/// * `use_embeddings` - Whether to generate embeddings or not.
#[derive(Clone)]
pub struct LLMActorParams {
    pub model: Arc<LlamaModel>,
    pub sampler_config: SamplerConfig,
    pub n_ctx: u32,
    pub use_embeddings: bool,
}

#[derive(Debug)]
pub struct LLMActorHandle {
    message_tx: std::sync::mpsc::Sender<WorkerMsg>,
}

impl LLMActorHandle {
    pub async fn new(params: LLMActorParams) -> Result<Self, InitWorkerError> {
        debug!("Initializing LLMActorHandle..");
        let (message_tx, message_rx) = std::sync::mpsc::channel();
        let (init_tx, init_rx) = oneshot::channel();

        std::thread::spawn(|| completion_worker_actor(message_rx, init_tx, params));

        trace!("Waiting for init result");
        let resp = match init_rx.await {
            Ok(Ok(())) => Ok(Self { message_tx }),
            Ok(Err(e)) => Err(e),
            Err(_recverr) => Err(InitWorkerError::NoResponse),
        };
        trace!("Got init result: {resp:?}");
        resp
    }

    pub async fn reset_context(&self) -> Result<(), oneshot::error::RecvError> {
        let (respond_to, response) = oneshot::channel();
        let _ = self.message_tx.send(WorkerMsg::ResetContext(respond_to));
        response.await
    }

    pub async fn read(
        &self,
        text: String,
    ) -> Result<Result<(), ReadError>, oneshot::error::RecvError> {
        let (respond_to, response_channel) = oneshot::channel();
        let _ = self
            .message_tx
            .send(WorkerMsg::ReadString(text, respond_to));
        response_channel.await
    }

    pub async fn write_until_done(
        &self,
        stop_words: Vec<String>,
    ) -> tokio_stream::wrappers::ReceiverStream<Result<WriteOutput, WriteError>> {
        let (respond_to, response_channel) = mpsc::channel(CHANNEL_SIZE);
        let _ = self
            .message_tx
            .send(WorkerMsg::WriteUntilDone(stop_words, respond_to));
        response_channel.into()
    }

    pub async fn get_embedding(
        &self,
    ) -> Result<Result<Vec<f32>, llama_cpp_2::EmbeddingsError>, oneshot::error::RecvError> {
        let (respond_to, response_channel) = oneshot::channel();
        let _ = self.message_tx.send(WorkerMsg::GetEmbedding(respond_to));
        response_channel.await
    }

    pub async fn generate_response(
        &self,
        text: String,
        stop_words: Vec<String>,
    ) -> tokio_stream::wrappers::ReceiverStream<Result<WriteOutput, GenerateResponseError>> {
        let (respond_to, response_channel) = mpsc::channel(CHANNEL_SIZE);
        let _ = self
            .message_tx
            .send(WorkerMsg::GenerateResponse(text, stop_words, respond_to));
        response_channel.into()
    }

    pub async fn generate_embedding(
        &self,
        text: String,
    ) -> Result<Vec<f32>, GenerateEmbeddingError> {
        let (respond_to, response_channel) = oneshot::channel();
        let _ = self
            .message_tx
            .send(WorkerMsg::GenerateEmbedding(text, respond_to));
        response_channel.await?
    }
}

fn completion_worker_actor(
    message_rx: std::sync::mpsc::Receiver<WorkerMsg>,
    init_tx: oneshot::Sender<Result<(), InitWorkerError>>,
    params: LLMActorParams,
) {
    match WorkerState::new(&params) {
        Ok(mut state) => {
            let _ = init_tx.send(Ok(())); // no way to recover from this send error

            // listen for messages forever
            while let Ok(msg) = message_rx.recv() {
                match handle_msg(state, msg) {
                    Ok(newstate) => {
                        state = newstate;
                    }
                    Err(_err) => {
                        return; // we died.
                    }
                }
            } // message queue dropped. we died.
        }
        Err(initerr) => {
            trace!("Init WorkerState failure.");
            let _ = init_tx.send(Err(initerr));
            // we died. not much to do.
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum InitWorkerError {
    #[error("Could not determine number of threads available: {0}")]
    ThreadCountError(#[from] std::io::Error),

    #[error("Could not create context: {0}")]
    CreateContextError(#[from] llama_cpp_2::LlamaContextLoadError),

    #[error("Got no response after initializing worker.")]
    NoResponse,
}

#[derive(Debug)]
struct WorkerState<'a> {
    n_past: i32,
    ctx: LlamaContext<'a>,
    sampler: LlamaSampler,
    big_batch: LlamaBatch,
    small_batch: LlamaBatch,
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("Could not determine number of threads available: {0}")]
    ThreadCountError(#[from] std::io::Error),

    #[error("Could not create context: {0}")]
    CreateContextError(#[from] llama_cpp_2::LlamaContextLoadError),

    #[error("Could not initialize worker: {0}")]
    InitWorkerError(#[from] InitWorkerError),

    #[error("Error reading string: {0}")]
    ReadError(#[from] ReadError),

    #[error("Error generating text: {0}")]
    WriteError(#[from] WriteError),

    #[error("Error getting embeddings: {0}")]
    EmbeddingsError(#[from] llama_cpp_2::EmbeddingsError),

    #[error("Could not send newly generated token out to the game engine.")]
    SendError, // this is actually a SendError<LLMOutput>, but that becomes recursive and weird

    // #[error("Could not receive from channel: {0}")]
    // RecvError(#[from] mpsc::error::RecvError),
    #[error("Global Inference Lock was poisoned.")]
    GILPoisonError, // this is actually a std::sync::PoisonError<std::sync::MutexGuard<'static, ()>>, but that doesn't implement Send, so we do this
}

#[derive(Debug, thiserror::Error)]
pub enum GenerateResponseError {
    #[error("Error reading string: {0}")]
    ReadError(#[from] ReadError),

    #[error("Error generating text: {0}")]
    WriteError(#[from] WriteError),
}

#[derive(Debug, thiserror::Error)]
pub enum GenerateEmbeddingError {
    #[error("Error reading string: {0}")]
    ReadError(#[from] ReadError),

    #[error("Error generating text: {0}")]
    EmbeddingsError(#[from] llama_cpp_2::EmbeddingsError),

    #[error("Error receiving response: {0}")]
    RecvError(#[from] oneshot::error::RecvError),
}

#[derive(Debug)]
pub enum WorkerMsg {
    ReadString(String, oneshot::Sender<Result<(), ReadError>>),
    WriteUntilDone(Vec<String>, mpsc::Sender<Result<WriteOutput, WriteError>>),
    GetEmbedding(oneshot::Sender<Result<Vec<f32>, llama_cpp_2::EmbeddingsError>>),
    ResetContext(oneshot::Sender<()>),
    GenerateResponse(
        String,
        Vec<String>,
        mpsc::Sender<Result<WriteOutput, GenerateResponseError>>,
    ),
    GenerateEmbedding(
        String,
        oneshot::Sender<Result<Vec<f32>, GenerateEmbeddingError>>,
    ),
}

fn handle_msg(state: WorkerState, msg: WorkerMsg) -> Result<WorkerState, ()> {
    // HACK
    // this is needed because contexts referencing the same model are not thread safe
    // if two contexts referencing the same model try to decode at the same time,
    // then llama.cpp segfaults and everybody dies and i become sad
    debug!("Worker handling message: {msg:?}");
    let _inference_lock = GLOBAL_INFERENCE_LOCK.lock().expect("GIL mutex poisoned.");

    match msg {
        WorkerMsg::ReadString(text, respond_to) => state.read_string(text).map_err(|e| {
            let _ = respond_to.send(Err(e));
            ()
        }),
        WorkerMsg::WriteUntilDone(stop_words, respond_to) => state
            .write_until_done(stop_words, |out| {
                let _ = respond_to.blocking_send(Ok(out));
            })
            .map_err(|e| {
                let _ = respond_to.blocking_send(Err(e.into()));
                ()
            }),
        WorkerMsg::GetEmbedding(respond_to) => match state.ctx.embeddings_seq_ith(0) {
            Ok(embd) => {
                let _ = respond_to.send(Ok(embd.to_vec()));
                Ok(state)
            }
            Err(e) => {
                let _ = respond_to.send(Err(e.into()));
                Err(())
            }
        },
        WorkerMsg::ResetContext(respond_to) => {
            let new_state = state.reset_context();
            respond_to.send(());
            Ok(new_state)
        }
        // read then write text until done
        WorkerMsg::GenerateResponse(text, stop_words, respond_to) => state
            .read_string(text)
            .map_err(|e| {
                let _ = respond_to.blocking_send(Err(e.into()));
                ()
            })?
            .write_until_done(stop_words, |out| {
                let _ = respond_to.blocking_send(Ok(out));
            })
            .map_err(|e| {
                let _ = respond_to.blocking_send(Err(e.into()));
                ()
            }),
        // read string then retrieve embedding
        WorkerMsg::GenerateEmbedding(text, respond_to) => {
            // try reading the string
            let state = match state.read_string(text) {
                Ok(new_state) => new_state,
                Err(e) => {
                    // error and return early, moving respond_to only once
                    let _ = respond_to.send(Err(e.into()));
                    return Err(());
                }
            };

            // try getting embeddings
            match state.ctx.embeddings_seq_ith(0) {
                Ok(embd) => {
                    // success!
                    let _ = respond_to.send(Ok(embd.to_vec()));
                    Ok(state.reset_context())
                }
                Err(e) => {
                    // :(
                    let _ = respond_to.send(Err(e.into()));
                    Err(())
                }
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("Could not tokenize string: {0}")]
    TokenizerError(#[from] llama_cpp_2::StringToTokenError),

    #[error("Could not add to batch: {0}")]
    BatchAddError(#[from] llama_cpp_2::llama_batch::BatchAddError),

    #[error("Llama.cpp failed decoding: {0}")]
    DecodeError(#[from] llama_cpp_2::DecodeError),
}

#[derive(Debug)]
pub enum WriteOutput {
    Token(String),
    Done(String),
}

#[derive(Debug, thiserror::Error)]
pub enum WriteError {
    #[error("Could not apply context shifting: {0}")]
    ContextShiftError(#[from] llama_cpp_2::context::kv_cache::KvCacheConversionError),

    #[error("Could not add token to batch: {0}")]
    BatchAddError(#[from] llama_cpp_2::llama_batch::BatchAddError),

    #[error("Llama.cpp failed decoding: {0}")]
    DecodeError(#[from] llama_cpp_2::DecodeError),

    #[error("Error sending message")]
    SendError,
}

impl<'a> WorkerState<'a> {
    fn new(params: &LLMActorParams) -> Result<WorkerState, InitWorkerError> {
        info!("Initializing WorkerState");
        // Set up context parameters using available parallelism
        let ctx = {
            let n_threads = std::thread::available_parallelism()?.get() as i32;
            let n_ctx = std::cmp::min(params.n_ctx, params.model.n_ctx_train());
            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(std::num::NonZero::new(n_ctx))
                .with_n_threads(n_threads)
                .with_n_threads_batch(n_threads)
                .with_embeddings(params.use_embeddings);

            // Create inference context and sampler
            params.model.new_context(&LLAMA_BACKEND, ctx_params)?
        };

        let big_batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);
        let small_batch = LlamaBatch::new(1, 1);

        let state = WorkerState {
            n_past: 0,
            sampler: make_sampler(&params.model, params.sampler_config.clone()),
            ctx,
            big_batch,
            small_batch,
        };
        Ok(state)
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn reset_context(mut self) -> Self {
        self.ctx.clear_kv_cache();
        self.n_past = 0;
        self
    }

    #[tracing::instrument(level = "info", skip(self))]
    fn read_string(mut self, text: String) -> Result<Self, ReadError> {
        let tokens = self.ctx.model.str_to_token(&text, AddBos::Never)?;
        let n_tokens = tokens.len();
        info!("Reading {n_tokens} tokens.");

        debug_assert!(tokens.len() > 0);
        debug_assert!(tokens.len() < self.ctx.n_ctx() as usize);

        {
            self.big_batch.clear();
            let seq_ids = &[0];
            for (i, token) in (0..).zip(tokens.iter()) {
                // Only compute logits for the last token to save computation
                let output_logits = i == n_tokens - 1;
                self.big_batch
                    .add(*token, self.n_past + i as i32, seq_ids, output_logits)?;
            }
        }

        // llm go brr
        let decode_span = debug_span!("read decode", n_tokens = n_tokens);
        let decode_guard = decode_span.enter();
        self.ctx.decode(&mut self.big_batch)?;
        drop(decode_guard);
        // brrr

        Ok(WorkerState {
            n_past: self.n_past + tokens.len() as i32,
            ..self
        })
    }

    #[tracing::instrument(level = "info", skip(self, respond))]
    fn write_until_done<F>(
        mut self,
        stop_words: Vec<String>,
        respond: F, // respond_to: Sender<Result<WriteOutput, WriteError>>,
    ) -> Result<Self, WriteError>
    where
        F: Fn(WriteOutput),
    {
        // Token generation loop
        info!("Worker writing until done");

        // pre-allocating 4096 bytes for the response string
        // 4096 is a very randomly chosen number. how does this affect performance?
        let mut full_response: String = String::with_capacity(4096);

        loop {
            // Check for context window overflow (it was in the end before)
            if self.n_past >= self.ctx.n_ctx() as i32 - 1 {
                self.n_past -= apply_context_shifting(&mut self.ctx, self.n_past)?;
                // check count
                // XXX: this check is slow
                debug_assert!(self.n_past == self.ctx.get_kv_cache_token_count());
            }

            // Sample next token, no need to use sampler.accept as sample already accepts the token.
            // using sampler.accept() will cause the sampler to crash when using grammar sampling.
            // https://github.com/utilityai/llama-cpp-rs/issues/604
            trace!("Applying sampler...");
            let new_token: LlamaToken = self.sampler.sample(&self.ctx, -1);

            // batch of one
            self.small_batch.clear();
            self.small_batch.add(new_token, self.n_past, &[0], true)?;

            // llm go brr
            let decode_span = trace_span!("write decode", n_past = self.n_past);
            let decode_guard = decode_span.enter();
            self.ctx.decode(&mut self.small_batch)?;
            drop(decode_guard);
            self.n_past += 1; // keep count

            // Convert token to text
            let token_string = self
                .ctx
                .model
                .token_to_str_with_size(new_token, MAX_TOKEN_STR_LEN, Special::Tokenize)
                .unwrap_or("�".to_string());
            // fall back to "U+FFFD REPLACEMENT CHARACTER"
            // when encountering bytes that aren't valid UTF-8
            // wikipedia: "used to replace an unknown, unrecognised, or unrepresentable character"

            trace!(?new_token, ?token_string);
            let has_eog = self.ctx.model.is_eog_token(new_token);

            if !has_eog {
                full_response.push_str(&token_string);
                trace!("Sending out token: {token_string}");
                respond(WriteOutput::Token(token_string));
            }

            let has_stop_word = stop_words
                .iter()
                .any(|stop_word| full_response.contains(stop_word));
            if has_eog || has_stop_word {
                break;
            }
        }

        // we're done!
        trace!("Sending out response: {full_response}");
        respond(WriteOutput::Done(full_response));
        Ok(self)
    }
}

fn dotproduct(a: &[f32], b: &[f32]) -> f32 {
    assert!(a.len() == b.len());
    a.iter().zip(b.iter()).map(|(x, y)| x * y).sum()
}

pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    let norm_a = dotproduct(a, a).sqrt();
    let norm_b = dotproduct(b, b).sqrt();
    if norm_a == 0. || norm_b == 0. {
        return f32::NAN;
    }
    dotproduct(a, b) / (norm_a * norm_b)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;
    use tokio_stream::StreamExt;

    async fn response_from_stream(
        stream: tokio_stream::wrappers::ReceiverStream<Result<WriteOutput, GenerateResponseError>>,
    ) -> Option<String> {
        stream
            .filter_map(|out| match out {
                Ok(WriteOutput::Done(resp)) => Some(resp),
                _ => None,
            })
            .next()
            .await
    }

    #[tokio::test]
    async fn test_simple_gen() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let params = LLMActorParams {
            model,
            sampler_config: SamplerConfig::default(),
            n_ctx: 4096,
            use_embeddings: false,
        };
        let stop_words = vec!["10".to_string()];

        let actor = LLMActorHandle::new(params)
            .await
            .expect("Failed creating actor");

        let stream = actor
            .generate_response("I'm gonna count to 10: 1, 2, 3, ".to_string(), stop_words)
            .await;

        let response: String = response_from_stream(stream).await.unwrap();
        assert!(response.contains("4, 5, 6, 7, 8, 9, 10"));
    }

    #[tokio::test]
    async fn test_embeddings() {
        test_utils::init_test_tracing();
        let model = test_utils::load_embeddings_model();

        let params = LLMActorParams {
            model,
            sampler_config: SamplerConfig::default(),
            n_ctx: 4096,
            use_embeddings: true,
        };

        let actor = LLMActorHandle::new(params)
            .await
            .expect("Failed creating actor");

        let copenhagen_embedding = actor
            .generate_embedding("Copenhagen is the capital of Denmark.".to_string())
            .await
            .unwrap();

        let berlin_embedding = actor
            .generate_embedding("Berlin is the capital of Germany.".to_string())
            .await
            .unwrap();

        let insult_embedding = actor
            .generate_embedding(
                "Your mother was a hamster and your father smelt of elderberries!".to_string(),
            )
            .await
            .unwrap();

        assert!(
            insult_embedding.len() == berlin_embedding.len()
                && berlin_embedding.len() == copenhagen_embedding.len()
                && copenhagen_embedding.len() == insult_embedding.len(),
            "not all embedding lengths were equal"
        );

        // cosine similarity should not care about order
        assert_eq!(
            cosine_similarity(&copenhagen_embedding, &berlin_embedding),
            cosine_similarity(&berlin_embedding, &copenhagen_embedding)
        );

        // any vector should have cosine similarity 1 to itself
        // (tolerate small float error)
        assert!(
            (cosine_similarity(&copenhagen_embedding, &copenhagen_embedding) - 1.0).abs() < 0.001,
        );

        // the insult should have a lower similarity than the two geography sentences
        assert!(
            cosine_similarity(&copenhagen_embedding, &insult_embedding)
                < cosine_similarity(&copenhagen_embedding, &berlin_embedding)
        );
    }

    #[tokio::test]
    async fn test_multiple_contexts_single_model() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let params = LLMActorParams {
            model,
            sampler_config: SamplerConfig::default(),
            n_ctx: 4096,
            use_embeddings: false,
        };
        let dk_actor = LLMActorHandle::new(params.clone()).await.unwrap();
        let de_actor = LLMActorHandle::new(params).await.unwrap();

        let dk_fut = response_from_stream(
            dk_actor
                .generate_response(
                    "The name of the capital city of Denmark is \"".to_string(),
                    vec!["Copenhagen".to_string()],
                )
                .await,
        );

        let de_fut = response_from_stream(
            de_actor
                .generate_response(
                    "The capital of Germany is called ".to_string(),
                    vec!["Berlin".to_string()],
                )
                .await,
        );

        let (dk_resp, de_resp) = tokio::join!(dk_fut, de_fut);

        let dk_resp = dk_resp.unwrap();
        let de_resp = de_resp.unwrap();

        assert!(
            dk_resp.to_lowercase().contains("copenhagen"),
            "Expected completion to contain 'Copenhagen', got: {dk_resp}"
        );
        assert!(
            de_resp.to_lowercase().contains("berlin"),
            "Expected completion to contain 'Berlin', got: {de_resp}"
        );
    }

    #[tokio::test]
    async fn test_context_shifting() {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let params = LLMActorParams {
            model,
            sampler_config: SamplerConfig::default(),
            n_ctx: 64,
            use_embeddings: false,
        };
        let actor = LLMActorHandle::new(params.clone()).await.unwrap();
        let stop_words = vec!["20".to_string()];

        let stream = actor
            .generate_response(
                "I'm going to count to 20: 1, 2, 3, 4, 5, 6, 7".to_string(),
                stop_words,
            )
            .await;

        let response = response_from_stream(stream).await.unwrap();
        assert!(
            response.contains("15, 16, 17, 18, 19, 20"),
            "Expected completion to count to 20, got: {response}"
        );
    }

    #[tokio::test]
    async fn test_stop_words() {
        crate::test_utils::init_test_tracing();

        // setup
        let model = test_utils::load_test_model();
        let params = LLMActorParams {
            model,
            sampler_config: SamplerConfig::default(),
            n_ctx: 1024,
            use_embeddings: false,
        };
        let actor = LLMActorHandle::new(params).await.unwrap();
        let stop_words = vec!["7".to_string()];

        // make response
        let stream = actor
            .generate_response(
                "I'm going to count to 10: 1, 2, 3, 4,".to_string(),
                stop_words,
            )
            .await;
        let response = response_from_stream(stream).await.unwrap();

        // check resp
        assert!(
            response.to_lowercase().contains("5, 6, "),
            "Expected output to contain text before stop token. Got: {response}"
        );
        assert!(
            response.to_lowercase().ends_with("7"),
            "Expected output to stop at stop token, but continued. Got: {response}"
        );
        assert!(
            !response.to_lowercase().contains("8"),
            "Expected output to stop at stop token, but continued. Got: {response}"
        );
    }
}
