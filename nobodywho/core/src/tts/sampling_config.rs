use crate::tts::sampling::SamplingParams;

#[derive(Clone, Debug)]
pub struct TtsSampling {
    pub temperature: f32,
    pub top_k: usize,
    pub top_p: f32,
    pub min_p: f32,
    pub cfg_weight: f32,
}

impl Default for TtsSampling {
    fn default() -> Self {
        Self {
            temperature: 0.8,
            top_k: 0,
            top_p: 1.0,
            min_p: 0.05,
            cfg_weight: 0.5,
        }
    }
}

impl From<&TtsSampling> for SamplingParams {
    fn from(sampling: &TtsSampling) -> Self {
        Self {
            temperature: sampling.temperature,
            top_k: sampling.top_k,
            top_p: sampling.top_p,
            min_p: sampling.min_p,
            cfg_weight: sampling.cfg_weight,
        }
    }
}
