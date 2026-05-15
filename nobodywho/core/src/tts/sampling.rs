//! Token sampling for Chatterbox-family TTS language models.
//!
//! Pipeline order: temperature → repetition penalty (applied by caller) →
//! top-k → min-p → top-p → multinomial, matching HF's logit warpers.

use std::cmp::Ordering;

/// Return the index of the largest element, treating NaNs as smaller than
/// everything else. Returns `0` on an empty slice.
pub(super) fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Divide every finite logit by `penalty`, raising it if negative. If `dedup`
/// is true, each token id in `generated` contributes at most once.
pub(super) fn apply_repetition_penalty(
    logits: &mut [f32],
    generated: &[i64],
    penalty: f32,
    dedup: bool,
) {
    if dedup {
        let mut seen = vec![false; logits.len()];
        for &token_id in generated {
            let idx = token_id as usize;
            if idx >= logits.len() || seen[idx] {
                continue;
            }
            seen[idx] = true;
            penalize(&mut logits[idx], penalty);
        }
    } else {
        for &token_id in generated {
            if let Some(score) = logits.get_mut(token_id as usize) {
                penalize(score, penalty);
            }
        }
    }
}

fn penalize(score: &mut f32, penalty: f32) {
    if *score < 0.0 {
        *score *= penalty;
    } else {
        *score /= penalty;
    }
}

/// Stable softmax. NaN/−∞ entries contribute zero probability; if no logit is
/// finite the result puts all mass on index 0.
pub(super) fn softmax(logits: &[f32]) -> Vec<f32> {
    let max_logit = logits
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f32::NEG_INFINITY, f32::max);

    if !max_logit.is_finite() {
        let mut probs = vec![0.0; logits.len()];
        if let Some(first) = probs.first_mut() {
            *first = 1.0;
        }
        return probs;
    }

    let mut probs: Vec<f32> = logits
        .iter()
        .map(|&v| {
            if v.is_finite() {
                (v - max_logit).exp()
            } else {
                0.0
            }
        })
        .collect();
    let sum: f32 = probs.iter().sum();
    if sum > 0.0 {
        for prob in &mut probs {
            *prob /= sum;
        }
    } else if let Some(first) = probs.first_mut() {
        *first = 1.0;
    }
    probs
}

/// Keep only the top `k` logits (by value), masking the rest to −∞. `k == 0` is a no-op.
pub(super) fn apply_top_k(logits: &mut [f32], k: usize) {
    if k == 0 || k >= logits.len() {
        return;
    }
    let mut sorted: Vec<(usize, f32)> = logits.iter().copied().enumerate().collect();
    sorted.sort_unstable_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(Ordering::Equal));
    for &(idx, _) in &sorted[k..] {
        logits[idx] = f32::NEG_INFINITY;
    }
}

/// Mask candidates below `min_p × top_prob`. Always preserves at least the top-1 candidate.
pub(super) fn apply_min_p(logits: &mut [f32], min_p: f32) {
    if min_p <= 0.0 {
        return;
    }

    let probs = softmax(logits);
    let top_prob = probs
        .iter()
        .copied()
        .fold(0.0f32, |acc, p| if p > acc { p } else { acc });
    let threshold = min_p * top_prob;
    let mut drop: Vec<bool> = probs.iter().map(|&p| p < threshold).collect();

    let top_idx = argmax(logits);
    if let Some(flag) = drop.get_mut(top_idx) {
        *flag = false;
    }

    for (idx, score) in logits.iter_mut().enumerate() {
        if drop[idx] {
            *score = f32::NEG_INFINITY;
        }
    }
}

/// Nucleus filter: mask the lowest-probability tail below `1 − top_p`. `top_p >= 1.0` is a no-op.
pub(super) fn apply_top_p(logits: &mut [f32], top_p: f32) {
    if top_p >= 1.0 {
        return;
    }

    let mut sorted: Vec<(usize, f32)> = logits.iter().copied().enumerate().collect();
    sorted.sort_unstable_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    let sorted_logits: Vec<f32> = sorted.iter().map(|(_, s)| *s).collect();
    let sorted_probs = softmax(&sorted_logits);
    let cutoff = 1.0 - top_p;
    let mut cumulative = 0.0f32;

    for (i, (orig_idx, _)) in sorted.iter().enumerate() {
        cumulative += sorted_probs[i];
        if cumulative <= cutoff && i + 1 < sorted.len() {
            logits[*orig_idx] = f32::NEG_INFINITY;
        }
    }
}

