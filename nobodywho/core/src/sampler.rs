use std::collections::HashMap;

use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::logit_bias::LlamaLogitBias;
use llama_cpp_2::{model::LlamaModel, token::LlamaToken};
use llguidance::toktrie::{InferenceCapabilities, TokEnv};
use llguidance::{api::TopLevelGrammar, Matcher, ParserFactory};
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::errors::SamplerError;

// ---- Presets ----

/// Some simple presets, that can be useful for basic sampling.
///
/// Each carries [`SamplerConfig::default`]'s steps and adds or overrides one of
/// them. [`SamplerPresets::greedy`] is the exception and needs none.
pub struct SamplerPresets;

const TOP_K_DEFAULT: i32 = 20;
const TOP_P_DEFAULT: f32 = 0.95;
const MIN_KEEP_DEFAULT: u32 = 1;
const TEMPERATURE_DEFAULT: f32 = 0.6;

impl SamplerPresets {
    pub fn top_k(k: i32) -> SamplerConfig {
        SamplerConfig::new(
            vec![],
            vec![
                ShiftStep::TopK { top_k: k },
                ShiftStep::default_top_p(),
                ShiftStep::default_temperature(),
            ],
            SampleStep::Dist,
            default_seed(),
        )
    }

    pub fn top_p(p: f32) -> SamplerConfig {
        SamplerConfig::new(
            vec![],
            vec![
                ShiftStep::default_top_k(),
                ShiftStep::TopP {
                    top_p: p,
                    min_keep: MIN_KEEP_DEFAULT,
                },
                ShiftStep::default_temperature(),
            ],
            SampleStep::Dist,
            default_seed(),
        )
    }

    pub fn greedy() -> SamplerConfig {
        SamplerConfig::new(vec![], vec![], SampleStep::Greedy, default_seed())
    }

    pub fn temperature(temperature: f32) -> SamplerConfig {
        SamplerConfig::new(
            vec![],
            vec![
                ShiftStep::default_top_k(),
                ShiftStep::default_top_p(),
                ShiftStep::Temperature { temperature },
            ],
            SampleStep::Dist,
            default_seed(),
        )
    }

    /// The default steps plus a DRY penalty, at the usual `multiplier` 0.8 and
    /// `base` 1.75 tuning. Applies before top_k, top_p, and temperature.
    pub fn dry() -> SamplerConfig {
        SamplerConfig::new(
            vec![],
            vec![
                ShiftStep::DRY {
                    multiplier: 0.8,
                    base: 1.75,
                    allowed_length: 2,
                    penalty_last_n: -1,
                    seq_breakers: vec![
                        "\n".to_string(),
                        ":".to_string(),
                        "\"".to_string(),
                        "*".to_string(),
                    ],
                },
                ShiftStep::default_top_k(),
                ShiftStep::default_top_p(),
                ShiftStep::default_temperature(),
            ],
            SampleStep::Dist,
            default_seed(),
        )
    }

    /// Constrain output to a JSON schema using llguidance.
    pub fn constrain_with_json_schema(schema: String) -> SamplerConfig {
        SamplerConfig::new(
            vec![ConstraintStep::JsonSchema(schema)],
            vec![
                ShiftStep::default_top_k(),
                ShiftStep::default_top_p(),
                ShiftStep::default_temperature(),
            ],
            SampleStep::Dist,
            default_seed(),
        )
    }

    /// Constrain output to a regular expression using llguidance.
    pub fn constrain_with_regex(pattern: String) -> SamplerConfig {
        SamplerConfig::new(
            vec![ConstraintStep::Regex(pattern)],
            vec![
                ShiftStep::default_top_k(),
                ShiftStep::default_top_p(),
                ShiftStep::default_temperature(),
            ],
            SampleStep::Dist,
            default_seed(),
        )
    }

    /// Constrain output using a Lark context-free grammar via llguidance.
    pub fn constrain_with_grammar(lark: String) -> SamplerConfig {
        SamplerConfig::new(
            vec![ConstraintStep::Lark(lark)],
            vec![
                ShiftStep::default_top_k(),
                ShiftStep::default_top_p(),
                ShiftStep::default_temperature(),
            ],
            SampleStep::Dist,
            default_seed(),
        )
    }

    /// Constrain output to a JSON object of any shape.
    pub fn json() -> SamplerConfig {
        SamplerConfig::new(
            vec![ConstraintStep::JsonSchema(JSON_OBJECT_SCHEMA.into())],
            vec![
                ShiftStep::default_top_k(),
                ShiftStep::default_top_p(),
                ShiftStep::default_temperature(),
            ],
            SampleStep::Dist,
            default_seed(),
        )
    }
}

