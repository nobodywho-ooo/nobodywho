use crate::errors::{InitWorkerError, LoadModelError, ReadError};
use crate::huggingface::{download_gguf, parse_model_path};
use crate::inference::{acquire_inference_lock, EngineContext, InferenceEngine};
use crate::memory;
use crate::model_selection;
use crate::tokenizer::{ProjectionModel, Tokenizer};
use lazy_static::lazy_static;
use llama_cpp_2::context::params::{LlamaContextParams, LlamaContextType, LlamaPoolingType};
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::AddBos;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::speculative::{MtpSpeculative, MtpSpeculativeParams};
use std::pin::pin;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use tracing::{debug, error, info, info_span, warn};

// Back-compat re-exports: bindings (Python, Godot, Flutter) import these via
// `nobodywho::llm::*`. The implementations now live in `crate::huggingface`.
pub use crate::huggingface::{
    default_progress_callback, get_cached_models, throttled_progress_callback,
    DownloadProgressCallback,
};

#[derive(Debug)]
pub(crate) struct GlobalInferenceLockToken;
lazy_static! {
    pub(crate) static ref GLOBAL_INFERENCE_LOCK: Mutex<GlobalInferenceLockToken> =
        Mutex::new(GlobalInferenceLockToken);
}

static LLAMA_BACKEND: LazyLock<LlamaBackend> =
    LazyLock::new(|| LlamaBackend::init().expect("Failed to initialize llama backend"));

#[derive(Debug)]
pub struct Model {
    pub(crate) language_model: LlamaModel,
    pub(crate) projection_model: Option<ProjectionModel>,
    /// Optional MTP draft model for split-file speculative decoding
    /// (e.g. Gemma-4's separate MTP-heads gguf). Loaded at model-load
    /// time so multiple workers can share it without reloading. Whether
    /// a given worker actually *uses* it for MTP is a per-worker
    /// decision (see `mtp` on `ChatConfig` / `Worker::new_with_type`).
    pub(crate) draft_model: Option<LlamaModel>,
}

impl Model {
    /// Returns true if this model can generate text (i.e. is an autoregressive decoder).
    ///
    /// Generative models never pool token representations, so `<arch>.pooling_type` is absent
    /// from their GGUF metadata (giving `Unspecified`). Encoder-only models (BERT, nomic-bert,
    /// etc.) always have this key set to CLS, Mean, or similar — a reliable,
    /// architecture-agnostic signal that the model cannot generate text.
    pub fn max_ctx(&self) -> u32 {
        self.language_model.n_ctx_train()
    }

    pub fn is_generative_model(&self) -> bool {
        let Ok(arch) = self.language_model.meta_val_str("general.architecture") else {
            return true;
        };
        let key = format!("{arch}.pooling_type");
        self.language_model
            .meta_val_str(&key)
            .ok()
            .and_then(|val| val.parse::<i32>().ok())
            .map(LlamaPoolingType::from)
            .unwrap_or(LlamaPoolingType::Unspecified)
            == LlamaPoolingType::Unspecified
    }
}

pub fn has_gpu_backend() -> bool {
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
        // TODO: account for memory available on backend device - .memory_total and .memory free
        //       we might use these with GGUF model metadata, to decide on a number of layers to offload
        match backend_device.device_type {
            llama_cpp_2::LlamaBackendDeviceType::Unknown => {
                continue;
            }
            llama_cpp_2::LlamaBackendDeviceType::Cpu => {
                continue;
            }
            llama_cpp_2::LlamaBackendDeviceType::Accelerator => {
                // Accelerator devices (e.g. NPUs) are auto-initialized by llama.cpp during
                // context creation regardless of n_gpu_layers — no explicit handling needed.
                continue;
            }
            llama_cpp_2::LlamaBackendDeviceType::IntegratedGpu => {
                return true;
            }
            llama_cpp_2::LlamaBackendDeviceType::Gpu => {
                return true;
            }
        }
    }

    false
}

