use godot::prelude::*;

use nobodywho::sampler::{SampleStep, SamplerBuilder, SamplerConfig, SamplerPresets, ShiftStep};

use crate::convert::variant_to_json;

// ====================================================================
// NobodyWhoSamplerConfig
// ====================================================================

/// An opaque sampler configuration. Build one with
/// [`NobodyWhoSamplerBuilder`] or [`NobodyWhoSamplerPresets`], then pass it
/// to `NobodyWhoChat.set_sampler_config()` or `NobodyWhoChat.create()`.
///
/// Mirrors the Python `SamplerConfig`: it's an opaque holder you can also
/// (de)serialize to/from JSON for persistence.
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoSamplerConfig {
    pub(crate) inner: SamplerConfig,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoSamplerConfig {
    /// Serialize to a JSON string, or null on a serialization error.
    #[func]
    fn to_json(&self) -> Variant {
        match serde_json::to_string(&self.inner) {
            Ok(s) => GString::from(&s).to_variant(),
            Err(e) => {
                godot_error!("NobodyWhoSamplerConfig.to_json: {e}");
                Variant::nil()
            }
        }
    }

    /// Deserialize from a JSON string. Resolves to null on a parse error.
    #[func]
    fn from_json(json: GString) -> Variant {
        match serde_json::from_str::<SamplerConfig>(&json.to_string()) {
            Ok(cfg) => Self::wrap(cfg).to_variant(),
            Err(e) => {
                godot_error!("NobodyWhoSamplerConfig.from_json: {e}");
                Variant::nil()
            }
        }
    }
}

impl NobodyWhoSamplerConfig {
    /// Wrap a core `SamplerConfig` in a new Godot object.
    pub(crate) fn wrap(inner: SamplerConfig) -> Gd<Self> {
        Gd::from_init_fn(|base| Self { inner, base })
    }
}

// ====================================================================
// NobodyWhoSamplerBuilder
// ====================================================================

/// A fluent builder for [`NobodyWhoSamplerConfig`]. Chain shift steps, then
/// finish with a sampling step (`dist()` / `greedy()` / `mirostat_v1()` /
/// `mirostat_v2()`). Each chainable method returns a *new* builder.
///
/// ```gdscript
/// var cfg = NobodyWhoSamplerBuilder.new().top_k(40).temperature(0.8).dist()
/// ```
///
/// Mirrors the Python `SamplerBuilder`.
#[derive(GodotClass)]
#[class(base=RefCounted)]
pub struct NobodyWhoSamplerBuilder {
    inner: SamplerBuilder,
    base: Base<RefCounted>,
}

#[godot_api]
impl IRefCounted for NobodyWhoSamplerBuilder {
    fn init(base: Base<RefCounted>) -> Self {
        Self {
            inner: SamplerBuilder::new(),
            base,
        }
    }
}

#[godot_api]
impl NobodyWhoSamplerBuilder {
    // --- shift steps (chainable, return new Gd<Self>) ---

    /// Keep only the top K most probable tokens. Typical: 40-50.
    #[func]
    fn top_k(&self, top_k: i32) -> Gd<Self> {
        self.shift(ShiftStep::TopK { top_k })
    }

    /// Keep tokens whose cumulative probability is below `top_p` (0.0-1.0).
    /// `min_keep` guarantees at least that many tokens survive.
    #[func]
    fn top_p(&self, top_p: f32, min_keep: u32) -> Gd<Self> {
        self.shift(ShiftStep::TopP { top_p, min_keep })
    }

    /// Keep tokens with probability above `min_p` * (most-likely probability).
    #[func]
    fn min_p(&self, min_p: f32, min_keep: u32) -> Gd<Self> {
        self.shift(ShiftStep::MinP { min_p, min_keep })
    }

    /// XTC: probabilistically exclude high-probability tokens for diversity.
    #[func]
    fn xtc(&self, xtc_probability: f32, xtc_threshold: f32, min_keep: u32) -> Gd<Self> {
        self.shift(ShiftStep::XTC {
            xtc_probability,
            xtc_threshold,
            min_keep,
        })
    }

    /// Typical-p sampling.
    #[func]
    fn typical_p(&self, typ_p: f32, min_keep: u32) -> Gd<Self> {
        self.shift(ShiftStep::TypicalP { typ_p, min_keep })
    }

    /// Constrain output to a JSON schema, given as a JSON string or a
    /// Dictionary. Returns the extended builder, or null on a bad schema.
    #[func]
    fn json_schema(&self, schema: Variant) -> Variant {
        match schema_to_string(&schema) {
            Ok(s) => self
                .rebuild(|b| b.constrain_with_json_schema(s))
                .to_variant(),
            Err(e) => {
                godot_error!("NobodyWhoSamplerBuilder.json_schema: {e}");
                Variant::nil()
            }
        }
    }

    /// Constrain output to a regular expression.
    #[func]
    fn regex(&self, pattern: GString) -> Gd<Self> {
        self.rebuild(|b| b.constrain_with_regex(pattern.to_string()))
    }

    /// Constrain output using a Lark context-free grammar.
    #[func]
    fn lark(&self, grammar: GString) -> Gd<Self> {
        self.rebuild(|b| b.constrain_with_grammar(grammar.to_string()))
    }

    /// DRY (Don't Repeat Yourself) repetition penalty.
    #[func]
    fn dry(
        &self,
        multiplier: f32,
        base: f32,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: Array<GString>,
    ) -> Gd<Self> {
        self.shift(ShiftStep::DRY {
            multiplier,
            base,
            allowed_length,
            penalty_last_n,
            seq_breakers: seq_breakers.iter_shared().map(|g| g.to_string()).collect(),
        })
    }

    /// Repetition/frequency/presence penalties.
    #[func]
    fn penalties(
        &self,
        penalty_last_n: i32,
        penalty_repeat: f32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> Gd<Self> {
        self.shift(ShiftStep::Penalties {
            penalty_last_n,
            penalty_repeat,
            penalty_freq,
            penalty_present,
        })
    }

    /// Apply temperature scaling (0.0 deterministic, 1.0 unchanged, >1.0 random).
    #[func]
    fn temperature(&self, temperature: f32) -> Gd<Self> {
        self.shift(ShiftStep::Temperature { temperature })
    }

    /// Dynamic temperature scaling (a.k.a. entropy), as in
    /// <https://arxiv.org/abs/2309.02772>. The final temperature lands in
    /// `[temperature - delta; temperature + delta]`, computed as
    /// `entropy^exponent`.
    #[func]
    fn dynamic_temperature(&self, temperature: f32, delta: f32, exponent: f32) -> Gd<Self> {
        self.shift(ShiftStep::DynamicTemperature {
            temperature,
            delta,
            exponent,
        })
    }

    /// Top-n-sigma sampling, as in "Top-n-sigma: Not All Logits Are You Need"
    /// (<https://arxiv.org/pdf/2411.07641>): keep tokens within `n`
    /// standard deviations from the mean.
    #[func]
    fn top_n_sigma(&self, n: f32) -> Gd<Self> {
        self.shift(ShiftStep::TopNSigma { n })
    }

    /// Modify the likelihood of specific tokens. `biases` is a Dictionary
    /// mapping token ID (int) to bias (float): `> 0` makes the token more
    /// likely, `< 0` less likely, and `-INF` bans it entirely. Returns the
    /// extended builder, or null if a key or value has the wrong type.
    #[func]
    fn logit_bias(&self, biases: VarDictionary) -> Variant {
        let mut parsed = std::collections::HashMap::new();
        for (key, value) in biases.iter_shared() {
            let token_id = match key.try_to::<i64>() {
                Ok(id) => id as i32,
                Err(_) => {
                    godot_error!("NobodyWhoSamplerBuilder.logit_bias: key {key} is not a token ID");
                    return Variant::nil();
                }
            };
            let bias = match value.try_to::<f32>() {
                Ok(b) => b,
                Err(_) => {
                    godot_error!(
                        "NobodyWhoSamplerBuilder.logit_bias: bias for token {token_id} is not a float"
                    );
                    return Variant::nil();
                }
            };
            parsed.insert(token_id, bias);
        }
        self.shift(ShiftStep::LogitBias { biases: parsed })
            .to_variant()
    }

    /// Constrain output to a JSON object of any shape. For schema-validated
    /// JSON, chain `json_schema()` instead.
    #[func]
    fn json(&self) -> Gd<Self> {
        self.rebuild(|b| b.json())
    }

    /// Set the RNG seed for random samplers (`dist`, `mirostat*`, `xtc`).
    /// `greedy` ignores it.
    #[func]
    fn seed(&self, seed: u32) -> Gd<Self> {
        self.rebuild(|b| b.seed(seed))
    }

    // --- sampling steps (terminals, return Gd<NobodyWhoSamplerConfig>) ---

    /// Finish: weighted-random sampling from the distribution.
    #[func]
    fn dist(&self) -> Gd<NobodyWhoSamplerConfig> {
        self.sample(SampleStep::Dist)
    }

    /// Finish: always pick the most probable token (deterministic).
    #[func]
    fn greedy(&self) -> Gd<NobodyWhoSamplerConfig> {
        self.sample(SampleStep::Greedy)
    }

    /// Finish: Mirostat v1 (perplexity-controlled).
    #[func]
    fn mirostat_v1(&self, tau: f32, eta: f32, m: i32) -> Gd<NobodyWhoSamplerConfig> {
        self.sample(SampleStep::MirostatV1 { tau, eta, m })
    }

    /// Finish: Mirostat v2 (perplexity-controlled, simplified).
    #[func]
    fn mirostat_v2(&self, tau: f32, eta: f32) -> Gd<NobodyWhoSamplerConfig> {
        self.sample(SampleStep::MirostatV2 { tau, eta })
    }
}

impl NobodyWhoSamplerBuilder {
    /// Append a shift step; return a fresh builder holding the extended chain.
    fn shift(&self, step: ShiftStep) -> Gd<Self> {
        self.rebuild(|b| b.shift(step))
    }

    /// Apply a closure to a cloned core builder; wrap the result in a new Gd.
    /// Cloning avoids `bind_mut` — every method stays `&self`.
    fn rebuild<F>(&self, f: F) -> Gd<Self>
    where
        F: FnOnce(SamplerBuilder) -> SamplerBuilder,
    {
        Gd::from_init_fn(|base| Self {
            inner: f(self.inner.clone()),
            base,
        })
    }

    /// Terminate the chain with a sampling step; return a finished config.
    fn sample(&self, step: SampleStep) -> Gd<NobodyWhoSamplerConfig> {
        NobodyWhoSamplerConfig::wrap(self.inner.clone().sample(step))
    }
}

// ====================================================================
// NobodyWhoSamplerPresets
// ====================================================================

/// A pure namespace class: static `#[func]`s returning ready-made
/// [`NobodyWhoSamplerConfig`]s. Not instantiable. Mirrors Python's
/// `SamplerPresets`.
///
/// ```gdscript
/// var cfg = NobodyWhoSamplerPresets.temperature(0.8)
/// ```
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoSamplerPresets {
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoSamplerPresets {
    /// The default sampler chain (top_k=20, top_p=0.95, temperature=0.6, dist).
    #[func]
    fn default() -> Gd<NobodyWhoSamplerConfig> {
        NobodyWhoSamplerConfig::wrap(SamplerConfig::default())
    }

    /// Top-k filtering only.
    #[func]
    fn top_k(top_k: i32) -> Gd<NobodyWhoSamplerConfig> {
        NobodyWhoSamplerConfig::wrap(SamplerPresets::top_k(top_k))
    }

    /// Nucleus (top-p) sampling only.
    #[func]
    fn top_p(top_p: f32) -> Gd<NobodyWhoSamplerConfig> {
        NobodyWhoSamplerConfig::wrap(SamplerPresets::top_p(top_p))
    }

    /// Always pick the most probable token.
    #[func]
    fn greedy() -> Gd<NobodyWhoSamplerConfig> {
        NobodyWhoSamplerConfig::wrap(SamplerPresets::greedy())
    }

    /// Temperature scaling only.
    #[func]
    fn temperature(temperature: f32) -> Gd<NobodyWhoSamplerConfig> {
        NobodyWhoSamplerConfig::wrap(SamplerPresets::temperature(temperature))
    }

    /// DRY sampler preset.
    #[func]
    fn dry() -> Gd<NobodyWhoSamplerConfig> {
        NobodyWhoSamplerConfig::wrap(SamplerPresets::dry())
    }

    /// Constrain output to a JSON object of any shape. For schema-validated
    /// JSON, use `constrain_with_json_schema()` instead.
    #[func]
    fn json() -> Gd<NobodyWhoSamplerConfig> {
        NobodyWhoSamplerConfig::wrap(SamplerPresets::json())
    }

    /// Constrain output to a JSON schema, given as a JSON string or a
    /// Dictionary. Returns the config, or null on a bad schema.
    #[func]
    fn constrain_with_json_schema(schema: Variant) -> Variant {
        match schema_to_string(&schema) {
            Ok(s) => NobodyWhoSamplerConfig::wrap(SamplerPresets::constrain_with_json_schema(s))
                .to_variant(),
            Err(e) => {
                godot_error!("NobodyWhoSamplerPresets.constrain_with_json_schema: {e}");
                Variant::nil()
            }
        }
    }

    /// Constrain output to a regular expression.
    #[func]
    fn constrain_with_regex(pattern: GString) -> Gd<NobodyWhoSamplerConfig> {
        NobodyWhoSamplerConfig::wrap(SamplerPresets::constrain_with_regex(pattern.to_string()))
    }

    /// Constrain output using a Lark (or GBNF) grammar.
    #[func]
    fn constrain_with_grammar(grammar: GString) -> Gd<NobodyWhoSamplerConfig> {
        NobodyWhoSamplerConfig::wrap(SamplerPresets::constrain_with_grammar(grammar.to_string()))
    }
}

/// Accept a JSON schema as either a JSON string or a Godot Dictionary.
fn schema_to_string(schema: &Variant) -> Result<String, String> {
    use godot::builtin::VariantType;
    match schema.get_type() {
        VariantType::STRING => Ok(schema.to::<GString>().to_string()),
        VariantType::DICTIONARY => {
            let json = variant_to_json(schema)?;
            serde_json::to_string(&json).map_err(|e| format!("failed to serialize schema: {e}"))
        }
        other => Err(format!(
            "expected a JSON string or a Dictionary, got {other:?}"
        )),
    }
}
