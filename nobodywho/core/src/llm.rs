use crate::errors::{InitWorkerError, LoadModelError, ReadError};
use lazy_static::lazy_static;
use llama_cpp_2::context::kv_cache::KvCacheConversionError;
use llama_cpp_2::context::params::{LlamaContextParams, LlamaPoolingType};
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::AddBos;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::token::LlamaToken;
use std::pin::pin;
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use tracing::{debug, debug_span, error, info, info_span, warn};

#[derive(Debug)]
pub(crate) struct GlobalInferenceLockToken;
lazy_static! {
    pub(crate) static ref GLOBAL_INFERENCE_LOCK: Mutex<GlobalInferenceLockToken> =
        Mutex::new(GlobalInferenceLockToken);
}

static LLAMA_BACKEND: LazyLock<LlamaBackend> =
    LazyLock::new(|| LlamaBackend::init().expect("Failed to initialize llama backend"));

pub type Model = Arc<LlamaModel>;

pub fn has_discrete_gpu() -> bool {
    #[cfg(any(
        all(target_os = "ios", target_arch = "aarch64", target_abi = "sim"),
        all(target_os = "ios", target_arch = "x86_64")
    ))]
    {
        // GPU-acceleration not working on ios simulators seems to be a known issue in llama.cpp:
        // https://github.com/ggml-org/llama.cpp/blob/017eceed61e885b79f6cf3542e0879be68c6e922/examples/llama.swiftui/llama.cpp.swift/LibLlama.swift#L66
        warn!("Running on iOS simulator. Disabling GPU support.");
        return false;
    }

    for backend_device in llama_cpp_2::list_llama_ggml_backend_devices() {
        // TODO: account for memoery available on backend device - .memory_total and .memory free
        //       we might use these with GGUF model metadata, to decide on an umber of layers to offload
        match backend_device.device_type {
            llama_cpp_2::LlamaBackendDeviceType::Unknown => {
                continue;
            }
            llama_cpp_2::LlamaBackendDeviceType::Cpu => {
                continue;
            }
            llama_cpp_2::LlamaBackendDeviceType::Accelerator => {
                // TODO: investigate: can we use this?
                continue;
            }
            llama_cpp_2::LlamaBackendDeviceType::IntegratedGpu => {
                // TODO: investigate: can we use this?
                continue;
            }
            llama_cpp_2::LlamaBackendDeviceType::Gpu => {
                return true;
            }
        }
    }

    false
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

#[derive(Debug)]
pub(crate) struct Worker<'a, S> {
    pub(crate) n_past: i32,
    pub(crate) ctx: LlamaContext<'a>,
    pub(crate) big_batch: LlamaBatch<'a>,
    pub(crate) small_batch: LlamaBatch<'a>,

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

#[derive(Debug)]
pub enum WriteOutput {
    Token(String),
    Done(String),
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
        let _gil_guard = GLOBAL_INFERENCE_LOCK.lock();
        let inference_lock_token = _gil_guard.unwrap();
        let tokens = self.ctx.model.str_to_token(&text, AddBos::Never)?;
        self.read_tokens(tokens, &inference_lock_token)
    }

    // ---------- IMPORTANT ----------
    // Should only be used under a global inference lock
    // This is a safety meassure to prevent bugs from multiple
    // contexts with the same model. It might not be necessary
    // but assume it is.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn read_tokens(
        &mut self,
        tokens: Vec<LlamaToken>,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, ReadError> {
        let n_tokens = tokens.len();
        debug!("Reading {n_tokens} tokens.");

        // can't read nothing
        debug_assert!(!tokens.is_empty());
        // can't read more than the context size
        debug_assert!(tokens.len() < self.ctx.n_ctx() as usize);

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
    pub fn remove_all_tokens_after_index_from_ctx(
        &mut self,
        index: u32,
    ) -> Result<(), KvCacheConversionError> {
        if self.n_past <= index as i32 {
            return Ok(());
        }

        self.ctx.clear_kv_cache_seq(Some(0), Some(index), None)?;
        self.n_past = index as i32;

        Ok(())
    }
}

#[cfg(test)]
mod tests {}
