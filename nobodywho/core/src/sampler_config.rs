use llama_cpp_2::model::LlamaModel;
use llama_cpp_2::sampling::LlamaSampler;

#[derive(Clone, Debug)]
pub struct SamplerConfig {
    pub method: SamplerMethod,
    pub penalty_last_n: i32,
    pub penalty_repeat: f32,
    pub penalty_freq: f32,
    pub penalty_present: f32,
    pub use_grammar: bool,
    pub gbnf_grammar: String,
}

pub const JSON_GRAMMAR: &str = r#"# this default gbnf grammar forces valid json output
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

impl Default for SamplerConfig {
    fn default() -> Self {
        Self {
            penalty_last_n: -1,
            penalty_repeat: 0.0,
            penalty_freq: 0.0,
            penalty_present: 0.0,
            use_grammar: false,
            gbnf_grammar: JSON_GRAMMAR.into(),
            method: SamplerMethod::MirostatV2(MirostatV2 {
                seed: 1234,
                temperature: 0.8,
                tau: 5.0,
                eta: 0.1,
            }),
        }
    }
}

/// ----- Sampler Methods -----

#[derive(Clone, Debug)]
pub enum SamplerMethod {
    Greedy(Greedy),
    DRY(DRY),
    TopK(TopK),
    TopP(TopP),
    MinP(MinP),
    XTC(XTC),
    TypicalP(TypicalP),
    Temperature(Temperature),
    MirostatV1(MirostatV1),
    MirostatV2(MirostatV2),
}

#[derive(Clone, Debug)]
pub struct Greedy {}

impl Default for Greedy {
    fn default() -> Self {
        Self {}
    }
}

#[derive(Clone, Debug)]
pub struct DRY {
    pub seed: u32,
    pub dry_multiplier: f32,
    pub dry_base: f32,
    pub dry_allowed_length: i32,
    pub dry_penalty_last_n: i32,
}

impl Default for DRY {
    fn default() -> Self {
        Self {
            seed: 1234,
            dry_multiplier: 0.0,
            dry_base: 1.75,
            dry_allowed_length: 2,
            dry_penalty_last_n: -1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TopK {
    pub top_k: i32,
    pub seed: u32,
}

impl Default for TopK {
    fn default() -> Self {
        Self {
            top_k: 40,
            seed: 1234,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TopP {
    pub seed: u32,
    pub min_keep: u32,
    pub top_p: f32,
}

impl Default for TopP {
    fn default() -> Self {
        Self {
            seed: 1234,
            top_p: 0.95,
            min_keep: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MinP {
    pub seed: u32,
    pub min_keep: u32,
    pub min_p: f32,
}

impl Default for MinP {
    fn default() -> Self {
        Self {
            seed: 1234,
            min_p: 0.05,
            min_keep: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct XTC {
    pub seed: u32,
    pub xtc_probability: f32,
    pub xtc_threshold: f32,
    pub min_keep: u32,
}

impl Default for XTC {
    fn default() -> Self {
        Self {
            xtc_probability: 0.00,
            xtc_threshold: 0.10,
            min_keep: 0,
            seed: 1234,
        }
    }
}

#[derive(Clone, Debug)]
pub struct TypicalP {
    pub seed: u32,
    pub typ_p: f32,
    pub min_keep: u32,
}

impl Default for TypicalP {
    fn default() -> Self {
        Self {
            seed: 1234,
            typ_p: 1.0,
            min_keep: 0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Temperature {
    pub seed: u32,
    pub temperature: f32,
}

impl Default for Temperature {
    fn default() -> Self {
        Self {
            seed: 1234,
            temperature: 0.8,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MirostatV1 {
    pub seed: u32,
    pub temperature: f32,
    pub tau: f32,
    pub eta: f32,
}

impl Default for MirostatV1 {
    fn default() -> Self {
        Self {
            seed: 1234,
            temperature: 0.8,
            tau: 5.0,
            eta: 0.1,
        }
    }
}

#[derive(Clone, Debug)]
pub struct MirostatV2 {
    pub seed: u32,
    pub temperature: f32,
    pub tau: f32,
    pub eta: f32,
}

impl Default for MirostatV2 {
    fn default() -> Self {
        Self {
            seed: 1234,
            temperature: 0.8,
            tau: 5.0,
            eta: 0.1,
        }
    }
}

pub fn make_sampler(model: &LlamaModel, sampler_config: SamplerConfig) -> LlamaSampler {
    let mut chainvec = Vec::new();

    // Add grammar sampler first if configured
    if sampler_config.use_grammar {
        chainvec.push(LlamaSampler::grammar(
            model,
            &sampler_config.gbnf_grammar,
            "root",
        ));
    }

    // Add penalties
    chainvec.push(LlamaSampler::penalties(
        sampler_config.penalty_last_n,
        sampler_config.penalty_repeat,
        sampler_config.penalty_freq,
        sampler_config.penalty_present,
    ));

    // Add method-specific samplers
    match sampler_config.method {
        SamplerMethod::Greedy(_) => {
            chainvec.push(LlamaSampler::greedy());
        }
        SamplerMethod::DRY(conf) => {
            chainvec.push(LlamaSampler::dry(
                model,
                conf.dry_multiplier,
                conf.dry_base,
                conf.dry_allowed_length,
                conf.dry_penalty_last_n,
                vec!["\n", ":", "\"", "*"],
            ));
            chainvec.push(LlamaSampler::dist(conf.seed));
        }
        SamplerMethod::TopK(conf) => {
            chainvec.push(LlamaSampler::top_k(conf.top_k));
            chainvec.push(LlamaSampler::dist(conf.seed));
        }
        SamplerMethod::TopP(conf) => {
            chainvec.push(LlamaSampler::top_p(conf.top_p, conf.min_keep as usize));
            chainvec.push(LlamaSampler::dist(conf.seed));
        }
        SamplerMethod::MinP(conf) => {
            chainvec.push(LlamaSampler::min_p(conf.min_p, conf.min_keep as usize));
            chainvec.push(LlamaSampler::dist(conf.seed));
        }
        SamplerMethod::XTC(conf) => {
            chainvec.push(LlamaSampler::xtc(
                conf.xtc_probability,
                conf.xtc_threshold,
                conf.min_keep as usize,
                conf.seed,
            ));
            chainvec.push(LlamaSampler::dist(conf.seed));
        }
        SamplerMethod::TypicalP(conf) => {
            chainvec.push(LlamaSampler::typical(conf.typ_p, conf.min_keep as usize));
            chainvec.push(LlamaSampler::dist(conf.seed));
        }
        SamplerMethod::Temperature(conf) => {
            chainvec.push(LlamaSampler::temp(conf.temperature));
            chainvec.push(LlamaSampler::dist(conf.seed));
        }
        SamplerMethod::MirostatV1(conf) => {
            chainvec.push(LlamaSampler::temp(conf.temperature));
            chainvec.push(LlamaSampler::mirostat(
                model.n_vocab(),
                conf.seed,
                conf.tau,
                conf.eta,
                100,
            ));
        }
        SamplerMethod::MirostatV2(conf) => {
            chainvec.push(LlamaSampler::temp(conf.temperature));
            chainvec.push(LlamaSampler::mirostat_v2(conf.seed, conf.tau, conf.eta));
        }
    }

    LlamaSampler::chain(chainvec, true)
}
