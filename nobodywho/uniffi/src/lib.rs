use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

uniffi::setup_scaffolding!("nobodywho");

/// Initialize logging so tracing output goes to Android logcat.
#[cfg(target_os = "android")]
fn init_logging() {
    use std::sync::Once;
    static INIT: Once = Once::new();
    INIT.call_once(|| {
        // Set up android_logger so log crate macros go to logcat
        android_logger::init_once(
            android_logger::Config::default()
                .with_max_level(log::LevelFilter::Debug)
                .with_tag("nobodywho"),
        );
        // Bridge tracing → log crate → android_logger → logcat
        tracing_log::LogTracer::init().ok();
        log::info!("NobodyWho logging initialized");
    });
}

#[cfg(not(target_os = "android"))]
fn init_logging() {}

// ---------- Error type ----------
// UniFFI 0.30 requires a proper error type instead of bare String.

#[derive(Debug, thiserror::Error, uniffi::Error)]
#[uniffi(flat_error)]
pub enum NobodyWhoError {
    #[error("{message}")]
    Error { message: String },
}

impl From<String> for NobodyWhoError {
    fn from(message: String) -> Self {
        NobodyWhoError::Error { message }
    }
}

// ---------- Prompt types ----------

/// A part of a multimodal prompt.  Mirrors the core `PromptPart` enum.
#[derive(uniffi::Enum, Clone)]
pub enum PromptPart {
    Text { content: String },
    Image { path: String },
    Audio { path: String },
}

// ---------- Message types ----------
// Mirror types for core Message/Role/Asset/ToolCall.
// Needed because core types contain PathBuf and serde_json::Value
// which UniFFI doesn't support natively.

#[derive(uniffi::Enum, Clone)]
pub enum Role {
    User,
    Assistant,
    System,
    Tool,
}

impl From<&nobodywho::chat::Role> for Role {
    fn from(r: &nobodywho::chat::Role) -> Self {
        match r {
            nobodywho::chat::Role::User => Role::User,
            nobodywho::chat::Role::Assistant => Role::Assistant,
            nobodywho::chat::Role::System => Role::System,
            nobodywho::chat::Role::Tool => Role::Tool,
        }
    }
}

impl From<&Role> for nobodywho::chat::Role {
    fn from(r: &Role) -> Self {
        match r {
            Role::User => nobodywho::chat::Role::User,
            Role::Assistant => nobodywho::chat::Role::Assistant,
            Role::System => nobodywho::chat::Role::System,
            Role::Tool => nobodywho::chat::Role::Tool,
        }
    }
}

#[derive(uniffi::Record, Clone)]
pub struct Asset {
    pub id: String,
    pub path: String,
}

#[derive(uniffi::Record, Clone)]
pub struct ToolParameter {
    pub name: String,
    /// JSON Schema for this parameter (e.g. `{"type": "string"}`).
    pub schema: String,
}

#[derive(uniffi::Record, Clone)]
pub struct ToolCall {
    pub name: String,
    pub arguments_json: String,
}

#[derive(uniffi::Enum, Clone)]
pub enum Message {
    Standard {
        role: Role,
        content: String,
        assets: Vec<Asset>,
    },
    ToolCalls {
        role: Role,
        content: String,
        tool_calls: Vec<ToolCall>,
    },
    ToolResult {
        role: Role,
        name: String,
        content: String,
    },
}

fn core_message_to_uniffi(m: &nobodywho::chat::Message) -> Message {
    match m {
        nobodywho::chat::Message::Standard {
            role,
            content,
            assets,
        } => Message::Standard {
            role: Role::from(role),
            content: content.clone(),
            assets: assets
                .iter()
                .map(|a| Asset {
                    id: a.id.clone(),
                    path: a.path.to_string_lossy().to_string(),
                })
                .collect(),
        },
        nobodywho::chat::Message::ToolCalls {
            role,
            content,
            tool_calls,
        } => Message::ToolCalls {
            role: Role::from(role),
            content: content.clone(),
            tool_calls: tool_calls
                .iter()
                .map(|tc| ToolCall {
                    name: tc.name.clone(),
                    arguments_json: tc.arguments.to_string(),
                })
                .collect(),
        },
        nobodywho::chat::Message::ToolResult {
            role,
            name,
            content,
        } => Message::ToolResult {
            role: Role::from(role),
            name: name.clone(),
            content: content.clone(),
        },
    }
}

