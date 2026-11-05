use crate::errors::LoadModelError;
use crate::huggingface::{default_progress_callback, ModelCache};
use crate::memory;
use crate::model_selection;
use crate::tokenizer::ProjectionModel;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::LlamaModel;
use std::path::PathBuf;
use std::sync::{LazyLock, Mutex};
use tracing::{error, info, info_span, warn};

// Back-compat re-exports: bindings (Python, Godot, Flutter) import these via
// `nobodywho::llm::*`. The implementations now live in `crate::huggingface`.
pub use crate::huggingface::{
    get_cached_models, parse_model_path, throttled_progress_callback, DownloadCancellationCallback,
    DownloadProgressCallback, GgufSource, RemoteFile,
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

/// Helper for parsing, downloading and loading models.
///
/// This is split into three steps internally, which allows downloading to be
/// done by the language bindings instead of Rust.
pub fn get_model(
    model_path: &str,
    use_gpu_if_available: bool,
    projection_model_path: Option<&str>,
    draft_model_path: Option<&str>,
    progress: Option<DownloadProgressCallback>,
) -> Result<Model, LoadModelError> {
    let sources = parse_model_paths(
        model_path,
        projection_model_path,
        draft_model_path,
        use_gpu_if_available,
    )?;
    let paths = download_models(&sources, progress, None)?;
    let model = load_model(model_path.to_string(), &paths, use_gpu_if_available)?;
    Ok(model)
}

/// Parse model paths such that they are ready to be downloaded (if necessary).
///
/// If `model_path` is `auto`, this will attempt to figure out the model based
/// for memory-based LLM selection.
#[tracing::instrument(level = "debug")]
pub fn parse_model_paths(
    language_model_path: &str,
    projection_model_path: Option<&str>,
    draft_model_path: Option<&str>,
    use_gpu_if_available: bool,
) -> Result<ModelSources, LoadModelError> {
    if language_model_path == "auto" && projection_model_path.is_some() {
        let msg = "Automatic model selection does not support projection models; pass an explicit multimodal model path";
        return Err(LoadModelError::InvalidModel(msg.to_string()));
    }

    let use_gpu = use_gpu_if_available && has_gpu_backend();
    let language_model_path = model_selection::resolve_model_path(language_model_path, use_gpu)?;

    let language_model = parse_model_path(language_model_path)?;
    let projection_model = projection_model_path.map(parse_model_path).transpose()?;
    let draft_model = draft_model_path.map(parse_model_path).transpose()?;

    Ok(ModelSources {
        language_model,
        projection_model,
        draft_model,
    })
}

/// Sources for a model and its associated projection and draft models.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelSources {
    pub language_model: GgufSource,
    pub projection_model: Option<GgufSource>,
    pub draft_model: Option<GgufSource>,
}

/// Download models into the cache (if necessary).
#[tracing::instrument(level = "debug", skip(progress, cancellation))]
pub fn download_models(
    sources: &ModelSources,
    progress: Option<DownloadProgressCallback>,
    cancellation: Option<DownloadCancellationCallback>,
) -> Result<ModelPaths, LoadModelError> {
    let progress = progress.unwrap_or_else(default_progress_callback);
    let language_model = match &sources.language_model {
        GgufSource::Remote(remote) => {
            let cache = ModelCache::open()?;
            cache.download_file(remote, &progress, &[], cancellation.as_ref())?
        }
        GgufSource::Local(path) => path.to_owned(),
    };
    let projection_model = match &sources.projection_model {
        Some(GgufSource::Remote(remote)) => {
            let cache = ModelCache::open()?;
            Some(cache.download_file(remote, &progress, &[], cancellation.as_ref())?)
        }
        Some(GgufSource::Local(path)) => Some(path.to_owned()),
        None => None,
    };
    let draft_model = match &sources.draft_model {
        Some(GgufSource::Remote(remote)) => {
            let cache = ModelCache::open()?;
            Some(cache.download_file(remote, &progress, &[], cancellation.as_ref())?)
        }
        Some(GgufSource::Local(path)) => Some(path.to_owned()),
        None => None,
    };
    Ok(ModelPaths {
        language_model,
        projection_model,
        draft_model,
    })
}

/// Paths to a model and its associated projection and draft models.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ModelPaths {
    pub language_model: PathBuf,
    pub projection_model: Option<PathBuf>,
    pub draft_model: Option<PathBuf>,
}

