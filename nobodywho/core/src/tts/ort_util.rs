//! Shared ONNX Runtime helpers for Chatterbox-family TTS backends.
//!
//! Both `chatterbox.rs` and `chatterbox_roest.rs` run similar autoregressive
//! pipelines on `ort::Session`s. This module holds the pieces that are truly
//! identical: tensor extraction, KV-cache tensor plumbing, session loading,
//! and pre-computed conditioning I/O.

use crate::errors::TtsError;
use crate::tts::{ort_execution_providers, TtsDevice};
use ort::memory::Allocator;
use ort::session::builder::{GraphOptimizationLevel, SessionBuilder};
use ort::session::Session;
use ort::value::{DynValue, Shape, Tensor};
use std::path::Path;
use tracing::info;

/// Owned tensor contents plus shape metadata, used to pass ONNX outputs around
/// without losing the runtime-dynamic shape information.
#[derive(Clone)]
pub(super) struct TensorData<T> {
    pub data: Vec<T>,
    pub shape: Vec<usize>,
}

impl TensorData<f32> {
    pub fn extract(value: &DynValue, name: &str) -> Result<Self, TtsError> {
        let (shape, data) = value
            .try_extract_tensor::<f32>()
            .map_err(|e| TtsError::Synthesis(format!("extract {name}: {e}")))?;
        Ok(Self {
            data: data.to_vec(),
            shape: shape.iter().map(|&d| d as usize).collect(),
        })
    }
}

impl TensorData<i64> {
    pub fn extract(value: &DynValue, name: &str) -> Result<Self, TtsError> {
        let (shape, data) = value
            .try_extract_tensor::<i64>()
            .map_err(|e| TtsError::Synthesis(format!("extract {name}: {e}")))?;
        Ok(Self {
            data: data.to_vec(),
            shape: shape.iter().map(|&d| d as usize).collect(),
        })
    }
}

/// Build an `ort::Session` from an ONNX file.
///
/// `disable_optimization` forces `GraphOptimizationLevel::Disable`, which Røst
/// requires because some of its exported graphs break under ORT's default
/// fusion passes. Chatterbox leaves it enabled.
pub(super) fn load_session(
    path: &Path,
    device: TtsDevice,
    disable_optimization: bool,
) -> Result<Session, TtsError> {
    let mut builder = SessionBuilder::new()
        .map_err(|e| TtsError::Init(format!("ort session builder: {e}")))?
        .with_log_level(ort::logging::LogLevel::Warning)
        .map_err(|e| TtsError::Init(format!("ort log level: {e}")))?;

    if disable_optimization {
        builder = builder
            .with_optimization_level(GraphOptimizationLevel::Disable)
            .map_err(|e| TtsError::Init(format!("ort optimization level: {e}")))?;
    }

    builder
        .with_execution_providers(ort_execution_providers(device))
        .map_err(|e| TtsError::Init(format!("ort execution providers: {e}")))?
        .commit_from_file(path)
        .map_err(|e| TtsError::Init(format!("ort load model {}: {e}", path.display())))
}

/// Look for a language-model ONNX file in `onnx_dir` using the given
/// candidate filenames, in preference order. Returns the first one found.
pub(super) fn find_language_model(
    onnx_dir: &Path,
    device: TtsDevice,
    disable_optimization: bool,
    candidates: &[&str],
) -> Result<Session, TtsError> {
    for name in candidates {
        let path = onnx_dir.join(name);
        if path.exists() {
            info!(model = name, "Loading language model");
            return load_session(&path, device, disable_optimization);
        }
    }
    Err(TtsError::Init(format!(
        "no language model ONNX file found in {}",
        onnx_dir.display()
    )))
}

/// Parse a `manifest.json` describing pre-computed conditioning tensors.
/// Returns `None` if the file is missing or malformed.
pub(super) fn read_cond_manifest(dir: &Path) -> Option<serde_json::Value> {
    let s = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    serde_json::from_str(&s).ok()
}

/// Load a raw `.bin` file laid out as little-endian f32s with the shape
/// described in the manifest entry.
pub(super) fn load_f32_tensor(
    dir: &Path,
    manifest: &serde_json::Value,
    name: &str,
) -> Option<TensorData<f32>> {
    let shape = shape_from_manifest(manifest, name)?;
    let bytes = std::fs::read(dir.join(format!("{name}.bin"))).ok()?;
    let data = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    Some(TensorData { data, shape })
}

