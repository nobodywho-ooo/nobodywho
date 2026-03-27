use std::collections::HashMap;
use std::sync::Arc;

use napi::bindgen_prelude::*;
use napi::threadsafe_function::ThreadsafeFunctionCallMode;
use napi_derive::napi;

// The core Message/Role types have Serialize/Deserialize but we can't impl
// napi's ToNapiValue/FromNapiValue on foreign types. So we use serde_json::Value
// as the bridge — it implements both serde traits and napi traits, and the JSON
// shape is the same either way. This avoids duplicating type definitions.

// ---------- Model ----------

#[napi]
pub struct Model {
    inner: Arc<nobodywho::llm::Model>,
}

#[napi]
impl Model {
    /// Load a GGUF model from disk.
    #[napi(factory)]
    pub async fn load(
        model_path: String,
        use_gpu: bool,
        image_model_path: Option<String>,
    ) -> Result<Self> {
        let model = nobodywho::llm::get_model_async(model_path, use_gpu, image_model_path)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;

        Ok(Self {
            inner: Arc::new(model),
        })
    }

    /// Check if a GPU backend is available.
    #[napi]
    pub fn has_gpu_backend() -> bool {
        nobodywho::llm::has_gpu_backend()
    }
}

// ---------- Chat ----------

#[napi]
pub struct Chat {
    inner: nobodywho::chat::ChatHandleAsync,
}

#[napi]
impl Chat {
    /// Create a new chat session.
    #[napi(constructor)]
    pub fn new(
        model: &Model,
        system_prompt: Option<String>,
        context_size: Option<u32>,
        template_variables: Option<HashMap<String, bool>>,
        tools: Option<Vec<&Tool>>,
        sampler: Option<&SamplerConfig>,
    ) -> Self {
        let core_tools: Vec<nobodywho::tool_calling::Tool> = tools
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.inner.clone())
            .collect();

        let sampler_config = sampler.map(|s| s.inner.clone()).unwrap_or_default();

        let chat = nobodywho::chat::ChatBuilder::new(Arc::clone(&model.inner))
            .with_context_size(context_size.unwrap_or(4096))
            .with_system_prompt(system_prompt)
            .with_template_variables(template_variables.unwrap_or_default())
            .with_tools(core_tools)
            .with_sampler(sampler_config)
            .build_async();