#[tracing::instrument(level = "info", skip(progress))]
pub fn get_model(
    model_path: &str,
    use_gpu_if_available: bool,
    mmproj_path: Option<&str>,
    draft_model_path: Option<&str>,
    progress: Option<DownloadProgressCallback>,
) -> Result<Model, LoadModelError> {
    if model_path == "auto" && mmproj_path.is_some() {
        return Err(LoadModelError::InvalidModel(
            "Automatic model selection does not support projection models; pass an explicit multimodal model path"
                .to_string(),
        ));
    }

    let use_gpu = use_gpu_if_available && has_gpu_backend();
    let model_path = model_selection::resolve_model_path(model_path, use_gpu)?;
    let model_progress = progress
        .clone()
        .unwrap_or_else(|| default_progress_callback(model_path));
    let real_model_path = download_gguf(parse_model_path(model_path)?, &model_progress, &[])?;
    let real_mmproj_path = match mmproj_path {
        Some(p) => {
            let mmproj_progress = progress
                .clone()
                .unwrap_or_else(|| default_progress_callback(p));
            Some(download_gguf(parse_model_path(p)?, &mmproj_progress, &[])?)
        }
        None => None,
    };
    let real_draft_model_path = match draft_model_path {
        Some(p) => {
            let draft_progress = progress
                .clone()
                .unwrap_or_else(|| default_progress_callback(p));
            Some(download_gguf(parse_model_path(p)?, &draft_progress, &[])?)
        }
        None => None,
    };

    // TODO: `LlamaModelParams` uses all devices by default. Set it to an empty list once an upstream device API is available.
    let loading_plan =
        memory::plan_model_loading(&real_model_path, real_mmproj_path.as_deref(), use_gpu);
    let gpu_layers = loading_plan.gpu_layers;
    for warning in &loading_plan.warnings {
        warn!("{}", warning);
    }

    info!(use_gpu = use_gpu, gpu_layers = gpu_layers, "Loading model");

    let model_params = LlamaModelParams::default().with_n_gpu_layers(gpu_layers);

    let model_params = pin!(model_params);
    let load_span = info_span!("model_load", path = %real_model_path.display());
    let _guard = load_span.enter();

    let language_model =
        LlamaModel::load_from_file(&LLAMA_BACKEND, &real_model_path, &model_params).map_err(
            |e| {
                if e.to_string().contains("null result") {
                    return LoadModelError::ModelLoadFailed {
                        path: real_model_path.display().to_string(),
                    };
                }
                let error_msg = format!(
                    "Bad model path: {} - Llama.cpp error: {}",
                    real_model_path.display(),
                    e
                );
                error!(error = %error_msg, "Failed to load model");
                LoadModelError::InvalidModel(error_msg)
            },
        )?;

    info!("Model loaded successfully");
    let projection_model = real_mmproj_path
        .as_ref()
        .map(|path| ProjectionModel::from_path(path, &language_model, use_gpu))
        .transpose()?;

    let draft_model = real_draft_model_path
        .as_ref()
        .map(|path| {
            info!(path = %path.display(), "Loading MTP draft model");
            LlamaModel::load_from_file(&LLAMA_BACKEND, path, &model_params).map_err(|e| {
                let error_msg = format!(
                    "Failed to load MTP draft model at {}: {}",
                    path.display(),
                    e
                );
                error!(error = %error_msg, "Failed to load MTP draft model");
                LoadModelError::InvalidModel(error_msg)
            })
        })
        .transpose()?;

    Ok(Model {
        language_model,
        projection_model,
        draft_model,
    })
}

/// Asynchronously loads a GGUF model from disk.
///
/// This function offloads the blocking model load operation to a background thread,
/// allowing the async runtime to remain responsive. This is particularly useful when
/// loading large models that can take several seconds to initialize.
///
/// # Arguments
///
/// * `model_path` - `auto` for memory-based LLM selection, or a path to a GGUF model
/// * `use_gpu_if_available` - Whether to attempt GPU acceleration if a discrete GPU is available
///
/// # Returns
///
/// Returns a `Model` on success, or a `LoadModelError` on failure.
///
/// # Errors
///
/// This function will return an error if:
/// * The model file is not found (`LoadModelError::ModelNotFound`)
/// * The model file is invalid or unsupported (`LoadModelError::InvalidModel`)
/// * The communication channel closes unexpectedly (`LoadModelError::ModelChannelError`)
#[tracing::instrument(level = "info", skip(progress))]
pub async fn get_model_async(
    model_path: String,
    use_gpu_if_available: bool,
    mmproj_path: Option<String>,
    draft_model_path: Option<String>,
    progress: Option<DownloadProgressCallback>,
) -> Result<Model, LoadModelError> {
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(4096);
    std::thread::spawn(move || {
        output_tx.blocking_send(get_model(
            &model_path,
            use_gpu_if_available,
            mmproj_path.as_deref(),
            draft_model_path.as_deref(),
            progress,
        ))
    });

    match output_rx.recv().await {
        Some(model) => return model,
        None => Err(LoadModelError::ModelChannelError),
    }
}

pub fn download_model(
    model_path: &str,
    headers: Vec<(String, String)>,
    progress: Option<DownloadProgressCallback>,
) -> Result<std::path::PathBuf, LoadModelError> {
    let progress = progress.unwrap_or_else(|| default_progress_callback(model_path));
    download_gguf(parse_model_path(model_path)?, &progress, &headers)
}

