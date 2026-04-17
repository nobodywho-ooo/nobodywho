use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

// Re-exports users need directly
pub use nobodywho::chat::{Message, Role, TokenStream, TokenStreamAsync};
pub use nobodywho::encoder::cosine_similarity;
pub use nobodywho::errors;
pub use nobodywho::sampler_config::{SampleStep, SamplerConfig, SamplerPresets, ShiftStep};
pub use nobodywho::tokenizer::{Prompt, Promptable};
pub use nobodywho::tool_calling::{Tool, ToolCall};

// ── Model / ModelBuilder ──────────────────────────────────────────────────────

/// Builder for loading a GGUF model.
///
/// Created via [`Model::builder`]. Call [`ModelBuilder::build`] or
/// [`ModelBuilder::build_async`] to finish.
///
/// # Example
/// ```no_run
/// use nobodywho_rust::Model;
///
/// // Basic
/// let model = Model::builder("model.gguf").build().unwrap();
///
/// // Vision model, CPU only
/// let model = Model::builder("model.gguf")
///     .use_gpu(false)
///     .with_mmproj("mmproj.gguf")
///     .build()
///     .unwrap();
/// ```
// TODO: mmproj called image_model_path in python?
// TOOD; use_gpu called use_gpu_if_available in Python?
// TODO: path called model_path in python
pub struct ModelBuilder {
    path: PathBuf,
    use_gpu: bool,
    mmproj: Option<PathBuf>,
}

impl ModelBuilder {
    fn new(path: impl AsRef<Path>) -> Self {
        Self {
            path: path.as_ref().to_path_buf(),
            use_gpu: true,
            mmproj: None,
        }
    }

    /// Override GPU usage. Defaults to `true`.
    pub fn use_gpu(mut self, flag: bool) -> Self {
        self.use_gpu = flag;
        self
    }

    /// Add a multimodal projector for vision/audio models.
    pub fn with_mmproj(mut self, path: impl AsRef<Path>) -> Self {
        self.mmproj = Some(path.as_ref().to_path_buf());
        self
    }

    /// Load the model synchronously.
    pub fn build(self) -> Result<Model, errors::LoadModelError> {
        let path_str = path_to_str(&self.path)?;
        let mmproj = self.mmproj.as_deref().map(path_to_str).transpose()?;
        nobodywho::llm::get_model(path_str, self.use_gpu, mmproj)
            .map(|m| Model { inner: Arc::new(m) })
    }

    /// Load the model asynchronously, keeping the executor responsive while
    /// large files are read from disk.
    pub async fn build_async(self) -> Result<Model, errors::LoadModelError> {
        let path_str = path_to_str(&self.path)?.to_owned();
        let mmproj = self
            .mmproj
            .as_deref()
            .map(path_to_str)
            .transpose()?
            .map(str::to_owned);
        nobodywho::llm::get_model_async(path_str, self.use_gpu, mmproj)
            .await
            .map(|m| Model { inner: Arc::new(m) })
    }
}

/// A loaded GGUF model.
///
/// Model instances can be shared between multiple [`Chat`], [`Encoder`], or
/// [`CrossEncoder`] instances. The underlying model data is reference-counted,
/// so cloning is cheap.
///
/// There is no `ModelAsync` variant — a regular `Model` works with both the
/// sync and async handle types.
// TODO: Inner called model in python package
#[derive(Clone)]
pub struct Model {
    pub(crate) inner: Arc<nobodywho::llm::Model>,
}

impl Model {
    /// Start building a model load from `path`.
    ///
    /// See [`ModelBuilder`] for the available options.
    pub fn builder(path: impl AsRef<Path>) -> ModelBuilder {
        ModelBuilder::new(path)
    }
}

fn path_to_str(path: &Path) -> Result<&str, errors::LoadModelError> {
    path.to_str().ok_or_else(|| {
        errors::LoadModelError::ModelNotFound(format!(
            "path is not valid UTF-8: {}",
            path.display()
        ))
    })
}

// ── ChatBuilder ───────────────────────────────────────────────────────────────
// TODO: Consider if this ChatBuilder is even required?
// It is almost 1-1 the same interface as what is in nobodywho::chat::ChatBuilder
// ... Consistency in how we write the package, ig?