        Self { inner: chat }
    }

    /// Send a message and get a token stream for the response.
    #[napi]
    pub fn ask(&self, message: String) -> TokenStream {
        TokenStream {
            inner: tokio::sync::Mutex::new(self.inner.ask(message)),
        }
    }

    /// Send a multimodal prompt (text + images/audio) and get a token stream.
    ///
    /// `parts` is an array of objects, each with a `type` field:
    /// - `{ type: "text", content: "..." }`
    /// - `{ type: "image", path: "/path/to/image.jpg" }`
    /// - `{ type: "audio", path: "/path/to/audio.wav" }`
    #[napi]
    pub fn ask_with_prompt(
        &self,
        #[napi(ts_arg_type = "Array<{ type: 'text', content: string } | { type: 'image', path: string } | { type: 'audio', path: string }>")] parts: Vec<serde_json::Value>,
    ) -> Result<TokenStream> {
        let mut prompt = nobodywho::tokenizer::Prompt::new();
        for part in parts {
            let part_type = part
                .get("type")
                .and_then(|v| v.as_str())
                .ok_or_else(|| Error::from_reason("Each prompt part must have a 'type' field"))?;
            match part_type {
                "text" => {
                    let content = part
                        .get("content")
                        .and_then(|v| v.as_str())
                        .ok_or_else(|| {
                            Error::from_reason("Text prompt part must have a 'content' field")
                        })?;
                    prompt.push_text(content);
                }
                "image" => {
                    let path = part.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                        Error::from_reason("Image prompt part must have a 'path' field")
                    })?;
                    prompt.push_image(path.as_ref());
                }
                "audio" => {
                    let path = part.get("path").and_then(|v| v.as_str()).ok_or_else(|| {
                        Error::from_reason("Audio prompt part must have a 'path' field")
                    })?;
                    prompt.push_audio(path.as_ref());
                }
                other => {
                    return Err(Error::from_reason(format!(
                        "Unknown prompt part type: '{}'. Expected 'text', 'image', or 'audio'",
                        other
                    )));
                }
            }
        }
        Ok(TokenStream {
            inner: tokio::sync::Mutex::new(self.inner.ask(prompt)),
        })
    }

    /// Stop the current generation.
    #[napi]
    pub fn stop_generation(&self) {
        self.inner.stop_generation();
    }

    /// Reset the chat context with a new system prompt and tools.
    #[napi]
    pub async fn reset_context(
        &self,
        system_prompt: Option<String>,
        tools: Option<Vec<&Tool>>,
    ) -> Result<()> {
        let core_tools: Vec<nobodywho::tool_calling::Tool> = tools
            .unwrap_or_default()
            .into_iter()
            .map(|t| t.inner.clone())
            .collect();
        self.inner
            .reset_chat(system_prompt, core_tools)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Reset the chat history, keeping the system prompt and tools.
    #[napi]
    pub async fn reset_history(&self) -> Result<()> {
        self.inner
            .reset_history()
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Get the current chat history as an array of message objects.
    #[napi(ts_return_type = "Promise<Array<any>>")]
    pub async fn get_chat_history(&self) -> Result<serde_json::Value> {
        let messages = self
            .inner
            .get_chat_history()
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;
        serde_json::to_value(&messages).map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Set the chat history from an array of message objects.
    #[napi]
    pub async fn set_chat_history(
        &self,
        #[napi(ts_arg_type = "Array<any>")] messages: serde_json::Value,
    ) -> Result<()> {
        let messages: Vec<nobodywho::chat::Message> =
            serde_json::from_value(messages).map_err(|e| Error::from_reason(e.to_string()))?;
        self.inner
            .set_chat_history(messages)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Get the current system prompt.
    #[napi]
    pub async fn get_system_prompt(&self) -> Result<Option<String>> {
        self.inner
            .get_system_prompt()
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Set the system prompt.
    #[napi]
    pub async fn set_system_prompt(&self, system_prompt: Option<String>) -> Result<()> {
        self.inner
            .set_system_prompt(system_prompt)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Set the tools available to the model.
    #[napi]
    pub async fn set_tools(&self, tools: Vec<&Tool>) -> Result<()> {
        let core_tools: Vec<nobodywho::tool_calling::Tool> =
            tools.into_iter().map(|t| t.inner.clone()).collect();
        self.inner
            .set_tools(core_tools)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Set a template variable.
    #[napi]
    pub async fn set_template_variable(&self, name: String, value: bool) -> Result<()> {
        self.inner
            .set_template_variable(name, value)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Get all template variables.
    #[napi]
    pub async fn get_template_variables(&self) -> Result<HashMap<String, bool>> {
        self.inner
            .get_template_variables()
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Set the sampler configuration.
    #[napi]
    pub async fn set_sampler_config(&self, sampler: &SamplerConfig) -> Result<()> {
        self.inner
            .set_sampler_config(sampler.inner.clone())
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Get the current sampler configuration as a JSON string.
    #[napi]
    pub async fn get_sampler_config_json(&self) -> Result<String> {
        let config = self
            .inner
            .get_sampler_config()
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;
        serde_json::to_string(&config).map_err(|e| Error::from_reason(e.to_string()))
    }
}

// ---------- TokenStream ----------

#[napi]
pub struct TokenStream {
    // Mutex needed because napi wraps objects in references with &self,
    // but TokenStreamAsync methods require &mut self.
    inner: tokio::sync::Mutex<nobodywho::chat::TokenStreamAsync>,
}

#[napi]
impl TokenStream {
    /// Get the next token. Returns null when generation is complete.
    #[napi]
    pub async fn next_token(&self) -> Option<String> {
        self.inner.lock().await.next_token().await
    }

    /// Wait for the full response to complete and return it.
    #[napi]
    pub async fn completed(&self) -> Result<String> {
        self.inner
            .lock()
            .await
            .completed()
            .await
            .map_err(|e| Error::from_reason(e.to_string()))
    }
}

// ---------- Tool ----------

#[napi]
pub struct Tool {
    inner: nobodywho::tool_calling::Tool,
}

#[napi]
impl Tool {
    /// Create a tool that the model can call during inference.
    ///
    /// The callback receives the tool arguments as a JSON string and must return
    /// a result as a string. The callback is invoked from the inference thread
    /// when the model calls this tool.
    #[napi(constructor)]
    pub fn new(
        name: String,
        description: String,
        json_schema: String,
        #[napi(ts_arg_type = "(argsJson: string) => string")] callback: Function<String, String>,
    ) -> Result<Self> {
        let schema: serde_json::Value = serde_json::from_str(&json_schema)
            .map_err(|e| Error::from_reason(format!("Invalid JSON schema: {e}")))?;

        // Create a threadsafe function so the callback can be invoked from
        // the inference thread (which is not the JS main thread).
        let tsfn = callback
            .build_threadsafe_function::<String>()
            .callee_handled::<false>()
            .build()?;

        let wrapped = move |args: serde_json::Value| -> String {
            let args_json = args.to_string();
            // Call the JS function synchronously from this thread and block
            // until we get the result back.
            let (tx, rx) = std::sync::mpsc::channel();
            tsfn.call_with_return_value(
                args_json,
                ThreadsafeFunctionCallMode::Blocking,
                move |result: napi::Result<String>, _env: Env| {
                    let _ = tx.send(
                        result.unwrap_or_else(|e| format!("Tool callback error: {e}")),
                    );
                    Ok(())
                },
            );
            rx.recv().unwrap_or_else(|_| "Tool callback channel closed".to_string())
        };

        let tool = nobodywho::tool_calling::Tool::new(name, description, schema, Arc::new(wrapped));

        Ok(Self { inner: tool })
    }
}

// ---------- Encoder ----------

#[napi]
pub struct Encoder {
    inner: nobodywho::encoder::EncoderAsync,
}

#[napi]
impl Encoder {
    /// Create a new encoder for generating text embeddings.
    #[napi(constructor)]
    pub fn new(model: &Model, context_size: Option<u32>) -> Self {
        let handle = nobodywho::encoder::EncoderAsync::new(
            Arc::clone(&model.inner),
            context_size.unwrap_or(4096),
        );
        Self { inner: handle }
    }

    /// Encode text into an embedding vector.
    #[napi]
    pub async fn encode(&self, text: String) -> Result<Vec<f64>> {
        let floats = self
            .inner
            .encode(text)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;
        // napi-rs doesn't support Vec<f32>, convert to f64
        Ok(floats.into_iter().map(|f| f as f64).collect())
    }
}

/// Compute the cosine similarity between two vectors.
#[napi]
pub fn cosine_similarity(a: Vec<f64>, b: Vec<f64>) -> f64 {
    let a_f32: Vec<f32> = a.into_iter().map(|f| f as f32).collect();
    let b_f32: Vec<f32> = b.into_iter().map(|f| f as f32).collect();
    nobodywho::encoder::cosine_similarity(&a_f32, &b_f32) as f64
}

// ---------- CrossEncoder ----------

#[napi]
pub struct CrossEncoder {
    inner: nobodywho::crossencoder::CrossEncoderAsync,
}

#[napi]
impl CrossEncoder {
    /// Create a new cross-encoder for ranking documents by relevance.
    #[napi(constructor)]
    pub fn new(model: &Model, context_size: Option<u32>) -> Self {
        let handle = nobodywho::crossencoder::CrossEncoderAsync::new(
            Arc::clone(&model.inner),
            context_size.unwrap_or(4096),
        );
        Self { inner: handle }
    }

    /// Rank documents by relevance to a query. Returns similarity scores.
    #[napi]
    pub async fn rank(&self, query: String, documents: Vec<String>) -> Result<Vec<f64>> {
        let scores = self
            .inner
            .rank(query, documents)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(scores.into_iter().map(|f| f as f64).collect())
    }

    /// Rank documents and return them sorted by descending relevance.
    /// Returns an array of [document, score] pairs.
    #[napi]
    pub async fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<Vec<String>>> {
        let results = self
            .inner
            .rank_and_sort(query, documents)
            .await
            .map_err(|e| Error::from_reason(e.to_string()))?;
        // Return as array of [doc, score_string] pairs since napi-rs
        // doesn't support tuples directly
        Ok(results
            .into_iter()
            .map(|(doc, score)| vec![doc, score.to_string()])
            .collect())
    }
}

// ---------- SamplerConfig ----------

#[napi]
#[derive(Clone)]
pub struct SamplerConfig {
    inner: nobodywho::sampler_config::SamplerConfig,
}

#[napi]
impl SamplerConfig {
    /// Serialize the sampler configuration to a JSON string.
    #[napi]
    pub fn to_json(&self) -> Result<String> {
        serde_json::to_string(&self.inner).map_err(|e| Error::from_reason(e.to_string()))
    }

    /// Deserialize a sampler configuration from a JSON string.
    #[napi(factory)]
    pub fn from_json(json_str: String) -> Result<Self> {
        let inner: nobodywho::sampler_config::SamplerConfig =
            serde_json::from_str(&json_str).map_err(|e| Error::from_reason(e.to_string()))?;
        Ok(Self { inner })
    }
}

// ---------- SamplerBuilder ----------

#[napi]
#[derive(Clone)]
pub struct SamplerBuilder {
    inner: nobodywho::sampler_config::SamplerConfig,
}

#[napi]
impl SamplerBuilder {
    /// Create a new SamplerBuilder to construct a custom sampler chain.
    #[napi(constructor)]
    pub fn new() -> Self {
        Self {
            inner: nobodywho::sampler_config::SamplerConfig::default(),
        }
    }

    /// Keep only the top K most probable tokens.
    #[napi]
    pub fn top_k(&self, top_k: i32) -> SamplerBuilder {
        SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::TopK { top_k }),
        }
    }

    /// Keep tokens whose cumulative probability is below top_p.
    #[napi]
    pub fn top_p(&self, top_p: f64, min_keep: u32) -> SamplerBuilder {
        SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::TopP {
                    top_p: top_p as f32,
                    min_keep,
                }),
        }
    }

    /// Keep tokens with probability above min_p * (probability of most likely token).
    #[napi]
    pub fn min_p(&self, min_p: f64, min_keep: u32) -> SamplerBuilder {
        SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::MinP {
                    min_p: min_p as f32,
                    min_keep,
                }),
        }
    }

    /// Apply temperature scaling to the probability distribution.
    #[napi]
    pub fn temperature(&self, temperature: f64) -> SamplerBuilder {
        SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::Temperature {
                    temperature: temperature as f32,
                }),
        }
    }

    /// XTC sampler that probabilistically excludes high-probability tokens.
    #[napi]
    pub fn xtc(&self, xtc_probability: f64, xtc_threshold: f64, min_keep: u32) -> SamplerBuilder {
        SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::XTC {
                    xtc_probability: xtc_probability as f32,
                    xtc_threshold: xtc_threshold as f32,
                    min_keep,
                }),
        }
    }

    /// Typical sampling: keeps tokens close to expected information content.
    #[napi]
    pub fn typical_p(&self, typ_p: f64, min_keep: u32) -> SamplerBuilder {
        SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::TypicalP {
                    typ_p: typ_p as f32,
                    min_keep,
                }),
        }
    }

    /// Apply a grammar constraint to enforce structured output.
    #[napi]
    pub fn grammar(
        &self,
        grammar: String,
        trigger_on: Option<String>,
        root: String,
    ) -> SamplerBuilder {
        SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::Grammar {
                    grammar,
                    trigger_on,
                    root,
                }),
        }
    }

    /// DRY (Don't Repeat Yourself) sampler to reduce repetition.
    #[napi]
    pub fn dry(
        &self,
        multiplier: f64,
        base: f64,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: Vec<String>,
    ) -> SamplerBuilder {
        SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::DRY {
                    multiplier: multiplier as f32,
                    base: base as f32,
                    allowed_length,
                    penalty_last_n,
                    seq_breakers,
                }),
        }
    }

    /// Apply repetition penalties to discourage repeated tokens.
    #[napi]
    pub fn penalties(
        &self,
        penalty_last_n: i32,
        penalty_repeat: f64,
        penalty_freq: f64,
        penalty_present: f64,
    ) -> SamplerBuilder {
        SamplerBuilder {
            inner: self
                .inner
                .clone()
                .shift(nobodywho::sampler_config::ShiftStep::Penalties {
                    penalty_last_n,
                    penalty_repeat: penalty_repeat as f32,
                    penalty_freq: penalty_freq as f32,
                    penalty_present: penalty_present as f32,
                }),
        }
    }

    /// Sample from the probability distribution (weighted random selection).
    #[napi]
    pub fn dist(&self) -> SamplerConfig {
        SamplerConfig {
            inner: self
                .inner
                .clone()
                .sample(nobodywho::sampler_config::SampleStep::Dist),
        }
    }

    /// Always select the most probable token (deterministic).
    #[napi]
    pub fn greedy(&self) -> SamplerConfig {
        SamplerConfig {
            inner: self
                .inner
                .clone()
                .sample(nobodywho::sampler_config::SampleStep::Greedy),
        }
    }

    /// Use Mirostat v1 algorithm for perplexity-controlled sampling.
    #[napi]
    pub fn mirostat_v1(&self, tau: f64, eta: f64, m: i32) -> SamplerConfig {
        SamplerConfig {
            inner: self
                .inner
                .clone()
                .sample(nobodywho::sampler_config::SampleStep::MirostatV1 {
                    tau: tau as f32,
                    eta: eta as f32,
                    m,
                }),
        }
    }

    /// Use Mirostat v2 algorithm for perplexity-controlled sampling.
    #[napi]
    pub fn mirostat_v2(&self, tau: f64, eta: f64) -> SamplerConfig {
        SamplerConfig {
            inner: self
                .inner
                .clone()
                .sample(nobodywho::sampler_config::SampleStep::MirostatV2 {
                    tau: tau as f32,
                    eta: eta as f32,
                }),
        }
    }
}

