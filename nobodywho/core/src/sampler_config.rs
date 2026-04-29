use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use serde::{Deserialize, Serialize};
use tracing::warn;

use crate::errors::SamplerError;

/// Some simple presets, that can be useful for basic sampling.
pub struct SamplerPresets;

impl SamplerPresets {
    pub fn top_k(k: i32) -> SamplerConfig {
        SamplerConfig::new()
            .shift(ShiftStep::TopK { top_k: k })
            .sample(SampleStep::Dist)
    }

    pub fn top_p(p: f32) -> SamplerConfig {
        SamplerConfig::new()
            .shift(ShiftStep::TopP {
                min_keep: 0,
                top_p: p,
            })
            .sample(SampleStep::Dist)
    }

    pub fn greedy() -> SamplerConfig {
        SamplerConfig::new().sample(SampleStep::Greedy)
    }

    pub fn temperature(temperature: f32) -> SamplerConfig {
        SamplerConfig::new()
            .shift(ShiftStep::Temperature { temperature })
            .sample(SampleStep::Dist)
    }

    pub fn dry() -> SamplerConfig {
        SamplerConfig::new()
            .shift(ShiftStep::DRY {
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
            })
            .sample(SampleStep::Dist)
    }

    pub fn json() -> SamplerConfig {
        SamplerConfig::default().shift(ShiftStep::Grammar {
            trigger_on: None,
            root: "root".into(),
            grammar: JSON_GRAMMAR.into(),
        })
    }

    pub fn grammar(grammar: String) -> SamplerConfig {
        SamplerConfig::default().shift(ShiftStep::Grammar {
            trigger_on: None,
            root: "root".into(),
            grammar,
        })
    }
}

/// Underlying sampler configuration API, with much more control and details.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SamplerConfig {
    steps: Vec<ShiftStep>,
    sample_step: Option<SampleStep>,
    #[serde(skip, default = "default_seed")]
    seed: u32,
}

fn default_seed() -> u32 {
    1234
}

impl SamplerConfig {
    pub fn new() -> Self {
        Self {
            steps: vec![],
            seed: 1234,
            sample_step: None,
        }
    }

    /// Appends a shift step to the end of the sampler chain.
    pub fn shift(mut self, step: ShiftStep) -> Self {
        self.steps.push(step);
        self
    }

    /// Prepends a shift step to the beginning of the sampler chain.
    /// This ensures the step is applied before any other shift steps.
    pub fn prepend(mut self, step: ShiftStep) -> Self {
        self.steps.insert(0, step);
        self
    }

    pub fn sample(mut self, step: SampleStep) -> Self {
        self.sample_step = Some(step);
        self
    }

