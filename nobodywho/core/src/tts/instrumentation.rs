//! Development-time debugging hooks for the Røst backend.
//!
//! None of this is used on the hot path in production — every function is
//! gated on an environment variable, so with no env vars set each helper is
//! a single branch and a return.
//!
//! | Env var                           | Effect                                               |
//! |-----------------------------------|------------------------------------------------------|
//! | `NOBODYWHO_TTS_DUMP_DIR`          | Directory for binary tensor dumps                    |
//! | `NOBODYWHO_TTS_DUMP_STEP`         | Decode step whose logits should be dumped            |
//! | `NOBODYWHO_TTS_SAMPLE_UNIFORMS`   | Comma-separated uniforms to replay during sampling   |
//! | `NOBODYWHO_TTS_FORCE_TOKENS`      | Comma-separated tokens to inject instead of sampling |
//! | `NOBODYWHO_TTS_DEBUG_TOKENS`      | Log the generated token sequence after synthesis     |

use crate::errors::TtsError;
use crate::tts::sampling::{DebugUniforms, UniformSource};
use std::path::Path;
use std::time::Duration;

/// Rolling timings for one synthesis call, logged at the end of generation.
#[derive(Default, Debug, Clone, Copy)]
pub(super) struct GenerationTimings {
    pub embed: Duration,
    pub lm: Duration,
    pub sample: Duration,
}

impl GenerationTimings {
    pub fn total(&self) -> Duration {
        self.embed + self.lm + self.sample
    }
}

/// Combines a uniform source (either system RNG or pinned uniforms) with an
/// optional list of per-step forced tokens. Picking forced tokens bypasses
/// `sample_token` entirely, which is useful when replaying the torch reference.
pub(super) struct DebugSampler {
    uniforms: DebugUniforms,
    use_system_rng: bool,
    forced_tokens: Vec<i64>,
}

impl DebugSampler {
    pub fn from_env() -> Result<Self, TtsError> {
        let uniforms_raw = std::env::var("NOBODYWHO_TTS_SAMPLE_UNIFORMS").ok();
        let (uniforms, use_system_rng) = match uniforms_raw {
            Some(raw) => (DebugUniforms::from_env_var(&raw)?, false),
            None => (DebugUniforms::new(Vec::new()), true),
        };

        let forced_tokens = std::env::var("NOBODYWHO_TTS_FORCE_TOKENS")
            .ok()
            .map(|s| {
                s.split(',')
                    .filter(|v| !v.trim().is_empty())
                    .map(|v| {
                        v.trim().parse::<i64>().map_err(|e| {
                            TtsError::Synthesis(format!(
                                "invalid NOBODYWHO_TTS_FORCE_TOKENS entry `{v}`: {e}"
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();

        Ok(Self {
            uniforms,
            use_system_rng,
            forced_tokens,
        })
    }

    pub fn forced_token(&self, step: usize) -> Option<i64> {
        self.forced_tokens.get(step).copied()
    }
}

impl UniformSource for DebugSampler {
    fn next_uniform(&mut self) -> f32 {
        if self.use_system_rng {
            rand::random()
        } else {
            self.uniforms.next_uniform()
        }
    }
}

/// Snapshot of the first decode step's tensors, written to disk when
/// `NOBODYWHO_TTS_DUMP_DIR` is set.
pub(super) struct FirstStepDump<'a> {
    pub inputs_embeds: &'a [f32],
    pub inputs_embeds_shape: [usize; 3],
    pub attention_mask: &'a [i64],
    pub attention_mask_shape: [usize; 2],
    pub logits: &'a [f32],
    pub logits_shape: [usize; 3],
    pub final_logits: &'a [f32],
}

/// Snapshot of an arbitrary decode step (selected by `NOBODYWHO_TTS_DUMP_STEP`).
pub(super) struct StepDump<'a> {
    pub step: usize,
    pub logits: &'a [f32],
    pub logits_shape: [usize; 3],
    pub processed_logits: &'a [f32],
    pub generated: &'a [i64],
}

pub(super) fn maybe_dump_first_step(dump: &FirstStepDump<'_>) -> Result<(), TtsError> {
    let Some(dir) = std::env::var_os("NOBODYWHO_TTS_DUMP_DIR") else {
        return Ok(());
    };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir)
        .map_err(|e| TtsError::Synthesis(format!("create dump dir {}: {e}", dir.display())))?;

    write_f32_bin(&dir.join("rust_inputs_embeds.bin"), dump.inputs_embeds)?;
    write_i64_bin(&dir.join("rust_attention_mask.bin"), dump.attention_mask)?;
    write_f32_bin(&dir.join("rust_logits.bin"), dump.logits)?;
    write_f32_bin(
        &dir.join("rust_final_logits_pre_penalty.bin"),
        dump.final_logits,
    )?;

    let manifest = serde_json::json!({
        "inputs_embeds_shape": dump.inputs_embeds_shape,
        "attention_mask_shape": dump.attention_mask_shape,
        "logits_shape": dump.logits_shape,
        "final_logits_shape": [dump.final_logits.len()],
    });
    std::fs::write(
        dir.join("rust_manifest.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|e| TtsError::Synthesis(format!("serialize dump manifest: {e}")))?,
    )
    .map_err(|e| TtsError::Synthesis(format!("write dump manifest: {e}")))
}

pub(super) fn maybe_dump_step(dump: &StepDump<'_>) -> Result<(), TtsError> {
    let Some(dir) = std::env::var_os("NOBODYWHO_TTS_DUMP_DIR") else {
        return Ok(());
    };
    let target_step = std::env::var("NOBODYWHO_TTS_DUMP_STEP")
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    if target_step != Some(dump.step) {
        return Ok(());
    }

    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir)
        .map_err(|e| TtsError::Synthesis(format!("create dump dir {}: {e}", dir.display())))?;

    write_f32_bin(&dir.join("rust_step_logits.bin"), dump.logits)?;
    write_f32_bin(
        &dir.join("rust_step_final_logits.bin"),
        dump.processed_logits,
    )?;
    write_i64_bin(&dir.join("rust_step_generated.bin"), dump.generated)?;

    let manifest = serde_json::json!({
        "step": dump.step,
        "logits_shape": dump.logits_shape,
        "final_logits_shape": [dump.processed_logits.len()],
        "generated_len": dump.generated.len(),
    });
    std::fs::write(
        dir.join("rust_step_manifest.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|e| TtsError::Synthesis(format!("serialize step manifest: {e}")))?,
    )
    .map_err(|e| TtsError::Synthesis(format!("write step manifest: {e}")))
}

fn write_f32_bin(path: &Path, data: &[f32]) -> Result<(), TtsError> {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for &value in data {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes)
        .map_err(|e| TtsError::Synthesis(format!("write {}: {e}", path.display())))
}

fn write_i64_bin(path: &Path, data: &[i64]) -> Result<(), TtsError> {
    let mut bytes = Vec::with_capacity(data.len() * 8);
    for &value in data {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes)
        .map_err(|e| TtsError::Synthesis(format!("write {}: {e}", path.display())))
}