// ---------- SamplerPresets ----------
// Namespace groups these as SamplerPresets.greedy(), SamplerPresets.temperature(), etc.

/// Get the default sampler configuration.
#[napi(namespace = "SamplerPresets")]
pub fn default_sampler() -> SamplerConfig {
    SamplerConfig {
        inner: nobodywho::sampler_config::SamplerConfig::default(),
    }
}

/// Create a sampler with top-k filtering only.
#[napi(namespace = "SamplerPresets")]
pub fn top_k(top_k: i32) -> SamplerConfig {
    SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::top_k(top_k),
    }
}

/// Create a sampler with nucleus (top-p) sampling.
#[napi(namespace = "SamplerPresets")]
pub fn top_p(top_p: f64) -> SamplerConfig {
    SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::top_p(top_p as f32),
    }
}

/// Create a greedy sampler (always picks most probable token).
#[napi(namespace = "SamplerPresets")]
pub fn greedy() -> SamplerConfig {
    SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::greedy(),
    }
}

/// Create a sampler with temperature scaling.
#[napi(namespace = "SamplerPresets")]
pub fn temperature(temperature: f64) -> SamplerConfig {
    SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::temperature(temperature as f32),
    }
}

/// Create a DRY sampler preset to reduce repetition.
#[napi(namespace = "SamplerPresets")]
pub fn dry() -> SamplerConfig {
    SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::dry(),
    }
}

/// Create a sampler configured for JSON output generation.
#[napi(namespace = "SamplerPresets")]
pub fn json() -> SamplerConfig {
    SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::json(),
    }
}

/// Create a sampler with a custom grammar constraint.
#[napi(namespace = "SamplerPresets")]
pub fn grammar(grammar: String) -> SamplerConfig {
    SamplerConfig {
        inner: nobodywho::sampler_config::SamplerPresets::grammar(grammar),
    }
}
