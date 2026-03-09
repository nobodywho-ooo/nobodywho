use nobodywho::chat::{ChatBuilder, ChatHandle, Message as CoreMessage, Role as CoreRole};
use nobodywho::crossencoder::CrossEncoder as CoreCrossEncoder;
use nobodywho::embedder::Embedder as CoreEmbedder;
use nobodywho::encoder::Encoder as CoreEncoder;
use nobodywho::errors::{
    CompletionError, CrossEncoderWorkerError, EmbedderWorkerError, EncoderWorkerError,
    GetterError, LoadModelError,
};
use nobodywho::llm;
use std::sync::{Arc, Mutex};

uniffi::include_scaffolding!("nobodywho");

pub use nobodywho::send_llamacpp_logs_to_tracing as init_logging;

// ---
// Errors
//
// Each variant wraps the actual thiserror error from core rather than a
// pre-serialised String, so the full error chain is preserved on the Rust
// side.  UniFFI serialises the error message by calling Display on the
// variant's contents, which thiserror derives from the error chain.
// ---

#[derive(Debug, thiserror::Error)]
pub enum NobodyWhoError {
    #[error("{0}")]
    LoadModel(#[from] LoadModelError),

    #[error("{0}")]
    Completion(#[from] CompletionError),

    #[error("{0}")]
    CrossEncoder(CrossEncoderWorkerError),

    #[error("{0}")]
    Embedder(EmbedderWorkerError),

    #[error("{0}")]
    Encoder(EncoderWorkerError),

    #[error("{0}")]
    Worker(GetterError),
}

// ---
// Role
//
// Redefined here rather than reusing core's Role because UniFFI requires types
// to be owned by the crate that generates the scaffolding.  Conversions are
// trivial since the variants are identical.
// ---

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl From<CoreRole> for Role {
    fn from(r: CoreRole) -> Self {
        match r {
            CoreRole::User => Role::User,
            CoreRole::Assistant => Role::Assistant,
            CoreRole::System => Role::System,
            CoreRole::Tool => Role::Tool,
        }
    }
}

impl From<Role> for CoreRole {
    fn from(r: Role) -> Self {
        match r {
            Role::User => CoreRole::User,
            Role::Assistant => CoreRole::Assistant,
            Role::System => CoreRole::System,
            Role::Tool => CoreRole::Tool,
        }
    }
}

// ---
// ToolCall
// ---

#[derive(Debug, Clone)]
pub struct ToolCall {
    pub name: String,
    /// JSON-encoded arguments, matching the schema declared when the tool was registered.
    pub arguments_json: String,
}

// ---
// Message
//
// Mirrors core::chat::Message exactly, including tool call and tool response
// variants.  The previous flat { role, content } struct silently dropped tool
// call data; this enum preserves it.
// ---

#[derive(Debug, Clone)]
pub enum Message {
    Plain {
        role: Role,
        content: String,
    },
    ToolCalls {
        role: Role,
        content: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolResponse {
        role: Role,
        name: String,
        content: String,
    },
}

impl From<CoreMessage> for Message {
    fn from(msg: CoreMessage) -> Self {
        match msg {
            CoreMessage::Message { role, content } => Message::Plain {
                role: role.into(),
                content,
            },
            CoreMessage::ToolCalls {
                role,
                content,
                tool_calls,
            } => Message::ToolCalls {
                role: role.into(),
                content,
                tool_calls: tool_calls
                    .into_iter()
                    .map(|tc| ToolCall {
                        name: tc.name,
                        arguments_json: tc.arguments.to_string(),
                    })
                    .collect(),
            },
            CoreMessage::ToolResp { role, name, content } => Message::ToolResponse {
                role: role.into(),
                name,
                content,
            },
        }
    }
}

// ---
// Model
// ---

pub struct Model {
    pub(crate) inner: nobodywho::llm::Model,
}

pub fn load_model(path: String, use_gpu: bool) -> Result<Arc<Model>, NobodyWhoError> {
    let inner = llm::get_model(&path, use_gpu)?;
    Ok(Arc::new(Model { inner }))
}

// ---
// ChatConfig
// ---

#[derive(Debug, Clone)]
pub struct ChatConfig {
    pub context_size: u32,
    pub system_prompt: Option<String>,
    pub allow_thinking: bool,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            context_size: 4096,
            system_prompt: None,
            allow_thinking: true,
        }
    }
}

// ---
// TokenStream
//
// Wraps core::chat::TokenStream in a Mutex so that the struct is Sync, which
// UniFFI requires for interface types (they are accessed via Arc<T>).
// The core TokenStream is Send but not Sync because it holds an
// UnboundedReceiver, so the Mutex is necessary.
// ---

pub struct TokenStream {
    inner: Mutex<nobodywho::chat::TokenStream>,
}

impl TokenStream {
    pub fn next_token(&self) -> Option<String> {
        self.inner.lock().unwrap().next_token()
    }

