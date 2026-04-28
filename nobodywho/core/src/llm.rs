use crate::errors::{InitWorkerError, LoadModelError, ReadError};
use crate::memory;
use crate::tokenizer::{ProjectionModel, Tokenizer, TokenizerChunk, TokenizerChunks};
use indicatif::{ProgressBar, ProgressStyle};
use lazy_static::lazy_static;
use llama_cpp_2::context::kv_cache::KvCacheConversionError;
use llama_cpp_2::context::params::{LlamaContextParams, LlamaPoolingType};
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::AddBos;
use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::mtmd::MtmdInputChunks;
use llama_cpp_2::token::LlamaToken;
use std::io::{Read, Write};
use std::pin::pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex, MutexGuard};
use std::time::Duration;
use tracing::{debug, debug_span, error, info, info_span, warn};

#[derive(Debug)]
pub(crate) struct GlobalInferenceLockToken;
lazy_static! {
    pub(crate) static ref GLOBAL_INFERENCE_LOCK: Mutex<GlobalInferenceLockToken> =
        Mutex::new(GlobalInferenceLockToken);
}

static LLAMA_BACKEND: LazyLock<LlamaBackend> =
    LazyLock::new(|| LlamaBackend::init().expect("Failed to initialize llama backend"));

/// Callback invoked during model downloads with `(downloaded_bytes, total_bytes)`.
///
/// Invoked on each read chunk from the single download thread. If the same callback
/// is shared across concurrent downloads, the closure is responsible for its own
/// synchronization (hence the `Sync` bound).
pub type DownloadProgressCallback = Arc<dyn Fn(u64, u64) + Send + Sync>;

/// Default terminal progress bar shown when the user doesn't pass their own callback.
///
/// Renders an `indicatif` bar with spinner, elapsed time, wide bar, binary byte counts,
/// throughput, and ETA. indicatif auto-disables on non-TTY stderr, so this is safe to use
/// unconditionally — GUI bindings (Godot, Flutter mobile) won't see output in production.
/// Detects a new download (model → mmproj transition) by watching for `total` to change,
/// finishes the previous bar, and starts a fresh one.
pub fn default_progress_callback() -> DownloadProgressCallback {
    let style = ProgressStyle::with_template(
        "{spinner:.green} [{elapsed_precise}] {wide_bar:.cyan/blue} \
         {binary_bytes}/{binary_total_bytes} ({binary_bytes_per_sec}, {eta})",
    )
    .expect("static progress bar template is valid")
    .progress_chars("█▉▊▋▌▍▎▏ ");

    let state: Arc<Mutex<(Option<ProgressBar>, u64)>> = Arc::new(Mutex::new((None, 0)));

    Arc::new(move |downloaded: u64, total: u64| {
        let mut s = state.lock().expect("progress bar mutex poisoned");
        if s.0.is_none() || s.1 != total {
            if let Some(old) = s.0.take() {
                old.finish_and_clear();
            }
            let bar = ProgressBar::new(total);
            bar.set_style(style.clone());
            bar.enable_steady_tick(Duration::from_millis(100));
            s.0 = Some(bar);
            s.1 = total;
        }
        let bar = s.0.as_ref().unwrap();
        bar.set_position(downloaded);
        if downloaded >= total {
            bar.finish();
            s.0 = None;
        }
    })
}

/// Wrap a progress callback so it fires at most ~10 Hz, with a guaranteed
/// final emit on completion.
///
/// Use this when each invocation of the user-provided callback is expensive —
/// typically because it crosses a language boundary (Dart isolate hop, JSI
/// hop, etc.). Without throttling, a fast download (thousands of chunks/sec)
/// would saturate the cross-language bridge. The Python binding does NOT need
/// this since a PyO3 callable invocation is cheap; it forwards every chunk.
///
/// Implementation: lock-free `AtomicU64` holding nanoseconds since a process-wide
/// epoch. The load/check/store has a benign race (two concurrent emitters could
/// both decide to emit within the same window) but `download_file` is
/// single-threaded per download, and an extra emit is harmless. Emits if
/// `now - last >= 100ms` or if `total > 0 && downloaded >= total` (completion,
/// so the UI never sticks at 99%). 0 is the "never emitted" sentinel.
pub fn throttled_progress_callback<F>(callback: F) -> DownloadProgressCallback
where
    F: Fn(u64, u64) + Send + Sync + 'static,
{
    static EPOCH: LazyLock<std::time::Instant> = LazyLock::new(std::time::Instant::now);
    const THROTTLE_NS: u64 = 100_000_000;

    let last_emit_ns = Arc::new(AtomicU64::new(0));
    Arc::new(move |downloaded: u64, total: u64| {
        let is_done = total > 0 && downloaded >= total;
        let now_ns = EPOCH.elapsed().as_nanos() as u64;
        let prev = last_emit_ns.load(Ordering::Relaxed);
        let due = prev == 0 || now_ns.saturating_sub(prev) >= THROTTLE_NS;
        if is_done || due {
            last_emit_ns.store(now_ns, Ordering::Relaxed);
            callback(downloaded, total);
        }
    })
}

