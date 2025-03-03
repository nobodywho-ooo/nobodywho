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
use std::collections::VecDeque;
use std::pin::pin;
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, LazyLock, Mutex};

const MAX_TOKEN_STR_LEN: usize = 128;

lazy_static! {
    static ref GLOBAL_INFERENCE_LOCK: Mutex<()> = Mutex::new(());
}

static LLAMA_BACKEND: LazyLock<LlamaBackend> =
    LazyLock::new(|| LlamaBackend::init().expect("Failed to initialize llama backend"));

#[derive(Debug)]
pub enum LLMOutput {
    Token(String),
    FatalErr(WorkerError),
    Done(String),
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

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("Could not determine number of threads available: {0}")]
    ThreadCountError(#[from] std::io::Error),

    #[error("Could not create context: {0}")]
    CreateContextError(#[from] llama_cpp_2::LlamaContextLoadError),

    #[error("Could not tokenize string: {0}")]
    TokenizerError(#[from] llama_cpp_2::StringToTokenError),

    #[error("Could not detokenize string: {0}")]
    Detokenize(#[from] llama_cpp_2::TokenToStringError),

    #[error("Could not add token to batch: {0}")]
    BatchAddError(#[from] llama_cpp_2::llama_batch::BatchAddError),

    #[error("Llama.cpp failed decoding: {0}")]
    DecodeError(#[from] llama_cpp_2::DecodeError),

    #[error("Lama.cpp failed fetching chat template from the model file. This is likely because you're using an older GGUF file, which might not include a chat template. For example, this is the case for most LLaMA2-based GGUF files. Try using a more recent GGUF model file. If you want to check if a given model includes a chat template, you can use the gguf-dump script from llama.cpp. Here is a more technical detailed error: {0}")]
    ChatTemplateError(#[from] llama_cpp_2::ChatTemplateError),

    #[error("Lama.cpp failed fetching chat template: {0}")]
    KvCacheConversionError(#[from] llama_cpp_2::context::kv_cache::KvCacheConversionError),

    #[error("Failed applying the jinja chat template: {0}")]
    ApplyTemplateError(#[from] minijinja::Error),

    #[error("Could not parse chat template as UTF8: {0}")]
    TemplateUtf8Error(#[from] std::str::Utf8Error),

    #[error("Could not send newly generated token out to the game engine.")]
    SendError, // this is actually a SendError<LLMOutput>, but that becomes recursive and weird.

    #[error("Global Inference Lock was poisoned.")]
    GILPoisonError, // this is actually a std::sync::PoisonError<std::sync::MutexGuard<'static, ()>>, but that doesn't implement Send, so we do this
}

/// Adds a sequence of tokens to the batch for processing.
///
/// # Arguments
/// * `batch` - The batch to add tokens to
/// * `tokens` - The sequence of tokens to add
/// * `pos` - The starting position in the context
/// * `seq_ids` - Sequence IDs for the tokens
///
/// # Returns
/// * `Ok(())` if successful
/// * `Err(WorkerError)` if batch addition fails
fn add_sequence(
    batch: &mut LlamaBatch,
    tokens: &[LlamaToken],
    pos: i32,
    seq_ids: &[i32],
) -> Result<(), WorkerError> {
    batch.clear();
    let n_tokens = tokens.len();
    for (i, token) in (0..).zip(tokens.iter()) {
        // Only compute logits for the last token to save computation
        let output_logits = i == n_tokens - 1;
        batch.add(*token, pos + i as i32, seq_ids, output_logits)?;
    }

    Ok(())
}

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
fn apply_context_shifting(ctx: &mut LlamaContext, n_past: i32) -> Result<i32, WorkerError> {
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

pub fn run_completion_worker(
    model: Arc<LlamaModel>,
    message_rx: Receiver<String>,
    completion_tx: Sender<LLMOutput>,
    sampler_config: SamplerConfig,
    n_ctx: u32,
    system_prompt: String,
    stop_tokens: Vec<String>,
) {
    if let Err(msg) = run_completion_worker_result(
        model,
        message_rx,
        &completion_tx,
        sampler_config,
        n_ctx,
        system_prompt,
        stop_tokens,
    ) {
        // Forward fatal errors to the consumer
        completion_tx
            .send(LLMOutput::FatalErr(msg))
            .expect("Could not send llm worker fatal error back to consumer.");
    }
}

pub struct LLMActorHandle {
    message_tx: Sender<WorkerMsg>,
    completion_rx: Receiver<LLMOutput>,
    completion_tx: Sender<LLMOutput>,
    chat_state: chat_state::ChatState,
}

impl LLMActorHandle {
    pub fn new(params: LLMActorParams) -> Self {
        let (message_tx, message_rx) = std::sync::mpsc::channel();
        let (completion_tx, completion_rx) = std::sync::mpsc::channel();

        let chat_state = chat_state::ChatState::new(
            params
                .model
                .get_chat_template()
                .expect("TODO: handle no template")
                .to_string()
                .expect("TODO: handle bad template"),
            params
                .model
                .token_to_str(params.model.token_bos(), Special::Tokenize)
                .expect("TODO"),
            params
                .model
                .token_to_str(params.model.token_eos(), Special::Tokenize)
                .expect("TODO"),
        );

        let completion_tx_clone = completion_tx.clone();
        std::thread::spawn(|| completion_worker_actor(message_rx, completion_tx_clone, params));

        Self {
            message_tx,
            completion_rx,
            completion_tx,
            chat_state,
        }
    }

    pub fn with_system_message(mut self: Self, system_prompt: String) -> Self {
        // Initialize chat state with model's chat template
        self.chat_state
            .add_message("system".to_string(), system_prompt);
        self
    }

    pub fn say(self: &mut Self, text: String) -> Result<(), SayError> {
        // Add user message to chat state
        self.chat_state.add_message("user".to_string(), text);
        let diff = self.chat_state.render_diff()?;
        // ask worker to read it
        self.message_tx.send(WorkerMsg::ReadString(diff))?;
        // ask worker to generate a response
        self.message_tx
            .send(WorkerMsg::WriteUntilDone(self.completion_tx.clone()))?;
        Ok(())
    }

    pub fn try_get(self: &mut Self) -> Result<LLMOutput, std::sync::mpsc::TryRecvError> {
        let out = self.completion_rx.try_recv()?;
        match out {
            LLMOutput::FatalErr(_) => Ok(out),
            LLMOutput::Token(_) => Ok(out),
            LLMOutput::Done(response) => {
                self.chat_state
                    .add_message("assistant".to_string(), response.clone());
                self.chat_state.render_diff().expect("TODO: handle err");
                Ok(LLMOutput::Done(response))
            }
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SayError {
    #[error("Template rendering error: {0}")]
    TemplateError(#[from] minijinja::Error),

    #[error("Error sending message: {0}")]
    SendError(#[from] std::sync::mpsc::SendError<WorkerMsg>),
}

struct WorkerState<'a> {
    n_past: i32,
    ctx: LlamaContext<'a>,
    sampler: LlamaSampler,
    big_batch: LlamaBatch,
    small_batch: LlamaBatch,
    stop_tokens: Vec<String>,
}

pub struct LLMActorParams {
    pub model: Arc<LlamaModel>,
    pub sampler_config: SamplerConfig,
    pub n_ctx: u32,
    pub stop_tokens: Vec<String>,
}

fn completion_worker_actor(
    message_rx: Receiver<WorkerMsg>,
    completion_tx: Sender<LLMOutput>,
    params: LLMActorParams,
) -> Result<(), WorkerError> {
    let model = params.model;

    // Set up context parameters using available parallelism
    let ctx = {
        let n_threads = std::thread::available_parallelism()?.get() as i32;
        let n_ctx = std::cmp::min(params.n_ctx, model.n_ctx_train());
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZero::new(n_ctx))
            .with_n_threads(n_threads)
            .with_n_threads_batch(n_threads);

        // Create inference context and sampler
        model.new_context(&LLAMA_BACKEND, ctx_params)?
    };

    let big_batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);
    let small_batch = LlamaBatch::new(1, 1);

    let mut state = WorkerState {
        n_past: 0,
        stop_tokens: params.stop_tokens,
        sampler: make_sampler(&model, params.sampler_config),
        ctx,
        big_batch,
        small_batch,
    };

    // listen for messages
    while let Ok(msg) = message_rx.recv() {
        match handle_msg(state, msg) {
            Ok(newstate) => {
                state = newstate;
            }
            Err(err) => {
                return Err(err);
            } // worker died. report error and give up
              // completion_tx.send(LLMOutput::FatalErr(err)).map_err(|_| WorkerError::SendError)?;
        }
    }

    // we only reach this code mressage_rx.recv() fails
    // this only happens when all senders are dropped
    Ok(()) // accept our fate
}

pub enum WorkerMsg {
    ReadString(String),
    WriteUntilDone(Sender<LLMOutput>),
}

fn handle_msg(mut state: WorkerState, msg: WorkerMsg) -> Result<WorkerState, WorkerError> {
    match msg {
        WorkerMsg::ReadString(text) => {
            let tokens = state.ctx.model.str_to_token(&text, AddBos::Always)?;

            debug_assert!(tokens.len() > 0);
            debug_assert!(tokens.len() < state.ctx.n_ctx() as usize);

            add_sequence(&mut state.big_batch, &tokens, state.n_past, &[0])?;

            state.ctx.decode(&mut state.big_batch)?;

            Ok(WorkerState {
                n_past: state.n_past + tokens.len() as i32,
                ..state
            })
        }
        WorkerMsg::WriteUntilDone(respond_to) => {
            // Token generation loop
            // TODO: would .with_capacity help here?
            let mut full_response: String = String::new();

            // HACK
            // this is needed because contexts referencing the same model are not thread safe
            // if two contexts referencing the same model try to decode at the same time,
            // then llama.cpp segfaults and everybody dies and i become sad
            let inference_lock = GLOBAL_INFERENCE_LOCK
                .lock()
                .map_err(|_| WorkerError::GILPoisonError)?;

            loop {
                // Check for context window overflow (it was in the end before)
                if state.n_past >= state.ctx.n_ctx() as i32 - 1 {
                    state.n_past -= apply_context_shifting(&mut state.ctx, state.n_past)?;
                    // TODO: big_batch isn't populated here...
                    debug_assert!(
                        state.n_past + state.big_batch.n_tokens() < state.ctx.n_ctx() as i32
                    );
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

                // keep count
                state.n_past += 1;

                debug_assert!(state.n_past == state.ctx.get_kv_cache_token_count());

                // Convert token to text and stream to user
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
                    respond_to
                        .send(LLMOutput::Token(output_string.clone()))
                        .map_err(|_| WorkerError::SendError)?;
                }

                if has_eog || has_stop_tokens {
                    break;
                }
            }
            drop(inference_lock);

            // we're done!
            respond_to
                .send(LLMOutput::Done(full_response))
                .map_err(|_| WorkerError::SendError)?;
            Ok(state)
        } // WorkerMsg::GetEmbeddings => Ok(state),
    }
}

/// Core implementation of the completion worker.
///
/// # Arguments
/// * `model` - The LLaMA model to use for inference
/// * `message_rx` - Channel receiver for incoming user messages
/// * `completion_tx` - Channel sender for completion outputs
/// * `sampler_config` - Configuration for the token sampler
/// * `n_ctx` - Maximum context length
/// * `system_prompt` - System prompt to initialize the chat
/// * `stop_tokens` - Tokens to stop generation at
/// # Returns
/// * `Ok(())` if the worker exits normally
/// * `Err(WorkerError)` on fatal errors
fn run_completion_worker_result(
    model: Arc<LlamaModel>,
    message_rx: Receiver<String>,
    completion_tx: &Sender<LLMOutput>,
    sampler_config: SamplerConfig,
    n_ctx: u32,
    system_prompt: String,
    stop_tokens: Vec<String>,
) -> Result<(), WorkerError> {
    // Set up context parameters using available parallelism
    let mut ctx = {
        let n_threads = std::thread::available_parallelism()?.get() as i32;
        let n_ctx = std::cmp::min(n_ctx, model.n_ctx_train());
        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZero::new(n_ctx))
            .with_n_threads(n_threads)
            .with_n_threads_batch(n_threads);

        // Create inference context and sampler
        model.new_context(&LLAMA_BACKEND, ctx_params)?
    };
    let mut sampler = make_sampler(&model, sampler_config);

    // Initialize chat state with model's chat template
    let mut chat_state = chat_state::ChatState::new(
        model.get_chat_template()?.to_string()?,
        model.token_to_str(model.token_bos(), Special::Tokenize)?,
        model.token_to_str(model.token_eos(), Special::Tokenize)?,
    );
    chat_state.add_message("system".to_string(), system_prompt);

    let mut n_past = 0; // Current position in context window
    let mut response = String::new();

    // initialize last_n_tokens
    // used to check for stop tokens
    // needs to be multiple tokens, in case one of the "stop tokens"
    // actually tokenizes to several tokens
    let longest_stop_token: usize = stop_tokens.iter().try_fold(0, |acc, token_str| {
        model
            .str_to_token(&token_str, AddBos::Never)
            .map(|tokens| acc.max(tokens.len()))
    })?;
    let mut last_n_tokens: VecDeque<String> = VecDeque::with_capacity(longest_stop_token);

    // Create batch for processing tokens
    let mut batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);

    // Main message processing loop
    while let Ok(content) = message_rx.recv() {
        // HACK
        // this is needed because contexts referencing the same model are not thread safe
        // if two contexts referencing the same model try to decode at the same time,
        // then llama.cpp segfaults and everybody dies and i become sad
        let inference_lock = GLOBAL_INFERENCE_LOCK
            .lock()
            .map_err(|_| WorkerError::GILPoisonError)?;

        // Add user message to chat state
        chat_state.add_message("user".to_string(), content);

        // Get the new tokens to process since last update
        let diff = chat_state.render_diff()?;
        let tokens = ctx.model.str_to_token(&diff, AddBos::Always)?;

        // TODO: here
        debug_assert!(tokens.len() > 0);
        debug_assert!(tokens.len() < n_ctx as usize);

        add_sequence(&mut batch, &tokens, n_past, &[0])?;

        ctx.decode(&mut batch)?;

        n_past += tokens.len() as i32;

        // Token generation loop
        loop {
            // Check for context window overflow (it was in the end before)
            if n_past >= ctx.n_ctx() as i32 - 1 {
                n_past -= apply_context_shifting(&mut ctx, n_past)?;

                assert!(n_past + batch.n_tokens() < ctx.n_ctx() as i32);
            }

            // Sample next token, no need to use sampler.accept as sample already accepts the token.
            // using sampler.accept() will cause the sampler to crash when using grammar sampling.
            // https://github.com/utilityai/llama-cpp-rs/issues/604
            let new_token: LlamaToken = sampler.sample(&ctx, -1);

            // Process current batch
            batch.clear();
            batch.add(new_token, n_past, &[0], true)?;

            assert!(batch.n_tokens() == 1);

            ctx.decode(&mut batch).unwrap();

            n_past += batch.n_tokens();

            // Check for end of generation (do not append the EOG token to the response)
            if ctx.model.is_eog_token(new_token) {
                break;
            }

            // Convert token to text and stream to user
            let output_string = ctx
                .model
                .token_to_str_with_size(new_token, MAX_TOKEN_STR_LEN, Special::Tokenize)
                .unwrap_or("�".to_string());
            // fall back to "U+FFFD REPLACEMENT CHARACTER"
            // when encountering bytes that aren't valid UTF-8
            // wikipedia: "used to replace an unknown, unrecognised, or unrepresentable character"

            response.push_str(&output_string);

            completion_tx
                .send(LLMOutput::Token(output_string.clone()))
                .map_err(|_| WorkerError::SendError)?;

            debug_assert!(n_past == ctx.get_kv_cache_token_count());

            // Check for stop tokens
            if stop_tokens.len() > 0 {
                if last_n_tokens.len() >= longest_stop_token {
                    last_n_tokens.pop_front();
                }
                last_n_tokens.push_back(output_string);
                if has_stop_tokens(&last_n_tokens, &stop_tokens) {
                    break;
                }
            }
        }

        // Update chat state with generated response
        chat_state.add_message("assistant".to_string(), response.clone());
        // render template again, just to set the length of the last template render
        // b/c the next diff should include only the next user msg, and not this assistant msg
        chat_state.render_diff()?;

        // Send completion signal
        completion_tx
            .send(LLMOutput::Done(response.clone()))
            .map_err(|_| WorkerError::SendError)?;

        response.clear();

        // I drop the inference_lock explicitly here because I think the rust
        // compiler might otherwise optimize and drop it early
        drop(inference_lock);
    }

    // We can't really throw an error here, since the other end of our channels seem to have died
    // but it's not `unreachable!()`, since we do end up here once the channels die.
    Ok(()) // accept our fate
}

/// Checks if the current generation should stop based on stop tokens.
/// This prevents the model from continuing after a stop sequence is detected.
///
/// # Arguments
/// * `last_n_tokens` - The last few tokens generated
/// * `stop_tokens` - List of token sequences that should stop generation
///
/// # Returns
/// * `should_stop` - Whether generation should stop
fn has_stop_tokens(last_n_tokens: &VecDeque<String>, stop_tokens: &[String]) -> bool {
    if last_n_tokens.is_empty() || stop_tokens.is_empty() {
        return false;
    }
    let last_n_concatenated: String = last_n_tokens.iter().fold(String::new(), |acc, x| acc + x);
    stop_tokens
        .iter()
        .any(|stop_token| last_n_concatenated.contains(stop_token))
}

pub enum EmbeddingsOutput {
    Embedding(Vec<f32>),
    FatalError(WorkerError),
}

pub fn run_embedding_worker(
    model: Arc<LlamaModel>,
    text_rx: Receiver<String>,
    embedding_tx: Sender<EmbeddingsOutput>,
) {
    // this function is a pretty thin wrapper to send back an `Err` if we get it
    if let Err(msg) = run_embedding_worker_result(model, text_rx, &embedding_tx) {
        embedding_tx
            .send(EmbeddingsOutput::FatalError(msg))
            .expect("Could not send llm worker fatal error back to consumer.");
    }
}

pub fn run_embedding_worker_result(
    model: Arc<LlamaModel>,
    text_rx: Receiver<String>,
    embedding_tx: &Sender<EmbeddingsOutput>,
) -> Result<(), WorkerError> {
    let n_threads = std::thread::available_parallelism()?.get() as i32;
    let ctx_params = LlamaContextParams::default()
        .with_n_threads(n_threads)
        .with_embeddings(true);

    let mut ctx = model.new_context(&LLAMA_BACKEND, ctx_params)?;

    while let Ok(text) = text_rx.recv() {
        // HACK see comment in completion worker
        let inference_lock = GLOBAL_INFERENCE_LOCK
            .lock()
            .map_err(|_| WorkerError::GILPoisonError)?;

        let mut batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);

        let tokens = ctx.model.str_to_token(&text, AddBos::Always)?;

        add_sequence(&mut batch, &tokens, 0, &[0]).expect("Failed to add sequence");

        ctx.clear_kv_cache();

        ctx.decode(&mut batch)?;

        let embedding = ctx.embeddings_seq_ith(0).unwrap().to_vec();
        embedding_tx
            .send(EmbeddingsOutput::Embedding(embedding))
            .map_err(|_| WorkerError::SendError)?;

        drop(inference_lock);
    }
    Ok(())
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
        () => {
            std::env::var("TEST_EMBEDDINGS_MODEL")
                .unwrap_or("embeddings.gguf".to_string())
                .as_str()
        };
    }

    #[test]
    fn test_actor_chat() {
        let model = get_model(test_model_path!(), true).unwrap();
        let system_prompt =
            "You are a helpful assistant. The user asks you a question, and you provide an answer."
                .to_string();

        let params = LLMActorParams {
            model,
            sampler_config: SamplerConfig::default(),
            n_ctx: 4096,
            stop_tokens: vec![],
        };

        let mut actor = LLMActorHandle::new(params).with_system_message(system_prompt);
        let result = actor.say("What is the capital of Denmark?".to_string());
        assert!(result.is_ok());

        let result: String;
        loop {
            match actor.try_get() {
                Ok(LLMOutput::Token(_)) => {}
                Ok(LLMOutput::Done(response)) => {
                    result = response;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                err => {
                    dbg!(err);
                    unreachable!()
                }
            }
        }

        assert!(
            result.contains("Copenhagen"),
            "Expected completion to contain 'Copenhagen', got: {result}"
        );

        let result = actor.say("What langauge do they speak there?".to_string());
        assert!(result.is_ok());

        let result: String;
        loop {
            match actor.try_get() {
                Ok(LLMOutput::Token(_)) => {}
                Ok(LLMOutput::Done(response)) => {
                    result = response;
                    break;
                }
                Err(std::sync::mpsc::TryRecvError::Empty) => {}
                _ => unreachable!(),
            }
        }

        assert!(
            result.contains("Danish"),
            "Expected completion to contain 'Danish', got: {result}"
        );
    }

    #[test]
    fn test_chat_completion() {
        let model = get_model(test_model_path!(), true).unwrap();

        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel();
        let (completion_tx, completion_rx) = std::sync::mpsc::channel();

        let system_prompt = "You are a helpful assistant. The user asks you a question, and you provide an answer. You take multiple turns to provide the answer. Be consice and only provide the answer".to_string();
        std::thread::spawn(|| {
            run_completion_worker(
                model,
                prompt_rx,
                completion_tx,
                SamplerConfig::default(),
                4096,
                system_prompt,
                vec![],
            )
        });

        prompt_tx
            .send("What is the capital of Denmark?".to_string())
            .unwrap();

        let result: String;
        loop {
            match completion_rx.recv() {
                Ok(LLMOutput::Token(_)) => {}
                Ok(LLMOutput::Done(response)) => {
                    result = response;
                    break;
                }
                _ => unreachable!(),
            }
        }
        assert!(
            result.contains("Copenhagen"),
            "Expected completion to contain 'Copenhagen', got: {result}"
        );

        prompt_tx
            .send("What language to they speak there?".to_string())
            .unwrap();
        let result: String;
        loop {
            match completion_rx.recv() {
                Ok(LLMOutput::Token(_)) => {}
                Ok(LLMOutput::Done(response)) => {
                    result = response;
                    break;
                }
                _ => unreachable!(),
            }
        }

        assert!(
            result.contains("Danish"),
            "Expected completion to contain 'Danish', got: {result}"
        );
    }

    #[test]
    fn test_embeddings() {
        let model = get_model(test_embeddings_model_path!(), true).unwrap();

        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel();
        let (embedding_tx, embedding_rx) = std::sync::mpsc::channel();

        std::thread::spawn(|| run_embedding_worker(model, prompt_rx, embedding_tx));

        prompt_tx
            .send("Copenhagen is the capital of Denmark.".to_string())
            .unwrap();
        let copenhagen_embedding = match embedding_rx.recv() {
            Ok(EmbeddingsOutput::Embedding(vec)) => vec,
            _ => panic!(),
        };

        prompt_tx
            .send("Berlin is the capital of Germany.".to_string())
            .unwrap();
        let berlin_embedding = match embedding_rx.recv() {
            Ok(EmbeddingsOutput::Embedding(vec)) => vec,
            _ => panic!(),
        };

        prompt_tx
            .send("Your mother was a hamster and your father smelt of elderberries!".to_string())
            .unwrap();
        let insult_embedding = match embedding_rx.recv() {
            Ok(EmbeddingsOutput::Embedding(vec)) => vec,
            _ => panic!(),
        };

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

    #[test]
    fn test_multiple_contexts_single_model() {
        let model = get_model(test_model_path!(), true).unwrap();

        let trivia_bot_system_prompt = "You are a trivia bot. You are asked a question, and you provide an answer. Be concise and only provide the answer".to_string();
        let (denmark_prompt_tx, denmark_prompt_rx) = std::sync::mpsc::channel();
        let (denmark_completion_tx, denmark_completion_rx) = std::sync::mpsc::channel();

        let model_clone = model.clone();
        std::thread::spawn(|| {
            run_completion_worker(
                model_clone,
                denmark_prompt_rx,
                denmark_completion_tx,
                SamplerConfig::default(),
                4096,
                trivia_bot_system_prompt,
                vec![],
            )
        });

        let trivia_bot_system_prompt = "You are a trivia bot. You are asked a question, and you provide an answer. Be concise and only provide the answer".to_string();
        let (germany_prompt_tx, germany_prompt_rx) = std::sync::mpsc::channel();
        let (germany_completion_tx, germany_completion_rx) = std::sync::mpsc::channel();

        std::thread::spawn(|| {
            run_completion_worker(
                model,
                germany_prompt_rx,
                germany_completion_tx,
                SamplerConfig::default(),
                4096,
                trivia_bot_system_prompt,
                vec![],
            )
        });

        denmark_prompt_tx
            .send("What is the capital of Denmark?".to_string())
            .unwrap();

        germany_prompt_tx
            .send("What is the capital of Germany?".to_string())
            .unwrap();

        // read dog output
        let result: String;
        loop {
            match denmark_completion_rx.recv() {
                Ok(LLMOutput::Token(_)) => {}
                Ok(LLMOutput::Done(response)) => {
                    result = response;
                    break;
                }
                _ => unreachable!(),
            }
        }
        assert!(
            result.to_lowercase().contains("copenhagen"),
            "Expected completion to contain 'Copenhagen', got: {result}"
        );

        // read cat output
        let result: String;
        loop {
            match germany_completion_rx.recv() {
                Ok(LLMOutput::Token(_)) => {}
                Ok(LLMOutput::Done(response)) => {
                    result = response;
                    break;
                }
                _ => unreachable!(),
            }
        }
        assert!(
            result.to_lowercase().contains("berlin"),
            "Expected completion to contain 'Berlin', got: {result}"
        );
    }

    #[test]
    fn test_context_shifting() {
        let model = get_model(test_model_path!(), true).unwrap();

        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel();
        let (completion_tx, completion_rx) = std::sync::mpsc::channel();

        let system_prompt = "You are a helpful assistant.".to_string();
        std::thread::spawn(|| {
            run_completion_worker(
                model,
                prompt_rx,
                completion_tx,
                SamplerConfig::default(),
                100, // very low context size. will be exceeded immediately
                system_prompt,
                vec![],
            )
        });

        prompt_tx
            .send("Please count down from 10 to 0, like this: Current 10, target 0. Current 9, target 0...".to_string())
            .unwrap();

        let result: String;
        loop {
            match completion_rx.recv() {
                Ok(LLMOutput::Token(t)) => {
                    println!("new token: {t}");
                }
                Ok(LLMOutput::Done(response)) => {
                    result = response;
                    break;
                }
                Ok(LLMOutput::FatalErr(e)) => {
                    println!("got fatal error: {e}");
                    panic!();
                }
                _ => unreachable!(),
            }
        }
        assert!(
            result.contains("Current 1, target 0"),
            "Expected completion to contain 'Current 0, target 0', got: {result}"
        );
    }

    #[test]
    fn test_stop_tokens() {
        let model = get_model(test_model_path!(), true).unwrap();

        let (prompt_tx, prompt_rx) = std::sync::mpsc::channel();
        let (completion_tx, completion_rx) = std::sync::mpsc::channel();

        let system_prompt = "You are a helpful assistant.".to_string();
        std::thread::spawn(|| {
            run_completion_worker(
                model,
                prompt_rx,
                completion_tx,
                SamplerConfig::default(),
                4096,
                system_prompt,
                vec!["horse".to_string()], // Stop at "horse"
            )
        });

        prompt_tx
            .send(
                "List these animals in alphabetical order: cat, dog, giraffe, horse, lion, mouse. Keep them in lowercase."
                    .to_string(),
            )
            .unwrap();

        let result: String;
        loop {
            match completion_rx.recv() {
                Ok(LLMOutput::Token(t)) => {
                    println!("new token: {t}");
                }
                Ok(LLMOutput::Done(response)) => {
                    result = response;
                    break;
                }
                Ok(LLMOutput::FatalErr(e)) => {
                    println!("got fatal error: {e}");
                    panic!();
                }
                _ => unreachable!(),
            }
        }

        assert!(
            result.to_lowercase().contains("giraffe"),
            "Expected output to contain text before stop token. Got: {result}"
        );
        assert!(
            result.to_lowercase().contains("horse"),
            "Expected output to contain stop token. Got: {result}"
        );
        assert!(
            !result.to_lowercase().contains("lion"),
            "Expected output to stop at stop token, but continued. Got: {result}"
        );
        assert!(
            !result.to_lowercase().contains("mouse"),
            "Expected output to stop at stop token, but continued. Got: {result}"
        );
    }
}