/// Load a raw `.bin` file laid out as little-endian i64s with the shape
/// described in the manifest entry.
pub(super) fn load_i64_tensor(
    dir: &Path,
    manifest: &serde_json::Value,
    name: &str,
) -> Option<TensorData<i64>> {
    let shape = shape_from_manifest(manifest, name)?;
    let bytes = std::fs::read(dir.join(format!("{name}.bin"))).ok()?;
    let data = bytes
        .chunks_exact(8)
        .map(|c| i64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
        .collect();
    Some(TensorData { data, shape })
}

fn shape_from_manifest(manifest: &serde_json::Value, name: &str) -> Option<Vec<usize>> {
    manifest
        .get(name)?
        .get("shape")?
        .as_array()?
        .iter()
        .map(|v| v.as_u64().map(|d| d as usize))
        .collect()
}

/// Shape metadata for the `past_key_values.{layer}.{key|value}` inputs used by
/// all Chatterbox-family language models.
pub(super) struct KvCacheLayout {
    pub num_layers: usize,
    pub num_kv_heads: usize,
    pub head_dim: usize,
    pub batch: usize,
}

impl KvCacheLayout {
    /// Number of tensor entries (2 per transformer layer: key + value).
    pub fn slot_count(&self) -> usize {
        self.num_layers * 2
    }

    /// Emit every `past_key_values.{layer}.{key|value}` input, delivered one
    /// at a time to `push`. On the first step (`past_seq_len == 0`) the tensors
    /// are empty; otherwise the contents of `past` are moved into owned
    /// tensors (leaving each slot as an empty `Vec` until the next step).
    ///
    /// A closure is used instead of a mutable `Vec<...>` so callers don't have
    /// to thread `SessionInputValue`/`Cow` lifetimes through this module.
    pub fn for_each_past_input(
        &self,
        past: &mut [Vec<f32>],
        past_seq_len: usize,
        mut push: impl FnMut(String, DynValue) -> Result<(), TtsError>,
    ) -> Result<(), TtsError> {
        for layer in 0..self.num_layers {
            for (kv_idx, kv_name) in ["key", "value"].iter().enumerate() {
                let cache_idx = layer * 2 + kv_idx;
                let name = format!("past_key_values.{layer}.{kv_name}");
                let tensor = if past_seq_len == 0 {
                    Tensor::<f32>::new(
                        &Allocator::default(),
                        Shape::new([
                            self.batch as i64,
                            self.num_kv_heads as i64,
                            0,
                            self.head_dim as i64,
                        ]),
                    )
                    .map_err(|e| TtsError::Synthesis(format!("empty kv {name}: {e}")))?
                } else {
                    let data = std::mem::take(&mut past[cache_idx]);
                    Tensor::from_array((
                        [self.batch, self.num_kv_heads, past_seq_len, self.head_dim],
                        data,
                    ))
                    .map_err(|e| TtsError::Synthesis(format!("kv {name}: {e}")))?
                };
                push(name, DynValue::from(tensor))?;
            }
        }
        Ok(())
    }

    /// Replace the in-memory cache with the LM's `present_*` outputs, which are
    /// laid out as `[logits, kv_0, kv_1, ...]` in the session output list.
    pub fn update_from_outputs(
        &self,
        past: &mut [Vec<f32>],
        lm_outputs: &ort::session::SessionOutputs,
    ) -> Result<(), TtsError> {
        for i in 0..self.slot_count() {
            past[i] = TensorData::<f32>::extract(&lm_outputs[1 + i], &format!("kv_{i}"))?.data;
        }
        Ok(())
    }
}

/// Detect the number of transformer layers from a language-model session by
/// counting `past_key_values.{N}.key` inputs.
pub(super) fn detect_num_layers(session: &Session) -> usize {
    session
        .inputs()
        .iter()
        .filter(|i| i.name().starts_with("past_key_values.") && i.name().ends_with(".key"))
        .count()
}

/// Return true if `session` accepts an input named `position_ids`.
pub(super) fn has_position_ids(session: &Session) -> bool {
    session.inputs().iter().any(|i| i.name() == "position_ids")
}