/// Load a [`Model`] from files on disk.
///
/// Set `use_gpu_if_available` to attempt GPU acceleration if a discrete GPU
/// is available.
///
/// # MTP speculative decoding
///
/// Pass `paths.draft_model` pointing to a compatible MTP heads gguf (e.g.
/// `mtp-gemma-4-E2B-it.gguf` for Gemma-4-E2B) to enable MTP speculative
/// decoding on chats built from this model.
///
/// For models that bundle MTP layers (e.g. Qwen3.5), pass the model file
/// itself; it is only loaded once. Whether MTP is actually used is a per-chat
/// decision — pass it through `Chat`-level config on the wrapping binding.
///
/// # Errors
///
/// Returns an error if the model is not found, is invalid or is unsupported.
pub fn load_model(
    source: String,
    paths: &ModelPaths,
    use_gpu_if_available: bool,
) -> Result<Model, LoadModelError> {
    LoadModelError::validate_model_file(&paths.language_model)?;
    if let Some(path) = &paths.projection_model {
        LoadModelError::validate_model_file(path)?;
    }
    if let Some(path) = &paths.draft_model {
        LoadModelError::validate_model_file(path)?;
    }

    let use_gpu = use_gpu_if_available && has_gpu_backend();
    let loading_plan = memory::plan_model_loading(
        &paths.language_model,
        paths.projection_model.as_deref(),
        use_gpu,
    );
    let gpu_layers = loading_plan.gpu_layers;
    for warning in &loading_plan.warnings {
        warn!("{}", warning);
    }

    info!(use_gpu = use_gpu, gpu_layers = gpu_layers, "Loading model");

    // llama.cpp skips bundled MTP layers unless asked, so load them with the target instead of twice.
    let mtp_bundled = paths
        .draft_model
        .as_deref()
        .is_some_and(|p| same_file(p, &paths.language_model));
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

    let span = info_span!("load model", path = ?paths.language_model).entered();
    let language_model =
        LlamaModel::load_from_file(&LLAMA_BACKEND, &paths.language_model, &model_params).map_err(
            |e| {
                if e.to_string().contains("null result") {
                    return LoadModelError::ModelLoadFailed {
                        path: paths.language_model.display().to_string(),
                    };
                }
                let error_msg = format!(
                    "Bad model path: {} - Llama.cpp error: {}",
                    paths.language_model.display(),
                    e
                );
                error!(error = %error_msg, "Failed to load model");
                LoadModelError::InvalidModel(error_msg)
            },
        )?;
    drop(span);

    info!("Model loaded successfully");
    // mtmd's current API chooses its own GPU; retain CPU projection on Android
    // until it can use the text model's selected device too.
    let projection_model = paths
        .projection_model
        .as_deref()
        .map(|path| {
            let _span =
                info_span!("load projection model", path = ?paths.projection_model).entered();
            ProjectionModel::from_path(
                path,
                &language_model,
                use_gpu && !cfg!(target_os = "android"),
            )
        })
        .transpose()?
        .map(Mutex::new);

    let draft_model = match &paths.draft_model {
        None => None,
        Some(_) if mtp_bundled => {
            info!("Using MTP layers bundled in the target model");
            Some(DraftModel::Bundled)
        }
        Some(path) => {
            let _span = info_span!("load draft model", path = ?paths.draft_model).entered();
            let draft_params = LlamaModelParams::default()
                .with_n_gpu_layers(gpu_layers)
                .with_load_mtp(true);
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

/// Helper to allow downloading and caching models with custom headers.
pub fn download_model(
    remote: &RemoteFile,
    headers: Vec<(String, String)>,
    progress: Option<DownloadProgressCallback>,
    cancellation: Option<DownloadCancellationCallback>,
) -> Result<PathBuf, LoadModelError> {
    ModelCache::open()?.download_file(
        &remote,
        &progress.unwrap_or_else(default_progress_callback),
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
        let result = parse_model_paths("auto", Some("projection.gguf"), None, true);
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
            cb("https://example.com/example.gguf", i, 10_000);
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
        cb("https://example.com/example.gguf", 50, 100);
        cb("https://example.com/example.gguf", 100, 100);
        assert_eq!(count.load(Ordering::Relaxed), 2);
    }
}
