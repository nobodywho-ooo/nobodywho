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
use tracing::{debug, debug_span, error, info, info_span, trace, trace_span, warn};

const MAX_TOKEN_STR_LEN: usize = 128;

lazy_static! {
    static ref GLOBAL_INFERENCE_LOCK: Mutex<()> = Mutex::new(());
}

static LLAMA_BACKEND: LazyLock<LlamaBackend> =
    LazyLock::new(|| LlamaBackend::init().expect("Failed to initialize llama backend"));

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

#[tracing::instrument(level = "info")]
pub fn get_model(
    model_path: &str,
    use_gpu_if_available: bool,
) -> Result<Arc<LlamaModel>, LoadModelError> {
    if !std::path::Path::new(model_path).exists() {
        let e = LoadModelError::ModelNotFound(model_path.into());
        error!(error = %e, "Model file not found");
        return Err(e);
    }

    // TODO: `LlamaModelParams` uses all devices by default. Set it to an empty list once an upstream device API is available.
    let use_gpu = use_gpu_if_available && has_discrete_gpu();
    let gpu_layers = if use_gpu { u32::MAX } else { 0 };

    info!(use_gpu = use_gpu, gpu_layers = gpu_layers, "Loading model");

    let model_params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);

    let model_params = pin!(model_params);
    let load_span = info_span!("model_load", path = model_path);
    let _guard = load_span.enter();

    let model =
        LlamaModel::load_from_file(&LLAMA_BACKEND, model_path, &model_params).map_err(|e| {
            let error_msg = format!("Bad model path: {} - Llama.cpp error: {}", model_path, e);
            error!(error = %error_msg, "Failed to load model");
            LoadModelError::InvalidModel(error_msg)
        })?;

    info!("Model loaded successfully");
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
struct WorkerState<'a, S> {
    n_past: i32,
    ctx: LlamaContext<'a>,
    big_batch: LlamaBatch,
    small_batch: LlamaBatch,

    marker: std::marker::PhantomData<S>,
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

#[derive(Debug, thiserror::Error)]
pub enum ReadError {
    #[error("Could not tokenize string: {0}")]
    TokenizerError(#[from] llama_cpp_2::StringToTokenError),

    #[error("Could not add to batch: {0}")]
    BatchAddError(#[from] llama_cpp_2::llama_batch::BatchAddError),

    #[error("Llama.cpp failed decoding: {0}")]
    DecodeError(#[from] llama_cpp_2::DecodeError),

    #[error("Could not apply context shifting: {0}")]
    ContextShiftError(#[from] llama_cpp_2::context::kv_cache::KvCacheConversionError),
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

// Type state markers
pub struct EmbeddingsWorker {}
pub struct GenerationWorker {}

impl<'a> WorkerState<'a, EmbeddingsWorker> {
    fn new_embeddings_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
    ) -> Result<WorkerState<'_, EmbeddingsWorker>, InitWorkerError> {
        WorkerState::new_with_type(model, n_ctx, true)
    }

    fn get_embedding(&self) -> Result<Vec<f32>, llama_cpp_2::EmbeddingsError> {
        Ok(self.ctx.embeddings_seq_ith(0)?.to_vec())
    }
}

impl<'a> WorkerState<'a, GenerationWorker> {
    fn new_generation_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
    ) -> Result<WorkerState<'_, GenerationWorker>, InitWorkerError> {
        WorkerState::new_with_type(model, n_ctx, false)
    }

    #[tracing::instrument(level = "info", skip(self, respond))]
    fn write_until_done<F>(
        &mut self,
        sampler_config: SamplerConfig,
        stop_words: Vec<String>,
        respond: F, // respond_to: Sender<Result<WriteOutput, WriteError>>,
    ) -> Result<&mut Self, WriteError>
    where
        F: Fn(WriteOutput),
    {
        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        // Token generation loop
        info!("Worker writing until done");

        // pre-allocating 4096 bytes for the response string
        // 4096 is a very randomly chosen number. how does this affect performance?
        let mut full_response: String = String::with_capacity(4096);

        // initialize sampler
        // stateful samplers only live for one response
        let mut sampler = make_sampler(&self.ctx.model, sampler_config);

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
            let new_token: LlamaToken = sampler.sample(&self.ctx, -1);

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

            let has_stop_words = stop_words
                .iter()
                .any(|stop_word| full_response.contains(stop_word));
            if has_eog || has_stop_words {
                break;
            }
        }

        // we're done!
        trace!("Sending out response: {full_response}");
        respond(WriteOutput::Done(full_response));
        Ok(self)
    }
}