#[derive(Debug)]
pub struct Model {
    pub(crate) language_model: LlamaModel,
    pub(crate) projection_model: Option<ProjectionModel>,
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

#[derive(Clone)]
enum ParsedModelPath {
    HuggingFaceUrl(String, String, String), // e.g. hf://owner/repo/model.gguf -> (owner, repo, filename)
    HttpUrl(String),                        // e.g. https://example.com/lol/qwen3.gguf
    FilesystemPath(std::path::PathBuf),     // e.g. ./qwen3.gguf
}

fn parse_model_path(
    model_path: &str,
) -> Result<ParsedModelPath, nom::Err<nom::error::Error<String>>> {
    use nom::branch::alt;
    use nom::bytes::complete::{tag, tag_no_case, take_until};
    use nom::combinator::{cut, map, rest, verify};
    use nom::sequence::{preceded, terminated};
    use nom::Parser;

    let mut parser = alt((
        // hf://owner/repo/filename.gguf (also hf:, huggingface:, huggingface://)
        map(
            preceded(
                alt((
                    tag_no_case("huggingface://"),
                    tag_no_case("huggingface:"),
                    tag_no_case("hf://"),
                    tag_no_case("hf:"),
                )),
                cut((
                    terminated(take_until("/"), tag("/")),
                    terminated(take_until("/"), tag("/")),
                    verify(rest, |s: &str| !s.is_empty()),
                )),
            ),
            |(owner, repo, filename): (&str, &str, &str)| {
                ParsedModelPath::HuggingFaceUrl(owner.into(), repo.into(), filename.into())
            },
        ),
        // https://... or http://...
        map(
            (alt((tag_no_case("https://"), tag_no_case("http://"))), rest),
            |(scheme, path): (&str, &str)| ParsedModelPath::HttpUrl(format!("{}{}", scheme, path)),
        ),
        // Anything else is a filesystem path
        map(rest, |p: &str| {
            ParsedModelPath::FilesystemPath(std::path::PathBuf::from(p))
        }),
    ));
    let result: nom::IResult<&str, ParsedModelPath> = parser.parse(model_path);
    result
        .map(|(_, parsed)| parsed)
        .map_err(|e| e.map(|e| e.cloned()))
}

/// takes a fancy path (possibly with hf: or https:// in front), and resolve it to a realized path
/// on the filesystem
fn resolve_fancy_path_to_fs(
    parsed_path: ParsedModelPath,
    progress: &DownloadProgressCallback,
) -> Result<std::path::PathBuf, LoadModelError> {
    let fs_model_path = match parsed_path {
        ParsedModelPath::HuggingFaceUrl(owner, repo, filename) => {
            download_model_from_hf(&owner, &repo, &filename, progress)?
        }
        ParsedModelPath::FilesystemPath(path) => path,
        ParsedModelPath::HttpUrl(url) => download_model_from_url(&url, progress)?,
    };

    if !fs_model_path.exists() {
        let e = LoadModelError::ModelNotFound(fs_model_path.to_string_lossy().into());
        error!(error = %e, "Model file not found");
        return Err(e);
    }

    Ok(fs_model_path)
}

#[tracing::instrument(level = "info", skip(progress))]
pub fn get_model(
    model_path: &str,
    use_gpu_if_available: bool,
    mmproj_path: Option<&str>,
    progress: Option<DownloadProgressCallback>,
) -> Result<Model, LoadModelError> {
    let progress = progress.unwrap_or_else(default_progress_callback);
    let real_model_path = resolve_fancy_path_to_fs(parse_model_path(model_path)?, &progress)?;
    let real_mmproj_path = mmproj_path
        .map(parse_model_path) // parse inside option
        .transpose()? // return early if parse fails
        .map(|p| resolve_fancy_path_to_fs(p, &progress)) // download the file if needed
        .transpose()?; // return early if download fails

    // TODO: `LlamaModelParams` uses all devices by default. Set it to an empty list once an upstream device API is available.
    let use_gpu = use_gpu_if_available && has_gpu_backend();
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

    Ok(Model {
        language_model,
        projection_model,
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
/// * `model_path` - Path to the GGUF model file
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
    progress: Option<DownloadProgressCallback>,
) -> Result<Model, LoadModelError> {
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(4096);
    std::thread::spawn(move || {
        output_tx.blocking_send(get_model(
            &model_path,
            use_gpu_if_available,
            mmproj_path.as_deref(),
            progress,
        ))
    });

    match output_rx.recv().await {
        Some(model) => return model,
        None => Err(LoadModelError::ModelChannelError),
    }
}

/// Get the cache directory for downloaded models.
///
/// On Android, the package name is read from `/proc/self/cmdline` and the user ID
/// is derived from the UID (`uid / 100000`). This avoids needing JNI or an Android
/// Context object, which isn't reliably available — Flutter loads native libraries
/// via `dlopen` (not `System.loadLibrary`), so `JNI_OnLoad` is never called.
///
/// On other platforms, uses the `dirs` crate to find the standard cache directory.
fn get_cache_dir() -> Result<std::path::PathBuf, crate::errors::LoadModelError> {
    let base = get_platform_cache_dir()?;
    Ok(base.join("nobodywho").join("models"))
}

#[cfg(target_os = "android")]
fn get_platform_cache_dir() -> Result<std::path::PathBuf, crate::errors::LoadModelError> {
    // Read the package name from /proc/self/cmdline. This file contains the process
    // name as a null-terminated string. On Android this is the package name
    // (e.g. "com.example.app"), possibly with a colon suffix for multi-process apps
    // (e.g. "com.example.app:remote").
    let cmdline = std::fs::read("/proc/self/cmdline").map_err(|e| {
        crate::errors::LoadModelError::DownloadError(format!(
            "Failed to read /proc/self/cmdline: {e}"
        ))
    })?;

    let package_name = cmdline
        .split(|&b| b == 0)
        .next()
        .and_then(|bytes| std::str::from_utf8(bytes).ok())
        .map(|s| s.split(':').next().unwrap_or(s))
        .ok_or_else(|| {
            crate::errors::LoadModelError::DownloadError(
                "Could not determine Android package name from /proc/self/cmdline".into(),
            )
        })?;

    // Derive the Android user ID from the Unix UID. Android assigns UIDs as:
    //   uid = user_id * 100000 + app_id
    // This gives the correct path on multi-user devices (e.g. GrapheneOS work
    // profiles), where /data/data/ is a symlink only valid for user 0.
    let uid = unsafe { libc::getuid() };
    let user_id = uid / 100000;

    Ok(std::path::PathBuf::from(format!(
        "/data/user/{user_id}/{package_name}/cache"
    )))
}

#[cfg(not(target_os = "android"))]
fn get_platform_cache_dir() -> Result<std::path::PathBuf, crate::errors::LoadModelError> {
    dirs::cache_dir().ok_or_else(|| {
        crate::errors::LoadModelError::DownloadError("Could not determine cache directory".into())
    })
}

/// Download a file from a URL to a local path, streaming to disk with progress logging.
///
/// Returns early if the file already exists at the target path.
/// Rejects paths containing `..` to prevent path traversal attacks.
fn download_file(
    url: &str,
    target_path: &std::path::Path,
    progress: &DownloadProgressCallback,
) -> Result<(), crate::errors::LoadModelError> {
    for component in target_path.components() {
        if component == std::path::Component::ParentDir {
            return Err(crate::errors::LoadModelError::DownloadError(
                "Path traversal detected: '..' is not allowed in model paths".into(),
            ));
        }
    }

    if target_path.exists() {
        info!("Using cached file: {}", target_path.display());
        return Ok(());
    }

    // Create parent directories
    if let Some(parent) = target_path.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            crate::errors::LoadModelError::DownloadError(format!(
                "Failed to create cache directory {}: {e}",
                parent.display()
            ))
        })?;
    }