fn uniffi_message_to_core(m: &Message) -> Result<nobodywho::chat::Message, NobodyWhoError> {
    match m {
        Message::Standard {
            role,
            content,
            assets,
        } => Ok(nobodywho::chat::Message::Standard {
            role: nobodywho::chat::Role::from(role),
            content: content.clone(),
            assets: assets
                .iter()
                .map(|a| nobodywho::chat::Asset {
                    id: a.id.clone(),
                    path: PathBuf::from(&a.path),
                })
                .collect(),
        }),
        Message::ToolCalls {
            role,
            content,
            tool_calls,
        } => {
            let tcs: Result<Vec<_>, NobodyWhoError> = tool_calls
                .iter()
                .map(|tc| {
                    let args: serde_json::Value = serde_json::from_str(&tc.arguments_json)
                        .map_err(|e| format!("Invalid tool call arguments JSON: {e}"))?;
                    Ok(nobodywho::tool_calling::ToolCall {
                        name: tc.name.clone(),
                        arguments: args,
                    })
                })
                .collect();
            Ok(nobodywho::chat::Message::ToolCalls {
                role: nobodywho::chat::Role::from(role),
                content: content.clone(),
                tool_calls: tcs?,
            })
        }
        Message::ToolResult {
            role,
            name,
            content,
        } => Ok(nobodywho::chat::Message::ToolResult {
            role: nobodywho::chat::Role::from(role),
            name: name.clone(),
            content: content.clone(),
        }),
    }
}

// ---------- RustModel ----------
// Wrapper intended to be wrapped again in the target language (e.g. as `Model`).

#[derive(uniffi::Object)]
pub struct RustModel {
    inner: Arc<nobodywho::llm::Model>,
}

/// Load a GGUF model from a local path or remote URL.
///
/// Accepts local filesystem paths, `hf://owner/repo/file.gguf` for HuggingFace downloads,
/// or `https://` URLs. Downloaded models are cached automatically.
///
/// This is a free function instead of an async constructor because
/// uniffi-bindgen-react-native generates invalid JS (`async static` instead
/// of `static async`) for async constructors.
#[uniffi::export]
pub async fn load_model(
    model_path: String,
    use_gpu: bool,
    projection_model_path: Option<String>,
) -> Result<Arc<RustModel>, NobodyWhoError> {
    init_logging();
    log::info!(
        "load_model called: path={}, gpu={}, mmproj={:?}",
        model_path,
        use_gpu,
        projection_model_path
    );

    let model = nobodywho::llm::get_model_async(model_path.clone(), use_gpu, projection_model_path)
        .await
        .map_err(|e| {
            let msg = format!("Failed to load model '{}': {}", model_path, e);
            log::error!("{}", msg);
            NobodyWhoError::Error { message: msg }
        })?;

    log::info!("load_model SUCCESS for {}", model_path);
    Ok(Arc::new(RustModel {
        inner: Arc::new(model),
    }))
}

// ---------- RustChat ----------
// Wrapper intended to be wrapped again in the target language (e.g. as `Chat`).

#[derive(uniffi::Object)]
pub struct RustChat {
    inner: nobodywho::chat::ChatHandleAsync,
}

#[uniffi::export]
impl RustChat {
    /// Create a new chat session.
    #[uniffi::constructor]
    pub fn new(
        model: &RustModel,
        system_prompt: Option<String>,
        context_size: u32,
        template_variables: Option<HashMap<String, bool>>,
        tools: Option<Vec<Arc<RustTool>>>,
        sampler: Option<Arc<SamplerConfig>>,
    ) -> Self {
        let core_tools: Vec<nobodywho::tool_calling::Tool> = tools
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.inner.clone())
            .collect();