// Common methods for any workstate type
impl<'a, T> WorkerState<'a, T> {
    fn new_with_type(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
        use_embeddings: bool,
    ) -> Result<WorkerState<'_, T>, InitWorkerError> {
        info!("Initializing WorkerState");

        // Set up context parameters using available parallelism
        let ctx = {
            let n_threads = std::thread::available_parallelism()?.get() as i32;
            let n_ctx = std::cmp::min(n_ctx, model.n_ctx_train());
            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(std::num::NonZero::new(n_ctx))
                .with_n_threads(n_threads)
                .with_n_threads_batch(n_threads)
                .with_embeddings(use_embeddings);

            // Create inference context and sampler
            model.new_context(&LLAMA_BACKEND, ctx_params)?
        };

        let big_batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);
        let small_batch = LlamaBatch::new(1, 1);

        let state = WorkerState {
            n_past: 0,
            ctx,
            big_batch,
            small_batch,
            marker: std::marker::PhantomData,
        };
        Ok(state)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn reset_context(mut self) -> Self {
        self.ctx.clear_kv_cache();
        self.n_past = 0;
        self
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn read_string(&mut self, text: String) -> Result<&mut Self, ReadError> {
        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();

        let tokens = self.ctx.model.str_to_token(&text, AddBos::Never)?;
        let n_tokens = tokens.len();
        debug!("Reading {n_tokens} tokens.");

        // can't read nothing
        debug_assert!(tokens.len() > 0);
        // can't read more than the context size
        debug_assert!(tokens.len() < self.ctx.n_ctx() as usize);

        // apply context shifting
        if self.n_past as usize + tokens.len() > self.ctx.n_ctx() as usize {
            debug!("Applying context shifting");
            self.n_past -= apply_context_shifting(&mut self.ctx, self.n_past)?;
        }

        {
            debug!("Populating batch");
            // make batch
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

        self.n_past += tokens.len() as i32;

        debug!("completed read operation");
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

    #[test]
    fn test_simple_gen() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let sampler = SamplerConfig::default();
        let mut worker = WorkerState::new_generation_worker(&model, 4096)?;
        let (sender, receiver) = std::sync::mpsc::channel();

        let f = move |x| match x {
            WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        worker
            .read_string("I'm gonna count to 10: 1, 2, 3, ".to_string())?
            .write_until_done(sampler, vec!["10".to_string()], f)?;

        let response = receiver.recv()?;

        assert!(response.contains("4, 5, 6, 7, 8, 9, 10"));

        Ok(())
    }

    #[test]
    fn test_embeddings() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_embeddings_model();

        let mut worker = WorkerState::new_embeddings_worker(&model, 1024)?;

        let copenhagen_embedding = worker
            .read_string("Copenhagen is the capital of Denmark.".to_string())?
            .get_embedding()?;

        let berlin_embedding = worker
            .read_string("Berlin is the capital of Germany.".to_string())?
            .get_embedding()?;

        let insult_embedding = worker
            .read_string(
                "Your mother was a hamster and your father smelt of elderberries!".to_string(),
            )?
            .get_embedding()?;

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

        Ok(())
    }

    #[test]
    fn test_multiple_contexts_single_model() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let sampler = SamplerConfig::default();
        let n_ctx = 1024;

        // Use two separate response containers for thread safety
        let dk_response = Arc::new(Mutex::new(None));
        let de_response = Arc::new(Mutex::new(None));

        // Clone references for thread use
        let model_clone = Arc::clone(&model);
        let dk_response_clone = Arc::clone(&dk_response);
        let de_response_clone = Arc::clone(&de_response);
        let dk_sampler = sampler.clone();

        // Start Denmark worker thread
        let dk_handle = std::thread::spawn(move || {
            let mut worker = WorkerState::new_generation_worker(&model_clone, n_ctx).unwrap();

            let f = move |x| {
                if let WriteOutput::Done(resp) = x {
                    let mut response = dk_response_clone.lock().unwrap();
                    *response = Some(resp);
                }
            };

            worker
                .read_string("The name of the capital city of Denmark is \"".to_string())
                .unwrap()
                .write_until_done(dk_sampler, vec!["Copenhagen".to_string()], f)
                .unwrap();
        });

        // Start Germany worker thread
        let de_handle = std::thread::spawn(move || {
            let mut worker = WorkerState::new_generation_worker(&model, n_ctx).unwrap();

            let f = move |x| {
                if let WriteOutput::Done(resp) = x {
                    let mut response = de_response_clone.lock().unwrap();
                    *response = Some(resp);
                }
            };

            worker
                .read_string("The capital of germany is called ".to_string())
                .unwrap()
                .write_until_done(sampler, vec!["Berlin".to_string()], f)
                .unwrap();
        });

        // Wait for threads to complete
        dk_handle.join().unwrap();
        de_handle.join().unwrap();

        // Retrieve and verify responses
        let dk_resp = dk_response
            .lock()
            .unwrap()
            .clone()
            .expect("No response from dk_worker");
        let de_resp = de_response
            .lock()
            .unwrap()
            .clone()
            .expect("No response from de_worker");

        assert!(
            dk_resp.to_lowercase().contains("copenhagen"),
            "Expected completion to contain 'Copenhagen', got: {dk_resp}"
        );
        assert!(
            de_resp.to_lowercase().contains("berlin"),
            "Expected completion to contain 'Berlin', got: {de_resp}"
        );

        Ok(())
    }

    #[test]
    fn test_context_shifting() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let sampler = SamplerConfig::default();

        let mut worker = WorkerState::new_generation_worker(&model, 64)?;

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        worker
            .read_string("I'm going to count to 20: 1, 2, 3, 4, 5, 6, 7".to_string())?
            .write_until_done(sampler, vec!["20".to_string()], f)?;

        let response = receiver.recv()?;
        assert!(
            response.contains("15, 16, 17, 18, 19, 20"),
            "Expected completion to count to 20, got: {response}"
        );

        Ok(())
    }

    #[tokio::test]
    async fn test_stop_tokens() -> Result<(), Box<dyn std::error::Error>> {
        crate::test_utils::init_test_tracing();

        let model = test_utils::load_test_model();
        let sampler = SamplerConfig::default();

        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        let mut worker = WorkerState::new_generation_worker(&model, 1024)?;
        worker
            .read_string("I'm going to count to 10: 1, 2, 3, 4,".to_string())?
            .write_until_done(sampler, vec!["7".to_string()], f)?;

        let response = receiver.recv()?;

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
        Ok(())
    }

    #[test]
    fn test_read_string_overrun() -> Result<(), Box<dyn std::error::Error>> {
        // this test looks a bit silly, but we had a bug
        // where we didn't apply context shifting while reading text
        // so now we test for it
        crate::test_utils::init_test_tracing();

        let model = test_utils::load_test_model();

        let mut worker = WorkerState::new_generation_worker(&model, 20)?;

        worker.read_string("1, 2, 3,".to_string())?;
        worker.read_string("1, 2, 3,".to_string())?;
        worker.read_string("1, 2, 3,".to_string())?;
        worker.read_string("1, 2, 3,".to_string())?;
        worker.read_string("1, 2, 3,".to_string())?;
        worker.read_string("1, 2, 3,".to_string())?;
        Ok(())
    }
}