fn read_add_bos_metadata(model: &LlamaModel) -> Result<AddBos, InitWorkerError> {
    match model.meta_val_str("tokenizer.ggml.add_bos_token") {
        Ok(val) => match val.as_str() {
            "true" => Ok(AddBos::Always),
            "false" => Ok(AddBos::Never),
            _ => Err(InitWorkerError::InvalidAddBosData(format!(
                "Invalid boolean value for tokenizer.ggml.add_bos_token: '{}'",
                val,
            ))),
        },
        Err(_) => {
            // Defaulting to true seems to be "safer" than defaulting to false
            // the GGUF files for the gpt-oss models (at least ones that I have seen in the wild)
            // don't have the add_bos metadata field, and have a massive aneurysm if they don't
            // get the bos.
            // could it be that omitting bos generally does more damage than including it?
            warn!("tokenizer.ggml.add_bos_token not found in GGUF metadata, defaulting to true");
            Ok(AddBos::Always)
        }
    }
}

#[derive(Debug)]
pub(crate) struct Worker<'a, S> {
    pub(crate) engine: InferenceEngine<'a>,
    pub(crate) extra: S,
}

pub trait PoolingType {
    fn pooling_type(&self) -> LlamaPoolingType;
}

/// Pooling type for a plain generative chat session (no pooling).
impl PoolingType for () {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::None
    }
}

pub type WriteOutput =
    crate::stream::StreamOutput<Box<dyn miette::Diagnostic + Send + Sync + 'static>>;

// Common methods for any workstate type
impl<'a, T> Worker<'a, T>
where
    T: PoolingType,
{
    pub(crate) fn new_with_type(
        model: &'a Model,
        n_ctx: u32,
        use_embeddings: bool,
        mtp: bool,
        extra: T,
    ) -> Result<Worker<'a, T>, InitWorkerError> {
        info!("Initializing worker");

        let projection_model = model.projection_model.as_ref();

        // Set up context parameters using available parallelism
        let n_threads = std::thread::available_parallelism()?.get() as i32;
        let ctx_plan = memory::plan_context(
            std::cmp::min(n_ctx, model.language_model.n_ctx_train()),
            projection_model.is_some(),
            memory::ModelArchitecture {
                n_layers: model.language_model.n_layer(),
                n_embd: model.language_model.n_embd() as u32,
                n_head: model.language_model.n_head(),
                n_head_kv: model.language_model.n_head_kv(),
            },
        )?;
        let planned_n_ctx = ctx_plan.n_ctx;
        let n_ubatch = ctx_plan.n_ubatch;
        for w in &ctx_plan.warnings {
            warn!("{}", w);
        }

        let ctx_params = LlamaContextParams::default()
            .with_n_ctx(std::num::NonZero::new(planned_n_ctx))
            .with_n_batch(planned_n_ctx) // n_batch sets the max size of a batch (i.e. max prompt size)
            .with_n_ubatch(n_ubatch)
            .with_n_threads(n_threads)
            .with_n_threads_batch(n_threads)
            .with_embeddings(use_embeddings)
            .with_pooling_type(extra.pooling_type());

        let ctx = model
            .language_model
            .new_context(&LLAMA_BACKEND, ctx_params)?;
        let n_batch = planned_n_ctx as usize;

        let big_batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);
        let small_batch = LlamaBatch::new(1, 1);

        let engine_ctx = if mtp {
            match &model.draft_model {
                Some(draft_model) => {
                    info!("Initializing MTP speculative draft context");
                    // MTP draft ctx is built with `ctx_other =
                    // target_ctx` (via `new_context_with_ctx_other`
                    // below), which sets `is_mem_shared = true` inside
                    // llama.cpp's MTP impl. That's how llama-server
                    // configures MTP for both split-file and same-file
                    // topologies, and it's what shares the draft's
                    // hidden-state carryover with the target's KV.
                    //
                    // With `is_mem_shared = true`:
                    //   - `spec.process(&batch)` is a no-op — it only
                    //     copies hidden states from the target via
                    //     `llama_get_embeddings_nextn_ith`; it does
                    //     not call `llama_decode` on the draft ctx.
                    //     (See `common/speculative.cpp` — the decode
                    //     block is gated on `if (!is_mem_shared)`.)
                    //   - `spec.draft(...)` runs an AR loop with
                    //     1-token batches, K+1 ≤ ~6 tokens total.
                    //
                    // So the draft ctx never needs compute buffer for
                    // more than a handful of tokens. Capping at 32
                    // saves ~1 GB of VRAM per MTP-enabled worker with
                    // no throughput cost.
                    let draft_batch_cap: u32 = 32;
                    let draft_params = LlamaContextParams::default()
                        .with_n_ctx(std::num::NonZero::new(planned_n_ctx))
                        .with_n_batch(draft_batch_cap)
                        .with_n_ubatch(draft_batch_cap)
                        .with_n_threads(n_threads)
                        .with_n_threads_batch(n_threads)
                        .with_context_type(LlamaContextType::Mtp)
                        .with_n_rs_seq(0);
                    let draft_ctx = draft_model.new_context_with_ctx_other(
                        &LLAMA_BACKEND,
                        draft_params,
                        &ctx,
                    )?;
                    let spec =
                        MtpSpeculative::new(ctx, draft_ctx, MtpSpeculativeParams::default())?;
                    EngineContext::Speculative(spec)
                }
                None => {
                    return Err(InitWorkerError::MtpSameFileNotYetSupported);
                }
            }
        } else {
            EngineContext::Solo(ctx)
        };

        let add_bos = read_add_bos_metadata(&model.language_model)?;
        debug!(?add_bos, "Read add_bos from GGUF metadata:");

        let tokenizer = Tokenizer::new(&model.language_model, projection_model, add_bos);

        let engine = InferenceEngine::new(
            engine_ctx,
            big_batch,
            small_batch,
            projection_model,
            n_batch,
            tokenizer,
            use_embeddings,
        );
        Ok(Worker { engine, extra })
    }

    /// Reset the KV cache and token count. Delegates to the inference engine.
    pub fn reset_context(&mut self) -> &mut Self {
        self.engine.reset_context();
        self
    }

    /// Tokenize `text` and read it into the context under the global inference lock.
    #[tracing::instrument(level = "trace", skip(self))]
    pub fn read_string(&mut self, text: String) -> Result<&mut Self, ReadError> {
        let inference_lock_token = acquire_inference_lock();
        let chunks = self.engine.tokenize(text, vec![])?;
        self.engine.read_chunks(chunks, &inference_lock_token)?;
        Ok(self)
    }
}