        let sampler_config = sampler.map(|s| s.inner.clone()).unwrap_or_default();

        let chat = nobodywho::chat::ChatBuilder::new(Arc::clone(&model.inner))
            .with_context_size(context_size)
            .with_system_prompt(system_prompt)
            .with_template_variables(template_variables.unwrap_or_default())
            .with_tools(core_tools)
            .with_sampler(sampler_config)
            .build_async();

        Self { inner: chat }
    }

    /// Send a message and get a token stream for the response.
    pub fn ask(&self, message: String) -> Arc<RustTokenStream> {
        log::info!("RustChat::ask called with message: {}", message);
        Arc::new(RustTokenStream {
            inner: tokio::sync::Mutex::new(self.inner.ask(message)),
        })
    }

    /// Send a multimodal prompt (text + images/audio) and get a token stream.
    ///
    /// `parts` is an ordered list of `PromptPart` items.
    /// Image and audio parts should contain a local file-system path.
    pub fn ask_with_prompt(&self, parts: Vec<PromptPart>) -> Arc<RustTokenStream> {
        let mut prompt = nobodywho::tokenizer::Prompt::new();
        for part in parts {
            match part {
                PromptPart::Text { content } => prompt.push_text(content),
                PromptPart::Image { path } => prompt.push_image(path.as_ref()),
                PromptPart::Audio { path } => prompt.push_audio(path.as_ref()),
            }
        }
        Arc::new(RustTokenStream {
            inner: tokio::sync::Mutex::new(self.inner.ask(prompt)),
        })
    }

    /// Stop the current generation.
    pub fn stop_generation(&self) {
        self.inner.stop_generation();
    }

    /// Reset the chat context with a new system prompt and tools.
    pub async fn reset_context(
        &self,
        system_prompt: Option<String>,
        tools: Option<Vec<Arc<RustTool>>>,
    ) -> Result<(), NobodyWhoError> {
        let core_tools: Vec<nobodywho::tool_calling::Tool> = tools
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.inner.clone())
            .collect();
        self.inner
            .reset_chat(system_prompt, core_tools)
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }

    /// Reset the chat history, keeping the system prompt and tools.
    pub async fn reset_history(&self) -> Result<(), NobodyWhoError> {
        self.inner
            .reset_history()
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }

    /// Get the current chat history as a list of messages.
    pub async fn get_chat_history(&self) -> Result<Vec<Message>, NobodyWhoError> {
        let messages = self
            .inner
            .get_chat_history()
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })?;
        Ok(messages.iter().map(core_message_to_uniffi).collect())
    }

    /// Set the chat history from a list of messages.
    pub async fn set_chat_history(&self, messages: Vec<Message>) -> Result<(), NobodyWhoError> {
        let core_messages: Result<Vec<_>, NobodyWhoError> =
            messages.iter().map(uniffi_message_to_core).collect();
        self.inner
            .set_chat_history(core_messages?)
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }

    /// Get the current system prompt.
    pub async fn get_system_prompt(&self) -> Result<Option<String>, NobodyWhoError> {
        self.inner
            .get_system_prompt()
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }

    /// Set the system prompt.
    pub async fn set_system_prompt(
        &self,
        system_prompt: Option<String>,
    ) -> Result<(), NobodyWhoError> {
        self.inner
            .set_system_prompt(system_prompt)
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }

    /// Set the tools available to the model.
    pub async fn set_tools(&self, tools: Vec<Arc<RustTool>>) -> Result<(), NobodyWhoError> {
        let core_tools: Vec<nobodywho::tool_calling::Tool> =
            tools.into_iter().map(|t| t.inner.clone()).collect();
        self.inner
            .set_tools(core_tools)
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }

    /// Set a template variable.
    pub async fn set_template_variable(
        &self,
        name: String,
        value: bool,
    ) -> Result<(), NobodyWhoError> {
        self.inner
            .set_template_variable(name, value)
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }

    /// Get all template variables.
    pub async fn get_template_variables(&self) -> Result<HashMap<String, bool>, NobodyWhoError> {
        self.inner
            .get_template_variables()
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }

    /// Set the sampler configuration.
    pub async fn set_sampler_config(&self, sampler: &SamplerConfig) -> Result<(), NobodyWhoError> {
        self.inner
            .set_sampler_config(sampler.inner.clone())
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }

    /// Get the current sampler configuration as a JSON string.
    pub async fn get_sampler_config_json(&self) -> Result<String, NobodyWhoError> {
        let config = self
            .inner
            .get_sampler_config()
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })?;
        serde_json::to_string(&config).map_err(|e| NobodyWhoError::Error {
            message: e.to_string(),
        })
    }
}

