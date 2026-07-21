use pyo3::prelude::*;
use std::sync::atomic::AtomicBool;
use std::sync::Arc;
use std::time::Duration;

use nobodywho::render_miette;

mod parse;

/// Gate for forwarding tracing events to Python's logging module.
/// Set to `true` after pyo3_log is installed, set to `false` via an `atexit`
/// handler before `Py_FinalizeEx` runs. This prevents worker threads from
/// calling into a partially-destroyed interpreter during shutdown.
static PYTHON_LOGGING_AVAILABLE: AtomicBool = AtomicBool::new(false);

/// `Model` objects contain a GGUF model. It is primarily useful for sharing a single model instance
/// between multiple `Chat`, `Encoder`, or `CrossEncoder` instances.
/// Sharing is efficient because the underlying model data is reference-counted.
/// There is no `ModelAsync` variant. A regular `Model` can be used with both `Chat` and `ChatAsync`.
#[pyclass]
pub struct Model {
    model: Arc<nobodywho::llm::Model>,
}

/// Wrap a Python `on_download_progress` argument into a core `DownloadProgressCallback`.
///
/// - `Some(py_callable)` → wraps it so the Python function is invoked on each chunk
///   with `(downloaded_bytes, total_bytes)`. Exceptions are printed and swallowed.
/// - `None` → returns `None`; core installs its own default terminal progress bar.
///
/// Returns `TypeError` if `py_callback` is not callable, so a non-callable argument
/// fails fast at construction rather than per-chunk during download.
fn resolve_on_download_progress(
    py_callback: Option<Py<PyAny>>,
) -> PyResult<Option<nobodywho::llm::DownloadProgressCallback>> {
    let Some(cb) = py_callback else {
        return Ok(None);
    };
    Python::attach(|py| {
        if !cb.bind(py).is_callable() {
            return Err(pyo3::exceptions::PyTypeError::new_err(
                "on_download_progress must be callable, taking (downloaded_bytes, total_bytes)",
            ));
        }
        Ok(())
    })?;
    Ok(Some(Arc::new(move |downloaded: u64, total: u64| {
        Python::attach(|py| {
            if let Err(e) = cb.call1(py, (downloaded, total)) {
                e.print(py);
            }
        });
    }) as nobodywho::llm::DownloadProgressCallback))
}

#[pymethods]
impl Model {
    /// Create a new Model from a GGUF file.
    ///
    /// Args:
    ///     model_path: Local path, `huggingface:` path, `https://` URL, or `auto` for memory-based model selection. Remote models are downloaded and cached automatically.
    ///     use_gpu_if_available: If True, attempts to use GPU acceleration. Defaults to True.
    ///     projection_model_path: Path or URL to a multimodal projector file for vision models. Accepts the same formats as model_path. Defaults to None.
    ///     draft_model_path: Path or URL to a compatible MTP draft-heads gguf (e.g. `mtp-gemma-4-E2B-it.gguf` for Gemma-4-E2B). Loading it lets subsequent Chats opt into MTP speculative decoding via `mtp=MtpConfig()` on `Chat(...)`. Adds around 5% to VRAM usage. Defaults to None.
    ///     on_download_progress: Optional callable invoked during model downloads with `(downloaded_bytes, total_bytes)`. Not called for locally cached models. If a projection model is also downloaded, the callback fires for each download sequentially, so `total_bytes` resets between them. Defaults to None.
    ///
    /// Returns:
    ///     A Model instance
    ///
    /// Raises:
    ///     RuntimeError: If the model file cannot be loaded
    #[new]
    #[pyo3(signature = (model_path: "os.PathLike | str", use_gpu_if_available = true, projection_model_path: "os.PathLike | str | None" = None, draft_model_path: "os.PathLike | str | None" = None, on_download_progress: "typing.Callable[[int, int], None] | None" = None) -> "Model")]
    pub fn new(
        model_path: std::path::PathBuf,
        use_gpu_if_available: bool,
        projection_model_path: Option<std::path::PathBuf>,
        draft_model_path: Option<std::path::PathBuf>,
        on_download_progress: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let path_str = model_path.to_str().ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Path contains invalid UTF-8: {}",
                model_path.display()
            ))
        })?;
        let mmproj_str = projection_model_path
            .as_ref()
            .map(|p| {
                p.to_str().ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "Path contains invalid UTF-8: {}",
                        p.display()
                    ))
                })
            })
            .transpose()?;
        let draft_str = draft_model_path
            .as_ref()
            .map(|p| {
                p.to_str().ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "Path contains invalid UTF-8: {}",
                        p.display()
                    ))
                })
            })
            .transpose()?;
        let progress = resolve_on_download_progress(on_download_progress)?;
        let model_result = nobodywho::llm::get_model(
            path_str,
            use_gpu_if_available,
            mmproj_str,
            draft_str,
            progress,
        );
        match model_result {
            Ok(model) => Ok(Self {
                model: Arc::new(model),
            }),
            Err(err) => Err(pyo3::exceptions::PyRuntimeError::new_err(render_miette(
                &err,
            ))),
        }
    }

    /// Asynchronously load a model from a GGUF file.
    ///
    /// This static method loads a model asynchronously, which is useful for loading large models
    /// without blocking the async event loop. The blocking model load operation is offloaded to
    /// a background thread, allowing other async tasks to continue running.
    ///
    /// Args:
    ///     model_path: Local path, `huggingface:` path, `https://` URL, or `auto` for memory-based model selection. Remote models are downloaded and cached automatically.
    ///     use_gpu_if_available: If True, attempts to use GPU acceleration. Defaults to True.
    ///     projection_model_path: Path or URL to a multimodal projector file for vision models. Accepts the same formats as model_path. Defaults to None.
    ///     draft_model_path: Path or URL to a compatible MTP draft-heads gguf. See `Model.__init__` for details. Defaults to None.
    ///     on_download_progress: Optional callable invoked during model downloads with `(downloaded_bytes, total_bytes)`. Not called for locally cached models. If a projection model is also downloaded, the callback fires for each download sequentially, so `total_bytes` resets between them. Defaults to None.
    ///
    /// Returns:
    ///     A Model instance wrapped in an awaitable (async function returns a coroutine)
    ///
    /// Raises:
    ///     RuntimeError: If the model file cannot be loaded
    #[staticmethod]
    #[pyo3(signature = (model_path: "os.PathLike | str", use_gpu_if_available = true, projection_model_path: "os.PathLike | str | None" = None, draft_model_path: "os.PathLike | str | None" = None, on_download_progress: "typing.Callable[[int, int], None] | None" = None) -> "Model")]
    pub async fn load_model_async(
        model_path: std::path::PathBuf,
        use_gpu_if_available: bool,
        projection_model_path: Option<std::path::PathBuf>,
        draft_model_path: Option<std::path::PathBuf>,
        on_download_progress: Option<Py<PyAny>>,
    ) -> PyResult<Self> {
        let path_str = model_path.to_str().ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Path contains invalid UTF-8: {}",
                model_path.display()
            ))
        })?;
        let mmproj_str = projection_model_path
            .as_ref()
            .map(|p| {
                p.to_str().ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "Path contains invalid UTF-8: {}",
                        p.display()
                    ))
                })
            })
            .transpose()?;
        let draft_str = draft_model_path
            .as_ref()
            .map(|p| {
                p.to_str().ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "Path contains invalid UTF-8: {}",
                        p.display()
                    ))
                })
            })
            .transpose()?;
        let progress = resolve_on_download_progress(on_download_progress)?;
        let model_result = nobodywho::llm::get_model_async(
            path_str.to_owned(),
            use_gpu_if_available,
            mmproj_str.map(str::to_owned),
            draft_str.map(str::to_owned),
            progress,
        )
        .await;
        match model_result {
            Ok(model) => Ok(Self {
                model: Arc::new(model),
            }),
            Err(err) => Err(pyo3::exceptions::PyRuntimeError::new_err(render_miette(
                &err,
            ))),
        }
    }

    /// The maximum context size this model was trained with.
    #[getter]
    pub fn max_ctx(&self) -> u32 {
        self.model.max_ctx()
    }
}

/// This type represents a `Model | str` from python
/// The intent is to allow passing a string path directly to constructors like Chat
/// to make the simplest possible usage of nobodywho even simpler
/// i.e. `Chat("./model.gguf")` instead of `Chat(Model("./model.gguf"))`
#[derive(FromPyObject)]
pub enum ModelOrPath<'py> {
    ModelObj(Bound<'py, Model>),
    Path(std::path::PathBuf),
}

impl<'py> ModelOrPath<'py> {
    /// returns nobodywho core's internal model struct from a python `str | Model`
    fn get_inner_model(&self) -> PyResult<Arc<nobodywho::llm::Model>> {
        match self {
            ModelOrPath::ModelObj(model_obj) => Ok(Arc::clone(&model_obj.borrow().model)),
            // default to (trying to) use GPU if a string is passed
            ModelOrPath::Path(path) => {
                let path_str = path.to_str().ok_or_else(|| {
                    pyo3::exceptions::PyValueError::new_err(format!(
                        "Path contains invalid UTF-8: {}",
                        path.display()
                    ))
                })?;
                nobodywho::llm::get_model(path_str, true, None, None, None)
                    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(render_miette(&e)))
                    .map(Arc::new)
            }
        }
    }
}

// ---------------------------------------------------------------------------
// STT
// ---------------------------------------------------------------------------

/// `STT` transcribes speech to text using a Whisper ONNX model.
///
/// `source` is a HuggingFace repo (`hf://owner/repo`, e.g.
/// `"hf://onnx-community/whisper-base"`) or a local directory path. `language`
/// is an ISO 639-1 code (e.g. `"en"`); omit or pass `None` to auto-detect.
/// `quantization` selects the ONNX precision variant to download and load: one
/// of `"default"`, `"fp16"`, `"int8"`, `"uint8"`, `"bnb4"`, `"q4"`,
/// `"q4f16"`, `"quantized"`; omit or pass `None` to use `"default"`.
///
/// Example::
///
///     stt = nobodywho.STT("hf://onnx-community/whisper-base")
///     text = stt.transcribe_file("recording.mp3").completed()
///
///     # Or stream tokens:
///     for piece in stt.transcribe_file("recording.mp3"):
///         print(piece, end="", flush=True)
#[pyclass]
pub struct STT {
    stt: nobodywho::stt::Stt,
}

#[pymethods]
impl STT {
    #[new]
    #[pyo3(signature = (source, language = None, quantization = None))]
    pub fn new(
        source: &str,
        language: Option<&str>,
        quantization: Option<&str>,
        py: Python,
    ) -> PyResult<Self> {
        let mut cfg = nobodywho::stt::WhisperConfig::new(source);
        cfg.language = language.map(String::from);
        if let Some(quantization) = quantization {
            cfg.quantization = quantization.to_string();
        }
        let stt = py
            .detach(|| nobodywho::stt::Stt::new(nobodywho::stt::SttConfig::Whisper(cfg)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { stt })
    }

    /// Transcribe an audio file (WAV / MP3). Returns a `TokenStream`.
    pub fn transcribe_file(&self, path: &str, py: Python) -> PyResult<TokenStream> {
        let stream = py
            .detach(|| self.stt.transcribe_file_stream(path))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(TokenStream {
            inner: SyncStreamInner::Stt(stream),
        })
    }

    /// Transcribe raw i16 PCM samples from a microphone. Returns a `TokenStream`.
    pub fn transcribe_pcm(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
        py: Python,
    ) -> PyResult<TokenStream> {
        let stream = py
            .detach(|| self.stt.transcribe_pcm_stream(samples, sample_rate))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(TokenStream {
            inner: SyncStreamInner::Stt(stream),
        })
    }
}

/// `STTAsync` is the async variant of `STT`.
#[pyclass]
pub struct STTAsync {
    stt: nobodywho::stt::Stt,
}

#[pymethods]
impl STTAsync {
    #[new]
    #[pyo3(signature = (source, language = None, quantization = None))]
    pub fn new(
        source: &str,
        language: Option<&str>,
        quantization: Option<&str>,
        py: Python,
    ) -> PyResult<Self> {
        let mut cfg = nobodywho::stt::WhisperConfig::new(source);
        cfg.language = language.map(String::from);
        if let Some(quantization) = quantization {
            cfg.quantization = quantization.to_string();
        }
        let stt = py
            .detach(|| nobodywho::stt::Stt::new(nobodywho::stt::SttConfig::Whisper(cfg)))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(Self { stt })
    }

    pub fn transcribe_file(&self, path: String) -> PyResult<TokenStreamAsync> {
        let stream = self
            .stt
            .transcribe_file_stream_async(path)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(TokenStreamAsync {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(AsyncStreamInner::Stt(stream))),
        })
    }