/// Owns a background worker thread's resources and ensures clean shutdown.
///
/// When dropped: sets the optional stop flag, closes the message channel (causing the
/// worker's `recv()` to return `Err`), then joins the thread. This ordering guarantees
/// the worker has fully exited before any statics (e.g. `LLAMA_BACKEND`) are destroyed.
pub(crate) struct WorkerGuard<T> {
    pub(crate) msg_tx: Option<std::sync::mpsc::Sender<T>>,
    join_handle: Option<std::thread::JoinHandle<()>>,
    should_stop: Option<Arc<AtomicBool>>,
}

impl<T> WorkerGuard<T> {
    pub(crate) fn new(
        msg_tx: std::sync::mpsc::Sender<T>,
        join_handle: std::thread::JoinHandle<()>,
        should_stop: Option<Arc<AtomicBool>>,
    ) -> Self {
        Self {
            msg_tx: Some(msg_tx),
            join_handle: Some(join_handle),
            should_stop,
        }
    }

    /// Send a message to the worker. Returns false if the worker is gone.
    pub(crate) fn send(&self, msg: T) -> bool {
        self.msg_tx.as_ref().is_some_and(|tx| tx.send(msg).is_ok())
    }

    /// Signal the worker to stop mid-generation (no-op if no stop flag).
    pub(crate) fn stop(&self) {
        if let Some(ref flag) = self.should_stop {
            flag.store(true, Ordering::Relaxed);
        }
    }
}

impl<T> Drop for WorkerGuard<T> {
    fn drop(&mut self) {
        if let Some(ref stop) = self.should_stop {
            stop.store(true, Ordering::Relaxed);
        }
        drop(self.msg_tx.take());
        if let Some(handle) = self.join_handle.take() {
            if let Err(e) = handle.join() {
                error!("Worker panicked: {:?}", e);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn rejects_projection_model_with_auto_selection() {
        let result = get_model("auto", true, Some("projection.gguf"), None, None);
        assert!(matches!(result, Err(LoadModelError::InvalidModel(_))));
    }

    #[test]
    fn throttled_callback_drops_intermediate_calls_within_window() {
        let count = Arc::new(AtomicUsize::new(0));
        let count_inner = Arc::clone(&count);
        let cb = throttled_progress_callback(move |_d, _t| {
            count_inner.fetch_add(1, Ordering::Relaxed);
        });

        // First call always emits; subsequent calls within 100ms are dropped.
        for i in 0..1000 {
            cb(i, 10_000);
        }
        let n = count.load(Ordering::Relaxed);
        assert!((1..=5).contains(&n), "expected 1–5 emits, got {}", n);
    }

    #[test]
    fn throttled_callback_always_emits_on_completion() {
        let count = Arc::new(AtomicUsize::new(0));
        let count_inner = Arc::clone(&count);
        let cb = throttled_progress_callback(move |_d, _t| {
            count_inner.fetch_add(1, Ordering::Relaxed);
        });

        // First call: emits. Second call inside the window but is_done=true: emits.
        cb(50, 100);
        cb(100, 100);
        assert_eq!(count.load(Ordering::Relaxed), 2);
    }
}