// ---------- RustTokenStream ----------
// Wrapper intended to be wrapped again in the target language (e.g. as `TokenStream`).

#[derive(uniffi::Object)]
pub struct RustTokenStream {
    // Mutex needed because UniFFI wraps objects in Arc (giving &self),
    // but TokenStreamAsync methods require &mut self.
    inner: tokio::sync::Mutex<nobodywho::chat::TokenStreamAsync>,
}

#[uniffi::export]
impl RustTokenStream {
    /// Get the next token. Returns None when generation is complete.
    pub async fn next_token(&self) -> Option<String> {
        let token = self.inner.lock().await.next_token().await;
        log::debug!("next_token: {:?}", token);
        token
    }

    /// Wait for the full response to complete and return it.
    pub async fn completed(&self) -> Result<String, NobodyWhoError> {
        self.inner
            .lock()
            .await
            .completed()
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }
}

// ---------- RustTool ----------
// Wrapper intended to be wrapped again in the target language (e.g. as `Tool`).
// The target language wrapper should handle ergonomic tool creation:
// accepting user-friendly parameter definitions and a typed callback,
// then implementing RustToolCallback to parse JSON args and dispatch.

/// Callback interface for tool functions.
/// Implement this in your language to provide the tool's logic.
/// The `call` method receives the tool arguments as a JSON string
/// and should return the tool's result as a string.
#[uniffi::export(callback_interface)]
pub trait RustToolCallback: Send + Sync {
    fn call(&self, arguments_json: String) -> String;
}

/// A pending tool call waiting for resolution from the language binding.
#[derive(uniffi::Record, Clone)]
pub struct PendingToolCall {
    pub call_id: String,
    pub arguments_json: String,
}

fn build_json_schema(parameters: &[ToolParameter]) -> serde_json::Value {
    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();
    for param in parameters {
        let param_schema: serde_json::Value = serde_json::from_str(&param.schema)
            .unwrap_or_else(|_| serde_json::json!({ "type": "string" }));
        properties.insert(param.name.clone(), param_schema);
        required.push(param.name.clone());
    }
    serde_json::json!({ "type": "object", "properties": properties, "required": required })
}

static CALL_ID_COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);

#[derive(uniffi::Object)]
pub struct RustTool {
    inner: nobodywho::tool_calling::Tool,
    pending_rx: Option<tokio::sync::Mutex<tokio::sync::mpsc::UnboundedReceiver<PendingToolCall>>>,
    resolvers: Option<Arc<std::sync::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>>>,
}

#[uniffi::export]
impl RustTool {
    /// Create a tool with a synchronous callback (for Swift, Kotlin).
    #[uniffi::constructor]
    pub fn new(
        name: String,
        description: String,
        parameters: Vec<ToolParameter>,
        callback: Box<dyn RustToolCallback>,
    ) -> Arc<Self> {
        let schema = build_json_schema(&parameters);
        let callback = Arc::new(callback);
        let wrapped = move |args: serde_json::Value| -> String { callback.call(args.to_string()) };
        let tool = nobodywho::tool_calling::Tool::new(name, description, schema, Arc::new(wrapped));
        Arc::new(Self {
            inner: tool,
            pending_rx: None,
            resolvers: None,
        })
    }

