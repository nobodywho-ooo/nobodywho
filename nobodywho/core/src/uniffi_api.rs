//! UniFFI API bindings for Swift/Kotlin
//!
//! This module provides a simplified API surface for UniFFI to generate
//! bindings for Swift and Kotlin.

use crate::chat::{ChatBuilder, ChatHandle, Message as CoreMessage, Role as CoreRole};
use crate::crossencoder::CrossEncoder as CoreCrossEncoder;
use crate::encoder::Encoder as CoreEncoder;
use crate::errors::LoadModelError;
use crate::llm;
use std::sync::Arc;

// Re-export for UDL
pub use crate::send_llamacpp_logs_to_tracing as init_logging;

/// Unified error type for UniFFI
#[derive(Debug, thiserror::Error)]
pub enum NobodyWhoError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("Invalid or unsupported model: {0}")]
    InvalidModel(String),
    #[error("Initialization error: {0}")]
    InitializationError(String),
    #[error("Inference error: {0}")]
    InferenceError(String),
    #[error("Other error: {0}")]
    Other(String),
}

impl From<LoadModelError> for NobodyWhoError {
    fn from(err: LoadModelError) -> Self {
        match err {
            LoadModelError::ModelNotFound(path) => NobodyWhoError::ModelNotFound(path),
            LoadModelError::InvalidModel(msg) => NobodyWhoError::InvalidModel(msg),
            LoadModelError::ModelChannelError => {
                NobodyWhoError::Other("Model channel error".to_string())
            }
            LoadModelError::Multimodal(e) => NobodyWhoError::Other(e.to_string()),
        }
    }
}

impl From<Box<dyn std::error::Error + Send + Sync>> for NobodyWhoError {
    fn from(err: Box<dyn std::error::Error + Send + Sync>) -> Self {
        NobodyWhoError::Other(err.to_string())
    }
}

/// Role in a chat message
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl From<CoreRole> for Role {
    fn from(role: CoreRole) -> Self {
        match role {
            CoreRole::User => Role::User,
            CoreRole::Assistant => Role::Assistant,
            CoreRole::System => Role::System,
            CoreRole::Tool => Role::Tool,
        }
    }
}

impl From<Role> for CoreRole {
    fn from(role: Role) -> Self {
        match role {
            Role::User => CoreRole::User,
            Role::Assistant => CoreRole::Assistant,
            Role::System => CoreRole::System,
            Role::Tool => CoreRole::Tool,
        }
    }
}

/// Chat message
#[derive(Debug, Clone)]
pub struct Message {
    pub role: Role,
    pub content: String,
}

impl From<CoreMessage> for Message {
    fn from(msg: CoreMessage) -> Self {
        Message {
            role: msg.role().clone().into(),
            content: msg.content().to_string(),
        }
    }
}

/// Model wrapper for UniFFI
pub struct Model {
    inner: Arc<llm::Model>,
    path: String,
}

impl Model {
    /// Get the model file path
    pub fn path(&self) -> String {
        self.path.clone()
    }
}

/// Load a model from a GGUF file
pub fn load_model(path: String, use_gpu: bool) -> Result<Arc<Model>, NobodyWhoError> {
    let model = llm::get_model(&path, use_gpu, None)?;
    Ok(Arc::new(Model {
        inner: Arc::new(model),
        path,
    }))
}

/// Chat configuration
#[derive(Debug, Clone)]
pub struct ChatConfig {
    pub context_size: u32,
    pub system_prompt: Option<String>,
}

impl Default for ChatConfig {
    fn default() -> Self {
        Self {
            context_size: 4096,
            system_prompt: None,
        }
    }
}

/// Chat session for conversational AI
pub struct Chat {
    handle: ChatHandle,
}

impl Chat {
    /// Create a new chat session
    pub fn new(model: Arc<Model>, config: ChatConfig) -> Result<Self, NobodyWhoError> {
        let mut builder =
            ChatBuilder::new(Arc::clone(&model.inner)).with_context_size(config.context_size);

        if let Some(prompt) = config.system_prompt {
            builder = builder.with_system_prompt(Some(&prompt));
        }

        let handle = builder.build();
        Ok(Self { handle })
    }

    /// Ask a question and block until we get the complete response
    pub fn ask_blocking(&self, prompt: String) -> Result<String, NobodyWhoError> {
        self.handle
            .ask(prompt)
            .completed()
            .map_err(|e| NobodyWhoError::InferenceError(e.to_string()))
    }

    /// Get chat history
    pub fn history(&self) -> Result<Vec<Message>, NobodyWhoError> {
        let messages = self
            .handle
            .get_chat_history()
            .map_err(|e| NobodyWhoError::Other(e.to_string()))?;

        Ok(messages.into_iter().map(Message::from).collect())
    }
}

/// A document scored by a cross-encoder reranker
#[derive(Debug, Clone)]
pub struct RankedDocument {
    pub content: String,
    pub score: f32,
}

/// Embedder for generating text embeddings
pub struct Embedder {
    inner: CoreEncoder,
}

impl Embedder {
    pub fn embed(&self, text: String) -> Result<Vec<f32>, NobodyWhoError> {
        self.inner
            .encode(text)
            .map_err(|e| NobodyWhoError::InferenceError(e.to_string()))
    }

    pub fn embed_batch(&self, texts: Vec<String>) -> Result<Vec<Vec<f32>>, NobodyWhoError> {
        texts
            .into_iter()
            .map(|t| {
                self.inner
                    .encode(t)
                    .map_err(|e| NobodyWhoError::InferenceError(e.to_string()))
            })
            .collect()
    }
}

/// Load an embedder model from a GGUF file
pub fn load_embedder(
    path: String,
    use_gpu: bool,
    context_size: u32,
) -> Result<Arc<Embedder>, NobodyWhoError> {
    let model = llm::get_model(&path, use_gpu, None)?;
    Ok(Arc::new(Embedder {
        inner: CoreEncoder::new(Arc::new(model), context_size),
    }))
}

/// Cross-encoder reranker for scoring document relevance
pub struct CrossEncoder {
    inner: CoreCrossEncoder,
}

impl CrossEncoder {
    pub fn rank(&self, query: String, documents: Vec<String>) -> Result<Vec<f32>, NobodyWhoError> {
        self.inner
            .rank(query, documents)
            .map_err(|e| NobodyWhoError::InferenceError(e.to_string()))
    }

    pub fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<RankedDocument>, NobodyWhoError> {
        self.inner
            .rank_and_sort(query, documents)
            .map(|results| {
                results
                    .into_iter()
                    .map(|(content, score)| RankedDocument { content, score })
                    .collect()
            })
            .map_err(|e| NobodyWhoError::InferenceError(e.to_string()))
    }
}

/// Load a cross-encoder reranker model from a GGUF file
pub fn load_cross_encoder(
    path: String,
    use_gpu: bool,
    context_size: u32,
) -> Result<Arc<CrossEncoder>, NobodyWhoError> {
    let model = llm::get_model(&path, use_gpu, None)?;
    Ok(Arc::new(CrossEncoder {
        inner: CoreCrossEncoder::new(Arc::new(model), context_size),
    }))
}
