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
pub struct SamplerPresets;

impl SamplerPresets {
    pub fn top_k(k: i32) -> SamplerConfig {
        SamplerConfig::new(
            vec![ShiftStep::TopK { top_k: k }],
            SampleStep::Dist,
            default_seed(),
        )
    }

    pub fn top_p(p: f32) -> SamplerConfig {
        SamplerConfig::new(
            vec![ShiftStep::TopP {
                min_keep: 0,
                top_p: p,
            }],
            SampleStep::Dist,
            default_seed(),
        )
    }

    pub fn greedy() -> SamplerConfig {
        SamplerConfig::new(vec![], SampleStep::Greedy, default_seed())
    }

    pub fn temperature(temperature: f32) -> SamplerConfig {
        SamplerConfig::new(
            vec![ShiftStep::Temperature { temperature }],
            SampleStep::Dist,
            default_seed(),
        )
    }

    pub fn dry() -> SamplerConfig {
        SamplerConfig::new(
            vec![ShiftStep::DRY {
                multiplier: 0.0,
                base: 1.75,
                allowed_length: 2,
                penalty_last_n: -1,
                seq_breakers: vec![
                    "\n".to_string(),
                    ":".to_string(),
                    "\"".to_string(),
                    "*".to_string(),
                ],
            }],
            SampleStep::Dist,
            default_seed(),
        )
    }

    /// Constrain output to a JSON schema using llguidance.
    pub fn constrain_with_json_schema(schema: String) -> SamplerConfig {
        SamplerConfig::new(
            vec![ShiftStep::JsonSchema(schema)],
            SampleStep::Dist,
            default_seed(),
        )
    }

    /// Constrain output to a regular expression using llguidance.
    pub fn constrain_with_regex(pattern: String) -> SamplerConfig {
        SamplerConfig::new(
            vec![ShiftStep::Regex(pattern)],
            SampleStep::Dist,
            default_seed(),
        )
    }

    /// Constrain output using a Lark context-free grammar via llguidance.
    pub fn constrain_with_grammar(lark: String) -> SamplerConfig {
        SamplerConfig::new(
            vec![ShiftStep::Lark(lark)],
            SampleStep::Dist,
            default_seed(),
        )
    }

    pub fn json() -> SamplerConfig {
        // the grammar must run before the truncation samplers: if top-k
        // runs first and none of the surviving candidates is grammar-valid,
        // the grammar masks out every token and generation aborts
        let mut steps = vec![ShiftStep::Grammar {
            trigger_on: None,
            root: "root".into(),
            grammar: JSON_GRAMMAR.into(),
        }];
        steps.extend(SamplerConfig::default().steps);
        SamplerConfig::new(steps, SampleStep::Dist, default_seed())
    }

    #[deprecated(note = "Use SamplerPresets::constrain_with_grammar() instead")]
    pub fn grammar(grammar: String) -> SamplerConfig {
        let mut steps = vec![ShiftStep::Grammar {
            trigger_on: None,
            root: "root".into(),
            grammar,
        }];
        steps.extend(SamplerConfig::default().steps);
        SamplerConfig::new(steps, SampleStep::Dist, default_seed())
    }
}

/// Sampler configuration struct.
///
/// Carries a single `seed` that is consumed by every random sampler in the
/// chain (`SampleStep::Dist`, `MirostatV1`, `MirostatV2`, and `ShiftStep::XTC`).
/// `SampleStep::Greedy` ignores it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplerConfig {
    pub steps: Vec<ShiftStep>,
    pub sample_step: SampleStep,
    #[serde(default = "default_seed")]
    pub seed: u32,
}

pub fn default_seed() -> u32 {
    1234
}

impl SamplerConfig {
    pub fn new(shift_steps: Vec<ShiftStep>, sample_step: SampleStep, seed: u32) -> Self {
        Self {
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
        // Grammar step goes first, so it constrains before anything else runs.
        let mut shift_steps = extra_step
            .into_iter()
            .map(Ok)
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
            ShiftStep::Grammar {
                grammar,
                trigger_on,
                root,
            } => match trigger_on {
                Some(trigger) => self.build_lazy_grammar(model, &grammar, &root, &trigger),
                None => self.build_regular_grammar(model, &grammar, &root),
            },
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
            ShiftStep::JsonSchema(schema) => llguidance_sampler(model, "json_schema", &schema, &[]),
            ShiftStep::Regex(pattern) => llguidance_sampler(model, "regex", &pattern, &[]),
            ShiftStep::Lark(lark) => {
                let lark = gbnf::gbnf_to_lark::any_to_lark(&lark)
                    .map_err(|e| SamplerError::GbnfConversionError(e.to_string()))?;
                llguidance_sampler(model, "lark", &lark, &[])
            }
            // Reachable only via serde or direct construction: the tool path used
            // to be the sole caller and now builds its own step. NOB-140 is to
            // give it a `SamplerPresets` constructor like its siblings above.
            ShiftStep::LarkWithSlices(lark, slices) => {
                llguidance_sampler(model, "lark", &lark, &slices)
            }
        }
    }

