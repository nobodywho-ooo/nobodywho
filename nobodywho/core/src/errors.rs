// Model errors

#[derive(Debug, thiserror::Error)]
pub enum LoadModelError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Invalid or unsupported GGUF model: {0}")]
    InvalidModel(String),
}

// Worker errors

// Generic worker errors

#[derive(Debug, thiserror::Error)]
pub enum InitWorkerError {
    #[error("Could not determine number of threads available: {0}")]
    ThreadCountError(#[from] std::io::Error),

    #[error("Could not create context: {0}")]
    CreateContextError(#[from] llama_cpp_2::LlamaContextLoadError),

    #[error("Failed getting chat template from model: {0}")]
    ChatTemplateError(#[from] FromModelError),

    #[error("Got no response after initializing worker.")]
    NoResponse,
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

    #[error("Error getting embeddings: {0}")]
    EmbeddingsError(#[from] llama_cpp_2::EmbeddingsError),

    #[error("Could not send newly generated token out to the game engine.")]
    SendError, // this is actually a SendError<LLMOutput>, but that becomes recursive and weird

    #[error("Global Inference Lock was poisoned.")]
    GILPoisonError, // this is actually a std::sync::PoisonError<std::sync::MutexGuard<'static, ()>>, but that doesn't implement Send, so we do this
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

    #[error("Function was called without an inference lock")]
    NoInferenceLockError,
}

// CrossEncoderWorker errors

#[derive(Debug, thiserror::Error)]
pub(crate) enum CrossEncoderWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorkerError(#[from] InitWorkerError),

    #[error("Error reading string: {0}")]
    ReadError(#[from] ReadError),

    #[error("Error getting classification score: {0}")]
    ClassificationError(String),
}

// EmbeddingWorker errors

#[derive(Debug, thiserror::Error)]
pub(crate) enum EmbeddingsWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorkerError(#[from] InitWorkerError),

    #[error("Error reading string: {0}")]
    ReadError(#[from] ReadError),

    #[error("Error generating text: {0}")]
    EmbeddingsError(#[from] llama_cpp_2::EmbeddingsError),
}

// ChatWorker errors

#[derive(thiserror::Error, Debug)]
pub(crate) enum ChatWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorkerError(#[from] InitWorkerError),

    #[error("Error reading string: {0}")]
    SayError(#[from] SayError),

    #[error("Init template error: {0}")]
    TemplateError(#[from] FromModelError),

    #[error("Error rendering template: {0}")]
    TemplateRenderError(#[from] minijinja::Error),

    #[error("Read error: {0}")]
    ReadError(#[from] ReadError),

    #[error("Error getting token difference: {0}")]
    RenderError(#[from] RenderError),
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

    #[error("Error converting token to bytes: {0}")]
    TokenToStringError(#[from] llama_cpp_2::TokenToStringError),

    #[error("Invalid sampler configuration")]
    InvalidSamplerConfig,

    #[error("Error during context shift: {0}")]
    ShiftError(#[from] ShiftError),

    #[error("Error reading in context after context shift: {0}")]
    ReadError(#[from] ReadError),

    #[error("Error rendering template after context shift: {0}")]
    RenderError(#[from] RenderError),

    #[error("Function was called without an inference lock")]
    NoInferenceLockError,

    #[error("Context size to small to contain generated respone!")]
    ContextSizeError,
}

#[derive(Debug, thiserror::Error)]
pub enum SayError {
    #[error("Error generating text: {0}")]
    WriteError(#[from] WriteError),

    #[error("Error reading string: {0}")]
    ReadError(#[from] ReadError),

    #[error("Error getting response: {0}")]
    ResponseError(#[from] std::sync::mpsc::RecvError),

    #[error("Error rendering chat template: {0}")]
    ChatTemplateRenderError(#[from] minijinja::Error),

    #[error("Error finding token difference: {0}")]
    RenderError(#[from] RenderError),

    #[error("Error during context shift {0}")]
    ShiftError(#[from] ShiftError),
}

#[derive(Debug, thiserror::Error)]
pub enum ShiftError {
    #[error("Missing expected message {0}")]
    MessageError(String),

    #[error("Could not tokenize template render {0}")]
    StringToTokenError(#[from] llama_cpp_2::StringToTokenError),

    #[error("Could not render messages with template {0}")]
    TemplateRenderError(#[from] minijinja::Error),

    #[error("Error reading token render into model {0}")]
    KVCacheUpdateError(#[from] ReadError),
}

// ChatState errors

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Template failed to render: {0}")]
    MiniJinjaError(#[from] minijinja::Error),

    #[error("Could not tokenize string: {0}")]
    CreateContextError(#[from] llama_cpp_2::StringToTokenError),
}

#[derive(Debug, thiserror::Error)]
pub enum FromModelError {
    #[error("Lama.cpp failed fetching chat template from the model file. This is likely because you're using an older GGUF file, which might not include a chat template. For example, this is the case for most LLaMA2-based GGUF files. Try using a more recent GGUF model file. If you want to check if a given model includes a chat template, you can use the gguf-dump script from llama.cpp. Here is a more technical detailed error: {0}")]
    ChatTemplateError(#[from] llama_cpp_2::ChatTemplateError),

    #[error("Could not parse chat template as UTF8: {0}")]
    TemplateUtf8Error(#[from] std::str::Utf8Error),

    #[error("Could not detokenize string: {0}")]
    Detokenize(#[from] llama_cpp_2::TokenToStringError),

    #[error("Tools were provided, but it looks like this model doesn't support tool calling.")]
    NoToolTemplateError,
}