/// Sampler configuration struct.
///
/// The chain runs `constraining_steps`, then `steps`, then `sample_step`: a
/// constraint has to mask before a shift step truncates away its valid tokens.
///
/// Carries a single `seed` that is consumed by every random sampler in the
/// chain (`SampleStep::Dist`, `MirostatV1`, `MirostatV2`, and `ShiftStep::XTC`).
/// `SampleStep::Greedy` ignores it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplerConfig {
    #[serde(default)]
    pub constraining_steps: Vec<ConstraintStep>,
    pub steps: Vec<ShiftStep>,
    pub sample_step: SampleStep,
    #[serde(default = "default_seed")]
    pub seed: u32,
}

pub fn default_seed() -> u32 {
    1234
}

impl SamplerConfig {
    pub fn new(
        constraining_steps: Vec<ConstraintStep>,
        shift_steps: Vec<ShiftStep>,
        sample_step: SampleStep,
        seed: u32,
    ) -> Self {
        Self {
            constraining_steps,
            steps: shift_steps,
            sample_step,
            seed,
        }
    }

    pub fn build_sampler(&self, model: &LlamaModel) -> Result<LlamaSampler, SamplerError> {
        self.build_sampler_with_prepended_step(model, None)
    }

    /// Builds a sampler chain, optionally with an already-built grammar step
    /// prepended. The caller owns that step so it can reuse a [`GrammarFactory`]
    /// across rebuilds instead of recompiling the grammar here.
    pub(crate) fn build_sampler_with_prepended_step(
        &self,
        model: &LlamaModel,
        extra_step: Option<LlamaSampler>,
    ) -> Result<LlamaSampler, SamplerError> {
        // Constraints go first, so they mask before anything truncates.
        let mut shift_steps = extra_step
            .into_iter()
            .map(Ok)
            .chain(
                self.constraining_steps
                    .iter()
                    .cloned()
                    .map(|step| self.build_constraint(model, step)),
            )
            .chain(
                self.steps
                    .iter()
                    .cloned()
                    .map(|step| self.build_step(model, step)),
            )
            .collect::<Result<Vec<_>, SamplerError>>()?;

        let final_sampler = match self.sample_step.clone() {
            SampleStep::Dist => LlamaSampler::dist(self.seed),
            SampleStep::Greedy => LlamaSampler::greedy(),
            SampleStep::MirostatV1 { tau, eta, m } => {
                LlamaSampler::mirostat(model.n_vocab(), self.seed, tau, eta, m)
            }
            SampleStep::MirostatV2 { tau, eta } => LlamaSampler::mirostat_v2(self.seed, tau, eta),
        };

        shift_steps.push(final_sampler);

        Ok(LlamaSampler::chain(shift_steps, true))
    }

    fn build_step(
        &self,
        model: &LlamaModel,
        step: ShiftStep,
    ) -> Result<LlamaSampler, SamplerError> {
        match step {
            ShiftStep::TopK { top_k } => Ok(LlamaSampler::top_k(top_k)),
            ShiftStep::TopP { min_keep, top_p } => {
                Ok(LlamaSampler::top_p(top_p, min_keep as usize))
            }
            ShiftStep::XTC {
                xtc_probability,
                xtc_threshold,
                min_keep,
            } => Ok(LlamaSampler::xtc(
                xtc_probability,
                xtc_threshold,
                min_keep as usize,
                self.seed,
            )),
            ShiftStep::TypicalP { typ_p, min_keep } => {
                Ok(LlamaSampler::typical(typ_p, min_keep as usize))
            }
            ShiftStep::MinP { min_keep, min_p } => {
                Ok(LlamaSampler::min_p(min_p, min_keep as usize))
            }
            ShiftStep::DRY {
                multiplier,
                base,
                allowed_length,
                penalty_last_n,
                seq_breakers,
            } => Ok(LlamaSampler::dry(
                model,
                multiplier,
                base,
                allowed_length,
                penalty_last_n,
                seq_breakers,
            )),
            ShiftStep::Penalties {
                penalty_last_n,
                penalty_repeat,
                penalty_freq,
                penalty_present,
            } => Ok(LlamaSampler::penalties(
                penalty_last_n,
                penalty_repeat,
                penalty_freq,
                penalty_present,
            )),
            ShiftStep::Temperature { temperature } => Ok(LlamaSampler::temp(temperature)),
            ShiftStep::DynamicTemperature {
                temperature,
                delta,
                exponent,
            } => Ok(LlamaSampler::temp_ext(temperature, delta, exponent)),
            ShiftStep::TopNSigma { n } => Ok(LlamaSampler::top_n_sigma(n)),
            ShiftStep::LogitBias { biases } => {
                let biases: Vec<_> = biases
                    .into_iter()
                    .map(|(token_id, bias)| LlamaLogitBias::new(LlamaToken::new(token_id), bias))
                    .collect();
                Ok(LlamaSampler::logit_bias(model.n_vocab(), &biases))
            }
        }
    }