    pub fn transcribe_pcm(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
    ) -> PyResult<TokenStreamAsync> {
        let stream = self
            .stt
            .transcribe_pcm_stream_async(samples, sample_rate)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(TokenStreamAsync {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(AsyncStreamInner::Stt(stream))),
        })
    }
}

// ---------------------------------------------------------------------------
// Token streams (shared by Chat and STT)
// ---------------------------------------------------------------------------

// Type-erased inner for sync streams — lets Chat and STT share one pyclass.
enum SyncStreamInner {
    Chat(nobodywho::chat::TokenStream),
    Stt(nobodywho::stream::TokenStream<nobodywho::errors::SttError>),
}

impl SyncStreamInner {
    fn next_token(&mut self) -> Result<Option<String>, String> {
        match self {
            Self::Chat(s) => s.next_token().map_err(|e| render_miette(&e)),
            Self::Stt(s) => s.next_token().map_err(|e| e.to_string()),
        }
    }
    fn completed(&mut self) -> Result<String, String> {
        match self {
            Self::Chat(s) => s.completed().map_err(|e| render_miette(&e)),
            Self::Stt(s) => s.completed().map_err(|e| e.to_string()),
        }
    }
}

// Type-erased inner for async streams.
enum AsyncStreamInner {
    Chat(nobodywho::chat::TokenStreamAsync),
    Stt(nobodywho::stream::TokenStreamAsync<nobodywho::errors::SttError>),
}

impl AsyncStreamInner {
    async fn next_token(&mut self) -> Result<Option<String>, String> {
        match self {
            Self::Chat(s) => s.next_token().await.map_err(|e| render_miette(&e)),
            Self::Stt(s) => s.next_token().await.map_err(|e| e.to_string()),
        }
    }
    async fn completed(&mut self) -> Result<String, String> {
        match self {
            Self::Chat(s) => s.completed().await.map_err(|e| render_miette(&e)),
            Self::Stt(s) => s.completed().await.map_err(|e| e.to_string()),
        }
    }
}

/// `TokenStream` is returned by `Chat.ask`, `STT.transcribe_file`, and `STT.transcribe_pcm`.
/// Iterate over it token-by-token or call `.completed()` for the full text at once.
/// Also see `TokenStreamAsync` for the async variant.

fn parse_tts_device(device: &str) -> PyResult<nobodywho::tts::TtsDevice> {
    match device.to_ascii_lowercase().as_str() {
        "auto" => Ok(nobodywho::tts::TtsDevice::Auto),
        "cpu" => Ok(nobodywho::tts::TtsDevice::Cpu),
        "cuda" => Ok(nobodywho::tts::TtsDevice::Cuda),
        _ => Err(pyo3::exceptions::PyValueError::new_err(
            "device must be one of 'auto', 'cpu', or 'cuda'",
        )),
    }
}

fn parse_tts_architecture(architecture: &str) -> PyResult<nobodywho::tts::TtsArchitecture> {
    architecture.parse().map_err(|()| {
        pyo3::exceptions::PyValueError::new_err(
            "architecture must be one of 'kokoro' or 'supertonic'",
        )
    })
}

fn build_tts_config(
    source: std::path::PathBuf,
    architecture: Option<&str>,
    voice: Option<String>,
    language: Option<String>,
    speed: Option<f32>,
    steps: Option<usize>,
    silence_duration: Option<f32>,
) -> PyResult<nobodywho::tts::TtsConfig> {
    let source = source.to_str().ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!(
            "Path contains invalid UTF-8: {}",
            source.display()
        ))
    })?;
    let architecture = architecture.map(parse_tts_architecture).transpose()?;
    let mut config = nobodywho::tts::TtsConfig::from_source(source, architecture).ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(
            "architecture is required for unknown TTS sources; pass architecture='kokoro' or architecture='supertonic'",
        )
    })?;

    match &mut config {
        nobodywho::tts::TtsConfig::Kokoro(config) => {
            if let Some(voice) = voice {
                config.voice = voice;
            }
            if let Some(language) = language {
                config.language = language;
            }
            if let Some(speed) = speed {
                config.speed = speed;
            }
        }
        nobodywho::tts::TtsConfig::Supertonic(config) => {
            if let Some(voice) = voice {
                config.voice = voice;
            }
            if let Some(language) = language {
                config.language = language;
            }
            if let Some(speed) = speed {
                config.speed = speed;
            }
            if let Some(steps) = steps {
                config.steps = steps;
            }
            if let Some(silence_duration) = silence_duration {
                config.silence_duration = silence_duration;
            }
        }
    }
    Ok(config)
}

/// `Tts` synthesizes speech to WAV bytes.
#[pyclass]
pub struct Tts {
    tts: nobodywho::tts::Tts,
}

#[pymethods]
impl Tts {
    /// Create a TTS synthesizer.
    ///
    /// Args:
    ///     source: Local model directory or HuggingFace repo (`hf://owner/repo`).
    ///     architecture: "kokoro" or "supertonic". Required for local or unknown sources.
    ///         Sources containing "kokoro" or "supertonic" infer the architecture when omitted.
    ///     voice: Voice name. Architecture default is used when omitted.
    ///     language: Language code. Architecture default is used when omitted.
    ///     speed: Speaking speed. Architecture default is used when omitted.
    ///     steps: Supertonic denoising steps. Ignored by Kokoro.
    ///     silence_duration: Supertonic silence between chunks in seconds.
    ///     device: "auto", "cpu", or "cuda". Defaults to "auto".
    #[new]
    #[pyo3(signature = (source: "os.PathLike | str", architecture: "typing.Literal['kokoro', 'supertonic'] | None" = None, voice = None, language = None, speed = None, steps = None, silence_duration = None, device: "typing.Literal['auto', 'cpu', 'cuda']" = "auto") -> "Tts")]
    pub fn new(
        source: std::path::PathBuf,
        architecture: Option<&str>,
        voice: Option<String>,
        language: Option<String>,
        speed: Option<f32>,
        steps: Option<usize>,
        silence_duration: Option<f32>,
        device: &str,
    ) -> PyResult<Self> {
        let device = parse_tts_device(device)?;
        let config = build_tts_config(
            source,
            architecture,
            voice,
            language,
            speed,
            steps,
            silence_duration,
        )?;
        let tts = nobodywho::tts::Tts::with_device(config, device)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(render_miette(&e)))?;
        Ok(Self { tts })
    }

    /// Synthesize text and return WAV bytes.
    pub fn synthesize(&self, text: String, py: Python<'_>) -> PyResult<Py<pyo3::types::PyBytes>> {
        let bytes = py
            .detach(|| self.tts.synthesize(text))
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(render_miette(&e)))?;
        Ok(pyo3::types::PyBytes::new(py, &bytes).unbind())
    }

    /// Synthesize text asynchronously and return WAV bytes.
    pub async fn synthesize_async(&self, text: String) -> PyResult<Py<pyo3::types::PyBytes>> {
        let bytes = self
            .tts
            .synthesize_async(text)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(render_miette(&e)))?;
        Python::attach(|py| Ok(pyo3::types::PyBytes::new(py, &bytes).unbind()))
    }
}

/// `TokenStream` represents an in-progress text completion. It is the return value of `Chat.ask`.
/// You can iterate over the tokens in a `TokenStream` using the normal python iterator protocol,
/// or by explicitly calling the `.next_token()` method.
/// If you want to wait for the entire response to be generated, you can call `.completed()`.
/// Also see `TokenStreamAsync`, for an async version of this class.
#[pyclass]
pub struct TokenStream {
    inner: SyncStreamInner,
}

#[pymethods]
impl TokenStream {
    pub fn next_token(&mut self, py: Python) -> PyResult<Option<String>> {
        py.detach(|| self.inner.next_token())
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    pub fn completed(&mut self, py: Python) -> PyResult<String> {
        py.detach(|| self.inner.completed())
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    pub fn __iter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    pub fn __next__(&mut self, py: Python) -> PyResult<Option<String>> {
        py.detach(|| self.inner.next_token())
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }
}

/// `TokenStreamAsync` is the async variant of `TokenStream`.
/// Supports `await stream.next_token()`, `await stream.completed()`, and `async for token in stream`.
#[pyclass]
pub struct TokenStreamAsync {
    inner: std::sync::Arc<tokio::sync::Mutex<AsyncStreamInner>>,
}

#[pymethods]
impl TokenStreamAsync {
    pub async fn next_token(&mut self) -> PyResult<Option<String>> {
        self.inner
            .lock()
            .await
            .next_token()
            .await
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    pub async fn completed(&mut self) -> PyResult<String> {
        self.inner
            .lock()
            .await
            .completed()
            .await
            .map_err(pyo3::exceptions::PyRuntimeError::new_err)
    }

    pub fn __aiter__(slf: PyRef<'_, Self>) -> PyRef<'_, Self> {
        slf
    }

    pub fn __anext__<'py>(&mut self, py: Python<'py>) -> PyResult<Bound<'py, pyo3::types::PyAny>> {
        let locals = pyo3_async_runtimes::TaskLocals::with_running_loop(py)?.copy_context(py)?;
        let inner = self.inner.clone();
        pyo3_async_runtimes::tokio::future_into_py_with_locals(py, locals, async move {
            match inner.lock().await.next_token().await {
                Ok(Some(t)) => Ok(t),
                Ok(None) => Err(pyo3::exceptions::PyStopAsyncIteration::new_err(())),
                Err(e) => Err(pyo3::exceptions::PyRuntimeError::new_err(e)),
            }
        })
    }
}

/// `Encoder` will let you generate vector representations of text.
/// It must be initialized with a model that specifically supports generating embeddings.
/// A regular chat/text-generation model will not just work.
/// Once initialized, you can call `.encode()` on a string, which returns a list of 32-bit floats.
/// See `EncoderAsync` for the async version of this class.
#[pyclass]
pub struct Encoder {
    encoder: Option<nobodywho::encoder::Encoder>,
}

impl Encoder {
    fn inner(&self) -> &nobodywho::encoder::Encoder {
        self.encoder.as_ref().expect("Encoder used after drop")
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        let encoder = self.encoder.take();
        Python::attach(|py| py.detach(|| drop(encoder)));
    }
}

#[pymethods]
impl Encoder {
    /// Create a new Encoder for generating text embeddings.
    ///
    /// Args:
    ///     model: An embedding model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
    ///     n_ctx: Context size (maximum sequence length). Defaults to 4096.
    ///
    /// Returns:
    ///     An Encoder instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded

    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096) -> "Encoder")]
    pub fn new(model: ModelOrPath, n_ctx: u32) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;
        let encoder = nobodywho::encoder::Encoder::new(nw_model, n_ctx);
        Ok(Self {
            encoder: Some(encoder),
        })
    }

    /// Generate an embedding vector for the given text. This method blocks until complete.
    ///
    /// Args:
    ///     text: The text to encode
    ///
    /// Returns:
    ///     A list of floats representing the embedding vector
    ///
    /// Raises:
    ///     RuntimeError: If encoding fails
    pub fn encode(&self, text: String, py: Python) -> PyResult<Vec<f32>> {
        py.detach(|| {
            self.inner()
                .encode(text)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
        })
    }
}

/// This is the async version of the `Encoder` class. See the docs on `Encoder` for more detail.
#[pyclass]
pub struct EncoderAsync {
    encoder_handle: Option<nobodywho::encoder::EncoderAsync>,
}

impl EncoderAsync {
    fn inner(&self) -> &nobodywho::encoder::EncoderAsync {
        self.encoder_handle
            .as_ref()
            .expect("EncoderAsync used after drop")
    }
}

impl Drop for EncoderAsync {
    fn drop(&mut self) {
        let handle = self.encoder_handle.take();
        Python::attach(|py| py.detach(|| drop(handle)));
    }
}

#[pymethods]
impl EncoderAsync {
    /// Create a new async Encoder for generating text embeddings.
    ///
    /// Args:
    ///     model: An embedding model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
    ///     n_ctx: Context size (maximum sequence length). Defaults to 4096.
    ///
    /// Returns:
    ///     An EncoderAsync instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded

    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096) -> "EncoderAsync")]
    pub fn new(model: ModelOrPath, n_ctx: u32) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;
        let encoder_handle = nobodywho::encoder::EncoderAsync::new(nw_model, n_ctx);
        Ok(Self {
            encoder_handle: Some(encoder_handle),
        })
    }

    /// Generate an embedding vector for the given text asynchronously.
    ///
    /// Args:
    ///     text: The text to encode
    ///
    /// Returns:
    ///     A list of floats representing the embedding vector
    ///
    /// Raises:
    ///     RuntimeError: If encoding fails
    async fn encode(&self, text: String) -> PyResult<Vec<f32>> {
        self.inner().encode(text).await.map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to receive embedding: {e}"
            ))
        })
    }
}