    pub fn completed(&self) -> Result<String, NobodyWhoError> {
        self.inner
            .lock()
            .unwrap()
            .completed()
            .map_err(NobodyWhoError::from)
    }
}

// ---
// Chat
//
// ChatHandle is Send but not Sync (JoinHandle is not Sync), so we wrap it in
// a Mutex to satisfy UniFFI's Send + Sync requirement.
// ---

pub struct Chat {
    handle: Mutex<ChatHandle>,
}

impl Chat {
    pub fn new(model: Arc<Model>, config: ChatConfig) -> Result<Self, NobodyWhoError> {
        let handle = ChatBuilder::new(Arc::clone(&model.inner))
            .with_context_size(config.context_size)
            .with_system_prompt(config.system_prompt.as_deref())
            .with_allow_thinking(config.allow_thinking)
            .build();
        Ok(Self {
            handle: Mutex::new(handle),
        })
    }

    pub fn ask(&self, prompt: String) -> Arc<TokenStream> {
        let stream = self.handle.lock().unwrap().ask(&prompt);
        Arc::new(TokenStream {
            inner: Mutex::new(stream),
        })
    }

    pub fn history(&self) -> Result<Vec<Message>, NobodyWhoError> {
        self.handle
            .lock()
            .unwrap()
            .get_chat_history()
            .map_err(NobodyWhoError::Worker)
            .map(|msgs| msgs.into_iter().map(Message::from).collect())
    }
}

// ---
// CrossEncoder
// ---

pub struct CrossEncoder {
    inner: CoreCrossEncoder,
}

impl CrossEncoder {
    pub fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, NobodyWhoError> {
        self.inner
            .rank(query, documents)
            .map_err(NobodyWhoError::CrossEncoder)
    }

    pub fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<RankedDocument>, NobodyWhoError> {
        self.inner
            .rank_and_sort(query, documents)
            .map_err(NobodyWhoError::CrossEncoder)
            .map(|ranked| {
                ranked
                    .into_iter()
                    .map(|(content, score)| RankedDocument { content, score })
                    .collect()
            })
    }
}

pub fn load_cross_encoder(
    path: String,
    use_gpu: bool,
    context_size: u32,
) -> Result<Arc<CrossEncoder>, NobodyWhoError> {
    let model = llm::get_model(&path, use_gpu)?;
    Ok(Arc::new(CrossEncoder {
        inner: CoreCrossEncoder::new(model, context_size),
    }))
}

// ---
// RankedDocument
// ---

#[derive(Debug, Clone)]
pub struct RankedDocument {
    pub content: String,
    pub score: f32,
}

// ---
// Encoder (CLS pooling)
// ---

pub struct Encoder {
    inner: CoreEncoder,
}

impl Encoder {
    pub fn encode(&self, text: String) -> Result<Vec<f32>, NobodyWhoError> {
        self.inner.encode(text).map_err(NobodyWhoError::Encoder)
    }
}

pub fn load_encoder(
    path: String,
    use_gpu: bool,
    context_size: u32,
) -> Result<Arc<Encoder>, NobodyWhoError> {
    let model = llm::get_model(&path, use_gpu)?;
    Ok(Arc::new(Encoder {
        inner: CoreEncoder::new(model, context_size),
    }))
}

// ---
// Embedder (Mean pooling)
// ---

pub struct Embedder {
    inner: CoreEmbedder,
}

impl Embedder {
    pub fn embed(&self, text: String) -> Result<Vec<f32>, NobodyWhoError> {
        self.inner.embed(text).map_err(NobodyWhoError::Embedder)
    }

    pub fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, NobodyWhoError> {
        self.inner
            .embed_batch(texts)
            .map_err(NobodyWhoError::Embedder)
    }
}

pub fn load_embedder(
    path: String,
    use_gpu: bool,
    context_size: u32,
) -> Result<Arc<Embedder>, NobodyWhoError> {
    let model = llm::get_model(&path, use_gpu)?;
    Ok(Arc::new(Embedder {
        inner: CoreEmbedder::new(model, context_size),
    }))
}