    fn build_constraint(
        &self,
        model: &LlamaModel,
        step: ConstraintStep,
    ) -> Result<LlamaSampler, SamplerError> {
        match step {
            // A schema always has JSON string bodies, so the slice pays for itself.
            ConstraintStep::JsonSchema(schema) => llguidance_sampler(
                model,
                "json_schema",
                &schema,
                &crate::tool_calling::json_body_slice_regexes(),
            ),
            ConstraintStep::Regex(pattern) => llguidance_sampler(model, "regex", &pattern, &[]),
            ConstraintStep::Lark(lark) => {
                let lark = gbnf::gbnf_to_lark::any_to_lark(&lark)
                    .map_err(|e| SamplerError::GbnfConversionError(e.to_string()))?;
                llguidance_sampler(model, "lark", &lark, &[])
            }
        }
    }
}

/// Reusable llguidance state for one model and slice set. The tokenizer env and
/// the slicer are functions of the vocabulary, not the grammar, so holding them
/// turns a grammar rebuild into `create_parser` (~5ms instead of ~400ms).
pub(crate) struct GrammarFactory {
    tok_env: TokEnv,
    slices: Vec<String>,
    factory: ParserFactory,
}

impl GrammarFactory {
    /// Builds a factory for `slices`, or returns `None` if `held` already serves
    /// them. Handing the new one back instead of storing it lets the caller finish
    /// its own fallible work before committing.
    pub(crate) fn build_if_stale(
        held: Option<&Self>,
        model: &LlamaModel,
        slices: Vec<String>,
    ) -> Result<Option<Self>, SamplerError> {
        match held {
            Some(factory) if factory.slices == slices => Ok(None),
            // A new slice set needs a new slicer but not a second vocab walk.
            Some(stale) => Self::from_tok_env(stale.tok_env.clone(), slices).map(Some),
            None => Self::new(model, slices).map(Some),
        }
    }

    fn new(model: &LlamaModel, slices: Vec<String>) -> Result<Self, SamplerError> {
        // The vocab walk behind `llguidance_tok_env` is what dominates the cost.
        Self::from_tok_env(LlamaSampler::llguidance_tok_env(model), slices)
    }

    fn from_tok_env(tok_env: TokEnv, slices: Vec<String>) -> Result<Self, SamplerError> {
        let factory = ParserFactory::new(&tok_env, InferenceCapabilities::default(), &slices)
            .map_err(|e| SamplerError::LlguidanceGrammarError(e.to_string()))?;
        Ok(Self {
            tok_env,
            slices,
            factory,
        })
    }

    /// A grammar step for a `json_schema`/`regex`/`lark` `tag` + content string.
    pub(crate) fn grammar_step(
        &self,
        tag: &str,
        grammar: &str,
    ) -> Result<LlamaSampler, SamplerError> {
        let tlg = TopLevelGrammar::from_tagged_str(tag, grammar)
            .map_err(|e| SamplerError::LlguidanceGrammarError(e.to_string()))?;
        let parser = self
            .factory
            .create_parser(tlg)
            .map_err(|e| SamplerError::LlguidanceGrammarError(e.to_string()))?;
        Ok(LlamaSampler::from(Matcher::new(Ok(parser))))
    }
}

/// Builds an llguidance [`LlamaSampler`] for a `json_schema`/`regex`/`lark`
/// `tag` + `grammar` content string. `slices` are optional vocabulary hints
/// (see [`crate::tool_calling::ToolFormatHandler::slice_regexes`]), `&[]` for none.
///
/// Builds a throwaway [`GrammarFactory`]; hold one instead if the same slice set
/// will be used again.
pub fn llguidance_sampler(
    model: &LlamaModel,
    tag: &str,
    grammar: &str,
    slices: &[String],
) -> Result<LlamaSampler, SamplerError> {
    GrammarFactory::new(model, slices.to_vec())?.grammar_step(tag, grammar)
}

impl Default for SamplerConfig {
    fn default() -> SamplerConfig {
        SamplerConfig::new(
            vec![],
            vec![
                ShiftStep::default_top_k(),
                ShiftStep::default_top_p(),
                ShiftStep::default_temperature(),
            ],
            SampleStep::Dist,
            default_seed(),
        )
    }
}

#[derive(Clone)]
pub struct SamplerBuilder {
    constraining_steps: Vec<ConstraintStep>,
    steps: Vec<ShiftStep>,
    seed: u32,
}

impl Default for SamplerBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl SamplerBuilder {
    pub fn new() -> Self {
        Self {
            constraining_steps: vec![],
            steps: vec![],
            seed: default_seed(),
        }
    }