/// A `CrossEncoder` is a kind of encoder that is trained to compare similarity between two texts.
/// It is particularly useful for searching a list of texts with a query, to find the closest one.
/// `CrossEncoder` requires a model made specifically for cross-encoding.
/// See `CrossEncoderAsync` for the async version of this class.
#[pyclass]
pub struct CrossEncoder {
    crossencoder: Option<nobodywho::crossencoder::CrossEncoder>,
}

impl CrossEncoder {
    fn inner(&self) -> &nobodywho::crossencoder::CrossEncoder {
        self.crossencoder
            .as_ref()
            .expect("CrossEncoder used after drop")
    }
}

impl Drop for CrossEncoder {
    fn drop(&mut self) {
        let crossencoder = self.crossencoder.take();
        Python::attach(|py| py.detach(|| drop(crossencoder)));
    }
}

#[pymethods]
impl CrossEncoder {
    /// Create a new CrossEncoder for comparing text similarity.
    ///
    /// Args:
    ///     model: A cross-encoder model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
    ///     n_ctx: Context size (maximum sequence length). Defaults to 4096.
    ///
    /// Returns:
    ///     A CrossEncoder instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded

    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096) -> "CrossEncoder")]
    pub fn new(model: ModelOrPath, n_ctx: u32) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;
        let crossencoder = nobodywho::crossencoder::CrossEncoder::new(nw_model, n_ctx);
        Ok(Self {
            crossencoder: Some(crossencoder),
        })
    }

    /// Compute similarity scores between a query and multiple documents. This method blocks.
    ///
    /// Args:
    ///     query: The query text
    ///     documents: List of documents to compare against the query
    ///
    /// Returns:
    ///     List of similarity scores (higher = more similar). Scores are in the same order as documents.
    ///
    /// Raises:
    ///     RuntimeError: If ranking fails
    pub fn rank(&self, query: String, documents: Vec<String>, py: Python) -> PyResult<Vec<f32>> {
        py.detach(|| {
            self.inner()
                .rank(query, documents)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
        })
    }

    /// Rank documents by similarity to query and return them sorted. This method blocks.
    ///
    /// Args:
    ///     query: The query text
    ///     documents: List of documents to compare against the query
    ///
    /// Returns:
    ///     List of (document, score) tuples sorted by descending similarity (most similar first).
    ///
    /// Raises:
    ///     RuntimeError: If ranking fails
    pub fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
        py: Python,
    ) -> PyResult<Vec<(String, f32)>> {
        py.detach(|| {
            self.inner()
                .rank_and_sort(query, documents)
                .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
        })
    }
}

/// This is the async version of `CrossEncoder`.
/// See the docs for `CrossEncoder` for more details.
#[pyclass]
pub struct CrossEncoderAsync {
    crossencoder_handle: Option<nobodywho::crossencoder::CrossEncoderAsync>,
}

impl CrossEncoderAsync {
    fn inner(&self) -> &nobodywho::crossencoder::CrossEncoderAsync {
        self.crossencoder_handle
            .as_ref()
            .expect("CrossEncoderAsync used after drop")
    }
}

impl Drop for CrossEncoderAsync {
    fn drop(&mut self) {
        let handle = self.crossencoder_handle.take();
        Python::attach(|py| py.detach(|| drop(handle)));
    }
}

#[pymethods]
impl CrossEncoderAsync {
    /// Create a new async CrossEncoder for comparing text similarity.
    ///
    /// Args:
    ///     model: A cross-encoder model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
    ///     n_ctx: Context size (maximum sequence length). Defaults to 4096.
    ///
    /// Returns:
    ///     A CrossEncoderAsync instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded

    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096) -> "CrossEncoderAsync")]
    pub fn new(model: ModelOrPath, n_ctx: u32) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;
        let crossencoder_handle = nobodywho::crossencoder::CrossEncoderAsync::new(nw_model, n_ctx);
        Ok(Self {
            crossencoder_handle: Some(crossencoder_handle),
        })
    }

    /// Compute similarity scores between a query and multiple documents asynchronously.
    ///
    /// Args:
    ///     query: The query text
    ///     documents: List of documents to compare against the query
    ///
    /// Returns:
    ///     List of similarity scores (higher = more similar). Scores are in the same order as documents.
    ///
    /// Raises:
    ///     RuntimeError: If ranking fails
    async fn rank(&self, query: String, documents: Vec<String>) -> PyResult<Vec<f32>> {
        self.inner().rank(query, documents).await.map_err(|e| {
            PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!(
                "Failed to receive ranking scores: {e}"
            ))
        })
    }

    /// Rank documents by similarity to query and return them sorted asynchronously.
    ///
    /// Args:
    ///     query: The query text
    ///     documents: List of documents to compare against the query
    ///
    /// Returns:
    ///     List of (document, score) tuples sorted by descending similarity (most similar first).
    ///
    /// Raises:
    ///     RuntimeError: If ranking fails
    async fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> PyResult<Vec<(String, f32)>> {
        self.inner()
            .rank_and_sort(query, documents)
            .await
            .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(format!("{}", e)))
    }
}

/// `Chat` is a general-purpose class for interacting with instruction-tuned conversational LLMs.
/// It should be initialized with a turn-taking LLM, which includes a chat template.
/// On a `Chat` instance, you can call `.ask()` with the prompt you intend to pass to the model,
/// which returns a `TokenStream`, representing the generated response.
/// `Chat` also supports calling tools.
/// When initializing a `Chat`, you can also specify additional generation configuration, like
/// what tools to provide, what sampling strategy to use for choosing tokens, what system prompt
/// to use, whether to allow extended thinking, etc.
/// See `ChatAsync` for the async version of this class.
/// Tuning for MTP speculative decoding. Pass an instance as the `mtp`
/// argument to `Chat`/`ChatAsync` to enable MTP; leave it `None` to disable.
/// Requires the `Model` to have been loaded with a compatible `draft_model_path`.
// `from_py_object` opts into the `FromPyObject` derive so `MtpConfig` can be
// accepted by value as the `mtp` argument. pyo3 0.28 made this opt-in for
// `Clone` pyclasses; without it the build fails under `-D deprecated`.
#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct MtpConfig {
    /// Maximum draft tokens proposed per speculative step (llama.cpp n_max).
    #[pyo3(get, set)]
    pub k_max: u32,
    /// Minimum draft-token probability the drafter will propose (llama.cpp p_min).
    #[pyo3(get, set)]
    pub p_min: f32,
}

#[pymethods]
impl MtpConfig {
    /// Create an MTP config. Defaults mirror core `MtpConfig::default()`.
    ///
    /// Args:
    ///     k_max: Max draft tokens proposed per speculative step. Defaults to 3.
    ///     p_min: Minimum draft-token probability accepted. Defaults to 0.0.
    #[new]
    #[pyo3(signature = (k_max = 3, p_min = 0.0))]
    fn new(k_max: u32, p_min: f32) -> Self {
        Self { k_max, p_min }
    }
}

impl From<MtpConfig> for nobodywho::chat::MtpConfig {
    fn from(c: MtpConfig) -> Self {
        nobodywho::chat::MtpConfig {
            k_max: c.k_max,
            p_min: c.p_min,
        }
    }
}

#[pyclass]
pub struct Chat {
    // Wrap in Option so we can take it in Drop to release the handle
    // while the GIL is temporarily dropped.
    chat_handle: Option<nobodywho::chat::ChatHandle>,
}

impl Chat {
    fn handle(&self) -> &nobodywho::chat::ChatHandle {
        self.chat_handle.as_ref().expect("Chat used after drop")
    }
}

impl Drop for Chat {
    fn drop(&mut self) {
        let handle = self.chat_handle.take();
        Python::attach(|py| py.detach(|| drop(handle)));
    }
}