    info!("Downloading {} -> {}", url, target_path.display());

    let response = ureq::get(url).call().map_err(|e| {
        crate::errors::LoadModelError::DownloadError(format!("HTTP request failed: {e}"))
    })?;

    let content_length: std::num::NonZeroU64 = response
        .headers()
        .get("content-length")
        .and_then(|v| v.to_str().ok())
        .and_then(|v| v.parse::<std::num::NonZeroU64>().ok())
        .ok_or_else(|| {
            crate::errors::LoadModelError::DownloadError(format!(
                "Server returned missing or zero Content-Length for {url}"
            ))
        })?;

    info!(
        "Download size: {:.1} GB",
        content_length.get() as f64 / 1_073_741_824.0
    );

    // Write to a temp file first, then rename — avoids partial files on failure.
    let tmp_path = target_path.with_file_name(format!(
        "{}.{:x}.part",
        target_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy(),
        rand::random::<u32>(),
    ));

    let download_result: Result<(), crate::errors::LoadModelError> = (|| {
        let mut file = std::fs::File::create(&tmp_path).map_err(|e| {
            crate::errors::LoadModelError::DownloadError(format!(
                "Failed to create temp file {}: {e}",
                tmp_path.display()
            ))
        })?;

        let body = response.into_body();
        let mut reader = body.into_reader();
        let mut downloaded: u64 = 0;
        let mut last_logged_pct: u64 = 0;
        let mut buf = vec![0u8; 256 * 1024]; // 256 KB chunks

        loop {
            let n = reader.read(&mut buf).map_err(|e| {
                crate::errors::LoadModelError::DownloadError(format!(
                    "Read error during download: {e}"
                ))
            })?;
            if n == 0 {
                break;
            }
            file.write_all(&buf[..n]).map_err(|e| {
                crate::errors::LoadModelError::DownloadError(format!(
                    "Write error during download: {e}"
                ))
            })?;
            downloaded += n as u64;

            progress(downloaded, content_length.get());

            let pct = (downloaded * 100) / content_length;
            if pct >= last_logged_pct + 5 {
                info!(
                    "Download progress: {pct}% ({downloaded}/{} bytes)",
                    content_length
                );
                last_logged_pct = pct;
            }
        }
        if downloaded != content_length.get() {
            return Err(crate::errors::LoadModelError::DownloadError(format!(
                "Download incomplete: got {downloaded}/{} bytes",
                content_length
            )));
        }
        Ok(())
    })();