/// Builder for creating a [`Chat`] or [`ChatAsync`] session.
///
/// Created via [`Chat::builder`]. All options have sensible defaults, so only
/// set what you need. Finish with [`ChatBuilder::build`] (sync) or
/// [`ChatBuilder::build_async`].
///
/// # Example
/// ```no_run
/// use nobodywho_rust::{Chat, Model};
///
/// # let model = Model::builder("model.gguf").build().unwrap();
/// let chat = Chat::builder(&model)
///     .with_system_prompt("You are a helpful assistant.")
///     .with_template_variable("enable_thinking", false)
///     .build();
/// ```
pub struct ChatBuilder {
    model: Arc<nobodywho::llm::Model>,
    n_ctx: u32,
    system_prompt: Option<String>,
    template_variables: HashMap<String, bool>,
    tools: Vec<Tool>,
    sampler: SamplerConfig,
}

impl ChatBuilder {
    fn new(model: &Model) -> Self {
        Self {
            model: Arc::clone(&model.inner),
            n_ctx: 4096,
            system_prompt: None,
            template_variables: HashMap::new(),
            tools: vec![],
            sampler: SamplerConfig::default(),
        }
    }

    /// Set the context window size in tokens. Defaults to `4096`.
    pub fn with_n_ctx(mut self, n_ctx: u32) -> Self {
        self.n_ctx = n_ctx;
        self
    }

    /// Set the system prompt.
    pub fn with_system_prompt(mut self, prompt: impl Into<String>) -> Self {
        self.system_prompt = Some(prompt.into());
        self
    }

    /// Set a single chat-template variable (e.g. `"enable_thinking"`).
    pub fn with_template_variable(mut self, name: impl Into<String>, value: bool) -> Self {
        self.template_variables.insert(name.into(), value);
        self
    }

    /// Set all chat-template variables at once, replacing any previously set.
    pub fn with_template_variables(
        mut self,
        vars: impl IntoIterator<Item = (String, bool)>,
    ) -> Self {
        self.template_variables = vars.into_iter().collect();
        self
    }

    /// Add a single tool the model may call.
    pub fn with_tool(mut self, tool: Tool) -> Self {
        self.tools.push(tool);
        self
    }

    /// Set the full list of tools the model may call.
    pub fn with_tools(mut self, tools: impl IntoIterator<Item = Tool>) -> Self {
        self.tools = tools.into_iter().collect();
        self
    }

    /// Set the token sampling configuration. Defaults to [`SamplerConfig::default`].
    pub fn with_sampler(mut self, sampler: SamplerConfig) -> Self {
        self.sampler = sampler;
        self
    }

    /// Build a synchronous [`Chat`].
    pub fn build(self) -> Chat {
        let inner = nobodywho::chat::ChatBuilder::new(self.model)
            .with_context_size(self.n_ctx)
            .with_template_variables(self.template_variables)
            .with_tools(self.tools)
            .with_system_prompt(self.system_prompt)
            .with_sampler(self.sampler)
            .build();
        Chat { inner }
    }

    /// Build an asynchronous [`ChatAsync`].
    pub fn build_async(self) -> ChatAsync {
        let inner = nobodywho::chat::ChatBuilder::new(self.model)
            .with_context_size(self.n_ctx)
            .with_template_variables(self.template_variables)
            .with_tools(self.tools)
            .with_system_prompt(self.system_prompt)
            .with_sampler(self.sampler)
            .build_async();
        ChatAsync { inner }
    }
}

// ── Chat ──────────────────────────────────────────────────────────────────────

/// Synchronous chat handle for conversational text generation.
///
/// See [`ChatAsync`] for the async variant.
pub struct Chat {
    inner: nobodywho::chat::ChatHandle,
}

impl Chat {
    /// Start building a chat session with the given model.
    ///
    /// See [`ChatBuilder`] for the available options.
    pub fn builder(model: &Model) -> ChatBuilder {
        ChatBuilder::new(model)
    }

    // NOTE: Far easier here to pass the ask since we don't need to borrow and whatnot it?
    // TODO: Become sure of *why* this is the case - why it is easier
    /// Send a prompt and return a streaming response.
    ///
    /// Accepts plain `String` or a multimodal [`Prompt`].
    pub fn ask(&self, prompt: impl Promptable) -> TokenStream {
        self.inner.ask(prompt)
    }

    /// Reset the conversation, clearing history and optionally changing the
    /// system prompt and tool list.
    pub fn reset(
        &self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
    ) -> Result<(), errors::SetterError> {
        self.inner.reset_chat(system_prompt, tools)
    }

    /// Clear chat history while keeping the current system prompt and tools.
    pub fn reset_history(&self) -> Result<(), errors::SetterError> {
        self.inner.reset_history()
    }