#[pymethods]
impl Chat {
    /// Create a new Chat instance for conversational text generation.
    ///
    /// Args:
    ///     model: A chat model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
    ///     n_ctx: Context size (maximum conversation length in tokens). Defaults to 4096.
    ///     system_prompt: System message to guide the model's behavior. Defaults to empty string.
    ///     template_variables: Dict of template variables to pass to the chat template (e.g., {"enable_thinking": True}). Defaults to empty dict.
    ///     tools: List of Tool instances the model can call. Defaults to empty list.
    ///     sampler: SamplerConfig for token selection. If not given, sampling settings
    ///         embedded in the model file (general.sampling.* metadata) are used when
    ///         present, otherwise SamplerConfig.default().
    ///     allow_thinking: DEPRECATED. Use template_variables={"enable_thinking": True} instead. If set, overrides enable_thinking in template_variables.
    ///     mtp: Optional MtpConfig to enable MTP speculative decoding on this chat.
    ///         Requires the `Model` to have been loaded with a compatible
    ///         `draft_model_path`. Adds around 5% to VRAM usage. Defaults to None.
    ///
    /// Returns:
    ///     A Chat instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded

    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096, system_prompt = None, template_variables: "dict[str, bool]" = std::collections::HashMap::<String, bool>::new(), tools: "list[Tool]" = Vec::<Tool>::new(), sampler: "SamplerConfig | None" = None, allow_thinking: "bool | None" = None, mtp: "MtpConfig | None" = None) -> "Chat")]
    pub fn new(
        model: ModelOrPath,
        n_ctx: u32,
        system_prompt: Option<&str>,
        template_variables: std::collections::HashMap<String, bool>,
        tools: Vec<Tool>,
        sampler: Option<SamplerConfig>,
        allow_thinking: Option<bool>,
        mtp: Option<MtpConfig>,
        py: Python<'_>,
    ) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;

        // Handle deprecated allow_thinking parameter
        let mut template_vars = template_variables;
        if let Some(allow) = allow_thinking {
            let msg = std::ffi::CString::new(format!(
                "allow_thinking parameter is deprecated. Use template_variables={{\"enable_thinking\": {}}} instead.",
                allow
            )).unwrap();
            PyErr::warn(
                py,
                &py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
                &msg,
                1,
            )?;
            template_vars.insert("enable_thinking".to_string(), allow);
        }

        let build_result = py.detach(|| {
            let mut builder = nobodywho::chat::ChatBuilder::new(nw_model)
                .with_context_size(n_ctx)
                .with_tools(tools.into_iter().map(|t| t.tool).collect())
                .with_template_variables(template_vars)
                .with_system_prompt(system_prompt);
            if let Some(mtp) = mtp {
                builder = builder.with_mtp(mtp.into());
            }
            // When no sampler is given, leave it unset so the worker falls back
            // to sampling settings embedded in the GGUF (general.sampling.*),
            // and only then to the built-in default.
            if let Some(s) = sampler {
                builder = builder.with_sampler(s.sampler_config);
            }
            builder.build()
        });
        let chat_handle = build_result
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(render_miette(&e)))?;

        Ok(Self {
            chat_handle: Some(chat_handle),
        })
    }

    /// Send a message to the model and get a streaming response.
    ///
    /// Args:
    ///     prompt: The user prompt to send (plain text or a multimodal Prompt)
    ///
    /// Returns:
    ///     A TokenStream that yields tokens as they are generated
    #[pyo3(signature = (prompt: "str | Prompt") -> "TokenStream")]
    pub fn ask(&self, prompt: PromptOrText) -> TokenStream {
        let stream = match prompt {
            PromptOrText::Text(text) => self.handle().ask(text),
            PromptOrText::PromptObj(prompt_obj) => {
                self.handle().ask(prompt_obj.borrow().prompt.clone())
            }
        };

        TokenStream {
            inner: SyncStreamInner::Chat(stream),
        }
    }

    /// Reset the conversation with a new system prompt and tools. Clears all chat history.
    ///
    /// Args:
    ///     system_prompt: New system message to guide the model's behavior
    ///     tools: New list of Tool instances the model can call
    ///
    /// Raises:
    ///     RuntimeError: If reset fails
    pub fn reset(
        &self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
        py: Python,
    ) -> PyResult<()> {
        py.detach(|| {
            self.handle()
                .reset_chat(system_prompt, tools.into_iter().map(|t| t.tool).collect())
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Clear the chat history while keeping the system prompt and tools unchanged.
    ///
    /// Raises:
    ///     RuntimeError: If reset fails
    pub fn reset_history(&self, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.handle()
                .reset_history()
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// DEPRECATED: Use set_template_variable("enable_thinking", value) instead.
    ///
    /// Enable or disable extended reasoning tokens for supported models.
    ///
    /// Args:
    ///     allow_thinking: If True, allows extended reasoning tokens
    ///
    /// Raises:
    ///     ValueError: If the setting cannot be changed
    pub fn set_allow_thinking(&self, allow_thinking: bool, py: Python) -> PyResult<()> {
        let msg = std::ffi::CString::new(format!(
            "set_allow_thinking is deprecated. Use set_template_variable(\"enable_thinking\", {}) instead.",
            allow_thinking
        )).unwrap();
        PyErr::warn(
            py,
            &py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
            &msg,
            1,
        )?;
        py.detach(|| {
            self.handle()
                .set_template_variable("enable_thinking".to_string(), allow_thinking)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
        })
    }

    /// Set a single template variable
    ///
    /// Args:
    ///     name: The name of the template variable (e.g., "enable_thinking")
    ///     value: The boolean value for the variable
    ///
    /// Raises:
    ///     RuntimeError: If the variable cannot be set
    pub fn set_template_variable(&self, name: String, value: bool, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.handle()
                .set_template_variable(name, value)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Set all template variables, replacing any existing ones.
    ///
    /// Args:
    ///     variables: Dict of template variable names to boolean values
    ///
    /// Raises:
    ///     RuntimeError: If the variables cannot be set
    pub fn set_template_variables(
        &self,
        variables: std::collections::HashMap<String, bool>,
        py: Python,
    ) -> PyResult<()> {
        py.detach(|| {
            self.handle()
                .set_template_variables(variables)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Get all template variables.
    ///
    /// Returns:
    ///     Dict of template variable names to boolean values
    ///
    /// Raises:
    ///     RuntimeError: If the variables cannot be retrieved
    pub fn get_template_variables(
        &self,
        py: Python,
    ) -> PyResult<std::collections::HashMap<String, bool>> {
        py.detach(|| {
            self.handle()
                .get_template_variables()
                .map(|vars| vars.into_iter().collect())
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Get the current chat history as a list of message dictionaries.
    ///
    /// Returns:
    ///     List of message dicts, each with 'role' (str) and 'content' (str) keys.
    ///     Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]
    ///
    /// Raises:
    ///     RuntimeError: If retrieval fails
    #[pyo3(signature = () -> "list[dict]")]
    pub fn get_chat_history(&self, py: Python) -> PyResult<Py<PyAny>> {
        let msgs = py.detach(|| {
            self.handle()
                .get_chat_history()
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })?;

        pythonize::pythonize(py, &msgs)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
            .map(|bound| bound.unbind())
    }

    /// Replace the chat history with a new list of messages.
    ///
    /// Args:
    ///     msgs: List of message dicts, each with 'role' (str) and 'content' (str) keys.
    ///           Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]
    ///
    /// Raises:
    ///     ValueError: If message format is invalid
    ///     RuntimeError: If setting history fails
    #[pyo3(signature = (msgs: "list[dict]") -> "None")]
    pub fn set_chat_history(&self, msgs: Bound<'_, PyAny>, py: Python) -> PyResult<()> {
        let msgs = pythonize::depythonize(&msgs)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;

        py.detach(|| {
            self.handle()
                .set_chat_history(msgs)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Stop the current text generation immediately.
    ///
    /// This can be used to cancel an in-progress generation if the response is taking too long
    /// or is no longer needed.
    pub fn stop_generation(&self, py: Python) {
        py.detach(|| self.handle().stop_generation())
    }

    /// Update the list of tools available to the model without resetting chat history.
    ///
    /// Args:
    ///     tools: New list of Tool instances the model can call
    ///
    /// Raises:
    ///     RuntimeError: If updating tools fails
    pub fn set_tools(&self, tools: Vec<Tool>, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.handle()
                .set_tools(tools.into_iter().map(|t| t.tool).collect())
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Update the system prompt without resetting chat history.
    ///
    /// Args:
    ///     system_prompt: New system message to guide the model's behavior
    ///
    /// Raises:
    ///     RuntimeError: If the system prompt cannot be changed
    pub fn set_system_prompt(&self, system_prompt: Option<String>, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.handle()
                .set_system_prompt(system_prompt)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Update the sampler configuration without resetting chat history.
    ///
    /// Args:
    ///     sampler: New SamplerConfig for token selection
    ///
    /// Raises:
    ///     RuntimeError: If the sampler config cannot be changed
    pub fn set_sampler_config(&self, sampler: SamplerConfig, py: Python) -> PyResult<()> {
        py.detach(|| {
            self.handle()
                .set_sampler_config(sampler.sampler_config)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Get the current sampler configuration.
    ///
    /// Returns:
    ///     The current SamplerConfig used for token selection
    ///
    /// Raises:
    ///     RuntimeError: If the sampler config cannot be retrieved
    pub fn get_sampler_config(&self, py: Python) -> PyResult<SamplerConfig> {
        py.detach(|| {
            self.handle()
                .get_sampler_config()
                .map(|sampler_config| SamplerConfig { sampler_config })
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Get context usage statistics.
    ///
    /// Returns:
    ///     ChatStats with context_size and context_used fields
    #[pyo3(signature = () -> "ChatStats")]
    pub fn stats(&self, py: Python) -> PyResult<ChatStats> {
        py.detach(|| {
            self.handle()
                .get_stats()
                .map(|s| ChatStats {
                    context_size: s.context_size,
                    context_used: s.context_used,
                })
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Get the current system prompt.
    ///
    /// Returns:
    ///     The current system prompt, or None if not set
    ///
    /// Raises:
    ///     RuntimeError: If the system prompt cannot be retrieved
    pub fn get_system_prompt(&self, py: Python) -> PyResult<Option<String>> {
        py.detach(|| {
            self.handle()
                .get_system_prompt()
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }

    /// Tokenize a prompt and return token IDs.
    ///
    /// Text tokens are returned as integers. Media embedding slots (images, audio)
    /// are returned as None — one None per context slot consumed.
    ///
    /// Note: tokenizing a prompt with images requires loading and processing those
    /// images through the projection model, so it is not a free operation.
    ///
    /// Args:
    ///     prompt: The text or multimodal Prompt to tokenize
    ///
    /// Returns:
    ///     list[int | None] — token IDs for text, None for each media embedding slot
    ///
    /// Raises:
    ///     RuntimeError: If tokenization fails
    #[pyo3(signature = (prompt: "str | Prompt") -> "list[int | None]")]
    pub fn tokenize(&self, prompt: PromptOrText, py: Python) -> PyResult<Vec<Option<i32>>> {
        let nw_prompt = match prompt {
            PromptOrText::Text(text) => nobodywho::tokenizer::Prompt::from(text),
            PromptOrText::PromptObj(prompt_obj) => prompt_obj.borrow().prompt.clone(),
        };
        py.detach(|| {
            self.handle()
                .tokenize(nw_prompt)
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
        })
    }
}

/// This is the async version of the `Chat` class.
/// See the docs for the `Chat` class for more information.
#[pyclass]
pub struct ChatAsync {
    // Option so we can take it in Drop to release it with the GIL temporarily dropped.
    chat_handle: Option<nobodywho::chat::ChatHandleAsync>,
}

impl ChatAsync {
    fn handle(&self) -> &nobodywho::chat::ChatHandleAsync {
        self.chat_handle
            .as_ref()
            .expect("ChatAsync used after drop")
    }
}

impl Drop for ChatAsync {
    fn drop(&mut self) {
        let handle = self.chat_handle.take();
        Python::attach(|py| py.detach(|| drop(handle)));
    }
}

#[pymethods]
impl ChatAsync {
    /// Create a new async Chat instance for conversational text generation.
    ///
    /// Args:
    ///     model: A chat model (Model instance, local path, `huggingface:` path, or `https://` URL to a GGUF file)
    ///     n_ctx: Context size (maximum conversation length in tokens). Defaults to 4096.
    ///     system_prompt: System message to guide the model's behavior. Defaults to empty string.
    ///     template_variables: Dict of template variables to pass to the chat template (e.g., {"enable_thinking": True}). Defaults to empty dict.
    ///     tools: List of Tool instances the model can call. Defaults to empty list.
    ///     sampler: SamplerConfig for token selection. If not given, sampling settings
    ///         embedded in the model file (general.sampling.* metadata) are used when
    ///         present, otherwise SamplerConfig.default().
    ///     allow_thinking: DEPRECATED. Use template_variables={"enable_thinking": True} instead. If set, overrides enable_thinking in template_variables.
    ///     mtp: Optional MtpConfig to enable MTP speculative decoding on this chat.
    ///         Requires the `Model` to have been loaded with a compatible
    ///         `draft_model_path`. Adds around 5% to VRAM usage. Defaults to None.
    ///
    /// Returns:
    ///     A ChatAsync instance
    ///
    /// Raises:
    ///     RuntimeError: If the model cannot be loaded

    #[new]
    #[pyo3(signature = (model: "Model | os.PathLike | str", n_ctx = 4096, system_prompt = None, template_variables: "dict[str, bool]" = std::collections::HashMap::<String, bool>::new(), tools: "list[Tool]" = vec![], sampler: "SamplerConfig | None" = None, allow_thinking: "bool | None" = None, mtp: "MtpConfig | None" = None) -> "ChatAsync")]
    pub fn new(
        model: ModelOrPath,
        n_ctx: u32,
        system_prompt: Option<&str>,
        template_variables: std::collections::HashMap<String, bool>,
        tools: Vec<Tool>,
        sampler: Option<SamplerConfig>,
        allow_thinking: Option<bool>,
        mtp: Option<MtpConfig>,
        py: Python<'_>,
    ) -> PyResult<Self> {
        let nw_model = model.get_inner_model()?;

        // Handle deprecated allow_thinking parameter
        let mut template_vars = template_variables;
        if let Some(allow) = allow_thinking {
            let msg = std::ffi::CString::new(format!(
                "allow_thinking parameter is deprecated. Use template_variables={{\"enable_thinking\": {}}} instead.",
                allow
            )).unwrap();
            PyErr::warn(
                py,
                &py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
                &msg,
                1,
            )?;
            template_vars.insert("enable_thinking".to_string(), allow);
        }

        let build_result = py.detach(|| {
            let mut builder = nobodywho::chat::ChatBuilder::new(nw_model)
                .with_context_size(n_ctx)
                .with_tools(tools.into_iter().map(|t| t.tool).collect())
                .with_template_variables(template_vars)
                .with_system_prompt(system_prompt);
            if let Some(mtp) = mtp {
                builder = builder.with_mtp(mtp.into());
            }
            // When no sampler is given, leave it unset so the worker falls back
            // to sampling settings embedded in the GGUF (general.sampling.*),
            // and only then to the built-in default.
            if let Some(s) = sampler {
                builder = builder.with_sampler(s.sampler_config);
            }
            builder.build_async()
        });
        let chat_handle = build_result
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(render_miette(&e)))?;
        Ok(Self {
            chat_handle: Some(chat_handle),
        })
    }

    /// Send a message to the model and get a streaming response asynchronously.
    ///
    /// Args:
    ///     prompt: The user prompt to send (plain text or a multimodal Prompt)
    ///
    /// Returns:
    ///     A TokenStreamAsync that yields tokens as they are generated
    #[pyo3(signature = (prompt: "str | Prompt") -> "TokenStreamAsync")]
    pub fn ask(&self, prompt: PromptOrText) -> TokenStreamAsync {
        let stream = match prompt {
            PromptOrText::Text(text) => self.handle().ask(text),
            PromptOrText::PromptObj(prompt_obj) => {
                self.handle().ask(prompt_obj.borrow().prompt.clone())
            }
        };

        TokenStreamAsync {
            inner: std::sync::Arc::new(tokio::sync::Mutex::new(AsyncStreamInner::Chat(stream))),
        }
    }

    /// Reset the conversation with a new system prompt and tools. Clears all chat history.
    ///
    /// Args:
    ///     system_prompt: New system message to guide the model's behavior
    ///     tools: New list of Tool instances the model can call
    ///
    /// Raises:
    ///     RuntimeError: If reset fails
    pub async fn reset(&self, system_prompt: Option<String>, tools: Vec<Tool>) -> PyResult<()> {
        self.handle()
            .reset_chat(system_prompt, tools.into_iter().map(|t| t.tool).collect())
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Clear the chat history while keeping the system prompt and tools unchanged.
    ///
    /// Raises:
    ///     RuntimeError: If reset fails
    pub async fn reset_history(&self) -> PyResult<()> {
        self.handle()
            .reset_history()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// DEPRECATED: Use set_template_variable("enable_thinking", value) instead.
    ///
    /// Enable or disable extended reasoning tokens for supported models.
    ///
    /// Args:
    ///     allow_thinking: If True, allows extended reasoning tokens
    ///
    /// Raises:
    ///     ValueError: If the setting cannot be changed
    pub async fn set_allow_thinking(&self, allow_thinking: bool) -> PyResult<()> {
        Python::attach(|py| {
            let msg = std::ffi::CString::new(format!(
                "set_allow_thinking is deprecated. Use set_template_variable(\"enable_thinking\", {}) instead.",
                allow_thinking
            )).unwrap();
            PyErr::warn(
                py,
                &py.get_type::<pyo3::exceptions::PyDeprecationWarning>(),
                &msg,
                1,
            )
        })?;
        self.handle()
            .set_template_variable("enable_thinking".to_string(), allow_thinking)
            .await
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Set a single template variable.
    ///
    /// Args:
    ///     name: The name of the template variable (e.g., "enable_thinking")
    ///     value: The boolean value for the variable
    ///
    /// Raises:
    ///     RuntimeError: If the variable cannot be set
    pub async fn set_template_variable(&self, name: String, value: bool) -> PyResult<()> {
        self.handle()
            .set_template_variable(name, value)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Set all template variables, replacing any existing ones.
    ///
    /// Args:
    ///     variables: Dict of template variable names to boolean values
    ///
    /// Raises:
    ///     RuntimeError: If the variables cannot be set
    pub async fn set_template_variables(
        &self,
        variables: std::collections::HashMap<String, bool>,
    ) -> PyResult<()> {
        self.handle()
            .set_template_variables(variables)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Get all template variables.
    ///
    /// Returns:
    ///     Dict of template variable names to boolean values
    ///
    /// Raises:
    ///     RuntimeError: If the variables cannot be retrieved
    pub async fn get_template_variables(
        &self,
    ) -> PyResult<std::collections::HashMap<String, bool>> {
        self.handle()
            .get_template_variables()
            .await
            .map(|vars| vars.into_iter().collect())
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Get the current chat history as a list of message dictionaries.
    ///
    /// Returns:
    ///     List of message dicts, each with 'role' (str) and 'content' (str) keys.
    ///     Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]
    ///
    /// Raises:
    ///     RuntimeError: If retrieval fails
    #[pyo3(signature = () -> "list[dict]")]
    pub async fn get_chat_history(&self) -> PyResult<Py<PyAny>> {
        let msgs = self
            .handle()
            .get_chat_history()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        Python::attach(|py| {
            pythonize::pythonize(py, &msgs)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
                .map(|bound| bound.unbind())
        })
    }

    /// Replace the chat history with a new list of messages.
    ///
    /// Args:
    ///     msgs: List of message dicts, each with 'role' (str) and 'content' (str) keys.
    ///           Example: [{"role": "user", "content": "Hello"}, {"role": "assistant", "content": "Hi!"}]
    ///
    /// Raises:
    ///     ValueError: If message format is invalid
    ///     RuntimeError: If setting history fails
    #[pyo3(signature = (msgs: "list[dict]") -> "None")]
    pub async fn set_chat_history(&self, msgs: Py<PyAny>) -> PyResult<()> {
        let msgs = Python::attach(|py| {
            let bound_msgs = msgs.bind(py);
            pythonize::depythonize(bound_msgs)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
        })?;

        self.handle()
            .set_chat_history(msgs)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Stop the current text generation immediately.
    ///
    /// This can be used to cancel an in-progress generation if the response is taking too long
    /// or is no longer needed.
    pub async fn stop_generation(&self) {
        self.handle().stop_generation()
    }

    /// Update the list of tools available to the model without resetting chat history.
    ///
    /// Args:
    ///     tools: New list of Tool instances the model can call
    ///
    /// Raises:
    ///     RuntimeError: If updating tools fails
    pub async fn set_tools(&self, tools: Vec<Tool>) -> PyResult<()> {
        self.handle()
            .set_tools(tools.into_iter().map(|t| t.tool).collect())
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Update the system prompt without resetting chat history.
    ///
    /// Args:
    ///     system_prompt: New system message to guide the model's behavior
    ///
    /// Raises:
    ///     RuntimeError: If the system prompt cannot be changed
    pub async fn set_system_prompt(&self, system_prompt: Option<String>) -> PyResult<()> {
        self.handle()
            .set_system_prompt(system_prompt)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Update the sampler configuration without resetting chat history.
    ///
    /// Args:
    ///     sampler: New SamplerConfig for token selection
    ///
    /// Raises:
    ///     RuntimeError: If the sampler config cannot be changed
    pub async fn set_sampler_config(&self, sampler: SamplerConfig) -> PyResult<()> {
        self.handle()
            .set_sampler_config(sampler.sampler_config)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Get the current sampler configuration.
    ///
    /// Returns:
    ///     The current SamplerConfig used for token selection
    ///
    /// Raises:
    ///     RuntimeError: If the sampler config cannot be retrieved
    pub async fn get_sampler_config(&self) -> PyResult<SamplerConfig> {
        self.handle()
            .get_sampler_config()
            .await
            .map(|sampler_config| SamplerConfig { sampler_config })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Get context usage statistics.
    ///
    /// Returns:
    ///     ChatStats with context_size and context_used fields
    #[pyo3(signature = () -> "ChatStats")]
    pub async fn stats(&self) -> PyResult<ChatStats> {
        self.handle()
            .get_stats()
            .await
            .map(|s| ChatStats {
                context_size: s.context_size,
                context_used: s.context_used,
            })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Get the current system prompt.
    ///
    /// Returns:
    ///     The current system prompt, or None if not set
    ///
    /// Raises:
    ///     RuntimeError: If the system prompt cannot be retrieved
    pub async fn get_system_prompt(&self) -> PyResult<Option<String>> {
        self.handle()
            .get_system_prompt()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Tokenize a prompt and return token IDs.
    ///
    /// Text tokens are returned as integers. Media embedding slots (images, audio)
    /// are returned as None — one None per context slot consumed.
    ///
    /// Note: tokenizing a prompt with images requires loading and processing those
    /// images through the projection model, so it is not a free operation.
    ///
    /// Args:
    ///     prompt: The text or multimodal Prompt to tokenize
    ///
    /// Returns:
    ///     list[int | None] — token IDs for text, None for each media embedding slot
    ///
    /// Raises:
    ///     RuntimeError: If tokenization fails
    #[pyo3(signature = (prompt: "str | Prompt") -> "list[int | None]")]
    pub async fn tokenize(&self, prompt: Py<PyAny>) -> PyResult<Vec<Option<i32>>> {
        let nw_prompt = Python::attach(|py| -> PyResult<nobodywho::tokenizer::Prompt> {
            let bound = prompt.bind(py);
            if let Ok(text) = bound.extract::<String>() {
                Ok(nobodywho::tokenizer::Prompt::from(text))
            } else if let Ok(p) = bound.cast::<crate::Prompt>() {
                Ok(p.borrow().prompt.clone())
            } else {
                Err(pyo3::exceptions::PyTypeError::new_err(
                    "prompt must be str or Prompt",
                ))
            }
        })?;
        self.handle()
            .tokenize(nw_prompt)
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

/// Compute the cosine similarity between two vectors.
/// Particularly useful for comparing embedding vectors from an Encoder.
///
/// Args:
///     a: First vector
///     b: Second vector (must have the same length as a)
///
/// Returns:
///     Similarity score between 0.0 and 1.0 (higher means more similar)
///
/// Raises:
///     ValueError: If vectors have different lengths
#[pyfunction]
fn cosine_similarity(a: Vec<f32>, b: Vec<f32>) -> PyResult<f32> {
    if a.len() != b.len() {
        return Err(PyErr::new::<pyo3::exceptions::PyValueError, _>(
            "Vectors must have the same length",
        ));
    }
    Ok(nobodywho::encoder::cosine_similarity(&a, &b))
}

/// Download a model from a remote URL or HuggingFace path and return the local path.
///
/// This is useful when you need to pass custom headers (e.g. for authentication).
/// For unauthenticated downloads, you can pass the path directly to `Chat` or `Model`.
///
/// Args:
///     model_path: Path or URL to a GGUF model file. Accepts a local file path, a `huggingface:` path, or an `https://` URL.
///     headers: Optional dict of HTTP headers to include in the download request (e.g. `{"Authorization": "Bearer hf_..."}`).
///     on_download_progress: Optional callable invoked during downloads with `(downloaded_bytes, total_bytes)`.
///
/// Returns:
///     Local path to the downloaded model file, which can be passed to `Model` or `Chat`.
///
/// Raises:
///     RuntimeError: If the download fails
#[pyfunction]
#[pyo3(signature = (model_path, headers=None, on_download_progress: "typing.Callable[[int, int], None] | None" = None))]
fn download_model(
    model_path: std::path::PathBuf,
    headers: Option<std::collections::HashMap<String, String>>,
    on_download_progress: Option<Py<PyAny>>,
) -> PyResult<std::path::PathBuf> {
    let path_str = model_path.to_str().ok_or_else(|| {
        pyo3::exceptions::PyValueError::new_err(format!(
            "Path contains invalid UTF-8: {}",
            model_path.display()
        ))
    })?;
    let headers_vec: Vec<(String, String)> = headers.unwrap_or_default().into_iter().collect();
    let progress = resolve_on_download_progress(on_download_progress)?;
    nobodywho::llm::download_model(path_str, headers_vec, progress)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(render_miette(&e)))
}

/// `SamplerConfig` contains the configuration for a token sampler. The mechanism by which
/// NobodyWho will sample a token from the probability distribution, to include in the
/// generation result.
/// A `SamplerConfig` can be constructed either using a preset function from the `SamplerPresets`
/// class, or by manually constructing a sampler chain using the `SamplerBuilder` class.
/// `SamplerConfig` supports serialization to/from JSON via `to_json()` and `from_json()`.
#[pyclass(from_py_object)]
#[derive(Clone, Default)]
pub struct SamplerConfig {
    sampler_config: nobodywho::sampler::SamplerConfig,
}

#[pymethods]
impl SamplerConfig {
    /// Serialize the sampler configuration to a JSON string.
    ///
    /// Returns:
    ///     A JSON string representing this sampler configuration
    ///
    /// Raises:
    ///     RuntimeError: If serialization fails
    pub fn to_json(&self) -> PyResult<String> {
        serde_json::to_string(&self.sampler_config)
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }

    /// Deserialize a sampler configuration from a JSON string.
    ///
    /// Args:
    ///     json_str: A JSON string representing a sampler configuration
    ///
    /// Returns:
    ///     A SamplerConfig instance
    ///
    /// Raises:
    ///     ValueError: If the JSON is invalid or doesn't represent a valid sampler configuration
    #[staticmethod]
    pub fn from_json(json_str: &str) -> PyResult<Self> {
        let sampler_config: nobodywho::sampler::SamplerConfig = serde_json::from_str(json_str)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self { sampler_config })
    }

    fn __repr__(&self) -> PyResult<String> {
        self.to_json()
    }
}

/// `SamplerBuilder` is used to manually construct a sampler chain.
/// A sampler chain consists of any number of probability-shifting steps, and a single sampling step.
/// Probability-shifting steps are operations that transform the probability distribution of next
/// tokens, as generated by the model. E.g. the top_k step will zero the probability of all tokens
/// that aren't among the top K most probable (where K is some integer).
/// A sampling step is a final step that selects a single token from the probability distribution
/// that results from applying all of the probability-shifting steps in order.
/// E.g. the `dist` sampling step selects a token with weighted randomness, and the
/// `greedy` sampling step always selects the most probable.
#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct SamplerBuilder {
    inner: nobodywho::sampler::SamplerBuilder,
}

impl Default for SamplerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[pymethods]
impl SamplerBuilder {
    /// Create a new SamplerBuilder to construct a custom sampler chain.
    #[new]
    pub fn new() -> Self {
        Self {
            inner: nobodywho::sampler::SamplerBuilder::new(),
        }
    }

    /// Keep only the top K most probable tokens. Typical values: 40-50.
    ///
    /// Args:
    ///     top_k: Number of top tokens to keep
    pub fn top_k(&self, top_k: i32) -> Self {
        shift_step(self.clone(), nobodywho::sampler::ShiftStep::TopK { top_k })
    }

    /// Keep tokens whose cumulative probability is below top_p. Typical values: 0.9-0.95.
    ///
    /// Args:
    ///     top_p: Cumulative probability threshold (0.0 to 1.0)
    ///     min_keep: Minimum number of tokens to always keep
    pub fn top_p(&self, top_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::TopP { top_p, min_keep },
        )
    }

    /// Keep tokens with probability above min_p * (probability of most likely token).
    ///
    /// Args:
    ///     min_p: Minimum relative probability threshold (0.0 to 1.0). Typical: 0.05-0.1.
    ///     min_keep: Minimum number of tokens to always keep
    pub fn min_p(&self, min_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::MinP { min_p, min_keep },
        )
    }

    /// XTC (eXclude Top Choices) sampler that probabilistically excludes high-probability tokens.
    /// This can increase output diversity by sometimes forcing the model to pick less obvious tokens.
    ///
    /// Args:
    ///     xtc_probability: Probability of applying XTC on each token (0.0 to 1.0)
    ///     xtc_threshold: Tokens with probability above this threshold may be excluded (0.0 to 1.0)
    ///     min_keep: Minimum number of tokens to always keep (prevents excluding all tokens)
    pub fn xtc(&self, xtc_probability: f32, xtc_threshold: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::XTC {
                xtc_probability,
                xtc_threshold,
                min_keep,
            },
        )
    }

    /// Set the RNG seed used by random samplers (`dist`, `mirostat_v1`, `mirostat_v2`, `xtc`).
    /// `greedy` ignores it. If unset, a default seed is used.
    pub fn seed(&self, seed: u32) -> Self {
        SamplerBuilder {
            inner: self.inner.clone().seed(seed),
        }
    }

    /// Typical sampling: keeps tokens close to expected information content.
    ///
    /// Args:
    ///     typ_p: Typical probability mass (0.0 to 1.0). Typical: 0.9.
    ///     min_keep: Minimum number of tokens to always keep
    pub fn typical_p(&self, typ_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::TypicalP { typ_p, min_keep },
        )
    }

    /// Apply a GBNF grammar constraint to enforce structured output.
    ///
    /// Deprecated: Use `SamplerPresets.constrain_with_grammar()` instead. It accepts both Lark and GBNF strings.
    ///
    /// Args:
    ///     grammar: Grammar specification in GBNF format (GGML BNF, a variant of BNF used by llama.cpp)
    ///     trigger_on: Optional string that, when generated, activates the grammar constraint.
    ///                 Useful for letting the model generate free-form text until a specific marker.
    ///     root: Name of the root grammar rule to start parsing from
    #[allow(deprecated)]
    pub fn grammar(&self, grammar: String, trigger_on: Option<String>, root: String) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::Grammar {
                grammar,
                trigger_on,
                root,
            },
        )
    }

    /// DRY (Don't Repeat Yourself) sampler to reduce repetition.
    ///
    /// Args:
    ///     multiplier: Penalty strength multiplier
    ///     base: Base penalty value
    ///     allowed_length: Maximum allowed repetition length
    ///     penalty_last_n: Number of recent tokens to consider
    ///     seq_breakers: List of strings that break repetition sequences
    pub fn dry(
        &self,
        multiplier: f32,
        base: f32,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: Vec<String>,
    ) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::DRY {
                multiplier,
                base,
                allowed_length,
                penalty_last_n,
                seq_breakers,
            },
        )
    }

    /// Apply repetition penalties to discourage repeated tokens.
    ///
    /// Args:
    ///     penalty_last_n: Number of recent tokens to penalize (0 = disable)
    ///     penalty_repeat: Base repetition penalty (1.0 = no penalty, >1.0 = penalize)
    ///     penalty_freq: Frequency penalty based on token occurrence count
    ///     penalty_present: Presence penalty for any token that appeared before
    pub fn penalties(
        &self,
        penalty_last_n: i32,
        penalty_repeat: f32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::Penalties {
                penalty_last_n,
                penalty_repeat,
                penalty_freq,
                penalty_present,
            },
        )
    }

    /// Apply temperature scaling to the probability distribution.
    ///
    /// Args:
    ///     temperature: Temperature value (0.0 = deterministic, 1.0 = unchanged, >1.0 = more random)
    pub fn temperature(&self, temperature: f32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::Temperature { temperature },
        )
    }

    /// Sample from the probability distribution (weighted random selection).
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    pub fn dist(&self) -> SamplerConfig {
        sample_step(self.clone(), nobodywho::sampler::SampleStep::Dist)
    }

    /// Always select the most probable token (deterministic).
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    pub fn greedy(&self) -> SamplerConfig {
        sample_step(self.clone(), nobodywho::sampler::SampleStep::Greedy)
    }

    /// Use Mirostat v1 algorithm for perplexity-controlled sampling.
    /// Mirostat dynamically adjusts sampling to maintain a target "surprise" level,
    /// producing more coherent output than fixed temperature. Good for long-form generation.
    ///
    /// Args:
    ///     tau: Target perplexity/surprise value (typically 3.0-5.0; lower = more focused)
    ///     eta: Learning rate for perplexity adjustment (typically 0.1)
    ///     m: Number of candidates to consider (typically 100)
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    pub fn mirostat_v1(&self, tau: f32, eta: f32, m: i32) -> SamplerConfig {
        sample_step(
            self.clone(),
            nobodywho::sampler::SampleStep::MirostatV1 { tau, eta, m },
        )
    }

    /// Use Mirostat v2 algorithm for perplexity-controlled sampling.
    /// Mirostat v2 is a simplified version of Mirostat that's often preferred.
    /// It dynamically adjusts sampling to maintain a target "surprise" level.
    ///
    /// Args:
    ///     tau: Target perplexity/surprise value (typically 3.0-5.0; lower = more focused)
    ///     eta: Learning rate for perplexity adjustment (typically 0.1)
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    pub fn mirostat_v2(&self, tau: f32, eta: f32) -> SamplerConfig {
        sample_step(
            self.clone(),
            nobodywho::sampler::SampleStep::MirostatV2 { tau, eta },
        )
    }
}

fn shift_step(builder: SamplerBuilder, step: nobodywho::sampler::ShiftStep) -> SamplerBuilder {
    SamplerBuilder {
        inner: builder.inner.shift(step),
    }
}

fn sample_step(builder: SamplerBuilder, step: nobodywho::sampler::SampleStep) -> SamplerConfig {
    SamplerConfig {
        sampler_config: builder.inner.sample(step),
    }
}

/// `SamplerPresets` is a static class which contains a bunch of functions to easily create a
/// `SamplerConfig` from some pre-defined sampler chain.
/// E.g. `SamplerPresets.temperature(0.8)` will return a `SamplerConfig` with temperature=0.8.
#[pyclass]
pub struct SamplerPresets {}

#[pymethods]
impl SamplerPresets {
    /// Get the default sampler configuration.
    #[staticmethod]
    #[allow(clippy::should_implement_trait)]
    pub fn default() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerConfig::default(),
        }
    }

    /// Create a sampler with top-k filtering only.
    ///
    /// Args:
    ///     top_k: Number of top tokens to keep
    #[staticmethod]
    pub fn top_k(top_k: i32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::top_k(top_k),
        }
    }

    /// Create a sampler with nucleus (top-p) sampling.
    ///
    /// Args:
    ///     top_p: Cumulative probability threshold (0.0 to 1.0)
    #[staticmethod]
    pub fn top_p(top_p: f32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::top_p(top_p),
        }
    }

    /// Create a greedy sampler (always picks most probable token).
    #[staticmethod]
    pub fn greedy() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::greedy(),
        }
    }

    /// Create a sampler with temperature scaling.
    ///
    /// Args:
    ///     temperature: Temperature value (lower = more focused, higher = more random)
    #[staticmethod]
    pub fn temperature(temperature: f32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::temperature(temperature),
        }
    }

    /// Create a DRY sampler preset to reduce repetition.
    #[staticmethod]
    pub fn dry() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::dry(),
        }
    }

    /// Create a sampler that constrains output to a JSON schema via llguidance.
    ///
    /// Args:
    ///     schema: JSON schema as a dict or a JSON string
    #[staticmethod]
    pub fn constrain_with_json_schema(schema: &Bound<'_, PyAny>) -> PyResult<SamplerConfig> {
        let schema_str: String = if let Ok(s) = schema.extract::<String>() {
            s
        } else {
            schema
                .py()
                .import("json")?
                .call_method1("dumps", (schema,))?
                .extract::<String>()?
        };
        Ok(SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::constrain_with_json_schema(
                schema_str,
            ),
        })
    }

    /// Create a sampler that constrains output to a regular expression via llguidance.
    ///
    /// Args:
    ///     pattern: Regular expression pattern
    #[staticmethod]
    pub fn constrain_with_regex(pattern: String) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::constrain_with_regex(pattern),
        }
    }

    /// Create a sampler that constrains output using a Lark grammar via llguidance.
    ///
    /// Args:
    ///     grammar: Lark grammar string
    #[staticmethod]
    pub fn constrain_with_grammar(grammar: String) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::constrain_with_grammar(grammar),
        }
    }

    /// Create a sampler that constrains output to valid JSON (any structure) using GBNF.
    ///
    /// For schema-validated JSON, use `constrain_with_json_schema()` instead.
    #[staticmethod]
    #[allow(deprecated)]
    pub fn json() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::json(),
        }
    }

    /// Deprecated: Use `SamplerPresets.constrain_with_grammar()` instead. It accepts both Lark and GBNF strings.
    #[staticmethod]
    #[allow(deprecated)]
    pub fn grammar(grammar: String) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::grammar(grammar),
        }
    }
}

/// A `Tool` is a wrapped python function, that can be passed as a tool for the model to call.
/// `Tool`s are constructed using the `@tool` decorator.
#[pyclass(from_py_object)]
pub struct Tool {
    tool: nobodywho::tool_calling::Tool,
    pyfunc: Py<PyAny>,
}

impl Clone for Tool {
    fn clone(&self) -> Self {
        Python::attach(|py| Self {
            tool: self.tool.clone(),
            pyfunc: self.pyfunc.clone_ref(py),
        })
    }
}

#[pymethods]
impl Tool {
    #[pyo3(signature = (*args, **kwargs) -> "T")]
    fn __call__(
        &self,
        args: &Bound<pyo3::types::PyTuple>,
        kwargs: Option<&Bound<pyo3::types::PyDict>>,
        py: Python,
    ) -> PyResult<Py<PyAny>> {
        self.pyfunc.call(py, args, kwargs)
    }
}

/// Context usage statistics returned by `Chat.stats()` and `ChatAsync.stats()`.
#[pyclass(get_all)]
pub struct ChatStats {
    /// The maximum number of tokens the context window can hold.
    pub context_size: u32,
    /// The number of tokens currently used in the context (KV cache position).
    pub context_used: u32,
}

#[pymethods]
impl ChatStats {
    fn __repr__(&self) -> String {
        format!(
            "ChatStats(context_size={}, context_used={})",
            self.context_size, self.context_used
        )
    }
}

/// A `Text` prompt part, used to build multimodal `Prompt`s.
///
/// Example:
///     prompt = Prompt([Text("Describe this"), Image("./img.jpg")])
#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct Text {
    text: String,
}

#[pymethods]
impl Text {
    #[new]
    pub fn new(text: String) -> Self {
        Self { text }
    }

    #[getter]
    pub fn text(&self) -> String {
        self.text.clone()
    }

    fn __repr__(&self) -> String {
        format!("Text({:?})", self.text)
    }
}

/// An `Image` prompt part, used to build multimodal `Prompt`s.
///
/// Example:
///     prompt = Prompt([Text("Describe this"), Image("./img.jpg")])
#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct Image {
    path: String,
}

#[pymethods]
impl Image {
    #[new]
    #[pyo3(signature = (path: "os.PathLike | str") -> "Image")]
    pub fn new(path: std::path::PathBuf) -> PyResult<Self> {
        let path_str = path.to_str().ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Path contains invalid UTF-8: {}",
                path.display()
            ))
        })?;
        Ok(Self {
            path: path_str.to_string(),
        })
    }

    #[getter]
    pub fn path(&self) -> String {
        self.path.clone()
    }

    fn __repr__(&self) -> String {
        format!("Image({:?})", self.path)
    }
}

/// An `Audio` prompt part, used to build multimodal `Prompt`s.
///
/// Example:
///     prompt = Prompt([Text("Transcribe this:"), Audio("./clip.wav")])
#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct Audio {
    path: String,
}

#[pymethods]
impl Audio {
    #[new]
    #[pyo3(signature = (path: "os.PathLike | str") -> "Audio")]
    pub fn new(path: std::path::PathBuf) -> PyResult<Self> {
        let path_str = path.to_str().ok_or_else(|| {
            pyo3::exceptions::PyValueError::new_err(format!(
                "Path contains invalid UTF-8: {}",
                path.display()
            ))
        })?;
        Ok(Self {
            path: path_str.to_string(),
        })
    }

    #[getter]
    pub fn path(&self) -> String {
        self.path.clone()
    }

    fn __repr__(&self) -> String {
        format!("Audio({:?})", self.path)
    }
}

/// A multimodal prompt consisting of interleaved `Text`, `Image`, and `Audio` parts.
///
/// Example:
///     prompt = Prompt([Text("Tell me what's in the image"), Image("./img.jpg")])
///     prompt = Prompt.from_json({"role": "user", "content": "Hello"})
#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct Prompt {
    prompt: nobodywho::tokenizer::Prompt,
}

#[pymethods]
impl Prompt {
    #[new]
    #[pyo3(signature = (parts: "list[Text | Image | Audio]" = Vec::<Py<PyAny>>::new()) -> "Prompt")]
    pub fn new(parts: Vec<Py<PyAny>>, py: Python) -> PyResult<Self> {
        let mut core_parts = Vec::new();

        for part in parts {
            let part = part.bind(py);

            if let Ok(text_part) = part.extract::<Bound<Text>>() {
                core_parts.push(nobodywho::tokenizer::PromptPart::Text(
                    text_part.borrow().text.clone(),
                ));
                continue;
            }

            if let Ok(image_part) = part.extract::<Bound<Image>>() {
                core_parts.push(nobodywho::tokenizer::PromptPart::Image(
                    image_part.borrow().path.clone().into(),
                ));
                continue;
            }

            if let Ok(audio_part) = part.extract::<Bound<Audio>>() {
                core_parts.push(nobodywho::tokenizer::PromptPart::Audio(
                    audio_part.borrow().path.clone().into(),
                ));
                continue;
            }

            return Err(pyo3::exceptions::PyTypeError::new_err(
                "Prompt parts must be Text(...), Image(...), or Audio(...)",
            ));
        }

        Ok(Self {
            prompt: nobodywho::tokenizer::Prompt::new(core_parts),
        })
    }

    #[staticmethod]
    #[pyo3(signature = (data: "object") -> "Prompt")]
    pub fn from_json(py: Python<'_>, data: Py<PyAny>) -> PyResult<Self> {
        let json_module = py.import("json")?;
        let json_str: String = json_module
            .call_method1("dumps", (data.bind(py),))?
            .extract()?;
        let value: serde_json::Value = serde_json::from_str(&json_str)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        Ok(Self {
            prompt: nobodywho::tokenizer::Prompt::from_json(value),
        })
    }
}

/// Internal helper: accept either plain text or a `Prompt`.
#[derive(FromPyObject)]
pub enum PromptOrText<'py> {
    PromptObj(Bound<'py, Prompt>),
    Text(String),
}

/// Decorator to convert a Python function into a Chat-compatible Tool instance.
///
/// The decorated function will be callable by the model during chat. The model sees the
/// function's name, description, and parameter types/descriptions to decide when to call it.
///
/// Both synchronous and asynchronous functions are supported. Async functions are executed
/// synchronously when called by the model.
///
/// Args:
///     description: A description of what the tool does (shown to the model)
///     params: Optional dict mapping parameter names to their descriptions (shown to the model)
///
/// Returns:
///     A decorator that transforms a function into a Tool instance
///
/// Examples:
///     @tool("Get the current weather for a city", params={"city": "The city name"})
///     def get_weather(city: str) -> str:
///         return f"Weather in {city}: sunny"
///
///     @tool("Fetch data from a URL", params={"url": "The URL to fetch"})
///     async def fetch_url(url: str) -> str:
///         import aiohttp
///         async with aiohttp.ClientSession() as session:
///             async with session.get(url) as response:
///                 return await response.text()
///
/// Note:
///     All function parameters must have type hints. The function should return a string.
///     Async functions (defined with 'async def') are automatically detected and handled.
#[pyfunction(signature = (description: "str", params: "dict[str, str] | None" = None) -> "typing.Callable[[typing.Callable[..., T]], Tool]")]
fn tool<'a>(
    description: String,
    params: Option<Py<pyo3::types::PyDict>>,
    py: Python<'a>,
) -> PyResult<Bound<'a, pyo3::types::PyCFunction>> {
    // extract hashmap from parameter descriptions, default to empty hashmap
    let params: std::collections::HashMap<String, String> = match params {
        Some(pd) => pd.extract(py)?,
        None => std::collections::HashMap::new(),
    };

    // the decorator returned when calling @tool(...)
    // a function that takes the native-python function and returns a callable `Tool` object
    let function_to_tool = move |args: &Bound<pyo3::types::PyTuple>,
                                 _kwargs: Option<&Bound<pyo3::types::PyDict>>|
          -> PyResult<Tool> {
        Python::attach(|py| {
            // extract the function from *args
            let fun: Py<PyAny> = args.get_item(0)?.extract()?;

            // get the name of the function
            let name = fun.getattr(py, "__name__")?.extract::<String>(py)?;

            // detect if function is async
            let inspect = PyModule::import(py, "inspect")?;
            let is_async = inspect
                .getattr("iscoroutinefunction")?
                .call1((&fun,))?
                .extract::<bool>()?;

            // generate json schema from function type annotations
            let json_schema = python_func_json_schema(py, &fun, &params)?;
            let decode_schema = json_schema.clone();

            let fun_clone = fun.clone_ref(py);

            // wrap the passed function in a json -> String function
            let wrapped_function = move |json: serde_json::Value| {
                Python::attach(|py| {
                    // construct kwargs to call the function with
                    let kwargs = match json_to_kwargs(py, json, decode_schema.to_owned()) {
                        Ok(kwargs) => kwargs,
                        Err(e) => return format!("ERROR: Failed to convert arguments: {e}"),
                    };

                    let py_result = if is_async {
                        let coroutine = match fun.call(py, (), Some(&kwargs)) {
                            Ok(coro) => coro,
                            Err(e) => return format!("ERROR: {e}"),
                        };

                        // Use Python's asyncio.run() to execute the coroutine
                        let asyncio = match py.import("asyncio") {
                            Ok(module) => module,
                            Err(e) => return format!("ERROR: Failed to import asyncio: {e}"),
                        };

                        asyncio.call_method1("run", (coroutine,)).map(|r| r.into())
                    } else {
                        fun.call(py, (), Some(&kwargs))
                    };

                    // extract a string from the result
                    // return an error string to the LLM if anything fails
                    match py_result.and_then(|r| r.extract::<String>(py)) {
                        Ok(s) => s,
                        Err(e) => format!("ERROR: {e}"),
                    }
                })
            };

            let tool = nobodywho::tool_calling::Tool::new(
                name,
                description.clone(),
                json_schema,
                std::sync::Arc::new(wrapped_function),
            );

            Ok(Tool {
                tool,
                pyfunc: fun_clone,
            })
        })
    };

    pyo3::types::PyCFunction::new_closure(py, None, None, function_to_tool)
}

