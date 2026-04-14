use crate::errors::{InitWorkerError, LoadModelError, ReadError};
use crate::memory;
use crate::tokenizer::{ProjectionModel, Tokenizer, TokenizerChunk, TokenizerChunks};
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
use std::pin::pin;
use std::rc::Rc;
use std::sync::atomic::{AtomicBool, Ordering};
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
    HuggingFaceUrl(String, String, String), // e.g. hf://owner/repo/model.gguf -> (repo_id, filename)
    HttpUrl(String),                        // e.g. https://example.com/lol/qwen3.gguf
    FilesystemPath(std::path::PathBuf),     // e.g. ./qwen3.gguf
}

fn parse_model_path(
    model_path: &str,
) -> Result<ParsedModelPath, nom::Err<nom::error::Error<String>>> {
    use nom::branch::alt;
    use nom::bytes::complete::{tag, take_until};
    use nom::combinator::{map, rest};
    use nom::sequence::{preceded, terminated};
    use nom::Parser;

    let mut parser = alt((
        // hf://owner/repo/filename.gguf
        map(
            preceded(
                tag("hf://"),
                (
                    terminated(take_until("/"), tag("/")),
                    terminated(take_until("/"), tag("/")),
                    rest,
                ),
            ),
            |(owner, repo, filename): (&str, &str, &str)| {
                ParsedModelPath::HuggingFaceUrl(owner.into(), repo.into(), filename.into())
            },
        ),
        // https://... or http://...
        map(
            (alt((tag("https://"), tag("http://"))), rest),
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
) -> Result<std::path::PathBuf, LoadModelError> {
    let fs_model_path = match parsed_path {
        ParsedModelPath::HuggingFaceUrl(owner, repo, filename) => {
            download_model_from_hf(&owner, &repo, &filename)?
        }
        ParsedModelPath::FilesystemPath(path) => path,
        ParsedModelPath::HttpUrl(_url) => {
            todo!()
        }
    };

    if !fs_model_path.exists() {
        let e = LoadModelError::ModelNotFound(fs_model_path.to_string_lossy().into());
        error!(error = %e, "Model file not found");
        return Err(e);
    }

    Ok(fs_model_path)
}

#[tracing::instrument(level = "info")]
pub fn get_model(
    model_path: &str,
    use_gpu_if_available: bool,
    mmproj_path: Option<&str>,
) -> Result<Model, LoadModelError> {
    let real_model_path = resolve_fancy_path_to_fs(parse_model_path(model_path)?)?;
    let real_mmproj_path = mmproj_path
        .map(parse_model_path) // parse inside option
        .transpose()? // return early if parse fails
        .map(resolve_fancy_path_to_fs) // download the file if needed
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
    let projection_model = mmproj_path
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
#[tracing::instrument(level = "info")]
pub async fn get_model_async(
    model_path: String,
    use_gpu_if_available: bool,
    mmproj_path: Option<String>,
) -> Result<Model, LoadModelError> {
    let (output_tx, mut output_rx) = tokio::sync::mpsc::channel(4096);
    std::thread::spawn(move || {
        output_tx.blocking_send(get_model(
            &model_path,
            use_gpu_if_available,
            mmproj_path.as_deref(),
        ))
    });

    match output_rx.recv().await {
        Some(model) => return model,
        None => Err(LoadModelError::ModelChannelError),
    }
}

/// Download a GGUF model from HuggingFace Hub and return the local path to it.
///
/// If the model is already cached locally, the cached path is returned without downloading.
///
/// The `model_id` argument can be any of the following formats:
/// - Shorthand: `"owner/repo/filename.gguf"`
/// - Resolve path: `"owner/repo/resolve/branch/filename.gguf"`
/// - With domain: `"huggingface.co/owner/repo/resolve/branch/filename.gguf"`
/// - Full URL: `"https://huggingface.co/owner/repo/resolve/branch/filename.gguf"`
pub fn download_model_from_hf(
    owner: &str,
    repo: &str,
    filename: &str,
) -> Result<std::path::PathBuf, crate::errors::LoadModelError> {
    let api = hf_hub::api::sync::Api::new()
        .map_err(|e| crate::errors::LoadModelError::DownloadError(e.to_string()))?;
    let repo = api.model(format!("{owner}/{repo}"));
    let path = repo
        .get(&filename)
        .map_err(|e| crate::errors::LoadModelError::DownloadError(e.to_string()))?;

    Ok(path)
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
mod tests {}