    /// Stop the current generation immediately.
    pub fn stop_generation(&self) {
        self.inner.stop_generation()
    }

    pub fn set_tools(&self, tools: Vec<Tool>) -> Result<(), errors::SetterError> {
        self.inner.set_tools(tools)
    }

    /// Set the system prompt. Pass `None` to clear it.
    pub fn set_system_prompt(&self, prompt: Option<String>) -> Result<(), errors::SetterError> {
        self.inner.set_system_prompt(prompt)
    }

    pub fn get_system_prompt(&self) -> Result<Option<String>, errors::GetterError> {
        self.inner.get_system_prompt()
    }

    pub fn set_sampler_config(&self, sampler: SamplerConfig) -> Result<(), errors::SetterError> {
        self.inner.set_sampler_config(sampler)
    }

    pub fn get_sampler_config(&self) -> Result<SamplerConfig, errors::GetterError> {
        self.inner.get_sampler_config()
    }

    pub fn set_template_variable(
        &self,
        name: String,
        value: bool,
    ) -> Result<(), errors::SetterError> {
        self.inner.set_template_variable(name, value)
    }

    pub fn set_template_variables(
        &self,
        variables: HashMap<String, bool>,
    ) -> Result<(), errors::SetterError> {
        self.inner.set_template_variables(variables)
    }

    pub fn get_template_variables(&self) -> Result<HashMap<String, bool>, errors::GetterError> {
        self.inner.get_template_variables()
    }

    pub fn get_chat_history(&self) -> Result<Vec<Message>, errors::GetterError> {
        self.inner.get_chat_history()
    }

    pub fn set_chat_history(&self, messages: Vec<Message>) -> Result<(), errors::SetterError> {
        self.inner.set_chat_history(messages)
    }
}

// ── ChatAsync ─────────────────────────────────────────────────────────────────
// NOTE: Again, consider if this is actually necessary, I mean it just calls the inner almost
// 1-1... Whack

/// Async chat handle for conversational text generation.
///
/// See [`Chat`] for the synchronous variant and [`Chat::builder`] to construct one.
pub struct ChatAsync {
    inner: nobodywho::chat::ChatHandleAsync,
}

impl ChatAsync {
    /// Send a prompt and return an async streaming response.
    ///
    /// Accepts plain `String` or a multimodal [`Prompt`].
    pub fn ask(&self, prompt: impl Promptable) -> TokenStreamAsync {
        self.inner.ask(prompt)
    }

    /// Reset the conversation, clearing history and optionally changing the
    /// system prompt and tool list.
    pub async fn reset(
        &self,
        system_prompt: Option<String>,
        tools: Vec<Tool>,
    ) -> Result<(), errors::SetterError> {
        self.inner.reset_chat(system_prompt, tools).await
    }

    /// Clear chat history while keeping the current system prompt and tools.
    pub async fn reset_history(&self) -> Result<(), errors::SetterError> {
        self.inner.reset_history().await
    }

    /// Stop the current generation immediately.
    pub fn stop_generation(&self) {
        self.inner.stop_generation()
    }

    pub async fn set_tools(&self, tools: Vec<Tool>) -> Result<(), errors::SetterError> {
        self.inner.set_tools(tools).await
    }

    /// Set the system prompt. Pass `None` to clear it.
    pub async fn set_system_prompt(
        &self,
        prompt: Option<String>,
    ) -> Result<(), errors::SetterError> {
        self.inner.set_system_prompt(prompt).await
    }

    pub async fn get_system_prompt(&self) -> Result<Option<String>, errors::GetterError> {
        self.inner.get_system_prompt().await
    }

    pub async fn set_sampler_config(
        &self,
        sampler: SamplerConfig,
    ) -> Result<(), errors::SetterError> {
        self.inner.set_sampler_config(sampler).await
    }

    pub async fn get_sampler_config(&self) -> Result<SamplerConfig, errors::GetterError> {
        self.inner.get_sampler_config().await
    }

    pub async fn set_template_variable(
        &self,
        name: String,
        value: bool,
    ) -> Result<(), errors::SetterError> {
        self.inner.set_template_variable(name, value).await
    }

    pub async fn set_template_variables(
        &self,
        variables: HashMap<String, bool>,
    ) -> Result<(), errors::SetterError> {
        self.inner.set_template_variables(variables).await
    }