/// Create a built-in tool that lets the LLM run sandboxed Python code.
///
/// The model can call this tool to execute self-contained Python snippets via the Monty
/// interpreter. No filesystem, network, or environment variable access is allowed unless
/// explicitly passed as a hardcoded value.
///
/// Args:
///     max_duration: Maximum wall-clock seconds the snippet may run. Defaults to no limit.
///     max_memory:   Maximum bytes of memory the snippet may allocate. Defaults to no limit.
///     max_recursion_depth: Maximum call-stack depth. Defaults to no limit.
///
/// Returns:
///     A Tool instance ready to pass to Chat or ChatAsync.
#[pyfunction]
#[pyo3(signature = (max_duration = None, max_memory = None, max_recursion_depth = None))]
fn python_tool(
    max_duration: Option<u64>,
    max_memory: Option<usize>,
    max_recursion_depth: Option<usize>,
    py: Python,
) -> PyResult<Tool> {
    let core_tool = nobodywho::tool_calling::Tool::python(
        max_duration.map(Duration::from_secs),
        max_memory,
        max_recursion_depth,
    );

    // Build a Python-callable wrapper so Tool.__call__(code="...") works too.
    let tool_fn = core_tool.function.clone();
    let pyfunc = pyo3::types::PyCFunction::new_closure(
        py,
        None,
        None,
        move |args: &Bound<pyo3::types::PyTuple>,
              kwargs: Option<&Bound<pyo3::types::PyDict>>|
              -> PyResult<String> {
            // Accept code as a positional or keyword argument.
            let code: String = if let Some(kw) = &kwargs {
                if let Ok(Some(val)) = kw.get_item("code") {
                    val.extract()?
                } else if !args.is_empty() {
                    args.get_item(0)?.extract()?
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "python_tool requires a 'code' argument",
                    ));
                }
            } else if !args.is_empty() {
                args.get_item(0)?.extract()?
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "python_tool requires a 'code' argument",
                ));
            };

            Ok(tool_fn(serde_json::json!({ "code": code })))
        },
    )?;

    Ok(Tool {
        tool: core_tool,
        pyfunc: pyfunc.into(),
    })
}