    pub fn to_stateful(&self, model: &LlamaModel) -> Result<LlamaSampler, SamplerError> {
        let sample_step = self
            .sample_step
            .clone()
            .ok_or(SamplerError::MissingSampleStep)?;

        let mut shift_steps = self
            .steps
            .iter()
            .map(|step| self.build_step(model, step.clone()))
            .collect::<Result<Vec<_>, SamplerError>>()?;

        let final_sampler = match sample_step {
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

impl Default for SamplerConfig {
    fn default() -> SamplerConfig {
        SamplerConfig::new()
            .shift(ShiftStep::TopK { top_k: 20 })
            .shift(ShiftStep::TopP {
                top_p: 0.95,
                min_keep: 1,
            })
            .shift(ShiftStep::Temperature { temperature: 0.6 })
            .sample(SampleStep::Dist)
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
    Grammar {
        trigger_on: Option<String>,
        root: String,
        grammar: String,
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

    let mut config = SamplerConfig::new();
    let mut has_sample_step = false;

    for name in &sampler_names {
        match *name {
            "temp" | "temperature" => {
                if let Some(t) = temp {
                    config = config.shift(ShiftStep::Temperature { temperature: t });
                }
            }
            "top_k" => {
                if let Some(k) = top_k {
                    config = config.shift(ShiftStep::TopK { top_k: k });
                }
            }
            "top_p" => {
                if let Some(p) = top_p {
                    config = config.shift(ShiftStep::TopP {
                        top_p: p,
                        min_keep: 1,
                    });
                }
            }
            "min_p" => {
                if let Some(p) = min_p {
                    config = config.shift(ShiftStep::MinP {
                        min_p: p,
                        min_keep: 1,
                    });
                }
            }
            "xtc" => {
                if let (Some(prob), Some(thresh)) = (xtc_probability, xtc_threshold) {
                    config = config.shift(ShiftStep::XTC {
                        xtc_probability: prob,
                        xtc_threshold: thresh,
                        min_keep: 1,
                    });
                }
            }
            "penalties" | "repeat_penalty" => {
                if penalty_last_n.is_some() || penalty_repeat.is_some() {
                    config = config.shift(ShiftStep::Penalties {
                        penalty_last_n: penalty_last_n.unwrap_or(64),
                        penalty_repeat: penalty_repeat.unwrap_or(1.0),
                        penalty_freq: penalty_freq.unwrap_or(0.0),
                        penalty_present: penalty_present.unwrap_or(0.0),
                    });
                }
            }
            "dist" => {
                config = config.sample(SampleStep::Dist);
                has_sample_step = true;
            }
            "greedy" => {
                config = config.sample(SampleStep::Greedy);
                has_sample_step = true;
            }
            "mirostat" => {
                if let Some(mode) = mirostat {
                    match mode {
                        1 => {
                            config = config.sample(SampleStep::MirostatV1 {
                                tau: mirostat_tau.unwrap_or(5.0),
                                eta: mirostat_eta.unwrap_or(0.1),
                                m: 100,
                            });
                            has_sample_step = true;
                        }
                        2 => {
                            config = config.sample(SampleStep::MirostatV2 {
                                tau: mirostat_tau.unwrap_or(5.0),
                                eta: mirostat_eta.unwrap_or(0.1),
                            });
                            has_sample_step = true;
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

    if !has_sample_step {
        config = config.sample(SampleStep::Dist);
    }

    Some(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shift_appends_to_end() {
        let config = SamplerConfig::new()
            .shift(ShiftStep::TopK { top_k: 40 })
            .shift(ShiftStep::Temperature { temperature: 0.8 });

        assert_eq!(config.steps.len(), 2);
        // Verify order: TopK first, Temperature second
        assert!(matches!(config.steps[0], ShiftStep::TopK { .. }));
        assert!(matches!(config.steps[1], ShiftStep::Temperature { .. }));
    }

    #[test]
    fn test_prepend_adds_to_beginning() {
        let config = SamplerConfig::new()
            .shift(ShiftStep::TopK { top_k: 40 })
            .prepend(ShiftStep::Temperature { temperature: 0.8 });

        assert_eq!(config.steps.len(), 2);
        // Verify order: Temperature first (prepended), TopK second
        assert!(matches!(config.steps[0], ShiftStep::Temperature { .. }));
        assert!(matches!(config.steps[1], ShiftStep::TopK { .. }));
    }

    #[test]
    fn test_serialize_deserialize_round_trip() {
        let config = SamplerConfig::default();
        let json = serde_json::to_string(&config).unwrap();
        let deserialized: SamplerConfig = serde_json::from_str(&json).unwrap();
        assert_eq!(format!("{:?}", config), format!("{:?}", deserialized));
    }

    #[test]
    fn test_grammar_prepend_with_custom_sampler() {
        let config = SamplerConfig::new()
            .shift(ShiftStep::TopK { top_k: 64 })
            .shift(ShiftStep::TopP {
                top_p: 0.95,
                min_keep: 2,
            })
            .shift(ShiftStep::Temperature { temperature: 0.8 })
            .prepend(ShiftStep::Grammar {
                trigger_on: Some("<tool_call>".into()),
                root: "superroot".into(),
                grammar: "...".into(),
            });

        assert_eq!(config.steps.len(), 4);
        // Verify grammar is at the beginning
        assert!(matches!(config.steps[0], ShiftStep::Grammar { .. }));
        // Verify custom sampler steps follow
        assert!(matches!(config.steps[1], ShiftStep::TopK { .. }));
        assert!(matches!(config.steps[2], ShiftStep::TopP { .. }));
        assert!(matches!(config.steps[3], ShiftStep::Temperature { .. }));
    }
}