    fn build_lazy_grammar(
        &self,
        model: &LlamaModel,
        grammar: &str,
        root: &str,
        trigger: &str,
    ) -> Result<LlamaSampler, SamplerError> {
        let token_result = model
            .str_to_token(trigger, llama_cpp_2::model::AddBos::Never)
            .map(|v| v.first().copied());

        let token = match token_result {
            Ok(Some(token)) => token,
            _ => {
                return Err(SamplerError::UnsupportedToolCallingTokenization);
            }
        };

        Ok(LlamaSampler::grammar_lazy(
            model,
            grammar,
            root,
            Vec::<&str>::new(),
            &[token],
        )?)
    }

    fn build_regular_grammar(
        &self,
        model: &LlamaModel,
        grammar: &str,
        root: &str,
    ) -> Result<LlamaSampler, SamplerError> {
        Ok(LlamaSampler::grammar(model, grammar, root)?)
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
            vec![
                ShiftStep::TopK { top_k: 20 },
                ShiftStep::TopP {
                    top_p: 0.95,
                    min_keep: 1,
                },
                ShiftStep::Temperature { temperature: 0.6 },
            ],
            SampleStep::Dist,
            default_seed(),
        )
    }
}

#[derive(Clone)]
pub struct SamplerBuilder {
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
            steps: vec![],
            seed: default_seed(),
        }
    }

    /// Appends a shift step to the end of the sampler chain.
    pub fn shift(mut self, step: ShiftStep) -> Self {
        self.steps.push(step);
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
            steps: self.steps,
            sample_step: step,
            seed: self.seed,
        }
    }
}

const JSON_GRAMMAR: &str = r#"# this default gbnf grammar forces valid json output
root   ::= object
value  ::= object | array | string | number | ("true" | "false" | "null") ws

object ::=
"{" ws (
            string ":" ws value
    ("," ws string ":" ws value)*
)? "}" ws

array  ::=
"[" ws (
            value
    ("," ws value)*
)? "]" ws

string ::=
"\"" (
    [^"\\\x7F\x00-\x1F] |
    "\\" (["\\bfnrt] | "u" [0-9a-fA-F]{4}) # escapes
)* "\"" ws

number ::= ("-"? ([0-9] | [1-9] [0-9]{0,15})) ("." [0-9]+)? ([eE] [-+]? [0-9] [1-9]{0,15})? ws

# Optional space: by convention, applied in this grammar after literal chars when allowed
ws ::= | " " | "\n" [ \t]{0,20}"#;

/// ----- Sampler Methods -----

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    /// Deprecated: use [`SamplerPresets::constrain_with_grammar`] instead. It accepts both Lark and GBNF strings.
    Grammar {
        trigger_on: Option<String>,
        root: String,
        grammar: String,
    },
    /// Constrain output to a JSON schema via llguidance.
    JsonSchema(String),
    /// Constrain output to a regular expression via llguidance.
    Regex(String),
    /// Constrain output using a Lark context-free grammar via llguidance.
    Lark(String),
    /// Like [`Lark`][ShiftStep::Lark] but with custom slice regexes passed to the `ParserFactory`.
    /// See [`llguidance_sampler`] for how slices speed up per-token constraint evaluation.
    LarkWithSlices(String, Vec<String>),
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
        steps,
        sample_step.unwrap_or(SampleStep::Dist),
        default_seed(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

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
            vec![
                ShiftStep::Grammar {
                    trigger_on: None,
                    root: "root".into(),
                    grammar: "root ::= \"zqxjvkw\"".into(),
                },
                ShiftStep::TopK { top_k: 1 },
            ],
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
        serde_json::from_str::<serde_json::Value>(&response)
            .unwrap_or_else(|e| panic!("response is not valid JSON ({e}): {response}"));
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
}