/// Create a bash interpreter tool that the LLM can use to run bash snippets.
///
/// Args:
///     max_commands: Maximum number of commands the snippet may execute. Defaults to no limit.
///
/// Returns:
///     A Tool instance ready to pass to Chat or ChatAsync.
#[pyfunction]
#[pyo3(signature = (max_commands = None))]
fn bash_tool(max_commands: Option<usize>, py: Python) -> PyResult<Tool> {
    let core_tool = nobodywho::tool_calling::Tool::bash(max_commands);

    let tool_fn = core_tool.function.clone();
    let pyfunc = pyo3::types::PyCFunction::new_closure(
        py,
        None,
        None,
        move |args: &Bound<pyo3::types::PyTuple>,
              kwargs: Option<&Bound<pyo3::types::PyDict>>|
              -> PyResult<String> {
            let commands: String = if let Some(kw) = &kwargs {
                if let Ok(Some(val)) = kw.get_item("commands") {
                    val.extract()?
                } else if !args.is_empty() {
                    args.get_item(0)?.extract()?
                } else {
                    return Err(pyo3::exceptions::PyTypeError::new_err(
                        "bash_tool requires a 'commands' argument",
                    ));
                }
            } else if !args.is_empty() {
                args.get_item(0)?.extract()?
            } else {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "bash_tool requires a 'commands' argument",
                ));
            };

            Ok(tool_fn(serde_json::json!({ "commands": commands })))
        },
    )?;

    Ok(Tool {
        tool: core_tool,
        pyfunc: pyfunc.into(),
    })
}