fn sample_multinomial(probs: &[f32]) -> usize {
    let mut r = rand::random::<f64>();
    for (idx, &p) in probs.iter().enumerate() {
        r -= p as f64;
        if r <= 0.0 {
            return idx;
        }
    }
    probs
        .iter()
        .enumerate()
        .rfind(|(_, p)| **p > 0.0)
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

/// Apply the full warper chain and draw one token. `logits` is mutated in place.
/// Short-circuits to [`argmax`] when `temperature <= 1e-6`.
pub(super) fn sample_token(logits: &mut [f32], params: &TtsSampling) -> usize {
    if params.temperature <= 1e-6 {
        return argmax(logits);
    }

    if params.temperature != 1.0 {
        for v in logits.iter_mut() {
            *v /= params.temperature;
        }
    }

    apply_top_k(logits, params.top_k);
    apply_min_p(logits, params.min_p);
    apply_top_p(logits, params.top_p);

    let probs = softmax(logits);
    sample_multinomial(&probs)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn params() -> TtsSampling {
        TtsSampling {
            temperature: 1.0,
            top_k: 0,
            top_p: 1.0,
            min_p: 0.0,
            cfg_weight: 0.0,
            repetition_penalty: 1.0,
        }
    }

    #[test]
    fn argmax_basic() {
        assert_eq!(argmax(&[0.1, 0.9, 0.3]), 1);
        assert_eq!(argmax(&[f32::NEG_INFINITY, 0.0]), 1);
        assert_eq!(argmax(&[]), 0);
    }

    #[test]
    fn repetition_penalty_dedup() {
        let mut logits = vec![1.0f32; 5];
        apply_repetition_penalty(&mut logits, &[0, 0, 0], 2.0, true);
        assert_eq!(logits[0], 0.5, "token 0 penalized once under dedup");
        assert_eq!(logits[1], 1.0);

        let mut logits = vec![1.0f32; 5];
        apply_repetition_penalty(&mut logits, &[0, 0, 0], 2.0, false);
        assert_eq!(
            logits[0], 0.125,
            "token 0 penalized three times without dedup"
        );
    }

    #[test]
    fn repetition_penalty_handles_negative_logits() {
        let mut logits = vec![-2.0_f32, 2.0];
        apply_repetition_penalty(&mut logits, &[0, 1], 2.0, true);
        assert_eq!(logits[0], -4.0);
        assert_eq!(logits[1], 1.0);
    }

    #[test]
    fn top_k_keeps_only_k() {
        let mut logits = vec![0.1, 0.2, 0.3, 0.4, 0.5];
        apply_top_k(&mut logits, 2);
        assert_eq!(logits[0], f32::NEG_INFINITY);
        assert_eq!(logits[1], f32::NEG_INFINITY);
        assert_eq!(logits[2], f32::NEG_INFINITY);
        assert_eq!(logits[3], 0.4);
        assert_eq!(logits[4], 0.5);
    }

    #[test]
    fn top_k_zero_is_noop() {
        let mut logits = vec![0.1, 0.2, 0.3];
        apply_top_k(&mut logits, 0);
        assert_eq!(logits, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn min_p_keeps_at_least_top_one() {
        let mut logits = vec![10.0, 0.0, -1.0];
        apply_min_p(&mut logits, 0.9);
        assert_eq!(logits[0], 10.0);
        assert_eq!(logits[1], f32::NEG_INFINITY);
        assert_eq!(logits[2], f32::NEG_INFINITY);
    }

    #[test]
    fn top_p_one_is_noop() {
        let mut logits = vec![0.1, 0.2, 0.3];
        apply_top_p(&mut logits, 1.0);
        assert_eq!(logits, vec![0.1, 0.2, 0.3]);
    }

    #[test]
    fn sample_token_greedy_when_temperature_zero() {
        let mut logits = vec![0.1, 0.9, 0.3];
        let p = TtsSampling {
            temperature: 0.0,
            ..params()
        };
        assert_eq!(sample_token(&mut logits, &p), 1);
    }

    #[test]
    fn softmax_handles_neg_infinity() {
        let probs = softmax(&[f32::NEG_INFINITY, 0.0, f32::NEG_INFINITY]);
        assert!((probs[1] - 1.0).abs() < 1e-6);
        assert_eq!(probs[0], 0.0);
        assert_eq!(probs[2], 0.0);
    }

    #[test]
    fn softmax_all_masked_falls_back_to_index_zero() {
        let probs = softmax(&[f32::NEG_INFINITY, f32::NEG_INFINITY]);
        assert_eq!(probs[0], 1.0);
        assert_eq!(probs[1], 0.0);
    }
}

/// Sampling parameters for the autoregressive token loop in
/// [`ChatterboxConfig`][crate::tts::ChatterboxConfig] and
/// [`RoestConfig`][crate::tts::RoestConfig]. Has no effect on Kokoro / Piper.
#[derive(Clone, Debug)]
pub struct TtsSampling {
    pub temperature: f32,
    pub top_k: usize,
    pub top_p: f32,
    pub min_p: f32,
    pub cfg_weight: f32,
    pub repetition_penalty: f32,
}

impl Default for TtsSampling {
    fn default() -> Self {
        Self {
            temperature: 0.8,
            top_k: 0,
            top_p: 1.0,
            min_p: 0.05,
            cfg_weight: 0.5,
            repetition_penalty: 2.0,
        }
    }
}

impl TtsSampling {
    /// Deterministic argmax; disables all filters and CFG.
    pub fn greedy() -> Self {
        Self {
            temperature: 0.0,
            top_k: 0,
            top_p: 1.0,
            min_p: 0.0,
            cfg_weight: 0.0,
            repetition_penalty: 1.0,
        }
    }

    /// Temperature-only sampling; disables top-k, top-p, min-p, and CFG.
    pub fn temperature(temperature: f32) -> Self {
        Self {
            temperature,
            top_k: 0,
            top_p: 1.0,
            min_p: 0.0,
            cfg_weight: 0.0,
            repetition_penalty: 1.0,
        }
    }

    /// Top-k sampling with temperature 1.0; disables top-p, min-p, and CFG.
    pub fn top_k(k: usize) -> Self {
        Self {
            temperature: 1.0,
            top_k: k,
            top_p: 1.0,
            min_p: 0.0,
            cfg_weight: 0.0,
            repetition_penalty: 1.0,
        }
    }
}