    if download_result.is_err() {
        if let Err(e) = std::fs::remove_file(&tmp_path) {
            warn!("Failed to clean up temp file {}: {e}", tmp_path.display());
        }
        return download_result;
    }

    // Rename temp file to final path
    std::fs::rename(&tmp_path, target_path).map_err(|e| {
        crate::errors::LoadModelError::DownloadError(format!(
            "Failed to rename temp file to {}: {e}",
            target_path.display()
        ))
    })?;

    info!("Download complete: {}", target_path.display());
    Ok(())
}

/// Download a GGUF model from HuggingFace Hub and return the local path to it.
///
/// If the model is already cached locally, the cached path is returned without downloading.
fn download_model_from_hf(
    owner: &str,
    repo: &str,
    filename: &str,
    progress: &DownloadProgressCallback,
) -> Result<std::path::PathBuf, crate::errors::LoadModelError> {
    let cache_dir = get_cache_dir()?;
    let target_path = cache_dir.join(owner).join(repo).join(filename);
    let url = format!("https://huggingface.co/{owner}/{repo}/resolve/main/{filename}");
    download_file(&url, &target_path, progress)?;
    Ok(target_path)
}

/// Download a model from a generic HTTP(S) URL and return the local path to it.
///
/// The file is cached by its URL path components under the cache directory.
fn download_model_from_url(
    url: &str,
    progress: &DownloadProgressCallback,
) -> Result<std::path::PathBuf, crate::errors::LoadModelError> {
    let cache_dir = get_cache_dir()?;
    // Derive a cache path from the URL: strip scheme, use the rest as path components
    let path_part = url
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    let target_path = cache_dir.join("http").join(path_part);
    download_file(url, &target_path, progress)?;
    Ok(target_path)
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
    pub(crate) n_past: i32,
    pub(crate) ctx: LlamaContext<'a>,
    pub(crate) big_batch: LlamaBatch<'a>,
    pub(crate) small_batch: LlamaBatch<'a>,
    pub(crate) projection_model: Option<&'a ProjectionModel>,
    pub(crate) tokenizer: Tokenizer<'a>,

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
        model: &'a Model,
        n_ctx: u32,
        use_embeddings: bool,
        extra: T,
    ) -> Result<Worker<'a, T>, InitWorkerError> {
        info!("Initializing worker");

        let projection_model = model.projection_model.as_ref();

        // Set up context parameters using available parallelism
        let ctx = {
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
            let n_ctx = ctx_plan.n_ctx;
            let n_ubatch = ctx_plan.n_ubatch;
            for w in &ctx_plan.warnings {
                warn!("{}", w);
            }

            let ctx_params = LlamaContextParams::default()
                .with_n_ctx(std::num::NonZero::new(n_ctx))
                .with_n_batch(n_ctx) // n_batch sets the max size of a batch (i.e. max prompt size)
                .with_n_ubatch(n_ubatch)
                .with_n_threads(n_threads)
                .with_n_threads_batch(n_threads)
                .with_embeddings(use_embeddings)
                .with_pooling_type(extra.pooling_type());

            // Create inference context and sampler
            model
                .language_model
                .new_context(&LLAMA_BACKEND, ctx_params)?
        };

        let big_batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);
        let small_batch = LlamaBatch::new(1, 1);

        let add_bos = read_add_bos_metadata(&model.language_model)?;
        debug!(?add_bos, "Read add_bos from GGUF metadata:");

        let tokenizer = Tokenizer::new(&model.language_model, projection_model, add_bos);

        let state = Worker {
            n_past: 0,
            ctx,
            big_batch,
            small_batch,
            projection_model,
            extra,
            tokenizer,
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
        let chunks = self.tokenizer.tokenize(text, vec![])?;
        self.read_chunks(chunks, &inference_lock_token)
    }

    pub fn read_chunks(
        &mut self,
        chunks: TokenizerChunks,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, ReadError> {
        for chunk in chunks.into_iter() {
            match chunk {
                TokenizerChunk::Text(tokens, _) => {
                    self.read_text_tokens(tokens, inference_lock_token)?;
                }
                TokenizerChunk::Image(embeddings, _) | TokenizerChunk::Audio(embeddings, _) => {
                    self.read_media_embeddings(embeddings, inference_lock_token)?;
                }
            }
        }

        Ok(self)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    fn read_media_embeddings(
        &mut self,
        embeddings: Rc<MtmdInputChunks>,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, ReadError> {
        let projection_model = self
            .projection_model
            .as_ref()
            .ok_or(ReadError::ProjectionModelNotInitialized)?;

        let n_tokens = embeddings.as_ref().total_tokens();
        debug!(n_tokens, "Reading media embeddings:");

        let decode_span = debug_span!("read media embeddings", n_tokens = n_tokens);
        let decode_guard = decode_span.enter();
        let n_ctx = self.ctx.n_ctx() as i32;
        self.n_past = embeddings.eval_chunks(
            &projection_model.ctx,
            &self.ctx,
            self.n_past,
            0,
            n_ctx,
            true,
        )?;

        drop(decode_guard);
        debug!(
            "Completed read media embeddings operation, n_past: {}",
            self.n_past
        );

        Ok(self)
    }

    // ---------- IMPORTANT ----------
    // Should only be used under a global inference lock
    // This is a safety meassure to prevent bugs from multiple
    // contexts with the same model. It might not be necessary
    // but assume it is.
    #[tracing::instrument(level = "trace", skip(self))]
    fn read_text_tokens(
        &mut self,
        tokens: Vec<LlamaToken>,
        inference_lock_token: &MutexGuard<'_, GlobalInferenceLockToken>,
    ) -> Result<&mut Self, ReadError> {
        let n_tokens = tokens.len();
        debug!(n_tokens, "Reading tokens:");

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

        debug!("Completed read tokens operation, n_past: {}", self.n_past);

        Ok(self)
    }

    #[tracing::instrument(level = "trace", skip(self))]
    pub fn remove_all_tokens_from_index_from_ctx(
        &mut self,
        index: usize,
    ) -> Result<(), KvCacheConversionError> {
        if self.n_past <= index as i32 {
            return Ok(());
        }

        let seq_rm_success = self
            .ctx
            .clear_kv_cache_seq(Some(0), Some(index as u32), None)?;

        if seq_rm_success {
            self.n_past = index as i32;
        } else {
            // Partial sequence removal is not supported by this model's memory type
            // (e.g. hybrid models with recurrent components). Fall back to full reset.
            warn!(
                index,
                n_past = self.n_past,
                "Partial KV cache removal not supported, falling back to full context reset"
            );
            self.reset_context();
        }

        Ok(())
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
        assert!(n >= 1 && n <= 5, "expected 1–5 emits, got {}", n);
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