// takes a python function (assumes static types), and returns a json schema for that function
fn python_func_json_schema(
    py: Python,
    fun: &Py<PyAny>,
    param_descriptions: &std::collections::HashMap<String, String>,
) -> PyResult<serde_json::Value> {
    // import inspect (from stdlib)
    let inspect = PyModule::import(py, "inspect")?;

    // call `inspect.getfullargspec`
    // (not sure when getfullargspec was first added- but it *is* in 3.4 and later)
    let getfullargspec = inspect.getattr("getfullargspec")?;
    let argspec = getfullargspec.call((fun,), None)?;
    let annotations = argspec
        .getattr("annotations")?
        .extract::<std::collections::HashMap<String, Bound<pyo3::types::PyAny>>>()?;
    let args = argspec.getattr("args")?.extract::<Vec<String>>()?;

    // check that all arguments are annotated
    if let Some(missing_arg) = args.iter().find(|arg| !annotations.contains_key(*arg)) {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "ERROR: Parameter '{missing_arg}' is missing a type hint. NobodyWho requires all tool function parameters to have static type hints. E.g.: `{missing_arg}: str`"
        )));
    }

    // check that return type is `str`
    // the intent of this is to force people to consider how to convert to string
    if annotations
        .get("return")
        .map(|t| t.getattr("__name__").map(|n| n.to_string()))
        .transpose()?
        != Some("str".to_string())
    {
        tracing::warn!(
            "Return type of this tool should be `str`. Anything else will be cast to string, which might lead to unexpected results. It's recommended that you add a return type annotation to the tool: `-> str:`"
        );
    }

    // check that names of parameter descriptions correspond to names of actual function arguments
    if let Some(invalid_param) = param_descriptions
        .keys()
        .find(|param| !args.contains(param))
    {
        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
            "ERROR: Parameter description provided for '{invalid_param}' but function has no such parameter. Available parameters: [{}]",
            args.join(", ")
        )));
    }

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for (key, value) in annotations {
        if key == "return" {
            continue;
        }

        let type_name = if value.getattr("__args__").is_ok() {
            // It's a GenericAlias (list[int], dict[str, int], etc.)
            // Use str() to get the full representation
            value.str()?.extract::<String>()?
        } else if let Ok(name) = value.getattr("__name__") {
            // Simple type like `int`, `str`, `bool`
            name.extract::<String>()?
        } else {
            // Fallback
            value.str()?.extract::<String>()?
        };

        let mut property = match parse::type_parser(type_name.as_str()) {
            Ok((_s, value)) => value,
            Err(_) => {
                return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                    "ERROR: Tool function contains an unsupported type hint: {type_name}"
                )));
            }
        };

        // Add description if available
        if let Some(description) = param_descriptions.get(&key) {
            if let serde_json::Value::Object(ref mut obj) = property {
                obj.insert("description".to_string(), serde_json::json!(description));
            }
        }

        // add to json schema properties
        properties.insert(key.clone(), property);

        // add to list of required keys for object
        // TODO: allow optional parameters for params that have a default argument
        required.push(key);
    }

    // assemble the complete json schema for an arguments object
    let kwargs_schema = serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required
    });

    Ok(kwargs_schema)
}