    pub async fn get_template_variables(
        &self,
    ) -> Result<HashMap<String, bool>, errors::GetterError> {
        self.inner.get_template_variables().await
    }

    pub async fn get_chat_history(&self) -> Result<Vec<Message>, errors::GetterError> {
        self.inner.get_chat_history().await
    }

    pub async fn set_chat_history(
        &self,
        messages: Vec<Message>,
    ) -> Result<(), errors::SetterError> {
        self.inner.set_chat_history(messages).await
    }
}

// ── Encoder ───────────────────────────────────────────────────────────────────

/// Generates dense vector embeddings from text.
///
/// Requires a model specifically trained for embeddings — a regular chat model
/// will not produce useful results.
///
/// See [`EncoderAsync`] for the async variant.
pub struct Encoder {
    inner: nobodywho::encoder::Encoder,
}

impl Encoder {
    pub fn new(model: &Model, n_ctx: u32) -> Self {
        Self {
            inner: nobodywho::encoder::Encoder::new(Arc::clone(&model.inner), n_ctx),
        }
    }

    pub fn encode(&self, text: String) -> Result<Vec<f32>, errors::EncoderWorkerError> {
        self.inner.encode(text)
    }
}

/// Async variant of [`Encoder`].
pub struct EncoderAsync {
    inner: nobodywho::encoder::EncoderAsync,
}

impl EncoderAsync {
    pub fn new(model: &Model, n_ctx: u32) -> Self {
        Self {
            inner: nobodywho::encoder::EncoderAsync::new(Arc::clone(&model.inner), n_ctx),
        }
    }

    // TODO: Should the await really be here? Feel like this would fuck something up. But then again...
    // Async is not my very most delicious I-could-have-this-all-day cup of tea
    // Python encoder has it the exact same place, so it is probably good
    pub async fn encode(&self, text: String) -> Result<Vec<f32>, errors::EncoderWorkerError> {
        self.inner.encode(text).await
    }
}

// ── CrossEncoder ──────────────────────────────────────────────────────────────

/// Cross-encoder for reranking documents against a query.
///
/// Useful for search: given a query and a list of documents, ranks them by
/// relevance. Requires a model trained for cross-encoding.
///
/// See [`CrossEncoderAsync`] for the async variant.
pub struct CrossEncoder {
    inner: nobodywho::crossencoder::CrossEncoder,
}

impl CrossEncoder {
    pub fn new(model: &Model, n_ctx: u32) -> Self {
        Self {
            inner: nobodywho::crossencoder::CrossEncoder::new(Arc::clone(&model.inner), n_ctx),
        }
    }

    /// Return a relevance score for each document (same order as input).
    pub fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, errors::CrossEncoderWorkerError> {
        self.inner.rank(query, documents)
    }

    /// Return `(document, score)` pairs sorted by descending relevance.
    pub fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<(String, f32)>, errors::CrossEncoderWorkerError> {
        self.inner.rank_and_sort(query, documents)
    }
}

/// Async variant of [`CrossEncoder`].
pub struct CrossEncoderAsync {
    inner: nobodywho::crossencoder::CrossEncoderAsync,
}

impl CrossEncoderAsync {
    pub fn new(model: &Model, n_ctx: u32) -> Self {
        Self {
            inner: nobodywho::crossencoder::CrossEncoderAsync::new(Arc::clone(&model.inner), n_ctx),
        }
    }

    /// Return a relevance score for each document (same order as input).
    pub async fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, errors::CrossEncoderWorkerError> {
        self.inner.rank(query, documents).await
    }

    /// Return `(document, score)` pairs sorted by descending relevance.
    pub async fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<(String, f32)>, errors::CrossEncoderWorkerError> {
        self.inner.rank_and_sort(query, documents).await
    }
}

// ── SamplerBuilder ────────────────────────────────────────────────────────────
// TODO: Consider if we need an interface function for SamplerConfig like from_json or to_json
// It is pretty straightforward to do, of course, but might not be completely 'revealed' y'know?

// TODO: Claude has left out a SamplerPresets imeplementation, indeed the whole struct
// By simply referencing nobodywho:sampler_config::SamplerConfig... which I guess is a hack
// And I guess it is explciit enough, but are we sure?

