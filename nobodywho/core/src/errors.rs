use llama_cpp_2::{context::kv_cache::KvCacheConversionError, TokenToStringError};

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
    ThreadCount(#[from] std::io::Error),

    #[error("Could not create context: {0}")]
    CreateContext(#[from] llama_cpp_2::LlamaContextLoadError),

    #[error("Failed getting chat template from model: {0}")]
    ChatTemplate(#[from] FromModelError),

    #[error("Failed to tokenize eos or bos tokens: {0}")]
    TokenToStringError(#[from] TokenToStringError),

    #[error("Got no response after initializing worker.")]
    NoResponse,
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
pub enum ReadError {
    #[error("Could not tokenize string: {0}")]
    Tokenizer(#[from] llama_cpp_2::StringToTokenError),

    #[error("Could not add to batch: {0}")]
    BatchAdd(#[from] llama_cpp_2::llama_batch::BatchAddError),

    #[error("Llama.cpp failed decoding: {0}")]
    Decode(#[from] llama_cpp_2::DecodeError),
}

// CrossEncoderWorker errors

#[derive(Debug, thiserror::Error)]
pub enum CrossEncoderWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorker(#[from] InitWorkerError),

    #[error("Error reading string: {0}")]
    Read(#[from] ReadError),

    #[error("Error getting classification score: {0}")]
    Classification(String),
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

// ChatWorker errors

#[derive(thiserror::Error, Debug)]
pub(crate) enum ChatWorkerError {
    #[error("Error initializing worker: {0}")]
    InitWorker(#[from] InitWorkerError),

    #[error("Error reading string: {0}")]
    Say(#[from] SayError),

    #[error("Init template error: {0}")]
    Template(#[from] FromModelError),

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
}

#[derive(Debug, thiserror::Error)]
pub enum WrappedResponseError {
    #[error("Error during context shift: {0}")]
    Shift(#[from] ShiftError),

    #[error("Error rendering chat history with chat template: {0}")]
    Render(#[from] RenderError),

    #[error("Error removing tokens not present in the common prefix: {0}")]
    KVCacheUpdate(#[from] KvCacheConversionError),

    #[error("Error syncing context and reading prompt: {0}")]
    ReadError(#[from] ContextSyncError),

    #[error("Error while generating response: {0}")]
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
#[derive(Debug, thiserror::Error)]

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

    #[error("Lazy GBNF grammar was specified, but the trigger token does not cleanly tokenize with the given model. You most likely tried to do tool calling with a model that doesn't natively support tool calling.")]
    UnsupportedToolCallingTokenization,

    #[error("Could not initialize lazy grammar: {0}")]
    LazyGrammarError(#[from] llama_cpp_2::GrammarError),
}

#[derive(Debug, thiserror::Error)]
pub enum DecodingError {
    #[error("Could not add token to batch: {0}")]
    BatchAdd(#[from] llama_cpp_2::llama_batch::BatchAddError),

    #[error("Llama.cpp failed decoding: {0}")]
    Decode(#[from] llama_cpp_2::DecodeError),
}

#[derive(Debug, thiserror::Error)]
pub enum SayError {
    #[error("Error getting response: {0}")]
    Response(#[from] std::sync::mpsc::RecvError),

    #[error("Error finding token difference: {0}")]
    Render(#[from] RenderError),

    #[error("Error creating response: {0}")]
    WrappedResponse(#[from] WrappedResponseError),
}

#[derive(Debug, thiserror::Error)]
pub enum ShiftError {
    #[error("Missing expected message {0}")]
    Message(String),

    #[error("Could not tokenize template render {0}")]
    StringToToken(#[from] llama_cpp_2::StringToTokenError),

    #[error("Could not render messages with template {0}")]
    TemplateRender(#[from] minijinja::Error),

    #[error("Error reading token render into model {0}")]
    KVCacheUpdate(#[from] ReadError),
}

#[derive(Debug, thiserror::Error)]
pub enum ContextSyncError {
    #[error("Error removing tokens from context {0}")]
    KvCacheConversionError(#[from] KvCacheConversionError),

    #[error("Could not tokenize template render {0}")]
    StringToToken(#[from] llama_cpp_2::StringToTokenError),

    #[error("Could not render messages {0}")]
    TemplateRender(#[from] RenderError),

    #[error("Error reading token render into model {0}")]
    KVCacheUpdate(#[from] ReadError),
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("Template failed to render: {0}")]
    MiniJinja(#[from] minijinja::Error),

    #[error("Could not tokenize string: {0}")]
    CreateContext(#[from] llama_cpp_2::StringToTokenError),
}

#[derive(Debug, thiserror::Error)]
pub enum FromModelError {
    #[error("Lama.cpp failed fetching chat template from the model file. This is likely because you're using an older GGUF file, which might not include a chat template. For example, this is the case for most LLaMA2-based GGUF files. Try using a more recent GGUF model file. If you want to check if a given model includes a chat template, you can use the gguf-dump script from llama.cpp. Here is a more technical detailed error: {0}")]
    ChatTemplate(#[from] llama_cpp_2::ChatTemplateError),

    #[error("Could not parse chat template as UTF8: {0}")]
    TemplateUtf8(#[from] std::str::Utf8Error),

    #[error("Could not detokenize string: {0}")]
    Detokenize(#[from] llama_cpp_2::TokenToStringError),

    #[error("Tools were provided, but it looks like this model doesn't support tool calling.")]
    NoToolTemplate,
}

#[derive(Debug, thiserror::Error)]
pub enum CompletionError {
    #[error("Worker thread terminated before completing the response. This usually indicates an error occurred during token generation (e.g., context shift failure, sampling error, or token decoding issue).")]
    WorkerCrashed,
}