    /// Create a tool with async callback support (for React Native).
    ///
    /// Use `next_pending_call` to await tool invocations and
    /// `resolve_pending_call` to return results. The inference thread
    /// blocks until resolved.
    #[uniffi::constructor]
    pub fn new_async(
        name: String,
        description: String,
        parameters: Vec<ToolParameter>,
    ) -> Arc<Self> {
        let schema = build_json_schema(&parameters);
        let (pending_tx, pending_rx) = tokio::sync::mpsc::unbounded_channel();
        let resolvers: Arc<std::sync::Mutex<HashMap<String, std::sync::mpsc::Sender<String>>>> =
            Arc::new(std::sync::Mutex::new(HashMap::new()));
        let resolvers_clone = resolvers.clone();

        let wrapped = move |args: serde_json::Value| -> String {
            let call_id = format!(
                "c{}",
                CALL_ID_COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            );
            let (tx, rx) = std::sync::mpsc::channel();
            resolvers_clone.lock().unwrap().insert(call_id.clone(), tx);
            let _ = pending_tx.send(PendingToolCall {
                call_id: call_id.clone(),
                arguments_json: args.to_string(),
            });
            rx.recv()
                .unwrap_or_else(|_| "Error: tool call dropped".into())
        };

        let tool = nobodywho::tool_calling::Tool::new(name, description, schema, Arc::new(wrapped));
        Arc::new(Self {
            inner: tool,
            pending_rx: Some(tokio::sync::Mutex::new(pending_rx)),
            resolvers: Some(resolvers),
        })
    }

    /// Await the next tool call from inference. Returns `None` when the tool is dropped.
    pub async fn next_pending_call(&self) -> Option<PendingToolCall> {
        self.pending_rx.as_ref()?.lock().await.recv().await
    }

    /// Resolve a pending tool call with the result string.
    pub fn resolve_pending_call(&self, call_id: String, result: String) {
        if let Some(resolvers) = &self.resolvers {
            if let Some(tx) = resolvers.lock().unwrap().remove(&call_id) {
                let _ = tx.send(result);
            }
        }
    }

    /// Get the JSON schema for this tool's parameters as a string.
    pub fn get_schema_json(&self) -> String {
        self.inner.json_schema.to_string()
    }
}

// ---------- RustEncoder ----------
// Wrapper intended to be wrapped again in the target language (e.g. as `Encoder`).

#[derive(uniffi::Object)]
pub struct RustEncoder {
    inner: nobodywho::encoder::EncoderAsync,
}

#[uniffi::export]
impl RustEncoder {
    /// Create a new encoder for generating text embeddings.
    #[uniffi::constructor]
    pub fn new(model: &RustModel, context_size: Option<u32>) -> Arc<Self> {
        let handle = nobodywho::encoder::EncoderAsync::new(
            Arc::clone(&model.inner),
            context_size.unwrap_or(4096),
        );
        Arc::new(Self { inner: handle })
    }

    /// Encode text into an embedding vector.
    pub async fn encode(&self, text: String) -> Result<Vec<f32>, NobodyWhoError> {
        self.inner
            .encode(text)
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }
}

/// Compute the cosine similarity between two vectors.
#[uniffi::export]
pub fn cosine_similarity(a: Vec<f32>, b: Vec<f32>) -> f32 {
    nobodywho::encoder::cosine_similarity(&a, &b)
}

// ---------- RustCrossEncoder ----------
// Wrapper intended to be wrapped again in the target language (e.g. as `CrossEncoder`).

#[derive(uniffi::Object)]
pub struct RustCrossEncoder {
    inner: nobodywho::crossencoder::CrossEncoderAsync,
}

#[uniffi::export]
impl RustCrossEncoder {
    /// Create a new cross-encoder for ranking documents by relevance.
    #[uniffi::constructor]
    pub fn new(model: &RustModel, context_size: Option<u32>) -> Arc<Self> {
        let handle = nobodywho::crossencoder::CrossEncoderAsync::new(
            Arc::clone(&model.inner),
            context_size.unwrap_or(4096),
        );
        Arc::new(Self { inner: handle })
    }

