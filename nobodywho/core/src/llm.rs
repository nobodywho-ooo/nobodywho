use crate::errors::LoadModelError;
use crate::huggingface::{download_gguf, parse_model_path};
use crate::memory;
use crate::model_selection;
use crate::tokenizer::ProjectionModel;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use std::pin::pin;
use std::sync::{LazyLock, Mutex};
use tracing::{error, info, info_span, warn};

// Back-compat re-exports: bindings (Python, Godot, Flutter) import these via
// `nobodywho::llm::*`. The implementations now live in `crate::huggingface`.
pub use crate::huggingface::{
    default_progress_callback, get_cached_models, throttled_progress_callback,
    DownloadCancellationCallback, DownloadProgressCallback,
};

pub(crate) static LLAMA_BACKEND: LazyLock<LlamaBackend> = LazyLock::new(|| {
    // HACK: On Qualcomm Snapdragon 750G, Adreno 619, Fairphone 4, the OpenCL
    // backend deadlocks and aborts when running `f32_f16_q1_vec`.
    //
    // FIXME(madsmtm): Fix this upstream in the llama.cpp OpenCL backend.
    #[cfg(target_os = "android")]
    unsafe {
        std::env::set_var("GGML_OPENCL_FA_F16_VEC_DK128", "0")
    };

    LlamaBackend::init().expect("Failed to initialize llama backend")
});

#[derive(Debug)]
pub struct Model {
    source: String,
    pub(crate) language_model: LlamaModel,
    pub(crate) projection_model: Option<Mutex<ProjectionModel>>,
    pub(crate) draft_model: Option<DraftModel>,
}

#[derive(Debug)]
pub(crate) enum DraftModel {
    /// A separate MTP-heads gguf, e.g. for Gemma-4.
    Separate(LlamaModel),
    /// MTP layers bundled in the target file, e.g. Qwen3.5; drafts run on `language_model`.
    Bundled,
}

fn same_file(a: &std::path::Path, b: &std::path::Path) -> bool {
    match (std::fs::canonicalize(a), std::fs::canonicalize(b)) {
        (Ok(a), Ok(b)) => a == b,
        _ => a == b,
    }
}

impl Model {
    /// The identifier used to load the model.
    pub fn source(&self) -> &str {
        &self.source
    }

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

    memory::select_best_gpu().is_some()
}

/// Load a GGUF model from disk.
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
pub fn get_model(
    model_path: &str,
    use_gpu_if_available: bool,
    mmproj_path: Option<&str>,
    draft_model_path: Option<&str>,
    progress: Option<DownloadProgressCallback>,
) -> Result<Model, LoadModelError> {
    get_model_cancellable(
        model_path,
        use_gpu_if_available,
        mmproj_path,
        draft_model_path,
        progress,
        None,
    )
}

