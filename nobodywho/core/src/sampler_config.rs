use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;

use crate::errors::SamplerError;

/// Some simple presets, that can be useful for basic sampling.
pub struct SamplerPresets;

impl SamplerPresets {
    pub fn default() -> SamplerConfig {
        SamplerConfig::new()
            .temperature(0.8)
            .sample(SampleStep::MirostatV2 { tau: 5.0, eta: 0.1 })
    }

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
            .temperature(temperature)
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
        SamplerConfig::new()
            .shift(ShiftStep::Grammar {
                trigger_on: None,
                root: "root".into(),
                grammar: JSON_GRAMMAR.into(),
            })
            .sample(SampleStep::MirostatV2 { tau: 5.0, eta: 0.1 })
    }
}

/// Underlying sampler configuration API, with much more control and details.
#[derive(Debug, Clone)]
pub struct SamplerConfig {
    steps: Vec<ShiftStep>,
    sample_step: Option<SampleStep>,
    seed: u32,
}

impl SamplerConfig {
    pub fn new() -> Self {
        return Self {
            steps: vec![],
            seed: 1234,
            sample_step: None,
        };
    }

    pub fn shift(mut self, step: ShiftStep) -> Self {
        self.steps.push(step);
        self
    }

    pub fn dry(
        mut self,
        multiplier: f32,
        base: f32,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: Vec<String>,
    ) -> Self {
        self.steps.push(ShiftStep::DRY {
            multiplier,
            base,
            allowed_length,
            penalty_last_n,
            seq_breakers,
        });
        self
    }

    pub fn penalties(
        mut self,
        penalty_last_n: i32,
        penalty_repeat: f32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> Self {
        self.steps.push(ShiftStep::Penalties {
            penalty_last_n,
            penalty_repeat,
            penalty_freq,
            penalty_present,
        });
        self
    }

    pub fn temperature(mut self, temperature: f32) -> Self {
        self.steps.push(ShiftStep::Temperature { temperature });
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

        match LlamaSampler::grammar_lazy(model, grammar, root, vec![trigger], &[token]) {
            Some(g) => Ok(g),
            None => Err(SamplerError::TriggerOrGrammarContainsNullBytes),
        }
    }

    fn build_regular_grammar(
        &self,
        model: &LlamaModel,
        grammar: &str,
        root: &str,
    ) -> Result<LlamaSampler, SamplerError> {
        match LlamaSampler::grammar(model, grammar, root) {
            Some(g) => Ok(g),
            None => Err(SamplerError::TriggerOrGrammarContainsNullBytes),
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

#[derive(Clone, Debug)]
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
#[derive(Clone, Debug)]
pub enum SampleStep {
    Dist,
    Greedy,
    MirostatV1 { tau: f32, eta: f32, m: i32 },
    MirostatV2 { tau: f32, eta: f32 },
}
