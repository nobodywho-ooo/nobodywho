use crate::chat_state;
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
use tokio::sync::mpsc::{Receiver, Sender};
use tokio::sync::{mpsc, oneshot};
use tokio_stream::StreamExt;
use tracing::{debug, error, info, trace};

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
        return Err(LoadModelError::ModelNotFound(model_path.into()));
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

    Ok(n_discard)
}

#[derive(Debug, thiserror::Error)]
pub enum SayError {
    #[error("Template rendering error: {0}")]
    TemplateError(#[from] minijinja::Error),
}

#[derive(Debug, thiserror::Error)]
pub enum LLMChatError {
    #[error("Lama.cpp failed fetching chat template from the model file. This is likely because you're using an older GGUF file, which might not include a chat template. For example, this is the case for most LLaMA2-based GGUF files. Try using a more recent GGUF model file. If you want to check if a given model includes a chat template, you can use the gguf-dump script from llama.cpp. Here is a more technical detailed error: {0}")]
    ChatTemplateError(#[from] llama_cpp_2::ChatTemplateError),

    #[error("Could not parse chat template as UTF8: {0}")]
    TemplateUtf8Error(#[from] std::str::Utf8Error),

    #[error("Could not detokenize string: {0}")]
    Detokenize(#[from] llama_cpp_2::TokenToStringError),
}

pub trait EmbedEngine {
    fn emit_embedding(&self, embd: Vec<f32>);
}

pub async fn simple_embedding_loop(
    params: LLMActorParams,
    mut embed_rx: Receiver<String>,
    embed_engine: Box<dyn EmbedEngine>,
) -> Result<(), ()> {
    trace!("In embedding loop");
    let actor = LLMActorHandle::new(params).await.expect("TODO: errors");
    trace!("Made embedding actor.");
    while let Some(text) = embed_rx.recv().await {
        trace!("Got embedding text: {text}");
        let embd = actor
            .embed(text)
            .await
            .expect("TODO: errors")
            .expect("TODO: errors");
        trace!("Responding with embedding.");
        embed_engine.emit_embedding(embd);
    }
    Ok(())
}

pub trait Engine {
    fn emit_token(&self, token: String);
    fn emit_response(&self, resp: String);
    fn emit_error(&self, err: String);
}

pub async fn simple_chat_loop(
    params: LLMActorParams,
    system_prompt: String,
    mut say_rx: Receiver<String>,
    engine: Box<dyn Engine>,
) -> Result<(), LLMChatError> {
    info!("Entering simple chat loop");

    // init chat state
    let template = params.model.get_chat_template()?.to_string()?;
    let bos = params
        .model
        .token_to_str(params.model.token_bos(), Special::Tokenize)?;
    let eos = params
        .model
        .token_to_str(params.model.token_eos(), Special::Tokenize)?;
    let mut chat_state = chat_state::ChatState::new(template, bos, eos);
    chat_state.add_message("system".to_string(), system_prompt);
    info!("Initialized chat state.");

    // init actor
    let actor = LLMActorHandle::new(params).await.expect("todo: error");
    info!("Initialized actor.");

    // wait for message from user
    while let Some(message) = say_rx.recv().await {
        chat_state.add_message("user".to_string(), message);
        let diff = chat_state.render_diff().expect("TODO: handle err");

        // read text into llm context
        // XXX: awaiting here means that we could end up waiting for up to one frame
        //      between having finished reading, and beginning to generate.
        //      a bit awkward... can we make it faster?
        actor.read(diff);

        // ask llm to respond
        let response_stream = actor.write_until_done().await;

        let full_response = response_stream
            .fold(None, |_, out| {
                debug!("Streamed out: {out:?}");
                match out {
                    Err(err) => {
                        error!("Got error from worker: {err:?}");
                        engine.emit_error(format!("{err:?}"));
                        None // TODO: error handling
                    }
                    Ok(WriteOutput::Token(token)) => {
                        trace!("Got new token: {token:?}");
                        engine.emit_token(token);
                        None
                    }
                    Ok(WriteOutput::Done(resp)) => Some(resp),
                }
            })
            .await
            .expect("TODO: error handling");

        engine.emit_response(full_response.clone());
        chat_state.add_message("assistant".to_string(), full_response);
        let _ = chat_state.render_diff();
    }

    // XXX: we only arrive here when the sender-part of the say channel is dropped
    // and in that case, we don't have anything to send our error to anyway
    Ok(()) // accept our fate
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
/// * `stop_tokens` - List of strings that will cause token generation to stop when encountered
#[derive(Clone)]
pub struct LLMActorParams {
    pub model: Arc<LlamaModel>,
    pub sampler_config: SamplerConfig,
    pub n_ctx: u32,
    pub stop_tokens: Vec<String>,
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
        trace!("Made channels");

        trace!("Spawning thread...");
        std::thread::spawn(|| completion_worker_actor(message_rx, init_tx, params));
        trace!("Spawned thread.");

        trace!("Waiting for init result");
        let resp = match init_rx.await {
            Ok(Ok(())) => Ok(Self { message_tx }),
            Ok(Err(e)) => Err(e),
            Err(_recverr) => Err(InitWorkerError::NoResponse),
        };
        trace!("Got init result: {resp:?}");
        resp
    }

    pub fn read(&self, text: String) {
        self.message_tx
            .send(WorkerMsg::ReadString(text))
            .expect("todo: handle error")
    }

    pub async fn write_until_done(
        &self,
    ) -> tokio_stream::wrappers::ReceiverStream<Result<WriteOutput, WriteError>> {
        let (respond_to, response_channel) = mpsc::channel(CHANNEL_SIZE);
        let _ = self.message_tx.send(WorkerMsg::WriteUntilDone(respond_to));
        response_channel.into()
    }

    pub fn reset_context(&self) {
        self.message_tx
            .send(WorkerMsg::ResetContext)
            .expect("todo: handle error");
    }

    pub async fn embed(
        &self,
        text: String,
    ) -> Option<Result<Vec<f32>, llama_cpp_2::EmbeddingsError>> {
        trace!("embed(): resetting context");
        self.reset_context();
        trace!("embed(): reading text");
        self.read(text);

        trace!("embed(): getting embedding");
        let (respond_to, mut response_channel) = mpsc::channel(1);
        let _ = self.message_tx.send(WorkerMsg::GetEmbedding(respond_to));
        response_channel.recv().await
    }
}

fn completion_worker_actor(
    message_rx: std::sync::mpsc::Receiver<WorkerMsg>,
    init_tx: oneshot::Sender<Result<(), InitWorkerError>>,
    params: LLMActorParams,
) {
    match init_worker(&params) {
        Ok(state) => {
            trace!("Init WorkerState success.");
            init_tx.send(Ok(())).expect("TODO: handle err");
            trace!("Sent workerstate success out");
            let mut state = state;
            // listen for messages
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
            init_tx.send(Err(initerr)).expect("TODO: handle err");
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

fn init_worker(params: &LLMActorParams) -> Result<WorkerState, InitWorkerError> {
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
        stop_tokens: params.stop_tokens.clone(),
        sampler: make_sampler(&params.model, params.sampler_config.clone()),
        ctx,
        big_batch,
        small_batch,
    };
    Ok(state)
}

#[derive(Debug)]
struct WorkerState<'a> {
    n_past: i32,
    ctx: LlamaContext<'a>,
    sampler: LlamaSampler,
    big_batch: LlamaBatch,
    small_batch: LlamaBatch,
    stop_tokens: Vec<String>,
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

    #[error("Could not send newly generated token out to the game engine.")]
    SendError, // this is actually a SendError<LLMOutput>, but that becomes recursive and weird

    // #[error("Could not receive from channel: {0}")]
    // RecvError(#[from] mpsc::error::RecvError),
    #[error("Global Inference Lock was poisoned.")]
    GILPoisonError, // this is actually a std::sync::PoisonError<std::sync::MutexGuard<'static, ()>>, but that doesn't implement Send, so we do this
}

#[derive(Debug)]
pub enum WorkerMsg {
    ReadString(String),
    WriteUntilDone(mpsc::Sender<Result<WriteOutput, WriteError>>),
    GetEmbedding(mpsc::Sender<Result<Vec<f32>, llama_cpp_2::EmbeddingsError>>),
    ResetContext,
}

fn handle_msg(mut state: WorkerState, msg: WorkerMsg) -> Result<WorkerState, WorkerError> {
    // HACK
    // this is needed because contexts referencing the same model are not thread safe
    // if two contexts referencing the same model try to decode at the same time,
    // then llama.cpp segfaults and everybody dies and i become sad
    debug!("Worker handling message: {msg:?}");
    let _inference_lock = GLOBAL_INFERENCE_LOCK
        .lock()
        .map_err(|_| WorkerError::GILPoisonError)?;

    match msg {
        WorkerMsg::ReadString(text) => Ok(read_string(state, text)?),
        WorkerMsg::WriteUntilDone(respond_to) => Ok(write_until_done(state, respond_to)?),
        WorkerMsg::GetEmbedding(respond_to) => {
            let embedding = state
                .ctx
                .embeddings_seq_ith(0)
                .expect("TODO: error handling")
                .to_vec();
            respond_to
                .blocking_send(Ok(embedding))
                .expect("TODO: error handling");
            Ok(state)
        }
        WorkerMsg::ResetContext => {
            state.ctx.clear_kv_cache();
            state.n_past = 0;
            Ok(state)
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

fn read_string(mut state: WorkerState, text: String) -> Result<WorkerState, ReadError> {
    info!("Worker reading string");
    let tokens = state.ctx.model.str_to_token(&text, AddBos::Never)?;
    let n_tokens = tokens.len();

    debug_assert!(tokens.len() > 0);
    debug_assert!(tokens.len() < state.ctx.n_ctx() as usize);

    {
        state.big_batch.clear();
        let seq_ids = &[0];
        for (i, token) in (0..).zip(tokens.iter()) {
            // Only compute logits for the last token to save computation
            let output_logits = i == n_tokens - 1;
            state
                .big_batch
                .add(*token, state.n_past + i as i32, seq_ids, output_logits)?;
        }
    }

    state.ctx.decode(&mut state.big_batch)?;

    Ok(WorkerState {
        n_past: state.n_past + tokens.len() as i32,
        ..state
    })
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

fn write_until_done(
    mut state: WorkerState,
    respond_to: Sender<Result<WriteOutput, WriteError>>,
) -> Result<WorkerState, WriteError> {
    // Token generation loop
    info!("Worker writing until done");

    // pre-allocating 4096 bytes for the response string
    // 4096 is a very randomly chosen number. how does this affect performance?
    let mut full_response: String = String::with_capacity(4096);

    loop {
        // Check for context window overflow (it was in the end before)
        if state.n_past >= state.ctx.n_ctx() as i32 - 1 {
            state.n_past -= apply_context_shifting(&mut state.ctx, state.n_past)?;
            // check count
            // XXX: this check is slow
            debug_assert!(state.n_past == state.ctx.get_kv_cache_token_count());
        }

        // Sample next token, no need to use sampler.accept as sample already accepts the token.
        // using sampler.accept() will cause the sampler to crash when using grammar sampling.
        // https://github.com/utilityai/llama-cpp-rs/issues/604
        let new_token: LlamaToken = state.sampler.sample(&state.ctx, -1);

        // batch of one
        state.small_batch.clear();
        state.small_batch.add(new_token, state.n_past, &[0], true)?;

        // llm go brr
        state.ctx.decode(&mut state.small_batch)?;
        state.n_past += 1; // keep count

        // Convert token to text
        let output_string = state
            .ctx
            .model
            .token_to_str_with_size(new_token, MAX_TOKEN_STR_LEN, Special::Tokenize)
            .unwrap_or("�".to_string());
        // fall back to "U+FFFD REPLACEMENT CHARACTER"
        // when encountering bytes that aren't valid UTF-8
        // wikipedia: "used to replace an unknown, unrecognised, or unrepresentable character"

        let has_stop_tokens = state
            .stop_tokens
            .iter()
            .any(|stop_token| full_response.contains(stop_token));
        let has_eog = state.ctx.model.is_eog_token(new_token);

        if !has_eog {
            full_response.push_str(&output_string);
            trace!("Sending out token: {output_string}");
            let _ = respond_to.blocking_send(Ok(WriteOutput::Token(output_string)));
        }
        if has_eog || has_stop_tokens {
            break;
        }
    }

    // we're done!
    trace!("Sending out response: {full_response}");
    let _ = respond_to.blocking_send(Ok(WriteOutput::Done(full_response)));
    Ok(state)
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

    macro_rules! test_model_path {
        () => {
            std::env::var("TEST_MODEL")
                .unwrap_or("model.gguf".to_string())
                .as_str()
        };
    }

    macro_rules! test_embeddings_model_path {
        () => {{
            std::env::var("TEST_EMBEDDINGS_MODEL")
                .unwrap_or("embeddings.gguf".to_string())
                .as_str()
        }};
    }

    struct MockEngine {
        response_tx: mpsc::Sender<String>,
    }

    impl MockEngine {
        fn new() -> (Self, mpsc::Receiver<String>) {
            let (response_tx, response_rx) = mpsc::channel(CHANNEL_SIZE);
            (Self { response_tx }, response_rx)
        }
    }

    impl Engine for MockEngine {
        fn emit_response(&self, resp: String) {
            self.response_tx.try_send(resp).expect("send failed!");
        }
        fn emit_token(&self, token: String) {
            debug!("MockEngine: {token}");
        }
        fn emit_error(&self, err: String) {
            error!("MockEngine: {err}")
        }
    }

    #[tokio::test(flavor = "current_thread")]
    async fn test_actor_chat() {
        tracing_subscriber::fmt()
            .with_max_level(tracing::Level::TRACE)
            .init();

        // Setup
        let model = get_model(test_model_path!(), true).unwrap();
        let system_prompt =
            "You are a helpful assistant. The user asks you a question, and you provide an answer."
                .to_string();
        let params = LLMActorParams {
            model,
            sampler_config: SamplerConfig::default(),
            n_ctx: 4096,
            stop_tokens: vec![],
            use_embeddings: false,
        };

        let (mock_engine, mut response_rx) = MockEngine::new();
        let (say_tx, say_rx) = mpsc::channel(CHANNEL_SIZE);

        let local = tokio::task::LocalSet::new();
        local.spawn_local(simple_chat_loop(
            params,
            system_prompt,
            say_rx,
            Box::new(mock_engine),
        ));

        let check_results = async move {
            let _ = say_tx
                .send("What is the capital of Denmark?".to_string())
                .await;
            let response = response_rx.recv().await.unwrap();
            assert!(
                response.contains("Copenhagen"),
                "Expected completion to contain 'Copenhagen', got: {response}"
            );

            let _ = say_tx
                .send("What language do they speak there?".to_string())
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

    // #[test]
    // fn test_embeddings() {
    //     let model = get_model(test_embeddings_model_path!(), true).unwrap();

    //     let params = LLMActorParams {
    //         model,
    //         sampler_config: SamplerConfig::default(),
    //         n_ctx: 4096,
    //         stop_tokens: vec![],
    //         use_embeddings: true,
    //     };

    //     let mut actor = LLMActorHandle::new(params);

    //     actor
    //         .embed("Copenhagen is the capital of Denmark.".to_string())
    //         .unwrap();
    //     let copenhagen_embedding = match actor.recv() {
    //         Ok(LLMOutput::Embedding(vec)) => vec,
    //         _ => panic!(),
    //     };

    //     actor
    //         .embed("Berlin is the capital of Germany.".to_string())
    //         .unwrap();
    //     let berlin_embedding = match actor.recv() {
    //         Ok(LLMOutput::Embedding(vec)) => vec,
    //         _ => panic!(),
    //     };

    //     actor
    //         .embed("Your mother was a hamster and your father smelt of elderberries!".to_string())
    //         .unwrap();
    //     let insult_embedding = match actor.recv() {
    //         Ok(LLMOutput::Embedding(vec)) => vec,
    //         _ => panic!(),
    //     };

    //     assert!(
    //         insult_embedding.len() == berlin_embedding.len()
    //             && berlin_embedding.len() == copenhagen_embedding.len()
    //             && copenhagen_embedding.len() == insult_embedding.len(),
    //         "not all embedding lengths were equal"
    //     );

    //     // cosine similarity should not care about order
    //     assert_eq!(
    //         cosine_similarity(&copenhagen_embedding, &berlin_embedding),
    //         cosine_similarity(&berlin_embedding, &copenhagen_embedding)
    //     );

    //     // any vector should have cosine similarity 1 to itself
    //     // (tolerate small float error)
    //     assert!(
    //         (cosine_similarity(&copenhagen_embedding, &copenhagen_embedding) - 1.0).abs() < 0.001,
    //     );

    //     // the insult should have a lower similarity than the two geography sentences
    //     assert!(
    //         cosine_similarity(&copenhagen_embedding, &insult_embedding)
    //             < cosine_similarity(&copenhagen_embedding, &berlin_embedding)
    //     );
    // }

    // #[test]
    // fn test_multiple_contexts_single_model() {
    //     let model = get_model(test_model_path!(), true).unwrap();

    //     let system_prompt =
    //         "You are a helpful assistant. The user asks you a question, and you provide an answer."
    //             .to_string();

    //     let params = LLMActorParams {
    //         model,
    //         sampler_config: SamplerConfig::default(),
    //         n_ctx: 4096,
    //         stop_tokens: vec![],
    //         use_embeddings: false,
    //     };
    //     let mut dk_chat = LLMChat::new(params.clone())
    //         .unwrap()
    //         .with_system_message(system_prompt.clone());
    //     let mut de_chat = LLMChat::new(params)
    //         .unwrap()
    //         .with_system_message(system_prompt);

    //     dk_chat
    //         .say("What is the capital of Denmark?".to_string())
    //         .unwrap();

    //     de_chat
    //         .say("What is the capital of Germany?".to_string())
    //         .unwrap();

    //     // read dk output
    //     let result = dk_chat.get_response_blocking().unwrap();
    //     assert!(
    //         result.to_lowercase().contains("copenhagen"),
    //         "Expected completion to contain 'Copenhagen', got: {result}"
    //     );

    //     // read cat output
    //     let result = de_chat.get_response_blocking().unwrap();
    //     assert!(
    //         result.to_lowercase().contains("berlin"),
    //         "Expected completion to contain 'Berlin', got: {result}"
    //     );
    // }

    // #[test]
    // fn test_context_shifting() {
    //     let model = get_model(test_model_path!(), true).unwrap();

    //     let system_prompt = "You are a helpful assistant.".to_string();
    //     let params = LLMActorParams {
    //         model,
    //         sampler_config: SamplerConfig::default(),
    //         n_ctx: 100, // very low context size. will be exceeded immediately
    //         stop_tokens: vec![],
    //         use_embeddings: false,
    //     };
    //     let mut chat = LLMChat::new(params.clone())
    //         .unwrap()
    //         .with_system_message(system_prompt.clone());

    //     chat
    //         .say("Please count down from 10 to 0, like this: Current 10, target 0. Current 9, target 0...".to_string())
    //         .unwrap();

    //     let result = chat.get_response_blocking().unwrap();
    //     assert!(
    //         result.contains("Current 1, target 0"),
    //         "Expected completion to contain 'Current 0, target 0', got: {result}"
    //     );
    // }

    // #[test]
    // fn test_stop_tokens() {
    //     let model = get_model(test_model_path!(), true).unwrap();

    //     let system_prompt = "You are a helpful assistant.".to_string();
    //     let params = LLMActorParams {
    //         model,
    //         sampler_config: SamplerConfig::default(),
    //         n_ctx: 4096,
    //         stop_tokens: vec!["horse".to_string()],
    //         use_embeddings: false,
    //     };
    //     let mut chat = LLMChat::new(params.clone())
    //         .unwrap()
    //         .with_system_message(system_prompt);
    //     chat.say("List these animals in alphabetical order: cat, dog, giraffe, horse, lion, mouse. Keep them in lowercase.".to_string()).unwrap();
    //     let result = chat.get_response_blocking().unwrap();

    //     assert!(
    //         result.to_lowercase().contains("giraffe"),
    //         "Expected output to contain text before stop token. Got: {result}"
    //     );
    //     assert!(
    //         result.to_lowercase().contains("horse"),
    //         "Expected output to contain stop token. Got: {result}"
    //     );
    //     assert!(
    //         !result.to_lowercase().contains("lion"),
    //         "Expected output to stop at stop token, but continued. Got: {result}"
    //     );
    //     assert!(
    //         !result.to_lowercase().contains("mouse"),
    //         "Expected output to stop at stop token, but continued. Got: {result}"
    //     );
    // }
}