    /// Rank documents by relevance to a query. Returns similarity scores.
    pub async fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, NobodyWhoError> {
        self.inner
            .rank(query, documents)
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })
    }

    /// Rank documents and return them sorted by descending relevance.
    /// Returns a JSON string of [document, score] pairs since UniFFI
    /// doesn't support tuples directly.
    pub async fn rank_and_sort_json(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<String, NobodyWhoError> {
        let results = self
            .inner
            .rank_and_sort(query, documents)
            .await
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })?;
        serde_json::to_string(&results).map_err(|e| NobodyWhoError::Error {
            message: e.to_string(),
        })
    }
}

// ---------- SamplerConfig ----------

#[derive(uniffi::Object)]
pub struct SamplerConfig {
    inner: nobodywho::sampler_config::SamplerConfig,
}

#[uniffi::export]
impl SamplerConfig {
    /// Serialize the sampler configuration to a JSON string.
    pub fn to_json(&self) -> Result<String, NobodyWhoError> {
        serde_json::to_string(&self.inner).map_err(|e| NobodyWhoError::Error {
            message: e.to_string(),
        })
    }

    /// Deserialize a sampler configuration from a JSON string.
    #[uniffi::constructor]
    pub fn from_json(json_str: String) -> Result<Arc<Self>, NobodyWhoError> {
        let inner: nobodywho::sampler_config::SamplerConfig = serde_json::from_str(&json_str)
            .map_err(|e| NobodyWhoError::Error {
                message: e.to_string(),
            })?;
        Ok(Arc::new(Self { inner }))
    }
}

// ---------- SamplerBuilder ----------

#[derive(uniffi::Object)]
pub struct SamplerBuilder {
    inner: nobodywho::sampler_config::SamplerConfig,
}

#[uniffi::export]
impl SamplerBuilder {
    /// Create a new SamplerBuilder to construct a custom sampler chain.
    #[uniffi::constructor]
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            inner: nobodywho::sampler_config::SamplerConfig::default(),
        })
    }

    // -- Shift steps (return new SamplerBuilder for chaining) --

    /// Keep only the top K most probable tokens.
    pub fn top_k(&self, top_k: i32) -> Arc<SamplerBuilder> {
        Arc::new(SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::TopK { top_k }),
        })
    }

    /// Keep tokens whose cumulative probability is below top_p.
    pub fn top_p(&self, top_p: f32, min_keep: u32) -> Arc<SamplerBuilder> {
        Arc::new(SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::TopP { top_p, min_keep }),
        })
    }

    /// Keep tokens with probability above min_p * (probability of most likely token).
    pub fn min_p(&self, min_p: f32, min_keep: u32) -> Arc<SamplerBuilder> {
        Arc::new(SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::MinP { min_p, min_keep }),
        })
    }

    /// Apply temperature scaling to the probability distribution.
    pub fn temperature(&self, temperature: f32) -> Arc<SamplerBuilder> {
        Arc::new(SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::Temperature { temperature }),
        })
    }

    /// XTC sampler that probabilistically excludes high-probability tokens.
    pub fn xtc(
        &self,
        xtc_probability: f32,
        xtc_threshold: f32,
        min_keep: u32,
    ) -> Arc<SamplerBuilder> {
        Arc::new(SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::XTC {
                    xtc_probability,
                    xtc_threshold,
                    min_keep,
                }),
        })
    }

    /// Typical sampling: keeps tokens close to expected information content.
    pub fn typical_p(&self, typ_p: f32, min_keep: u32) -> Arc<SamplerBuilder> {
        Arc::new(SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::TypicalP { typ_p, min_keep }),
        })
    }

    /// Apply a grammar constraint to enforce structured output.
    pub fn grammar(
        &self,
        grammar: String,
        trigger_on: Option<String>,
        root: String,
    ) -> Arc<SamplerBuilder> {
        Arc::new(SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::Grammar {
                    grammar,
                    trigger_on,
                    root,
                }),
        })
    }

    /// DRY (Don't Repeat Yourself) sampler to reduce repetition.
    pub fn dry(
        &self,
        multiplier: f32,
        base: f32,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: Vec<String>,
    ) -> Arc<SamplerBuilder> {
        Arc::new(SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::DRY {
                    multiplier,
                    base,
                    allowed_length,
                    penalty_last_n,
                    seq_breakers,
                }),
        })
    }

    /// Apply repetition penalties to discourage repeated tokens.
    pub fn penalties(
        &self,
        penalty_last_n: i32,
        penalty_repeat: f32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> Arc<SamplerBuilder> {
        Arc::new(SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::Penalties {
                    penalty_last_n,
                    penalty_repeat,
                    penalty_freq,
                    penalty_present,
                }),
        })
    }

    // -- Sample steps (finalize to SamplerConfig) --

    /// Sample from the probability distribution (weighted random selection).
    pub fn dist(&self) -> Arc<SamplerConfig> {
        Arc::new(SamplerConfig {
            inner: self
                .inner
                .clone()
                .sample(nobodywho::sampler_config::SampleStep::Dist),
        })
    }

    /// Always select the most probable token (deterministic).
    pub fn greedy(&self) -> Arc<SamplerConfig> {
        Arc::new(SamplerConfig {
            inner: self
                .inner
                .clone()
                .sample(nobodywho::sampler_config::SampleStep::Greedy),
        })
    }

    /// Use Mirostat v1 algorithm for perplexity-controlled sampling.
    pub fn mirostat_v1(&self, tau: f32, eta: f32, m: i32) -> Arc<SamplerConfig> {
        Arc::new(SamplerConfig {
            inner: self
                .inner
                .clone()
                .sample(nobodywho::sampler_config::SampleStep::MirostatV1 { tau, eta, m }),
        })
    }

    /// Use Mirostat v2 algorithm for perplexity-controlled sampling.
    pub fn mirostat_v2(&self, tau: f32, eta: f32) -> Arc<SamplerConfig> {
        Arc::new(SamplerConfig {
            inner: self
                .inner
                .clone()
                .sample(nobodywho::sampler_config::SampleStep::MirostatV2 { tau, eta }),
        })
    }
}