// takes a sede_json::value, assumed to be an object, and returns a PyDict
fn json_to_kwargs(
    py: Python,
    json: serde_json::Value,
    json_schema: serde_json::Value,
) -> PyResult<Bound<pyo3::types::PyDict>> {
    let py_dict = pyo3::types::PyDict::new(py);

    match json {
        serde_json::Value::Object(obj) => {
            for (k, v) in obj {
                let obj_schema = match json_schema.get("properties") {
                    Some(props) => match props.get(k.clone()) {
                        Some(obj_schema) => obj_schema,
                        None => {
                            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                                "jsonschema does not contain schema for parameter: {}",
                                k.clone()
                            )));
                        }
                    },
                    None => {
                        return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                            "jsonschema is constructed incorrectly :{}",
                            json_schema
                        )));
                    }
                };
                let value_py = json_value_to_py(py, &v, obj_schema)?;
                py_dict.set_item(k, value_py)?;
            }
            Ok(py_dict)
        }
        _ =>
        // it's not an object. fail hard.
        // this branch should be impossible to hit.
        {
            Err(pyo3::exceptions::PyValueError::new_err(
                "Tool was passed some json that wasn't an object. It must be an object.",
            ))
        }
    }
}

// Helper function to convert serde_json::Value to PyObject
fn json_value_to_py<'py>(
    py: Python<'py>,
    value: &serde_json::Value,
    obj_schema: &serde_json::Value,
) -> PyResult<Py<PyAny>> {
    let obj_type = match obj_schema.get("type") {
        Some(serde_json::Value::String(obj_type)) => obj_type,
        _ => {
            return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                "jsonschema does not contain type:{}",
                obj_schema
            )));
        }
    };

    match value {
        serde_json::Value::Null => Ok(py.None()),
        serde_json::Value::Bool(b) => Ok(pyo3::types::PyBool::new(py, *b).to_owned().into()),
        serde_json::Value::Number(n) => {
            if obj_type == "number" {
                if let Some(f) = n.as_f64() {
                    Ok(pyo3::types::PyFloat::new(py, f).into())
                } else {
                    Err(pyo3::exceptions::PyValueError::new_err("Invalid number"))
                }
            } else if let Some(i) = n.as_i128() {
                Ok(pyo3::types::PyInt::new(py, i).into())
            } else if let Some(i) = n.as_u128() {
                Ok(pyo3::types::PyInt::new(py, i).into())
            } else {
                Err(pyo3::exceptions::PyValueError::new_err("Invalid number"))
            }
        }
        serde_json::Value::String(s) => Ok(pyo3::types::PyString::new(py, s).into()),
        serde_json::Value::Array(arr) => {
            let item_schema = match obj_schema.get("items") {
                Some(item_schema) => item_schema,
                _ => {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "jsonschema does not contain items schema for array:{}",
                        obj_schema
                    )));
                }
            };
            let py_items: PyResult<Vec<_>> = arr
                .iter()
                .map(|v| json_value_to_py(py, v, item_schema))
                .collect();
            // Array is actually tuple
            if let Some(serde_json::Value::Array(prefix_items)) = obj_schema.get("prefixItems") {
                let py_items: PyResult<Vec<_>> = arr
                    .iter()
                    .zip(prefix_items.iter())
                    .map(|(v, schema)| json_value_to_py(py, v, schema))
                    .collect();
                let pytuple = pyo3::types::PyTuple::new(py, py_items?);
                match pytuple {
                    Ok(tuple) => Ok(tuple.into()),
                    Err(_) => Err(pyo3::exceptions::PyValueError::new_err(
                        "Could not convert tuple",
                    )),
                }
            // Array is actually a set
            } else if obj_schema.get("uniqueItems").is_some() {
                let pyset = pyo3::types::PySet::new(py, py_items?);
                match pyset {
                    Ok(set) => Ok(set.into()),
                    Err(_) => Err(pyo3::exceptions::PyValueError::new_err("Invalid number")),
                }
            // Array is a list
            } else {
                let pylist = pyo3::types::PyList::new(py, py_items?);
                match pylist {
                    Ok(list) => Ok(list.into()),
                    Err(_) => Err(pyo3::exceptions::PyValueError::new_err("Invalid number")),
                }
            }
        }
        serde_json::Value::Object(obj) => {
            let py_dict = pyo3::types::PyDict::new(py);
            let additional_prop_schema = match obj_schema.get("additionalProperties") {
                Some(item_schema) => item_schema,
                _ => {
                    return Err(pyo3::exceptions::PyTypeError::new_err(format!(
                        "jsonschema does not contain additionalProperties schema for map:{}",
                        obj_schema
                    )));
                }
            };
            for (k, v) in obj {
                let value_py = json_value_to_py(py, v, additional_prop_schema)?;
                py_dict.set_item(k, value_py)?;
            }
            Ok(py_dict.into())
        }
    }
}

/// Returns every cached .gguf model paired with its byte size.
///
/// Returns:
///     list[tuple[str, int]]: each entry is (absolute path, size in bytes).
///
/// Raises:
///     RuntimeError: If the cache directory cannot be read
#[pyfunction]
fn get_cached_models() -> PyResult<Vec<(String, usize)>> {
    nobodywho::llm::get_cached_models()
        .map_err(|e| PyErr::new::<pyo3::exceptions::PyRuntimeError, _>(e.to_string()))?
        .into_iter()
        .map(|(path, size)| Ok((path.to_string_lossy().into_owned(), size)))
        .collect()
}

#[pymodule(name = "nobodywho")]
pub mod nobodywhopython {
    use pyo3::prelude::*;

    struct LogForwardingLayer;

    impl<S: tracing::Subscriber> tracing_subscriber::Layer<S> for LogForwardingLayer {
        fn on_event(
            &self,
            event: &tracing::Event<'_>,
            _ctx: tracing_subscriber::layer::Context<'_, S>,
        ) {
            if !crate::PYTHON_LOGGING_AVAILABLE.load(std::sync::atomic::Ordering::Acquire) {
                return;
            }

            let metadata = event.metadata();

            // Force all llama-cpp logs to TRACE level
            let level = if metadata.target().contains("llama") || metadata.target().contains("ggml")
            {
                log::Level::Trace
            } else {
                match *metadata.level() {
                    tracing::Level::ERROR => log::Level::Error,
                    tracing::Level::WARN => log::Level::Warn,
                    tracing::Level::INFO => log::Level::Info,
                    tracing::Level::DEBUG => log::Level::Debug,
                    tracing::Level::TRACE => log::Level::Trace,
                }
            };

            // Visitor that captures all fields, not just the message
            struct FieldVisitor {
                message: Option<String>,
                fields: Vec<(String, String)>,
            }

            impl tracing::field::Visit for FieldVisitor {
                fn record_str(&mut self, field: &tracing::field::Field, value: &str) {
                    if field.name() == "message" {
                        self.message = Some(value.to_string());
                    } else {
                        self.fields
                            .push((field.name().to_string(), value.to_string()));
                    }
                }

                fn record_debug(
                    &mut self,
                    field: &tracing::field::Field,
                    value: &dyn std::fmt::Debug,
                ) {
                    let formatted = format!("{:?}", value);
                    if field.name() == "message" && self.message.is_none() {
                        self.message = Some(formatted);
                    } else if field.name() != "message" {
                        self.fields.push((field.name().to_string(), formatted));
                    }
                }
            }

            let mut visitor = FieldVisitor {
                message: None,
                fields: Vec::new(),
            };
            event.record(&mut visitor);

            // Build log message with file, line, and all structured fields
            let mut log_msg = String::new();

            // Add file and line number if available
            if let (Some(file), Some(line)) = (metadata.file(), metadata.line()) {
                log_msg.push_str(&format!("{}:{} ", file, line));
            }

            // Add the main message
            if let Some(message) = visitor.message {
                log_msg.push_str(&message);
            }

            // Add structured fields
            if !visitor.fields.is_empty() {
                log_msg.push(' ');
                for (i, (key, value)) in visitor.fields.iter().enumerate() {
                    if i > 0 {
                        log_msg.push_str(", ");
                    }
                    log_msg.push_str(&format!("{}={}", key, value));
                }
            }

            if !log_msg.is_empty() {
                log::logger().log(
                    &log::Record::builder()
                        .args(format_args!("{}", log_msg))
                        .level(level)
                        .target(metadata.target())
                        .file(metadata.file())
                        .line(metadata.line())
                        .build(),
                );
            }
        }
    }

    #[pymodule_init]
    fn init(_m: &Bound<'_, PyModule>) -> PyResult<()> {
        // Ensure Python threading is properly set up for background threads
        // This is safe to call even when Python is already initialized
        pyo3::Python::initialize();

        // STEP 1: Initialize rust log -> python logging bridge FIRST
        // By setting filter to Trace, we forward all logs and let Python's logging config handle filtering
        pyo3_log::Logger::new(_m.py(), pyo3_log::Caching::LoggersAndLevels)?
            .filter(log::LevelFilter::Trace)
            .install()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;

        // STEP 2: Initialize tracing subscriber with our custom layer
        // LogForwardingLayer forwards tracing events directly to the log crate
        use tracing_subscriber::layer::SubscriberExt;
        let subscriber = tracing_subscriber::registry().with(LogForwardingLayer);

        // Try to set the global subscriber, ignore if already set (e.g., in tests)
        let _ = tracing::subscriber::set_global_default(subscriber);

        // STEP 3: Route llamacpp logs to tracing dispatcher
        // Flow: llamacpp -> tracing -> LogForwardingLayer -> log crate -> pyo3_log -> Python
        nobodywho::send_llamacpp_logs_to_tracing();

        // STEP 4: Enable log forwarding and register shutdown hook.
        // The atexit handler disables forwarding before Py_FinalizeEx so that
        // worker threads that are still alive don't call into a partially-destroyed
        // interpreter (which would cause SIGABRT).
        crate::PYTHON_LOGGING_AVAILABLE.store(true, std::sync::atomic::Ordering::Release);
        let atexit = _m.py().import("atexit")?;
        let cleanup_fn = wrap_pyfunction!(cleanup_logging, _m.py())?;
        atexit.call_method1("register", (cleanup_fn,))?;

        Ok(())
    }

    #[pyfunction]
    fn cleanup_logging() {
        crate::PYTHON_LOGGING_AVAILABLE.store(false, std::sync::atomic::Ordering::Release);
    }

    #[pymodule_export]
    use super::bash_tool;
    #[pymodule_export]
    use super::cosine_similarity;
    #[pymodule_export]
    use super::download_model;
    #[pymodule_export]
    use super::get_cached_models;
    #[pymodule_export]
    use super::python_tool;
    #[pymodule_export]
    use super::tool;
    #[pymodule_export]
    use super::Audio;
    #[pymodule_export]
    use super::Chat;
    #[pymodule_export]
    use super::ChatAsync;
    #[pymodule_export]
    use super::ChatStats;
    #[pymodule_export]
    use super::CrossEncoder;
    #[pymodule_export]
    use super::CrossEncoderAsync;
    #[pymodule_export]
    use super::Encoder;
    #[pymodule_export]
    use super::EncoderAsync;
    #[pymodule_export]
    use super::Image;
    #[pymodule_export]
    use super::Model;
    #[pymodule_export]
    use super::MtpConfig;
    #[pymodule_export]
    use super::Prompt;
    #[pymodule_export]
    use super::STTAsync;
    #[pymodule_export]
    use super::SamplerBuilder;
    #[pymodule_export]
    use super::SamplerConfig;
    #[pymodule_export]
    use super::SamplerPresets;
    #[pymodule_export]
    use super::Text;
    #[pymodule_export]
    use super::TokenStream;
    #[pymodule_export]
    use super::TokenStreamAsync;
    #[pymodule_export]
    use super::Tool;
    #[pymodule_export]
    use super::Tts;
    #[pymodule_export]
    use super::STT;
}
