use crate::sampler_config::{make_sampler, SamplerConfig};
use lazy_static::lazy_static;
use llama_cpp_2::context::params::{LlamaContextParams, LlamaPoolingType};
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::model::{AddBos, Special};
use llama_cpp_2::token::LlamaToken;
use std::pin::pin;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, LazyLock, Mutex};
use tracing::{debug, debug_span, error, info, info_span, trace, trace_span, warn};

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

    // Delete the first `n_discard` tokens
    ctx.clear_kv_cache_seq(
        Some(0),
        Some(n_keep as u32),
        Some((n_keep + n_discard) as u32),
    )?;

    // Shift the context left with `n_discard` tokens
    ctx.kv_cache_seq_add(
        0,
        Some((n_keep + n_discard) as u32),
        Some(n_past as u32),
        -n_discard,
    )?;

    debug!(target: "Context shifted", ?n_discard);

    Ok(n_discard)
}

#[derive(Debug, thiserror::Error)]
pub enum InitWorkerError {
    #[error("Could not determine number of threads available: {0}")]
    ThreadCountError(#[from] std::io::Error),

    #[error("Could not create context: {0}")]
    CreateContextError(#[from] llama_cpp_2::LlamaContextLoadError),

    #[error("Failed getting chat template from model: {0}")]
    ChatTemplateError(#[from] crate::chat_state::FromModelError),

    #[error("Got no response after initializing worker.")]
    NoResponse,
}

#[derive(Debug)]
pub(crate) struct Worker<'a, S> {
    n_past: i32,
    pub(crate) ctx: LlamaContext<'a>,
    big_batch: LlamaBatch,
    small_batch: LlamaBatch,

    pub(crate) extra: S,
}

pub trait PoolingType {
    fn pooling_type(&self) -> LlamaPoolingType;
}

impl<'a, T> PoolingType for Worker<'a, T> {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::Unspecified
    }
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

    #[error("Error converting token to bytes:  {0}")]
    TokenToStringError(#[from] llama_cpp_2::TokenToStringError),

    #[error("Invalid sampler configuration")]
    InvalidSamplerConfig,
}

pub trait GenerationCapability {}
pub trait Stoppable {
    fn stop(&self) -> bool;
}
// Type state markers
pub struct GenerationWorker {
    should_stop: Arc<AtomicBool>,
}

impl GenerationCapability for GenerationWorker {}
impl Stoppable for GenerationWorker {
    fn stop(&self) -> bool {
        self.should_stop.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[cfg(test)]
impl<'a> Worker<'_, GenerationWorker> {
    fn new_generation_worker(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
    ) -> Result<Worker<'_, GenerationWorker>, InitWorkerError> {
        Worker::new_with_type(
            model,
            n_ctx,
            false,
            GenerationWorker {
                should_stop: Arc::new(AtomicBool::new(false)),
            },
        )
    }
}

impl PoolingType for GenerationWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::None
    }
}

impl<'a, T> Worker<'a, T>
where
    T: GenerationCapability + Stoppable,
{
    #[tracing::instrument(level = "info", skip(self, sampler_config, stop_words, respond))]
    pub fn write_until_done<F>(
        &mut self,
        sampler_config: SamplerConfig,
        stop_words: Vec<String>,
        mut respond: F,
    ) -> Result<&mut Self, WriteError>
    where
        F: FnMut(WriteOutput),
    {
        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        // Token generation loop
        info!("Worker writing until done");

        // pre-allocating 4096 bytes for the response string
        // 4096 is a very randomly chosen number. how does this affect performance?
        let mut full_response: String = String::with_capacity(4096);

        // initialize sampler
        // stateful samplers only live for one response
        let mut sampler = make_sampler(&self.ctx.model, sampler_config)
            .ok_or(WriteError::InvalidSamplerConfig)?;

        let mut token_bytes_vec = Vec::new();

        while !self.extra.stop() {
            // Check for context window overflow (it was in the end before)
            if self.n_past >= self.ctx.n_ctx() as i32 {
                self.n_past -= apply_context_shifting(&mut self.ctx, self.n_past)?;
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

            // Attempt to convert token(s) to bytes
            let token_bytes = self
                .ctx
                .model
                .token_to_bytes(new_token, Special::Tokenize)?;

            token_bytes_vec.extend(token_bytes);

            // Attempt to convert bytes to utf8 string.

            let token_str = match std::str::from_utf8(&token_bytes_vec) {
                Ok(str) => str,
                Err(_) => {
                    if token_bytes_vec.len() > 4 {
                        "�"
                    } else {
                        continue;
                    }
                }
            };

            // Basic solution to split up graphemes. If the current token bytes cannot
            // be converted into a string then we try to read more tokens till we have
            // at least four bytes. If these still cannot be converted into a string,
            // we assume that the model/sampler has produced a useless token somewhere.
            // This we currently handle by discarding all of the current bytes, but more
            // intelligent solutions could be a good idea.

            trace!(?new_token, ?token_str);
            let has_eog = self.ctx.model.is_eog_token(new_token);

            if !has_eog {
                full_response.push_str(token_str);
                trace!("Sending out token: {token_str}");
                respond(WriteOutput::Token(token_str.to_string()));
            }

            // done using token_str, so now we can clear token_bytes_vec
            token_bytes_vec.clear();

            let has_stop_words = stop_words
                .iter()
                .any(|stop_word| full_response.contains(stop_word));
            if has_eog || has_stop_words {
                break;
            }
        }

        // we're done!
        debug!("Sending out response: {full_response}");
        respond(WriteOutput::Done(full_response));
        Ok(self)
    }
}

// Common methods for any workstate type
impl<'a, T> Worker<'a, T>
where
    T: PoolingType,
{
    pub(crate) fn new_with_type(
        model: &Arc<LlamaModel>,
        n_ctx: u32,
        use_embeddings: bool,
        extra: T,
    ) -> Result<Worker<'_, T>, InitWorkerError> {
        info!("Initializing Worker");

        // Set up context parameters using available parallelism
        let ctx = {
            let n_threads = std::thread::available_parallelism()?.get() as i32;
            let n_ctx = std::cmp::min(n_ctx, model.n_ctx_train());
            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(std::num::NonZero::new(n_ctx))
                .with_n_threads(n_threads)
                .with_n_threads_batch(n_threads)
                .with_embeddings(use_embeddings)
                .with_pooling_type(extra.pooling_type());

            // Create inference context and sampler
            model.new_context(&LLAMA_BACKEND, ctx_params)?
        };

        let big_batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);
        let small_batch = LlamaBatch::new(1, 1);

        let state = Worker {
            n_past: 0,
            ctx,
            big_batch,
            small_batch,
            extra,
        };
        Ok(state)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn reset_context(&mut self) -> &mut Self {
        self.ctx.clear_kv_cache();
        self.n_past = 0;
        self
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn read_string(&mut self, text: String) -> Result<&mut Self, ReadError> {
        let tokens = self.ctx.model.str_to_token(&text, AddBos::Never)?;
        self.read_tokens(tokens)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn read_tokens(&mut self, tokens: Vec<LlamaToken>) -> Result<&mut Self, ReadError> {
        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();

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

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn remove_all_tokens_after_index_from_ctx(&mut self, index: u32) -> Result<(), ReadError> {
        if self.n_past <= index as i32 {
            return Ok(());
        }

        self.ctx.clear_kv_cache_seq(Some(0), Some(index), None)?;
        self.n_past = index as i32;
        return Ok(());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils;

    #[test]
    fn test_simple_gen() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();

        let sampler = SamplerConfig::default();
        let mut worker = Worker::new_generation_worker(&model, 4096)?;
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
    fn test_multiple_contexts_single_model() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();
        let model = test_utils::load_test_model();
        let sampler = SamplerConfig::default();
        let n_ctx = 4096;

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
            let mut worker = Worker::new_generation_worker(&model_clone, n_ctx).unwrap();
            let f = move |x| {
                if let WriteOutput::Done(resp) = x {
                    let mut response = dk_response_clone.lock().unwrap();
                    *response = Some(resp);
                }
            };

            worker
                .read_string("<think>\nCopenhagen is the capital of Denmark\n</think>\nThe name of the capital city of Denmark is \"".to_string())
                .unwrap()
                .write_until_done(dk_sampler, vec!["Copenhagen".to_string()], f)
                .unwrap();
        });

        // Start Germany worker thread
        let de_handle = std::thread::spawn(move || {
            let mut worker = Worker::new_generation_worker(&model, n_ctx).unwrap();
            let f = move |x| {
                if let WriteOutput::Done(resp) = x {
                    let mut response = de_response_clone.lock().unwrap();
                    *response = Some(resp);
                }
            };

            worker
                .read_string("<think>\nBerlin is the capital of Germany\n</think>\nThe capital of germany is called ".to_string())
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

        let n_ctx = 10;
        let mut worker = Worker::new_generation_worker(&model, n_ctx)?;
        let (sender, receiver) = std::sync::mpsc::channel();
        let f = move |x| match x {
            WriteOutput::Done(resp) => {
                sender.send(resp).unwrap();
            }
            _ => (),
        };

        worker
            .read_string("Once upon a time".to_string())?
            .write_until_done(sampler, vec!["\n".to_string()], f)?;

        let response = receiver.recv()?;
        assert!(
            model.str_to_token(&response, AddBos::Never).unwrap().len() > n_ctx as usize,
            "Expected response longer than n_ctx"
        );

        Ok(())
    }

    #[test]
    fn test_stop_tokens() -> Result<(), Box<dyn std::error::Error>> {
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

        let mut worker = Worker::new_generation_worker(&model, 1024)?;
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

        let mut worker = Worker::new_generation_worker(&model, 20)?;

        worker.read_string("1, 2, 3,".to_string())?;
        worker.read_string("1, 2, 3,".to_string())?;
        worker.read_string("1, 2, 3,".to_string())?;
        worker.read_string("1, 2, 3,".to_string())?;
        worker.read_string("1, 2, 3,".to_string())?;
        worker.read_string("1, 2, 3,".to_string())?;
        Ok(())
    }
}