// ---------- SamplerPresets ----------
// Free functions for uniffi-bindgen-react-native compatibility.
// The TypeScript wrapper collects these into a static SamplerPresets class.

/// Get the default sampler configuration.
#[uniffi::export]
pub fn sampler_preset_default() -> Arc<SamplerConfig> {
    Arc::new(SamplerConfig {
        inner: nobodywho::sampler_config::SamplerConfig::default(),
    })
}

/// Create a sampler with top-k filtering only.
#[uniffi::export]
pub fn sampler_preset_top_k(top_k: i32) -> Arc<SamplerConfig> {
    Arc::new(SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::top_k(top_k),
    })
}

/// Create a sampler with nucleus (top-p) sampling.
#[uniffi::export]
pub fn sampler_preset_top_p(top_p: f32) -> Arc<SamplerConfig> {
    Arc::new(SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::top_p(top_p),
    })
}

/// Create a greedy sampler (always picks most probable token).
#[uniffi::export]
pub fn sampler_preset_greedy() -> Arc<SamplerConfig> {
    Arc::new(SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::greedy(),
    })
}

/// Create a sampler with temperature scaling.
#[uniffi::export]
pub fn sampler_preset_temperature(temperature: f32) -> Arc<SamplerConfig> {
    Arc::new(SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::temperature(temperature),
    })
}

/// Create a DRY sampler preset to reduce repetition.
#[uniffi::export]
pub fn sampler_preset_dry() -> Arc<SamplerConfig> {
    Arc::new(SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::dry(),
    })
}

/// Create a sampler configured for JSON output generation.
#[uniffi::export]
pub fn sampler_preset_json() -> Arc<SamplerConfig> {
    Arc::new(SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::json(),
    })
}

/// Create a sampler with a custom grammar constraint.
#[uniffi::export]
pub fn sampler_preset_grammar(grammar: String) -> Arc<SamplerConfig> {
    Arc::new(SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::grammar(grammar),
    })
}
