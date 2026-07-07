use llama_cpp_2::{context::kv_cache::KvCacheConversionError, TokenToStringError};
use std::path::PathBuf;

// Memory errors

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum MemoryError {
    #[error(
        "Not enough memory for context. Required: ~{required_gb:.1} GB, available: ~{available_gb:.1} GB"
    )]
    #[diagnostic(code(nobodywho::insufficient_memory), help("{suggestion}"))]
    InsufficientMemory {
        required_gb: f64,
        available_gb: f64,
        suggestion: String,
    },
}

// Model errors

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum LoadModelError {
    #[error("Model not found: {path}")]
    #[diagnostic(
        code(nobodywho::model_not_found),
        help("Couldn't locate the directory - resolved to: {resolved}")
    )]
    ModelNotFoundDir { path: String, resolved: String },

    #[error("Model not found: {path}")]
    #[diagnostic(
        code(nobodywho::model_not_found),
        help(
            "The directory exists, but '{filename}' could not be found - resolved to: {resolved}"
        )
    )]
    ModelNotFoundFile {
        path: String,
        filename: String,
        resolved: String,
    },
    #[error("Failed to load model metadata for: {path}")]
    #[diagnostic(
        code(nobodywho::not_a_gguf_file),
        help(
            "It seems '{filename}' is not a .gguf file - nobodywho only accepts GGUF models for LLMs.\n\
             Search for a GGUF model on https://huggingface.co/models?library=gguf&sort=likes\n\
             or browse nobodywho's supported models at https://huggingface.co/NobodyWho"
        )
    )]
    NotAGgufFile { path: String, filename: String },

    #[error("Cannot read model: {path} is a directory")]
    #[diagnostic(
        code(nobodywho::model_is_directory),
        help("Pass a path to a .gguf file, not a directory")
    )]
    IsADirectory { path: String },

    #[error("Permission denied reading model: {path}")]
    #[diagnostic(
        code(nobodywho::model_permission_denied),
        help("Check that the file is readable by the current user")
    )]
    PermissionDenied { path: String },

    #[error("Failed to load model: {path}")]
    #[diagnostic(
        code(nobodywho::model_load_failed),
        help(
            "llama.cpp could not load the model. Common causes:\n\
             - The model is too large for available memory — try a smaller or more quantized version (e.g. Q4_K_M instead of Q8_0)\n\
             - The model architecture is not supported by the current llama.cpp version\n\
             - The model file is corrupted"
        )
    )]
    ModelLoadFailed { path: String },

    #[error("Invalid or unsupported GGUF model: {0}")]
    InvalidModel(String),
    #[error("Multimodal error: {0}")]
    Multimodal(#[from] MultimodalError),
    #[error("Channel for receiving model was closed unexpectedly")]
    ModelChannelError,
    #[error("Failed parsing model path: {0}")]
    FailedParsingModelPath(#[from] nom::Err<nom::error::Error<String>>),
    #[error("Failed to download model: authentication required")]
    #[diagnostic(
        code(nobodywho::download_unauthorized),
        help(
            "This could mean:\n\
             1. The repo or file does not exist - check the owner, repo, and filename\n\
             2. The model is gated and requires authentication:\n\
             \n\
             download_model(\"hf://...\", headers={{\"Authorization\": \"Bearer YOUR_TOKEN\"}})\n\
             \n\
             Get a token at https://huggingface.co/settings/tokens"
        )
    )]
    DownloadUnauthorized { url: String },

    #[error("Failed to download model: access denied")]
    #[diagnostic(
        code(nobodywho::download_forbidden),
        help(
            "You need to accept this model's license AND authenticate to download it.\n\
             Accept the license at: {model_page_url}\n\
             \n\
             Then pass your HuggingFace token as a header:\n\
             \n\
             download_model(\"hf://...\", headers={{\"Authorization\": \"Bearer YOUR_TOKEN\"}})\n\
             \n\
             Get a token at https://huggingface.co/settings/tokens"
        )
    )]
    DownloadForbidden { url: String, model_page_url: String },

    #[error("Failed to download model: not found")]
    #[diagnostic(
        code(nobodywho::download_not_found),
        help(
            "Check that the owner, repo, and filename are correct.\n\
             Expected format: hf://owner/repo/filename.gguf\n\
             Browse available GGUF models at https://huggingface.co/models?library=gguf"
        )
    )]
    DownloadNotFound { url: String },

    #[error("Failed to download model: unexpected HTTP status {status} for {url}")]
    #[diagnostic(code(nobodywho::download_http_status))]
    DownloadHttpStatus { url: String, status: u16 },

    #[error("HTTP request failed: {url}")]
    #[diagnostic(code(nobodywho::download_http_request))]
    HttpRequest {
        url: String,
        #[source]
        source: ureq::Error,
    },

    #[error("Path traversal detected: {path:?} contains '..'")]
    #[diagnostic(
        code(nobodywho::download_path_traversal),
        help("Model paths must not contain '..' — sanitize the input before passing it in.")
    )]
    PathTraversal { path: PathBuf },

    #[error("Failed to create cache directory {path:?}")]
    #[diagnostic(code(nobodywho::download_create_cache_dir))]
    CreateCacheDir {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Failed to create temporary download file {path:?}")]
    #[diagnostic(code(nobodywho::download_create_temp_file))]
    CreateTempFile {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Read error while downloading {url}")]
    #[diagnostic(code(nobodywho::download_read))]
    ReadDownload {
        url: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Write error while downloading to {path:?}")]
    #[diagnostic(code(nobodywho::download_write))]
    WriteDownload {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Download incomplete from {url}: got {got}/{expected} bytes")]
    #[diagnostic(code(nobodywho::download_incomplete))]
    IncompleteDownload {
        url: String,
        got: u64,
        expected: u64,
    },

    #[error("Failed to rename {from:?} to {to:?}")]
    #[diagnostic(code(nobodywho::download_rename_temp))]
    RenameTempFile {
        from: PathBuf,
        to: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("Could not determine cache directory: {0}")]
    CacheDir(#[from] GetCacheDirError),
}

#[derive(Debug, thiserror::Error)]
pub enum GetCacheDirError {
    #[error("Could not determine cache directory")]
    NoCacheDir,
    #[cfg(target_os = "android")]
    #[error("Failed to read /proc/self/cmdline: {0}")]
    ReadCmdline(#[from] std::io::Error),
    #[cfg(target_os = "android")]
    #[error("Could not determine Android package name from /proc/self/cmdline")]
    NoPackageName,
}

#[derive(Debug, thiserror::Error)]
pub enum GetCachedModelsError {
    #[error("Could not determine cache directory: {0}")]
    CacheDir(#[from] GetCacheDirError),
    #[error("Failed to walk cache directory: {0}")]
    Walk(#[from] walkdir::Error),
}

fn normalize_path(path: &std::path::Path) -> std::path::PathBuf {
    use std::path::Component;
    let mut result = std::path::PathBuf::new();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                result.pop();
            }
            Component::CurDir => {}
            c => result.push(c),
        }
    }
    result
}

fn extract_hf_model_page(url: &str) -> Option<String> {
    let path = url.strip_prefix("https://huggingface.co/")?;
    let mut parts = path.splitn(3, '/');
    let owner = parts.next()?;
    let repo = parts.next()?;
    Some(format!("https://huggingface.co/{owner}/{repo}"))
}

impl LoadModelError {
    pub fn from_http_status(url: &str, status: u16) -> Self {
        match status {
            401 => LoadModelError::DownloadUnauthorized {
                url: url.to_owned(),
            },
            403 => {
                let model_page_url = extract_hf_model_page(url).unwrap_or_else(|| url.to_owned());
                LoadModelError::DownloadForbidden {
                    url: url.to_owned(),
                    model_page_url,
                }
            }
            404 => LoadModelError::DownloadNotFound {
                url: url.to_owned(),
            },
            _ => LoadModelError::DownloadHttpStatus {
                url: url.to_owned(),
                status,
            },
        }
    }

    pub fn validate_model_file(fs_path: &std::path::Path) -> Result<(), Self> {
        use std::io::Read;

        if fs_path.is_dir() {
            return Err(LoadModelError::IsADirectory {
                path: fs_path.to_string_lossy().into_owned(),
            });
        }

        let mut file = std::fs::File::open(fs_path).map_err(|e| match e.kind() {
            std::io::ErrorKind::PermissionDenied => LoadModelError::PermissionDenied {
                path: fs_path.to_string_lossy().into_owned(),
            },
            _ => LoadModelError::from_missing_path(fs_path),
        })?;

        let mut magic = [0u8; 4];
        let is_gguf = file.read_exact(&mut magic).is_ok() && &magic == b"GGUF";
        if !is_gguf {
            let path = fs_path.to_string_lossy().into_owned();
            let filename = fs_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_else(|| path.clone());
            return Err(LoadModelError::NotAGgufFile { path, filename });
        }

        Ok(())
    }

    pub fn from_missing_path(fs_path: &std::path::Path) -> Self {
        let path = fs_path.to_string_lossy().into_owned();

        let parent = fs_path.parent().unwrap_or(std::path::Path::new("."));
        let parent = if parent.as_os_str().is_empty() {
            std::path::Path::new(".")
        } else {
            parent
        };

        if parent.exists() {
            let filename = fs_path
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default();
            let resolved = std::fs::canonicalize(parent)
                .map(|p| p.join(&filename).display().to_string())
                .unwrap_or_else(|_| path.clone());
            LoadModelError::ModelNotFoundFile {
                path,
                filename,
                resolved,
            }
        } else {
            let resolved = std::path::absolute(fs_path)
                .map(|p| normalize_path(&p).display().to_string())
                .unwrap_or_else(|_| path.clone());
            LoadModelError::ModelNotFoundDir { path, resolved }
        }
    }
}

// Worker errors

// Generic worker errors

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum InitWorkerError {
    #[error("Model is not an LLM: {architecture}")]
    #[diagnostic(
        code(nobodywho::not_an_llm),
        help(
            "'{architecture}' models pool token representations into embeddings - they cannot generate text.\n\
             Use nobodywho.Encoder() for embeddings, or load a generative model (e.g. Llama, Qwen, Gemma)."
        )
    )]
    NotAnLLM { architecture: String },

    #[error("Could not determine number of threads available: {0}")]
    ThreadCount(#[from] std::io::Error),

    #[error("Could not create context: {0}")]
    CreateContext(#[from] llama_cpp_2::LlamaContextLoadError),

    #[error("Chat template not found")]
    #[diagnostic(
        code(nobodywho::chat_template_not_found),
        help(
            "This is likely because you're using an older GGUF file that doesn't include a chat template.\n\
             Most LLaMA2-based GGUF files don't have one. Try using a more recent GGUF model."
        )
    )]
    ChatTemplate(#[from] SelectTemplateError),

    #[error("Failed to tokenize eos or bos tokens: {0}")]
    TokenToStringError(#[from] TokenToStringError),

    #[error("Got no response after initializing worker.")]
    NoResponse,

    #[error("Failed parsing tokenizer.ggml.add_bos field: {0}")]
    InvalidAddBosData(String),

    #[error("Failed to detect tool calling format: {0}")]
    ToolFormatDetection(#[from] crate::tool_calling::ToolFormatError),

    #[error("Could not initialize projection model: {0}")]
    ProjectionModel(#[from] MultimodalError),

    #[error("Insufficient memory for context: {0}")]
    #[diagnostic(transparent)]
    Memory(#[from] MemoryError),
}

#[derive(Debug, thiserror::Error)]
pub enum InitContextError {
    #[error("Could not determine number of threads available: {0}")]
    ThreadCount(#[from] std::io::Error),

    #[error("Could not create context: {0}")]
    CreateContext(#[from] llama_cpp_2::LlamaContextLoadError),
}

impl From<InitContextError> for InitWorkerError {
    fn from(value: InitContextError) -> Self {
        match value {
            InitContextError::ThreadCount(e) => InitWorkerError::ThreadCount(e),
            InitContextError::CreateContext(e) => InitWorkerError::CreateContext(e),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("Could not determine number of threads available: {0}")]
    ThreadCount(#[from] std::io::Error),

    #[error("Could not create context: {0}")]
    CreateContext(#[from] llama_cpp_2::LlamaContextLoadError),

    #[error("Could not initialize worker: {0}")]
    InitWorker(#[from] InitWorkerError),

    #[error("Error reading string: {0}")]
    Read(#[from] ReadError),

    #[error("Error getting embeddings: {0}")]
    Embeddings(#[from] llama_cpp_2::EmbeddingsError),

    #[error("Could not send newly generated token out to the game engine.")]
    Send, // this is actually a SendError<LLMOutput>, but that becomes recursive and weird

    #[error("Global Inference Lock was poisoned.")]
    GILPoison, // this is actually a std::sync::PoisonError<std::sync::MutexGuard<'static, ()>>, but that doesn't implement Send, so we do this
}

#[derive(Debug, thiserror::Error)]
pub enum SetterError {
    #[error("Worker terminated before processing setter: {0}")]
    SetterError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum GetterError {
    #[error("Worker terminated before processing getter: {0}")]
    GetterError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum TokenizeError {
    #[error("Worker terminated before processing tokenize request")]
    WorkerTerminated,
    #[error("Tokenization error: {0}")]
    Tokenization(#[from] TokenizationError),
    #[error("Multimodal error: {0}")]
    Multimodal(#[from] MultimodalError),
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ReadError {
    #[error("Could not add to batch: {0}")]
    BatchAdd(#[from] llama_cpp_2::llama_batch::BatchAddError),

    #[error("Llama.cpp failed decoding: {0}")]
    Decode(#[from] llama_cpp_2::DecodeError),

    #[error("Projection model not initialized")]
    ProjectionModelNotInitialized,

    #[error("Llama.cpp failed reading media embeddings: {0}")]
    FailedReadingMediaEmbeddings(#[from] llama_cpp_2::mtmd::MtmdEvalError),

    #[error("Could not tokenize string: {0}")]
    FailedToTokenize(#[from] TokenizationError),

    #[error("Input is too large for the context window: {n_tokens} tokens but n_ctx is {n_ctx}")]
    #[diagnostic(
        code(nobodywho::input_exceeds_context),
        help(
            "The message is too large to fit in the context window.\n\
             Either shorten the message, or increase n_ctx when constructing Chat."
        )
    )]
    InputExceedsContext { n_tokens: usize, n_ctx: usize },
}

// CrossEncoderWorker errors

#[derive(Debug, thiserror::Error)]
pub enum CrossEncoderWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorker(#[from] InitWorkerError),

    #[error("Error reading string: {0}")]
    Read(#[from] ReadError),

    #[error("Worker crashed while waiting for response. Enable logging for details.")]
    NoResponse,

    #[error("Llama.cpp failed getting embeddings: {0}")]
    GettingEmbeddings(#[from] llama_cpp_2::EmbeddingsError),

    #[error("Empty classification head")]
    EmptyClassificationHead,
}

// EncoderWorker errors

#[derive(Debug, thiserror::Error)]
pub enum EncoderWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorker(#[from] InitWorkerError),

    #[error("Error reading string: {0}")]
    Read(#[from] ReadError),

    #[error("Error encoding text: {0}")]
    Embeddings(#[from] llama_cpp_2::EmbeddingsError),

    #[error("Error encoding: {0}")]
    Encode(String),
}

// HuggingFace download errors

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum HuggingFaceError {
    #[error("invalid model source {0:?}: must be an existing directory or `owner/repo`")]
    #[diagnostic(code(nobodywho::hf_invalid_source))]
    InvalidSource(String),

    #[error("Could not determine cache directory")]
    CacheDir(#[from] GetCacheDirError),

    #[error("Failed to list HuggingFace repo tree for {repo:?}")]
    #[diagnostic(code(nobodywho::hf_list_repo_tree))]
    ListRepoTree {
        repo: String,
        #[source]
        source: ureq::Error,
    },

    #[error("Failed to read HuggingFace repo tree response for {repo:?}")]
    #[diagnostic(code(nobodywho::hf_read_repo_tree))]
    ReadRepoTree {
        repo: String,
        #[source]
        source: ureq::Error,
    },

    #[error("Failed to parse HuggingFace repo tree response for {repo:?}")]
    #[diagnostic(code(nobodywho::hf_parse_repo_tree))]
    ParseRepoTree {
        repo: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("HuggingFace repo {repo:?}@{revision} has no files")]
    #[diagnostic(code(nobodywho::hf_empty_repo))]
    EmptyRepo { repo: String, revision: String },

    #[error("HuggingFace repo {repo:?} is missing required file(s): {}", files.join(", "))]
    #[diagnostic(code(nobodywho::hf_missing_required_files))]
    MissingRequiredFiles { repo: String, files: Vec<String> },

    #[error("Failed to download entry {path:?} from HuggingFace repo")]
    #[diagnostic(code(nobodywho::hf_download_entry))]
    DownloadEntry {
        path: String,
        #[source]
        source: Box<LoadModelError>,
    },
}

// TTS errors

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum TtsError {
    // ── Config / language ────────────────────────────────────────────
    #[error("Language {language:?} is not supported")]
    #[diagnostic(
        code(nobodywho::tts_unsupported_language),
        help("Supported languages: {supported}")
    )]
    UnsupportedLanguage { language: String, supported: String },

    // ── Voice safetensors loading ────────────────────────────────────
    #[error("Could not read voice {voice:?}")]
    #[diagnostic(code(nobodywho::tts_voice_read))]
    VoiceRead {
        voice: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Could not parse voice {voice:?}")]
    #[diagnostic(code(nobodywho::tts_voice_parse))]
    VoiceParse {
        voice: String,
        #[source]
        source: safetensors::SafeTensorError,
    },

    #[error("Voice {voice:?} is missing the `style` tensor")]
    #[diagnostic(code(nobodywho::tts_voice_no_style))]
    VoiceMissingStyle {
        voice: String,
        #[source]
        source: safetensors::SafeTensorError,
    },

    #[error("Voice {voice:?} `style` has dtype {dtype:?}, expected F32")]
    #[diagnostic(code(nobodywho::tts_voice_bad_dtype))]
    VoiceBadDtype {
        voice: String,
        dtype: safetensors::Dtype,
    },

    #[error(
        "Voice {voice:?} `style` has shape {shape:?}, expected [rows, {style_dim}] with rows >= 2"
    )]
    #[diagnostic(code(nobodywho::tts_voice_bad_shape))]
    VoiceBadShape {
        voice: String,
        shape: Vec<usize>,
        style_dim: usize,
    },

    // ── config.json / vocab ──────────────────────────────────────────
    #[error("Could not open kokoro config {path}")]
    #[diagnostic(code(nobodywho::tts_config_open))]
    ConfigOpen {
        path: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Could not parse kokoro config {path}")]
    #[diagnostic(code(nobodywho::tts_config_parse))]
    ConfigParse {
        path: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("Kokoro vocab is empty in {path}")]
    #[diagnostic(code(nobodywho::tts_vocab_empty))]
    VocabEmpty { path: String },

    // ── espeak setup ─────────────────────────────────────────────────
    #[error("Could not create espeak data dir")]
    #[diagnostic(code(nobodywho::tts_espeak_data_dir))]
    EspeakDataDir {
        #[source]
        source: std::io::Error,
    },

    #[error("Could not install bundled espeak language {lang:?} to {dir}")]
    #[diagnostic(code(nobodywho::tts_espeak_install_lang))]
    EspeakInstallLanguage {
        lang: String,
        dir: String,
        #[source]
        source: std::io::Error,
    },

    #[error("Could not initialize espeak translator")]
    #[diagnostic(code(nobodywho::tts_espeak_init))]
    EspeakInit {
        #[source]
        source: espeak_ng::Error,
    },

    // ── phonemization ────────────────────────────────────────────────
    #[error("Espeak phonemization failed")]
    #[diagnostic(code(nobodywho::tts_espeak_phonemize))]
    EspeakPhonemize {
        #[source]
        source: espeak_ng::Error,
    },

    #[error("Espeak phonemization failed for OOV word {word:?}")]
    #[diagnostic(code(nobodywho::tts_espeak_oov))]
    EspeakOov {
        word: String,
        #[source]
        source: espeak_ng::Error,
    },

    #[error("Misaki g2p failed")]
    #[diagnostic(code(nobodywho::tts_misaki_g2p))]
    MisakiG2p {
        #[source]
        source: misaki_rs::g2p::G2PError,
    },

    // ── Output validation ────────────────────────────────────────────
    #[error("Text produced no phonemes")]
    #[diagnostic(code(nobodywho::tts_no_phonemes))]
    NoPhonemes,

    #[error("No phonemes mapped to vocab IDs")]
    #[diagnostic(code(nobodywho::tts_no_vocab_match))]
    NoVocabMatch,

    #[error("Input is {count} phonemes; max {max}")]
    #[diagnostic(
        code(nobodywho::tts_too_many_phonemes),
        help("Chunking is not yet implemented — break the text into shorter pieces.")
    )]
    TooManyPhonemes { count: usize, max: usize },

    // ── Supertonic validation ────────────────────────────────────────
    #[error("Text cannot be empty")]
    #[diagnostic(code(nobodywho::tts_empty_text))]
    EmptyText,

    #[error("Missing TTS asset: {path}")]
    #[diagnostic(code(nobodywho::tts_missing_asset))]
    MissingAsset { path: String },

    #[error("Unknown Supertonic voice '{voice}'. Available voices: {available}")]
    #[diagnostic(code(nobodywho::tts_missing_voice))]
    MissingVoice { voice: String, available: String },

    #[error("Invalid TTS asset {path}: {message}")]
    #[diagnostic(code(nobodywho::tts_invalid_asset))]
    InvalidAsset { path: String, message: String },

    #[error("Invalid TTS config: {message}")]
    #[diagnostic(code(nobodywho::tts_invalid_config))]
    InvalidConfig { message: String },

    // ── Worker thread plumbing ───────────────────────────────────────
    #[error("TTS worker thread is no longer running")]
    #[diagnostic(code(nobodywho::tts_worker_dead))]
    WorkerDead,

    // ── External error pass-through ──────────────────────────────────
    #[error("ONNX Runtime error")]
    Ort(#[from] ort::Error),

    #[error("WAV encoding failed")]
    Wav(#[from] hound::Error),

    #[error("I/O error")]
    Io(#[from] std::io::Error),

    #[error("JSON parse error")]
    Json(#[from] serde_json::Error),

    #[error("Tensor shape error")]
    Shape(#[from] ndarray::ShapeError),

    #[error("Model download failed")]
    HuggingFace(#[from] HuggingFaceError),
}

// STT errors

#[derive(Debug, thiserror::Error)]
pub enum SttError {
    #[error("Error initializing STT: {0}")]
    Init(String),

    #[error("Error during transcription: {0}")]
    Transcription(String),

    #[error("ONNX Runtime error: {0}")]
    Ort(#[from] ort::Error),

    #[error("Audio decode error: {0}")]
    Audio(String),
}

impl From<HuggingFaceError> for SttError {
    fn from(e: HuggingFaceError) -> Self {
        SttError::Init(e.to_string())
    }
}

// VAD errors

#[derive(Debug, thiserror::Error)]
pub enum VadError {
    #[error("Error initializing VAD: {0}")]
    Init(String),

    #[error("ONNX Runtime error: {0}")]
    Ort(#[from] ort::Error),

    #[error("Audio resample error: {0}")]
    Audio(String),
}

impl From<HuggingFaceError> for VadError {
    fn from(e: HuggingFaceError) -> Self {
        VadError::Init(e.to_string())
    }
}

// ChatWorker errors

#[derive(thiserror::Error, Debug)]
pub(crate) enum ChatWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorker(#[from] InitWorkerError),

    #[error("Error reading string: {0}")]
    Say(#[from] SayError),

    #[error("Init template error: {0}")]
    Template(#[from] SelectTemplateError),

    #[error("Error rendering template: {0}")]
    TemplateRender(#[from] minijinja::Error),

    #[error("Read error: {0}")]
    Read(#[from] ReadError),

    #[error("Error getting token difference: {0}")]
    Render(#[from] RenderError),

    #[error("Error removing tokens from KvCache: {0}")]
    KvCacheConversion(#[from] KvCacheConversionError),

    #[error("Error during context syncing: {0}")]
    ContextSyncError(#[from] ContextSyncError),

    #[error("Error setting tools: {0}")]
    SetTools(#[from] SetToolsError),
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum WrappedResponseError {
    #[error("Error during context shift: {0}")]
    #[diagnostic(transparent)]
    Shift(#[from] ShiftError),

    #[error("Error rendering chat history with chat template: {0}")]
    Render(#[from] RenderError),

    #[error("Error removing tokens not present in the common prefix: {0}")]
    KVCacheUpdate(#[from] KvCacheConversionError),

    #[error("Error syncing context and reading prompt: {0}")]
    #[diagnostic(transparent)]
    ReadError(#[from] ContextSyncError),

    #[error("Error while generating response: {0}")]
    #[diagnostic(transparent)]
    GenerateResponse(#[from] GenerateResponseError),

    #[error("Error receiving generated response: {0}")]
    Receive(#[from] std::sync::mpsc::RecvError),
}

#[derive(Debug, thiserror::Error)]
pub enum InferenceError {
    #[error("Error reading tokens: {0}")]
    Read(#[from] ReadError),

    #[error("Error while generating response: {0}")]
    GenerateResponse(#[from] GenerateResponseError),
}
#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum GenerateResponseError {
    #[error("Error removing tokens from context after context shift")]
    KVCacheUpdate(#[from] KvCacheConversionError),

    #[error("Error reading updated chat template render after context shift: {0}")]
    Read(#[from] ReadError),

    #[error("Error rendering template after context shift: {0}")]
    Render(#[from] RenderError),

    #[error("Error syncing context after context shift: {0}")]
    ReadError(#[from] ContextSyncError),

    #[error("Error during context shift: {0}")]
    #[diagnostic(
        code(nobodywho::context_too_small_for_response),
        help(
            "The message fits in the context but there is not enough room left for the response.\n\
             Either shorten the message or increase n_ctx when constructing Chat."
        )
    )]
    Shift(#[from] ShiftError),

    #[error("Error converting token to bytes: {0}")]
    TokenToString(#[from] llama_cpp_2::TokenToStringError),

    #[error("Error while decoding next token: {0}")]
    Decoding(#[from] DecodingError),

    #[error("Context size too small to contain generated response!")]
    ContextSize,

    #[error("Invalid sampler configuration: {0}")]
    InvalidSamplerConfig(#[from] SamplerError),
}

#[derive(Debug, thiserror::Error)]
pub enum SamplerError {
    #[error("Sample step is missing in the sampler! Maybe you did forget to add .sample() call?")]
    MissingSampleStep,

    #[error(
        "Lazy GBNF grammar was specified, but the trigger token does not cleanly tokenize with the given model. You most likely tried to do tool calling with a model that doesn't natively support tool calling."
    )]
    UnsupportedToolCallingTokenization,

    #[error("Could not initialize lazy grammar: {0}")]
    LazyGrammarError(#[from] llama_cpp_2::GrammarError),

    #[error("Could not initialize llguidance grammar: {0}")]
    LlguidanceGrammarError(llama_cpp_2::GrammarError),

    #[error("Could not convert GBNF grammar to Lark: {0}")]
    GbnfConversionError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum DecodingError {
    #[error("Could not add token to batch: {0}")]
    BatchAdd(#[from] llama_cpp_2::llama_batch::BatchAddError),

    #[error("Llama.cpp failed decoding: {0}")]
    Decode(#[from] llama_cpp_2::DecodeError),
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum SayError {
    #[error("Error getting response: {0}")]
    Response(#[from] std::sync::mpsc::RecvError),

    #[error("Error finding token difference: {0}")]
    Render(#[from] RenderError),

    #[error("Error creating response: {0}")]
    #[diagnostic(transparent)]
    WrappedResponse(#[from] WrappedResponseError),

    #[error("Tokenization error: {0}")]
    Tokenization(#[from] TokenizationError),

    #[error("Multimodal error: {0}")]
    Multimodal(#[from] MultimodalError),

    #[error("Error generating response: {0}")]
    #[diagnostic(transparent)]
    GenerateResponse(#[from] GenerateResponseError),
}

#[derive(Debug, thiserror::Error)]
pub enum MultimodalError {
    #[error("Failed to load image from '{path}': {error}")]
    LoadImage { path: String, error: String },

    #[error("Failed to load audio from '{path}': {error}")]
    LoadAudio { path: String, error: String },

    #[error("Multimodal context not initialized. Use with_mmproj() when building ChatHandle.")]
    ContextNotInitialized,

    #[error("Projection model not initialized. Use with_mmproj() when building ChatHandle.")]
    ProjectionModelNotInitialized,

    #[error("Failed to set chunk ID for bitmap: {0}")]
    FailedToSetBitmapId(#[from] std::ffi::NulError),
}

#[derive(Debug, thiserror::Error)]
pub enum TokenizationError {
    #[error("Could not tokenize string: {0}")]
    StringToToken(#[from] llama_cpp_2::StringToTokenError),

    #[error("Failed to tokenize image {image_index} of {total_images}: {error}")]
    ImageTokenizationFailed {
        image_index: usize,
        total_images: usize,
        error: String,
    },

    #[error(
        "Failed to tokenize text segment at position {position} (preview: {text_preview}): {error}"
    )]
    TextTokenizationFailed {
        position: usize,
        text_preview: String,
        error: String,
    },

    #[error("Projection model failed to tokenize image bitmap: {0}")]
    ProjectionTokenizationError(String),

    #[error(
        "Media marker mismatch: found {n_markers} media markers in template but received {n_bitmaps} media items. Each media placeholder in the prompt must have a corresponding media item.\n\nTemplate preview: {template_preview}"
    )]
    MediaMarkerMismatch {
        n_markers: usize,
        n_bitmaps: usize,
        template_preview: String,
    },
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ShiftError {
    #[error("Context shift failed: no user messages in chat history")]
    #[diagnostic(
        code(nobodywho::context_shift_no_user_messages),
        help(
            "The chat history appears to be corrupted - it contains no user messages.\n\
             Are you calling set_chat_history() with a history that has no user messages?"
        )
    )]
    NoUserMessages,

    #[error("Context shift failed: not enough messages to shift")]
    #[diagnostic(
        code(nobodywho::context_shift_too_few_messages),
        help(
            "There is likely only one large message which is larger than the context window.\n\
             Either shorten the message so it fits into the context, or increase the context size by setting a larger n_ctx."
        )
    )]
    TooFewMessages,

    #[error("Context shift failed: internal error: {0}")]
    InternalError(String),

    #[error("Could not tokenize template render {0}")]
    StringToToken(#[from] llama_cpp_2::StringToTokenError),

    #[error("Could not render messages with template {0}")]
    TemplateRender(#[from] RenderError),

    #[error("Error reading token render into model {0}")]
    #[diagnostic(transparent)]
    KVCacheUpdate(#[from] ReadError),

    #[error("Could not tokenize string: {0}")]
    Tokenize(#[from] TokenizationError),
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum ContextSyncError {
    #[error("Error removing tokens from context {0}")]
    KvCacheConversionError(#[from] KvCacheConversionError),

    #[error("Could not tokenize template render {0}")]
    StringToToken(#[from] llama_cpp_2::StringToTokenError),

    #[error("Could not render messages {0}")]
    TemplateRender(#[from] RenderError),

    #[error("Error reading token render into model {0}")]
    #[diagnostic(transparent)]
    KVCacheUpdate(#[from] ReadError),

    #[error("Error tokenizing chunks: {0}")]
    Tokenize(#[from] TokenizationError),

    #[error("Error shifting context: {0}")]
    #[diagnostic(transparent)]
    Shift(#[from] ShiftError),
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Template failed to render: {0}")]
    MiniJinja(#[from] minijinja::Error),

    #[error("Could not tokenize string: {0}")]
    CreateContext(#[from] llama_cpp_2::StringToTokenError),

    #[error("Could not tokenize string: {0}")]
    Tokenize(#[from] TokenizationError),
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum SelectTemplateError {
    #[error("{0}")]
    ChatTemplate(String),

    #[error("Could not parse chat template as UTF8: {0}")]
    TemplateUtf8(#[from] std::str::Utf8Error),

    #[error("Could not detokenize string: {0}")]
    Detokenize(#[from] llama_cpp_2::TokenToStringError),

    #[error("Could not create chat template: {0}")]
    CreateChatTemplate(#[from] minijinja::Error),

    #[error("Tools were provided, but it looks like this model doesn't support tool calling.")]
    NoToolTemplate,
}

impl From<llama_cpp_2::ChatTemplateError> for SelectTemplateError {
    fn from(e: llama_cpp_2::ChatTemplateError) -> Self {
        SelectTemplateError::ChatTemplate(e.to_string())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SetToolsError {
    #[error("Failed syncing context to include the new tools: {0}")]
    ContextSync(#[from] ContextSyncError),
    #[error("Failed selecting chat template for the new tools: {0}")]
    SelectTemplate(#[from] SelectTemplateError),
    #[error("Failed rendering chat template with the new tools: {0}")]
    Render(#[from] RenderError),
}

#[derive(Debug, thiserror::Error, miette::Diagnostic)]
pub enum CompletionError {
    #[error(
        "Worker thread terminated before completing the response. This usually indicates an error occurred during token generation (e.g., context shift failure, sampling error, or token decoding issue)."
    )]
    WorkerCrashed,

    #[error(transparent)]
    #[diagnostic(transparent)]
    WorkerError(Box<dyn miette::Diagnostic + Send + Sync + 'static>),
}