// TODO: Check if these "See [SamplerConfig]" things are actually re-exported
// (And what that even means?)
/// Fluent builder for constructing a [`SamplerConfig`] step by step.
///
/// A sampler chain consists of zero or more probability-shifting steps
/// followed by exactly one sampling step. Call any combination of shifting
/// methods, then finish with `.dist()`, `.greedy()`, or a Mirostat variant.
///
/// # Example
/// ```
/// use nobodywho_rust::SamplerBuilder;
/// let sampler = SamplerBuilder::new().top_k(40).temperature(0.8).dist();
/// ```
///
/// For common presets see [`SamplerPresets`].
#[derive(Clone)]
pub struct SamplerBuilder {
    config: SamplerConfig,
}

impl Default for SamplerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplerBuilder {
    pub fn new() -> Self {
        Self {
            config: SamplerConfig::default(),
        }
    }

    pub fn top_k(self, top_k: i32) -> Self {
        Self {
            config: self.config.shift(ShiftStep::TopK { top_k }),
        }
    }

    pub fn top_p(self, top_p: f32, min_keep: u32) -> Self {
        Self {
            config: self.config.shift(ShiftStep::TopP { top_p, min_keep }),
        }
    }

    pub fn min_p(self, min_p: f32, min_keep: u32) -> Self {
        Self {
            config: self.config.shift(ShiftStep::MinP { min_p, min_keep }),
        }
    }

    pub fn typical_p(self, typ_p: f32, min_keep: u32) -> Self {
        Self {
            config: self.config.shift(ShiftStep::TypicalP { typ_p, min_keep }),
        }
    }

    pub fn xtc(self, xtc_probability: f32, xtc_threshold: f32, min_keep: u32) -> Self {
        Self {
            config: self.config.shift(ShiftStep::XTC {
                xtc_probability,
                xtc_threshold,
                min_keep,
            }),
        }
    }

    pub fn temperature(self, temperature: f32) -> Self {
        Self {
            config: self.config.shift(ShiftStep::Temperature { temperature }),
        }
    }

    /// Apply a GBNF grammar constraint. `root` is the grammar's start rule.
    ///
    /// To activate the grammar only after a specific string is generated,
    /// chain `.grammar_trigger(trigger)` immediately after.
    pub fn grammar(self, grammar: impl Into<String>, root: impl Into<String>) -> Self {
        Self {
            config: self.config.shift(ShiftStep::Grammar {
                grammar: grammar.into(),
                trigger_on: None,
                root: root.into(),
            }),
        }
    }

    // TODO: Should we call this "with_trigger" for consistency? (You're making a grammar...)
    /// Activate the most recently added grammar only after `trigger` is generated.
    ///
    /// Must be called immediately after [`.grammar()`](Self::grammar).
    pub fn grammar_trigger(mut self, trigger: impl Into<String>) -> Self {
        if let Some(ShiftStep::Grammar { trigger_on, .. }) = self.config.steps_mut().last_mut() {
            *trigger_on = Some(trigger.into());
        }
        self
    }

    pub fn dry(
        self,
        multiplier: f32,
        base: f32,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: Vec<String>,
    ) -> Self {
        Self {
            config: self.config.shift(ShiftStep::DRY {
                multiplier,
                base,
                allowed_length,
                penalty_last_n,
                seq_breakers,
            }),
        }
    }

    pub fn penalties(
        self,
        penalty_last_n: i32,
        penalty_repeat: f32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> Self {
        Self {
            config: self.config.shift(ShiftStep::Penalties {
                penalty_last_n,
                penalty_repeat,
                penalty_freq,
                penalty_present,
            }),
        }
    }

    /// Finish with weighted-random sampling.
    pub fn dist(self) -> SamplerConfig {
        self.config.sample(SampleStep::Dist)
    }

    /// Finish with greedy (always most-probable token) sampling.
    pub fn greedy(self) -> SamplerConfig {
        self.config.sample(SampleStep::Greedy)
    }

    /// Finish with Mirostat v1 perplexity-controlled sampling.
    pub fn mirostat_v1(self, tau: f32, eta: f32, m: i32) -> SamplerConfig {
        self.config.sample(SampleStep::MirostatV1 { tau, eta, m })
    }

    /// Finish with Mirostat v2 perplexity-controlled sampling.
    pub fn mirostat_v2(self, tau: f32, eta: f32) -> SamplerConfig {
        self.config.sample(SampleStep::MirostatV2 { tau, eta })
    }
}

// NOTE: Rest of space here in lib.py for Python side would be taken for various struct...
// implementations such as tool, image, prompt, etc. This is not necessary for Rust-Rust
// There is an argument to be made to make it more explicit, but we will have to ask people
// Probably all the json_value_to_py and similar stuff can be left out no problemo
