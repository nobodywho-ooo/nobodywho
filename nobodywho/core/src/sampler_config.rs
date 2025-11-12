use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;
use tracing::error;

/// Some simple presets, that can be useful for basic sampling.
pub struct SamplerPresets;

impl SamplerPresets {
    pub fn default(model: &LlamaModel) -> LlamaSampler {
        SamplerConfig::new(model)
            .shift(ShiftStep::Temperature { temperature: 0.8 })
            .sample(SampleStep::MirostatV2 { tau: 5.0, eta: 0.1 })
    }

    pub fn top_k(model: &LlamaModel, k: i32) -> LlamaSampler {
        SamplerConfig::new(model)
            .constrain(ConstrainStep::TopK { top_k: k })
            .sample(SampleStep::Dist)
    }

    pub fn top_p(model: &LlamaModel, p: f32) -> LlamaSampler {
        SamplerConfig::new(model)
            .constrain(ConstrainStep::TopP {
                min_keep: 0,
                top_p: p,
            })
            .sample(SampleStep::Dist)
    }

    pub fn greedy(model: &LlamaModel) -> LlamaSampler {
        SamplerConfig::new(model).sample(SampleStep::Greedy)
    }

    pub fn temperature(model: &LlamaModel, temperature: f32) -> LlamaSampler {
        SamplerConfig::new(model)
            .shift(ShiftStep::Temperature { temperature })
            .sample(SampleStep::Dist)
    }

    pub fn dry(model: &LlamaModel) -> LlamaSampler {
        SamplerConfig::new(model)
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

    pub fn json(model: &LlamaModel) -> LlamaSampler {
        SamplerConfig::new(model)
            .constrain(ConstrainStep::Grammar {
                trigger_on: None,
                root: "root".into(),
                grammar: JSON_GRAMMAR,
            })
            .sample(SampleStep::MirostatV2 { tau: 5.0, eta: 0.1 })
    }
}

/// Underlying sampler configuration API, with much more control and details.
#[derive(Debug)]
pub struct SamplerConfig<'a> {
    steps: Vec<LlamaSampler>,
    seed: u32,
    model: &'a LlamaModel,
}

impl<'a> SamplerConfig<'a> {
    pub fn new(model: &'a LlamaModel) -> Self {
        return Self {
            steps: vec![],
            seed: 1234,
            model,
        };
    }

    pub fn constrain(mut self, step: ConstrainStep) -> Self {
        match step {
            ConstrainStep::TopK { top_k } => self.steps.push(LlamaSampler::top_k(top_k)),
            ConstrainStep::TopP { min_keep, top_p } => self
                .steps
                .push(LlamaSampler::top_p(top_p, min_keep as usize)),
            ConstrainStep::XTC {
                xtc_probability,
                xtc_threshold,
                min_keep,
            } => self.steps.push(LlamaSampler::xtc(
                xtc_probability,
                xtc_threshold,
                min_keep as usize,
                self.seed,
            )),
            ConstrainStep::TypicalP { typ_p, min_keep } => self
                .steps
                .push(LlamaSampler::typical(typ_p, min_keep as usize)),
            ConstrainStep::MinP { min_keep, min_p } => self
                .steps
                .push(LlamaSampler::min_p(min_p, min_keep as usize)),
            ConstrainStep::Grammar {
                grammar,
                trigger_on,
                root,
            } => match trigger_on {
                Some(trigger) => self.add_lazy_grammar(&grammar, &root, trigger),
                None => self.add_regular_grammar(&grammar, &root),
            },
        };

        self
    }

    pub fn shift(mut self, step: ShiftStep) -> Self {
        match step {
            ShiftStep::DRY {
                multiplier,
                base,
                allowed_length,
                penalty_last_n,
                seq_breakers,
            } => self.steps.push(LlamaSampler::dry(
                self.model,
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
            } => self.steps.push(LlamaSampler::penalties(
                penalty_last_n,
                penalty_repeat,
                penalty_freq,
                penalty_present,
            )),
            ShiftStep::Temperature { temperature } => {
                self.steps.push(LlamaSampler::temp(temperature))
            }
        }

        self
    }

    pub fn sample(mut self, step: SampleStep) -> LlamaSampler {
        match step {
            SampleStep::Dist => self.steps.push(LlamaSampler::dist(self.seed)),
            SampleStep::Greedy => self.steps.push(LlamaSampler::greedy()),
            SampleStep::MirostatV1 { tau, eta, m } => self.steps.push(LlamaSampler::mirostat(
                self.model.n_vocab(),
                self.seed,
                tau,
                eta,
                m,
            )),
            SampleStep::MirostatV2 { tau, eta } => self
                .steps
                .push(LlamaSampler::mirostat_v2(self.seed, tau, eta)),
        };

        LlamaSampler::chain(self.steps, true)
    }

    fn add_lazy_grammar(&mut self, grammar: &str, root: &str, trigger: &str) {
        let token_result = self
            .model
            .str_to_token(trigger, llama_cpp_2::model::AddBos::Never)
            .map(|v| v.first().copied());

        let token = match token_result {
            Ok(Some(token)) => token,
            _ => {
                error!("Lazy GBNF grammar was specified, but the trigger token does not cleanly tokenize with the given model. You most likely tried to do tool calling with a model that doesn't natively support tool calling.");
                return;
            }
        };

        match LlamaSampler::grammar_lazy(self.model, grammar, root, vec![trigger], &[token]) {
            Some(g) => self.steps.push(g),
            None => error!("Failed to create lazy grammar sampler"),
        }
    }

    fn add_regular_grammar(&mut self, grammar: &str, root: &str) {
        match LlamaSampler::grammar(self.model, grammar, root) {
            Some(g) => self.steps.push(g),
            None => error!("Failed to create grammar sampler"),
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
pub enum ConstrainStep<'a> {
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
        trigger_on: Option<&'a str>,
        root: String,
        grammar: &'a str,
    },
}
#[derive(Clone, Debug)]
pub enum ShiftStep {
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
enum SampleStep {
    Dist,
    Greedy,
    MirostatV1 { tau: f32, eta: f32, m: i32 },
    MirostatV2 { tau: f32, eta: f32 },
}