    /// Appends a shift step; the chain runs them in the order they were added.
    /// A penalty added after a truncation step only sees what survived it.
    pub fn shift(mut self, step: ShiftStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Appends a constraint. Constraints run before every shift step regardless
    /// of where they are added, for the reason in [`SamplerConfig`].
    pub fn constrain(mut self, step: ConstraintStep) -> Self {
        self.constraining_steps.push(step);
        self
    }

    /// Set the RNG seed used by random samplers (`Dist`, `Mirostat*`, `XTC`).
    /// `Greedy` ignores it. If unset, `default_seed()` is used.
    pub fn seed(mut self, seed: u32) -> Self {
        self.seed = seed;
        self
    }

    pub fn sample(self, step: SampleStep) -> SamplerConfig {
        SamplerConfig {
            constraining_steps: self.constraining_steps,
            steps: self.steps,
            sample_step: step,
            seed: self.seed,
        }
    }

    /// Constrain output to a JSON object of any shape. Use
    /// [`constrain_with_json_schema`][Self::constrain_with_json_schema] to pin
    /// down the structure too.
    pub fn json(self) -> Self {
        self.constrain(ConstraintStep::JsonSchema(JSON_OBJECT_SCHEMA.into()))
    }

    /// Constrain output to a JSON schema.
    pub fn constrain_with_json_schema(self, schema: String) -> Self {
        self.constrain(ConstraintStep::JsonSchema(schema))
    }

    /// Constrain output to a regular expression.
    pub fn constrain_with_regex(self, pattern: String) -> Self {
        self.constrain(ConstraintStep::Regex(pattern))
    }

    /// Constrain output to a grammar, given as either Lark or GBNF.
    pub fn constrain_with_grammar(self, grammar: String) -> Self {
        self.constrain(ConstraintStep::Lark(grammar))
    }
}

/// Any JSON object, for the `json` preset and builder step. A schema rather than
/// a hand-written grammar so it takes the [`ConstraintStep::JsonSchema`] path and its
/// slices; an object rather than a bare `{}`, since an any-value constraint is
/// already satisfied by a one-token scalar and models answer `false` and stop.
const JSON_OBJECT_SCHEMA: &str = r#"{"type":"object"}"#;

/// A step that masks out the tokens some format disallows. Held apart from
/// [`ShiftStep`] because it has to run before any of them — see [`SamplerConfig`].
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ConstraintStep {
    /// Constrain output to a JSON schema via llguidance.
    JsonSchema(String),
    /// Constrain output to a regular expression via llguidance.
    Regex(String),
    /// Constrain output using a Lark context-free grammar via llguidance.
    /// GBNF is accepted too, and converted before use.
    Lark(String),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum ShiftStep {
    TopK {
        top_k: i32,
    },
    TopP {
        min_keep: u32,
        top_p: f32,
    },
    MinP {
        min_keep: u32,
        min_p: f32,
    },
    #[serde(rename = "xtc")]
    XTC {
        xtc_probability: f32,
        xtc_threshold: f32,
        min_keep: u32,
    },
    TypicalP {
        typ_p: f32,
        min_keep: u32,
    },
    #[serde(rename = "dry")]
    DRY {
        multiplier: f32,
        base: f32,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: Vec<String>,
    },
    Penalties {
        penalty_last_n: i32,
        penalty_repeat: f32,
        penalty_freq: f32,
        penalty_present: f32,
    },
    Temperature {
        temperature: f32,
    },
    /// Apply dynamic temperature scaling (a.k.a. entropy) described in the paper
    /// <https://arxiv.org/abs/2309.02772>.
    #[serde(rename = "temp_ext")]
    DynamicTemperature {
        /// Temperature value (lower = more focused, higher = more random)
        temperature: f32,
        /// Dynamic temperature range.
        ///
        /// The final temperature will be in the range of `[temperature - delta; temperature + delta]`.
        delta: f32,
        /// Temperature is calculated as `entropy^exponent` (bounded by the range above).
        exponent: f32,
    },
    /// Top-nσ sampling as described in academic paper "Top-nσ: Not All Logits Are You Need"
    /// <https://arxiv.org/pdf/2411.07641>
    TopNSigma {
        /// Number of standard deviations from the mean to include in sampling.
        n: f32,
    },
    /// Modify the likelihood of specific tokens.
    LogitBias {
        /// Mapping from token ID to its bias.
        ///
        /// The bias modifies the likelihood of the token being selected
        /// (`>0.0` means higher probability of the token being selected).
        /// Use [`f32::NEG_INFINITY`] to ban a token.
        biases: HashMap<i32, f32>,
    },
    // FIXME(madsmtm): Add `Infill` variant once `llama-cpp-rs` supports it?
}

impl ShiftStep {
    /// The top-k step [`SamplerConfig::default`] and the presets use.
    pub fn default_top_k() -> Self {
        ShiftStep::TopK {
            top_k: TOP_K_DEFAULT,
        }
    }

    /// The top-p step [`SamplerConfig::default`] and the presets use.
    pub fn default_top_p() -> Self {
        ShiftStep::TopP {
            top_p: TOP_P_DEFAULT,
            min_keep: MIN_KEEP_DEFAULT,
        }
    }

    /// The temperature step [`SamplerConfig::default`] and the presets use.
    pub fn default_temperature() -> Self {
        ShiftStep::Temperature {
            temperature: TEMPERATURE_DEFAULT,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "type", content = "value", rename_all = "snake_case")]
pub enum SampleStep {
    Dist,
    Greedy,
    MirostatV1 { tau: f32, eta: f32, m: i32 },
    MirostatV2 { tau: f32, eta: f32 },
}

fn read_meta_f32(model: &LlamaModel, key: &str) -> Option<f32> {
    model.meta_val_str(key).ok()?.trim().parse::<f32>().ok()
}

fn read_meta_i32(model: &LlamaModel, key: &str) -> Option<i32> {
    model.meta_val_str(key).ok()?.trim().parse::<i32>().ok()
}

pub(crate) fn read_sampler_from_metadata(model: &LlamaModel) -> Option<SamplerConfig> {
    let temp = read_meta_f32(model, "general.sampling.temp");
    let top_k = read_meta_i32(model, "general.sampling.top_k");
    let top_p = read_meta_f32(model, "general.sampling.top_p");
    let min_p = read_meta_f32(model, "general.sampling.min_p");
    let xtc_probability = read_meta_f32(model, "general.sampling.xtc_probability");
    let xtc_threshold = read_meta_f32(model, "general.sampling.xtc_threshold");
    let penalty_last_n = read_meta_i32(model, "general.sampling.penalty_last_n");
    let penalty_repeat = read_meta_f32(model, "general.sampling.penalty_repeat");
    let penalty_freq = read_meta_f32(model, "general.sampling.penalty_freq");
    let penalty_present = read_meta_f32(model, "general.sampling.penalty_present");
    let mirostat = read_meta_i32(model, "general.sampling.mirostat");
    let mirostat_tau = read_meta_f32(model, "general.sampling.mirostat_tau");
    let mirostat_eta = read_meta_f32(model, "general.sampling.mirostat_eta");

    // Return None early if no sampling keys are present in this GGUF
    if temp.is_none()
        && top_k.is_none()
        && top_p.is_none()
        && min_p.is_none()
        && xtc_probability.is_none()
        && penalty_last_n.is_none()
        && mirostat.is_none()
    {
        return None;
    }

    // Use sequence key if present to determine step order, otherwise fall back to llama.cpp default
    let sequence_str = model.meta_val_str("general.sampling.sequence").ok();
    let sampler_names: Vec<&str> = if let Some(ref seq) = sequence_str {
        seq.split(';').map(str::trim).collect()
    } else {
        vec![
            "penalties",
            "top_k",
            "top_p",
            "min_p",
            "xtc",
            "temp",
            "dist",
        ]
    };

    let mut steps = Vec::new();
    let mut sample_step = None;

    for name in &sampler_names {
        match *name {
            "temp" | "temperature" => {
                if let Some(t) = temp {
                    steps.push(ShiftStep::Temperature { temperature: t });
                }
            }
            "top_k" => {
                if let Some(k) = top_k {
                    steps.push(ShiftStep::TopK { top_k: k });
                }
            }
            "top_p" => {
                if let Some(p) = top_p {
                    steps.push(ShiftStep::TopP {
                        top_p: p,
                        min_keep: 1,
                    });
                }
            }
            "min_p" => {
                if let Some(p) = min_p {
                    steps.push(ShiftStep::MinP {
                        min_p: p,
                        min_keep: 1,
                    });
                }
            }
            "xtc" => {
                if let (Some(prob), Some(thresh)) = (xtc_probability, xtc_threshold) {
                    steps.push(ShiftStep::XTC {
                        xtc_probability: prob,
                        xtc_threshold: thresh,
                        min_keep: 1,
                    });
                }
            }
            "penalties" | "repeat_penalty" => {
                if penalty_last_n.is_some() || penalty_repeat.is_some() {
                    steps.push(ShiftStep::Penalties {
                        penalty_last_n: penalty_last_n.unwrap_or(64),
                        penalty_repeat: penalty_repeat.unwrap_or(1.0),
                        penalty_freq: penalty_freq.unwrap_or(0.0),
                        penalty_present: penalty_present.unwrap_or(0.0),
                    });
                }
            }
            "dist" => {
                sample_step = Some(SampleStep::Dist);
            }
            "greedy" => {
                sample_step = Some(SampleStep::Greedy);
            }
            "mirostat" => {
                if let Some(mode) = mirostat {
                    match mode {
                        1 => {
                            sample_step = Some(SampleStep::MirostatV1 {
                                tau: mirostat_tau.unwrap_or(5.0),
                                eta: mirostat_eta.unwrap_or(0.1),
                                m: 100,
                            });
                        }
                        2 => {
                            sample_step = Some(SampleStep::MirostatV2 {
                                tau: mirostat_tau.unwrap_or(5.0),
                                eta: mirostat_eta.unwrap_or(0.1),
                            });
                        }
                        _ => {}
                    }
                }
            }
            unknown => warn!(
                "Unknown sampler step '{}' in GGUF metadata, skipping",
                unknown
            ),
        }
    }

    Some(SamplerConfig::new(
        vec![],
        steps,
        sample_step.unwrap_or(SampleStep::Dist),
        default_seed(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    /// A preset that overrides one default step keeps the other two, in place.
    #[test]
    fn preset_replaces_only_its_own_default_step() {
        let defaults = SamplerConfig::default().steps;
        let cases = [
            (SamplerPresets::top_k(5), ShiftStep::TopK { top_k: 5 }),
            (
                SamplerPresets::top_p(0.5),
                ShiftStep::TopP {
                    top_p: 0.5,
                    min_keep: 1,
                },
            ),
            (
                SamplerPresets::temperature(0.2),
                ShiftStep::Temperature { temperature: 0.2 },
            ),
        ];

        for (config, expected) in cases {
            assert_eq!(
                config.steps.len(),
                defaults.len(),
                "a preset should override a default step, not add one: {:?}",
                config.steps
            );
            let differing: Vec<_> = config
                .steps
                .iter()
                .zip(&defaults)
                .filter(|(step, default)| step != default)
                .map(|(step, _)| step.clone())
                .collect();
            assert_eq!(
                differing,
                vec![expected],
                "exactly one step should differ from the defaults, got: {:?}",
                config.steps
            );
        }
    }

    /// A constraint preset carries its constraint in `constraining_steps`, and
    /// samples the tokens it leaves with the default steps.
    #[test]
    fn constraint_preset_samples_with_the_defaults() {
        for config in [
            SamplerPresets::constrain_with_regex("yes|no".into()),
            SamplerPresets::constrain_with_json_schema("{}".into()),
            SamplerPresets::constrain_with_grammar("start: \"a\"".into()),
            SamplerPresets::json(),
        ] {
            assert_eq!(
                config.constraining_steps.len(),
                1,
                "expected exactly the one constraint, got: {:?}",
                config.constraining_steps
            );
            assert_eq!(
                config.steps,
                SamplerConfig::default().steps,
                "a constraint preset should sample with the default steps"
            );
        }
    }

    /// The DRY preset leads with its penalty: a penalty after truncation only
    /// sees the tokens that survived it.
    #[test]
    fn dry_preset_penalises_before_truncating() {
        let steps = SamplerPresets::dry().steps;
        assert!(
            matches!(steps.first(), Some(ShiftStep::DRY { .. })),
            "the dry preset should lead with a DRY step, got: {steps:?}"
        );
        assert_eq!(
            &steps[1..],
            &SamplerConfig::default().steps[..],
            "the default steps should follow the DRY step, got: {steps:?}"
        );
    }

    /// llama.cpp treats multiplier 0, base < 1 or last_n 0 as "DRY off", which
    /// would leave the preset sampling exactly like the default one.
    #[test]
    fn dry_preset_is_enabled() {
        let steps = SamplerPresets::dry().steps;
        let ShiftStep::DRY {
            multiplier,
            base,
            penalty_last_n,
            ..
        } = &steps[0]
        else {
            panic!("the dry preset should lead with a DRY step, got: {steps:?}");
        };
        assert!(*multiplier > 0.0, "DRY is disabled by a zero multiplier");
        assert!(*base >= 1.0, "DRY is disabled by a base below 1");
        assert!(*penalty_last_n != 0, "DRY is disabled by a zero last_n");
    }

    /// A matching slice set reuses the held factory; a different one needs a new
    /// slicer but not a second vocab walk.
    #[test]
    fn grammar_factory_build_if_stale_reuses_tok_env() {
        let model = &crate::test_utils::load_test_model().language_model;
        let factory = GrammarFactory::build_if_stale(None, model, vec![])
            .expect("building a factory")
            .expect("no held factory means a new one");

        assert!(
            GrammarFactory::build_if_stale(Some(&factory), model, vec![])
                .expect("checking the same slice set")
                .is_none(),
            "the held factory already serves an unchanged slice set"
        );

        let slices = crate::tool_calling::json_body_slice_regexes();
        let rebuilt = GrammarFactory::build_if_stale(Some(&factory), model, slices.clone())
            .expect("rebuilding for another slice set")
            .expect("a different slice set needs a new factory");

        assert_eq!(rebuilt.slices, slices, "rebuilt holds the new slice set");
        assert!(
            Arc::ptr_eq(&factory.tok_env, &rebuilt.tok_env),
            "the tokenizer env is shared, not walked again"
        );
    }

    #[test]
    fn test_json_preset_builds_sampler() {
        let path = std::env::var("TEST_MODEL").expect("set TEST_MODEL to a gguf path");
        let model = crate::llm::get_model(&path, false, None, None, None).expect("load model");

        let res = SamplerPresets::json().build_sampler(&model.language_model);
        assert!(res.is_ok(), "json preset failed: {:?}", res.err());
    }

    /// Model-independent regression test for issue #421: a grammar whose
    /// only valid first token is never in the top-k forces an empty
    /// candidate set when the grammar runs after truncation, which aborts
    /// the process with an uncatchable C++ exception. With the grammar
    /// first the literal is emitted regardless of the model.
    #[test]
    fn test_ordering_grammar_first_with_unlikely_literal() {
        let path = std::env::var("TEST_MODEL").expect("set TEST_MODEL to a gguf path");
        let model = std::sync::Arc::new(
            crate::llm::get_model(&path, false, None, None, None).expect("load model"),
        );

        let cfg = SamplerConfig::new(
            vec![ConstraintStep::Lark("root ::= \"zqxjvkw\"".into())],
            vec![ShiftStep::TopK { top_k: 1 }],
            SampleStep::Dist,
            default_seed(),
        );

        let chat = crate::chat::ChatBuilder::new(model)
            .build()
            .expect("build chat");
        chat.set_sampler_config(cfg).expect("set sampler config");
        let response = chat
            .ask("Say hello.")
            .completed()
            .expect("generation with grammar-first unlikely literal failed");

        assert_eq!(response, "zqxjvkw");
    }

    /// Regression test for issue #421, mirroring the Godot repro (start
    /// worker, set the json preset, ask). Before the fix the grammar step
    /// ran after top-k, and models whose top candidates contained no
    /// grammar-valid token (e.g. thinking models such as Qwen3) crashed
    /// the process during generation.
    #[test]
    fn test_json_preset_full_generation() {
        let path = std::env::var("TEST_MODEL").expect("set TEST_MODEL to a gguf path");
        let model = std::sync::Arc::new(
            crate::llm::get_model(&path, false, None, None, None).expect("load model"),
        );

        let chat = crate::chat::ChatBuilder::new(model)
            .build()
            .expect("build chat");
        chat.set_sampler_config(SamplerPresets::json())
            .expect("set sampler config");
        let response = chat
            .ask("Return {\"hello\": \"world\"}.")
            .completed()
            .expect("generation with json preset failed");

        assert!(!response.is_empty(), "empty response");
        let parsed = serde_json::from_str::<serde_json::Value>(&response)
            .unwrap_or_else(|e| panic!("response is not valid JSON ({e}): {response}"));
        // A bare `{}` schema would also be valid JSON, but it lets the model
        // finish with a one-token scalar like `false`, so we constrain to an object.
        assert!(parsed.is_object(), "expected an object, got: {response}");
    }

    #[test]
    fn test_shift_appends_to_end() {
        let config = SamplerBuilder::new()
            .shift(ShiftStep::TopK { top_k: 40 })
            .shift(ShiftStep::Temperature { temperature: 0.8 })
            .sample(SampleStep::Dist);

        assert_eq!(config.steps.len(), 2);
        // Verify order: TopK first, Temperature second
        assert!(matches!(config.steps[0], ShiftStep::TopK { .. }));
        assert!(matches!(config.steps[1], ShiftStep::Temperature { .. }));
    }

    /// Shift steps stay in the order they were chained — including penalties,
    /// which are no longer moved ahead of truncation on the caller's behalf.
    #[test]
    fn test_shift_keeps_the_order_it_was_given() {
        let config = SamplerBuilder::new()
            .shift(ShiftStep::TopK { top_k: 40 })
            .shift(ShiftStep::DRY {
                multiplier: 0.8,
                base: 1.75,
                allowed_length: 2,
                penalty_last_n: -1,
                seq_breakers: vec!["\n".to_string()],
            })
            .shift(ShiftStep::LogitBias {
                biases: HashMap::from([(7, 2.0)]),
            })
            .sample(SampleStep::Dist);

        assert!(
            matches!(config.steps[0], ShiftStep::TopK { .. })
                && matches!(config.steps[1], ShiftStep::DRY { .. })
                && matches!(config.steps[2], ShiftStep::LogitBias { .. }),
            "expected [top_k, dry, logit_bias], got: {:?}",
            config.steps
        );
    }

    /// A constraint lands in its own list wherever it is chained, so it cannot
    /// end up behind a truncation step.
    #[test]
    fn test_constraints_are_kept_apart_from_shift_steps() {
        let config = SamplerBuilder::new()
            .shift(ShiftStep::TopK { top_k: 40 })
            .constrain_with_regex("yes|no".into())
            .json()
            .sample(SampleStep::Dist);

        assert_eq!(
            config.steps,
            vec![ShiftStep::TopK { top_k: 40 }],
            "the constraints should not be in `steps`"
        );
        assert!(
            matches!(config.constraining_steps[0], ConstraintStep::Regex(_))
                && matches!(config.constraining_steps[1], ConstraintStep::JsonSchema(_)),
            "constraints should keep the order they were added, got: {:?}",
            config.constraining_steps
        );
    }

    /// The builder counterpart of `test_ordering_grammar_first_with_unlikely_literal`:
    /// chaining the constraint last must not put it after the truncation step.
    #[test]
    fn test_builder_constraint_survives_top_k_one() {
        let path = std::env::var("TEST_MODEL").expect("set TEST_MODEL to a gguf path");
        let model = std::sync::Arc::new(
            crate::llm::get_model(&path, false, None, None, None).expect("load model"),
        );

        let cfg = SamplerBuilder::new()
            .shift(ShiftStep::TopK { top_k: 1 })
            .constrain_with_grammar("root ::= \"zqxjvkw\"".into())
            .sample(SampleStep::Dist);

        let chat = crate::chat::ChatBuilder::new(model)
            .build()
            .expect("build chat");
        chat.set_sampler_config(cfg).expect("set sampler config");
        let response = chat
            .ask("Say hello.")
            .completed()
            .expect("generation with a builder-added constraint failed");

        assert_eq!(response, "zqxjvkw");
    }

    #[test]
    fn test_serialize_deserialize_round_trip() {
        let config = SamplerConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SamplerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{:?}", config), format!("{:?}", deserialized));
    }

    /// v2.2.0 stored `SamplerConfig` JSON without the per-step `seed` fields:
    /// `Dist` was a unit variant, `MirostatV1`/`MirostatV2`/`XTC` had no seed.
    /// After this refactor those fields became required. To avoid breaking users
    /// who persisted sampler configs from older versions, the new `seed` fields
    /// should fall back to `default_seed()` when missing from JSON.
    #[test]
    fn test_deserialize_legacy_v2_2_0_json() {
        let legacy_dist_and_xtc = r#"{
            "steps": [
                {"type":"top_k","value":{"top_k":20}},
                {"type":"xtc","value":{"xtc_probability":0.1,"xtc_threshold":0.5,"min_keep":1}}
            ],
            "sample_step": {"type":"dist"}
        }"#;
        let cfg: SamplerConfig = serde_json::from_str(legacy_dist_and_xtc)
            .expect("legacy v2.2.0 JSON with dist + xtc (no seed fields) should deserialize");
        assert_eq!(cfg.steps.len(), 2);
        assert!(matches!(cfg.sample_step, SampleStep::Dist));

        let legacy_mirostat_v2 = r#"{
            "steps": [],
            "sample_step": {"type":"mirostat_v2","value":{"tau":5.0,"eta":0.1}}
        }"#;
        let cfg: SamplerConfig = serde_json::from_str(legacy_mirostat_v2)
            .expect("legacy v2.2.0 JSON with mirostat_v2 (no seed field) should deserialize");
        assert!(matches!(cfg.sample_step, SampleStep::MirostatV2 { .. }));

        let legacy_mirostat_v1 = r#"{
            "steps": [],
            "sample_step": {"type":"mirostat_v1","value":{"tau":5.0,"eta":0.1,"m":100}}
        }"#;
        let cfg: SamplerConfig = serde_json::from_str(legacy_mirostat_v1)
            .expect("legacy v2.2.0 JSON with mirostat_v1 (no seed field) should deserialize");
        assert!(matches!(cfg.sample_step, SampleStep::MirostatV1 { .. }));
    }

    /// Constraints used to be `ShiftStep`s, so a config saved by an older
    /// version has them inside `steps`. They belong in `constraining_steps` now,
    /// and the old shape is rejected rather than silently sampled unconstrained.
    #[test]
    fn test_deserialize_rejects_constraint_in_shift_steps() {
        let constraint_in_steps = r#"{
            "steps": [
                {"type":"regex","value":"yes|no"},
                {"type":"top_k","value":{"top_k":20}}
            ],
            "sample_step": {"type":"dist"}
        }"#;
        let err = serde_json::from_str::<SamplerConfig>(constraint_in_steps)
            .expect_err("a constraint inside `steps` should be rejected");
        assert!(
            err.to_string().contains("unknown variant `regex`"),
            "the error should name the misplaced step, got: {err}"
        );

        let moved = r#"{
            "constraining_steps": [{"type":"regex","value":"yes|no"}],
            "steps": [{"type":"top_k","value":{"top_k":20}}],
            "sample_step": {"type":"dist"}
        }"#;
        let cfg: SamplerConfig =
            serde_json::from_str(moved).expect("the same config, with the constraint moved");
        assert_eq!(cfg.constraining_steps.len(), 1);
        assert_eq!(cfg.steps.len(), 1);
    }
}
