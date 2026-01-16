use flutter_rust_bridge::{DartFnFuture, Rust2DartSendError};
// ^ in general I've only done fully-qualified imports, but these things need to be imported to
// satisfy some frb macros

#[flutter_rust_bridge::frb(opaque)]
pub struct Model {
    model: nobodywho::llm::Model,
}

impl Model {
    #[frb]
    pub fn load(model_path: &str, #[frb(default = true)] use_gpu: bool) -> Result<Self, String> {
        let model = nobodywho::llm::get_model(model_path, use_gpu).map_err(|e| e.to_string())?;
        Ok(Self { model })
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct Chat {
    chat: nobodywho::chat::ChatHandleAsync,
}

impl Chat {
    /// Create chat from existing model.
    ///
    /// Args:
    ///     model: A Model instance
    ///     system_prompt: System message to guide the model's behavior
    ///     context_size: Context size (maximum conversation length in tokens)
    ///     tools: List of Tool instances the model can call
    ///     sampler: SamplerConfig for token selection. Pass null to use default sampler.
    #[flutter_rust_bridge::frb(sync)]
    pub fn new(
        model: &Model,
        #[frb(default = "null")] system_prompt: Option<String>,
        #[frb(default = 4096)] context_size: u32,
        #[frb(default = true)] allow_thinking: bool,
        #[frb(default = "const []")] tools: Vec<Tool>,
        #[frb(default = "null")] sampler: Option<SamplerConfig>,
    ) -> Self {
        let sampler_config = sampler.map(|s| s.sampler_config).unwrap_or_default();

        let chat = {
            let mut chat_builder = nobodywho::chat::ChatBuilder::new(model.model.clone())
                .with_context_size(context_size)
                .with_allow_thinking(allow_thinking)
                .with_tools(tools.into_iter().map(|t| t.tool).collect())
                .with_sampler(sampler_config);

            if let Some(system_prompt) = system_prompt {
                chat_builder = chat_builder.with_system_prompt(system_prompt);
            }

            chat_builder.build_async()
        };

        Self { chat }
    }

    /// Create chat directly from a model path.
    ///
    /// Args:
    ///     model_path: Path to GGUF model file
    ///     system_prompt: System message to guide the model's behavior
    ///     context_size: Context size (maximum conversation length in tokens)
    ///     tools: List of Tool instances the model can call
    ///     sampler: SamplerConfig for token selection. Pass null to use default sampler.
    ///     use_gpu: Whether to use GPU acceleration. Defaults to true.
    #[flutter_rust_bridge::frb(sync)]
    pub fn from_path(
        model_path: &str,
        #[frb(default = "null")] system_prompt: Option<String>,
        #[frb(default = 4096)] context_size: u32,
        #[frb(default = true)] allow_thinking: bool,
        #[frb(default = "const []")] tools: Vec<Tool>,
        #[frb(default = "null")] sampler: Option<SamplerConfig>,
        #[frb(default = true)] use_gpu: bool,
    ) -> Result<Self, String> {
        let model = nobodywho::llm::get_model(model_path, use_gpu).map_err(|e| e.to_string())?;
        let sampler_config = sampler.map(|s| s.sampler_config).unwrap_or_default();

        let chat = {
            let mut chat_builder = nobodywho::chat::ChatBuilder::new(model)
                .with_context_size(context_size)
                .with_allow_thinking(allow_thinking)
                .with_tools(tools.into_iter().map(|t| t.tool).collect())
                .with_sampler(sampler_config);

            if let Some(system_prompt) = system_prompt {
                chat_builder = chat_builder.with_system_prompt(system_prompt);
            }

            chat_builder.build_async()
        };

        Ok(Self { chat })
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn ask(&self, message: String) -> TokenStream {
        TokenStream {
            stream: self.chat.ask(message),
        }
    }

    pub async fn get_chat_history(
        &self,
    ) -> Result<Vec<nobodywho::chat::Message>, nobodywho::errors::GetterError> {
        self.chat.get_chat_history().await
    }

    pub async fn set_chat_history(
        &self,
        messages: Vec<nobodywho::chat::Message>,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat.set_chat_history(messages).await
    }

    pub async fn set_sampler_config(
        &self,
        sampler_config: SamplerConfig,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat
            .set_sampler_config(sampler_config.sampler_config)
            .await
    }

    pub async fn reset_context(
        &self,
        system_prompt: String,
        tools: Vec<Tool>,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat
            .reset_chat(system_prompt, tools.into_iter().map(|t| t.tool).collect())
            .await
    }

    pub async fn reset_history(&self) -> Result<(), nobodywho::errors::SetterError> {
        self.chat.reset_history().await
    }

    pub async fn set_allow_thinking(
        &self,
        allow_thinking: bool,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat.set_allow_thinking(allow_thinking).await
    }

    pub async fn set_system_prompt(
        &self,
        system_prompt: String,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat.set_system_prompt(system_prompt).await
    }

    pub async fn set_tools(&self, tools: Vec<Tool>) -> Result<(), nobodywho::errors::SetterError> {
        self.chat
            .set_tools(tools.into_iter().map(|t| t.tool).collect())
            .await
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn stop_generation(&self) {
        self.chat.stop_generation()
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct TokenStream {
    stream: nobodywho::chat::TokenStreamAsync,
}

impl TokenStream {
    pub async fn iter(
        &mut self,
        sink: crate::frb_generated::StreamSink<String>,
    ) -> Result<(), Rust2DartSendError> {
        while let Some(token) = self.stream.next_token().await {
            sink.add(token)?;
        }
        Ok(())
    }

    pub async fn next_token(&mut self) -> Option<String> {
        self.stream.next_token().await
    }

    pub async fn completed(&mut self) -> Result<String, nobodywho::errors::CompletionError> {
        self.stream.completed().await
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct Encoder {
    handle: nobodywho::encoder::EncoderAsync,
}

impl Encoder {
    #[flutter_rust_bridge::frb(sync)]
    pub fn new(model: Model, #[frb(default = 4096)] n_ctx: u32) -> Self {
        let handle = nobodywho::encoder::EncoderAsync::new(model.model.clone(), n_ctx);
        Self { handle }
    }

    pub async fn encode(
        &self,
        text: String,
    ) -> Result<Vec<f32>, nobodywho::errors::EncoderWorkerError> {
        self.handle.encode(text).await
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct CrossEncoder {
    handle: nobodywho::crossencoder::CrossEncoderAsync,
}

impl CrossEncoder {
    #[flutter_rust_bridge::frb(sync)]
    pub fn new(model: Model, #[frb(default = 4096)] n_ctx: u32) -> Self {
        let handle = nobodywho::crossencoder::CrossEncoderAsync::new(model.model.clone(), n_ctx);
        Self { handle }
    }

    pub async fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, nobodywho::errors::CrossEncoderWorkerError> {
        self.handle.rank(query, documents).await
    }

    pub async fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<(String, f32)>, nobodywho::errors::CrossEncoderWorkerError> {
        self.handle.rank_and_sort(query, documents).await
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn cosine_similarity(a: Vec<f32>, b: Vec<f32>) -> f32 {
    nobodywho::encoder::cosine_similarity(&a, &b)
}

#[flutter_rust_bridge::frb(opaque)]
pub struct Tool {
    tool: nobodywho::chat::Tool,
}

#[flutter_rust_bridge::frb(sync)]
pub fn new_tool_impl(
    function: impl Fn(String) -> DartFnFuture<String> + Send + Sync + 'static,
    name: String,
    description: String,
    runtime_type: String,
) -> Result<Tool, String> {
    let json_schema = dart_function_type_to_json_schema(&runtime_type)?;

    // TODO: this seems to silently block forever if we get a type error on the dart side.
    //       it'd be *much* better to fail hard and throw a dart exception if that happens
    //       we might have to fix it on the dart side...
    let sync_callback = move |json: serde_json::Value| {
        futures::executor::block_on(async { function(json.to_string()).await })
    };

    let tool = nobodywho::chat::Tool::new(
        name,
        description,
        json_schema,
        std::sync::Arc::new(sync_callback),
    );

    Ok(Tool { tool })
}

/// Converts a Dart function runtimeType string directly to a JSON schema
/// Example input: "({String a, int b}) => String"
/// Returns a JSON schema for the function parameters
/// XXX: this whole function is vibe-coded, and hence the implementation is pretty messy...
#[tracing::instrument(ret, level = "debug")]
fn dart_function_type_to_json_schema(runtime_type: &str) -> Result<serde_json::Value, String> {
    // Match the pattern: ({params}) => returnType
    let re = regex::Regex::new(r"^\(\{([^}]*)\}\)\s*=>\s*(.+)$")
        .map_err(|e| format!("Regex error: {}", e))?;

    let captures = re.captures(runtime_type).ok_or_else(|| {
        if !runtime_type.contains("({") {
            format!(
                "Tool function must take only named parameters, got function type: {:?}",
                runtime_type
            )
        } else {
            "Invalid function type format".to_string()
        }
    })?;

    let params_str = &captures[1];
    let _return_type = captures[2].trim();

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    // Parse parameters if any exist
    if !params_str.trim().is_empty() {
        for param in params_str.split(',') {
            let param = param.trim();

            // Check if parameter is required (and strip the keyword if present)
            let param_without_required = if param.starts_with("required ") {
                &param[9..] // Skip "required "
            } else {
                param
            };

            // Find the last space to split type and name
            let last_space = param_without_required
                .rfind(' ')
                .ok_or_else(|| format!("Invalid parameter format: '{}'", param))?;

            let param_type = param_without_required[..last_space].trim();
            let param_name = param_without_required[last_space + 1..].trim();

            // Convert Dart type to JSON schema type
            let schema_type = match param_type {
                "String" => serde_json::json!({ "type": "string" }),
                "int" => serde_json::json!({ "type": "integer" }),
                "double" => serde_json::json!({ "type": "number" }),
                "num" => serde_json::json!({ "type": "number" }),
                "bool" => serde_json::json!({ "type": "boolean" }),
                "DateTime" => serde_json::json!({ "type": "string", "format": "date-time" }),
                t if t.starts_with("List<") && t.ends_with('>') => {
                    let inner = &t[5..t.len() - 1];
                    let inner_schema = match inner {
                        "String" => serde_json::json!({ "type": "string" }),
                        "int" => serde_json::json!({ "type": "integer" }),
                        "double" | "num" => serde_json::json!({ "type": "number" }),
                        "bool" => serde_json::json!({ "type": "boolean" }),
                        _ => serde_json::json!({ "type": "object" }),
                    };
                    serde_json::json!({
                        "type": "array",
                        "items": inner_schema
                    })
                }
                t if t.starts_with("Map<") && t.ends_with('>') => {
                    // For simplicity, assume string keys and try to parse value type
                    let generics = &t[4..t.len() - 1];
                    let parts: Vec<&str> = generics.split(',').collect();
                    if parts.len() == 2 {
                        let value_type = parts[1].trim();
                        let value_schema = match value_type {
                            "String" => serde_json::json!({ "type": "string" }),
                            "int" => serde_json::json!({ "type": "integer" }),
                            "double" | "num" => serde_json::json!({ "type": "number" }),
                            "bool" => serde_json::json!({ "type": "boolean" }),
                            _ => serde_json::json!({ "type": "object" }),
                        };
                        serde_json::json!({
                            "type": "object",
                            "additionalProperties": value_schema
                        })
                    } else {
                        serde_json::json!({ "type": "object" })
                    }
                }
                _ => serde_json::json!({ "type": "object" }),
            };

            properties.insert(param_name.to_string(), schema_type);
            required.push(param_name.to_string());
        }
    }

    Ok(serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    }))
}

#[flutter_rust_bridge::frb(sync)]
pub fn init_debug_log() {
    // XXX: this is just for logging during dev
    // TODO: make something with configurable log levels
    //       maybe something that integrates with dart's standard logging packages
    tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .try_init()
        .ok();
}

// TODO:
// - blocking ask
// - embeddings
// - cross encoder

/// `SamplerConfig` contains the configuration for a token sampler. The mechanism by which
/// NobodyWho will sample a token from the probability distribution, to include in the
/// generation result.
/// A `SamplerConfig` can be constructed either using a preset function from the `SamplerPresets`
/// class, or by manually constructing a sampler chain using the `SamplerBuilder` class.
#[flutter_rust_bridge::frb(opaque)]
#[derive(Clone, Default)]
pub struct SamplerConfig {
    sampler_config: nobodywho::sampler_config::SamplerConfig,
}

fn shift_step(
    builder: SamplerBuilder,
    step: nobodywho::sampler_config::ShiftStep,
) -> SamplerBuilder {
    SamplerBuilder {
        sampler_config: builder.sampler_config.shift(step),
    }
}

fn sample_step(
    builder: SamplerBuilder,
    step: nobodywho::sampler_config::SampleStep,
) -> SamplerConfig {
    SamplerConfig {
        sampler_config: builder.sampler_config.sample(step),
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
#[flutter_rust_bridge::frb(opaque)]
#[derive(Clone)]
pub struct SamplerBuilder {
    sampler_config: nobodywho::sampler_config::SamplerConfig,
}

impl SamplerBuilder {
    /// Create a new SamplerBuilder to construct a custom sampler chain.
    #[flutter_rust_bridge::frb(sync)]
    pub fn new() -> Self {
        Self {
            sampler_config: nobodywho::sampler_config::SamplerConfig::default(),
        }
    }

    /// Keep only the top K most probable tokens. Typical values: 40-50.
    ///
    /// Args:
    ///     top_k: Number of top tokens to keep
    #[flutter_rust_bridge::frb(sync)]
    pub fn top_k(&self, top_k: i32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::TopK { top_k },
        )
    }

    /// Keep tokens whose cumulative probability is below top_p. Typical values: 0.9-0.95.
    ///
    /// Args:
    ///     top_p: Cumulative probability threshold (0.0 to 1.0)
    ///     min_keep: Minimum number of tokens to always keep
    #[flutter_rust_bridge::frb(sync)]
    pub fn top_p(&self, top_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::TopP { top_p, min_keep },
        )
    }

    /// Keep tokens with probability above min_p * (probability of most likely token).
    ///
    /// Args:
    ///     min_p: Minimum relative probability threshold (0.0 to 1.0). Typical: 0.05-0.1.
    ///     min_keep: Minimum number of tokens to always keep
    #[flutter_rust_bridge::frb(sync)]
    pub fn min_p(&self, min_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::MinP { min_p, min_keep },
        )
    }

    /// XTC (eXclude Top Choices) sampler that probabilistically excludes high-probability tokens.
    /// This can increase output diversity by sometimes forcing the model to pick less obvious tokens.
    ///
    /// Args:
    ///     xtc_probability: Probability of applying XTC on each token (0.0 to 1.0)
    ///     xtc_threshold: Tokens with probability above this threshold may be excluded (0.0 to 1.0)
    ///     min_keep: Minimum number of tokens to always keep (prevents excluding all tokens)
    #[flutter_rust_bridge::frb(sync)]
    pub fn xtc(&self, xtc_probability: f32, xtc_threshold: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::XTC {
                xtc_probability,
                xtc_threshold,
                min_keep,
            },
        )
    }

    /// Typical sampling: keeps tokens close to expected information content.
    ///
    /// Args:
    ///     typ_p: Typical probability mass (0.0 to 1.0). Typical: 0.9.
    ///     min_keep: Minimum number of tokens to always keep
    #[flutter_rust_bridge::frb(sync)]
    pub fn typical_p(&self, typ_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::TypicalP { typ_p, min_keep },
        )
    }

    /// Apply temperature scaling to the probability distribution.
    ///
    /// Args:
    ///     temperature: Temperature value (0.0 = deterministic, 1.0 = unchanged, >1.0 = more random)
    #[flutter_rust_bridge::frb(sync)]
    pub fn temperature(&self, temperature: f32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::Temperature { temperature },
        )
    }

    /// Apply a grammar constraint to enforce structured output.
    ///
    /// Args:
    ///     grammar: Grammar specification in GBNF format (GGML BNF, a variant of BNF used by llama.cpp)
    ///     trigger_on: Optional string that, when generated, activates the grammar constraint.
    ///                 Useful for letting the model generate free-form text until a specific marker.
    ///     root: Name of the root grammar rule to start parsing from
    #[flutter_rust_bridge::frb(sync)]
    pub fn grammar(&self, grammar: String, trigger_on: Option<String>, root: String) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::Grammar {
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
    #[flutter_rust_bridge::frb(sync)]
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
            nobodywho::sampler_config::ShiftStep::DRY {
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
    #[flutter_rust_bridge::frb(sync)]
    pub fn penalties(
        &self,
        penalty_last_n: i32,
        penalty_repeat: f32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler_config::ShiftStep::Penalties {
                penalty_last_n,
                penalty_repeat,
                penalty_freq,
                penalty_present,
            },
        )
    }

    /// Sample from the probability distribution (weighted random selection).
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    #[flutter_rust_bridge::frb(sync)]
    pub fn dist(&self) -> SamplerConfig {
        sample_step(self.clone(), nobodywho::sampler_config::SampleStep::Dist)
    }

    /// Always select the most probable token (deterministic).
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    #[flutter_rust_bridge::frb(sync)]
    pub fn greedy(&self) -> SamplerConfig {
        sample_step(self.clone(), nobodywho::sampler_config::SampleStep::Greedy)
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
    #[flutter_rust_bridge::frb(sync)]
    pub fn mirostat_v1(&self, tau: f32, eta: f32, m: i32) -> SamplerConfig {
        sample_step(
            self.clone(),
            nobodywho::sampler_config::SampleStep::MirostatV1 { tau, eta, m },
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
    #[flutter_rust_bridge::frb(sync)]
    pub fn mirostat_v2(&self, tau: f32, eta: f32) -> SamplerConfig {
        sample_step(
            self.clone(),
            nobodywho::sampler_config::SampleStep::MirostatV2 { tau, eta },
        )
    }
}

/// `SamplerPresets` is a static class which contains a bunch of functions to easily create a
/// `SamplerConfig` from some pre-defined sampler chain.
/// E.g. `SamplerPresets.temperature(0.8)` will return a `SamplerConfig` with temperature=0.8.
#[flutter_rust_bridge::frb(opaque)]
pub struct SamplerPresets {
    _private: (),
}

impl SamplerPresets {
    /// Get the default sampler configuration.
    #[flutter_rust_bridge::frb(sync)]
    pub fn default_sampler() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerConfig::default(),
        }
    }

    /// Create a sampler with top-k filtering only.
    ///
    /// Args:
    ///     top_k: Number of top tokens to keep
    #[flutter_rust_bridge::frb(sync)]
    pub fn top_k(top_k: i32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::top_k(top_k),
        }
    }

    /// Create a sampler with nucleus (top-p) sampling.
    ///
    /// Args:
    ///     top_p: Cumulative probability threshold (0.0 to 1.0)
    #[flutter_rust_bridge::frb(sync)]
    pub fn top_p(top_p: f32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::top_p(top_p),
        }
    }

    /// Create a greedy sampler (always picks most probable token).
    #[flutter_rust_bridge::frb(sync)]
    pub fn greedy() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::greedy(),
        }
    }

    /// Create a sampler with temperature scaling.
    ///
    /// Args:
    ///     temperature: Temperature value (lower = more focused, higher = more random)
    #[flutter_rust_bridge::frb(sync)]
    pub fn temperature(temperature: f32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::temperature(temperature),
        }
    }

    /// Create a DRY sampler preset to reduce repetition.
    #[flutter_rust_bridge::frb(sync)]
    pub fn dry() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::dry(),
        }
    }

    /// Create a sampler configured for JSON output generation.
    /// Uses a grammar constraint to ensure the model outputs only valid JSON.
    #[flutter_rust_bridge::frb(sync)]
    pub fn json() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::json(),
        }
    }

    /// Create a sampler with a custom grammar constraint.
    ///
    /// Args:
    ///     grammar: Grammar specification in GBNF format (GGML BNF, a variant of BNF used by llama.cpp)
    #[flutter_rust_bridge::frb(sync)]
    pub fn grammar(grammar: String) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler_config::SamplerPresets::grammar(grammar),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dart_function_to_schema() {
        let schema = dart_function_type_to_json_schema(
            "({String name, int age, List<String> tags}) => String",
        )
        .unwrap();
        let expected = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["name", "age", "tags"],
            "additionalProperties": false
        });
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_empty_params() {
        let schema = dart_function_type_to_json_schema("({}) => String").unwrap();
        let expected = serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        });
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_single_string_parameter() {
        let dart_type = "({required String text}) => Future<String>";
        let json_schema = dart_function_type_to_json_schema(dart_type).unwrap();
        let expected = serde_json::json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": [ "text" ],
            "additionalProperties": false,
        });
        assert_eq!(json_schema, expected);
    }
}