#[tracing::instrument(level = "info", skip(progress, cancellation))]
pub fn get_model_cancellable(
    model_path: &str,
    use_gpu_if_available: bool,
    mmproj_path: Option<&str>,
    draft_model_path: Option<&str>,
    progress: Option<DownloadProgressCallback>,
    cancellation: Option<DownloadCancellationCallback>,
) -> Result<Model, LoadModelError> {
    if model_path == "auto" && mmproj_path.is_some() {
        return Err(LoadModelError::InvalidModel(
            "Automatic model selection does not support projection models; pass an explicit multimodal model path"
                .to_string(),
        ));
    }

    let source = model_path.to_owned();
    let use_gpu = use_gpu_if_available && has_gpu_backend();
    let model_path = model_selection::resolve_model_path(model_path, use_gpu)?;
    let model_progress = progress
        .clone()
        .unwrap_or_else(|| default_progress_callback(model_path));
    let real_model_path = download_gguf(
        parse_model_path(model_path)?,
        &model_progress,
        &[],
        cancellation.as_ref(),
    )?;
    let real_mmproj_path = match mmproj_path {
        Some(p) => {
            let mmproj_progress = progress
                .clone()
                .unwrap_or_else(|| default_progress_callback(p));
            Some(download_gguf(
                parse_model_path(p)?,
                &mmproj_progress,
                &[],
                cancellation.as_ref(),
            )?)
        }
        None => None,
    };
    let real_draft_model_path = match draft_model_path {
        Some(p) => {
            let draft_progress = progress
                .clone()
                .unwrap_or_else(|| default_progress_callback(p));
            Some(download_gguf(
                parse_model_path(p)?,
                &draft_progress,
                &[],
                cancellation.as_ref(),
            )?)
        }
        None => None,
    };

    let loading_plan =
        memory::plan_model_loading(&real_model_path, real_mmproj_path.as_deref(), use_gpu);
    let gpu_layers = loading_plan.gpu_layers;
    for warning in &loading_plan.warnings {
        warn!("{}", warning);
    }

    info!(use_gpu = use_gpu, gpu_layers = gpu_layers, "Loading model");

    // llama.cpp skips bundled MTP layers unless asked, so load them with the target instead of twice.
    let mtp_bundled = real_draft_model_path
        .as_deref()
        .is_some_and(|p| same_file(p, &real_model_path));
    let mut model_params = LlamaModelParams::default()
        .with_n_gpu_layers(gpu_layers)
        .with_load_mtp(mtp_bundled);

    if cfg!(target_os = "emscripten") {
        // Emscripten doesn't support partial unmapping, so if we try to use
        // mmap there, it leaks a _lot_.
        model_params = model_params.with_use_mmap(false);
    }

    if cfg!(target_os = "android") {
        let device = if use_gpu && gpu_layers > 0 {
            memory::select_best_gpu()
        } else {
            None
        };
        if let Some(device) = &device {
            info!(backend = %device.backend, device = %device.name, gpu_layers, "Selected Android GPU");
        }
        // An empty list explicitly selects CPU. Do not let llama.cpp combine
        // OpenCL and Vulkan devices backed by the same physical GPU.
        let devices: Vec<_> = device.into_iter().map(|d| d.index).collect();
        model_params = model_params.with_devices(&devices).map_err(|e| {
            LoadModelError::InvalidModel(format!("Failed to select Android GPU: {e}"))
        })?;
    }

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
    // mtmd's current API chooses its own GPU; retain CPU projection on Android
    // until it can use the text model's selected device too.
    let projection_model = real_mmproj_path
        .as_ref()
        .map(|path| {
            ProjectionModel::from_path(
                path,
                &language_model,
                use_gpu && !cfg!(target_os = "android"),
            )
        })
        .transpose()?
        .map(Mutex::new);

    let draft_model = match real_draft_model_path.as_ref() {
        None => None,
        Some(_) if mtp_bundled => {
            info!("Using MTP layers bundled in the target model");
            Some(DraftModel::Bundled)
        }
        Some(path) => {
            info!(path = %path.display(), "Loading MTP draft model");
            let draft_params = pin!(LlamaModelParams::default()
                .with_n_gpu_layers(gpu_layers)
                .with_load_mtp(true));
            let draft =
                LlamaModel::load_from_file(&LLAMA_BACKEND, path, &draft_params).map_err(|e| {
                    let error_msg = format!(
                        "Failed to load MTP draft model at {}: {}",
                        path.display(),
                        e
                    );
                    error!(error = %error_msg, "Failed to load MTP draft model");
                    LoadModelError::InvalidModel(error_msg)
                })?;
            Some(DraftModel::Separate(draft))
        }
    };

    Ok(Model {
        source,
        language_model,
        projection_model,
        draft_model,
    })
}

pub fn download_model(
    model_path: &str,
    headers: Vec<(String, String)>,
    progress: Option<DownloadProgressCallback>,
) -> Result<std::path::PathBuf, LoadModelError> {
    download_model_cancellable(model_path, headers, progress, None)
}

pub fn download_model_cancellable(
    model_path: &str,
    headers: Vec<(String, String)>,
    progress: Option<DownloadProgressCallback>,
    cancellation: Option<DownloadCancellationCallback>,
) -> Result<std::path::PathBuf, LoadModelError> {
    let progress = progress.unwrap_or_else(|| default_progress_callback(model_path));
    download_gguf(
        parse_model_path(model_path)?,
        &progress,
        &headers,
        cancellation.as_ref(),
    )
}

pub type WriteOutput =
    crate::stream::StreamOutput<Box<dyn miette::Diagnostic + Send + Sync + 'static>>;

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    #[test]
    fn model_keeps_its_source() {
        let model = crate::test_utils::load_test_model();
        let source = std::env::var("TEST_MODEL").unwrap_or_else(|_| "model.gguf".to_string());
        assert_eq!(model.source(), source);
    }

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
