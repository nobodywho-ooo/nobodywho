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
use ort::session::{Session, SessionInputValue, SessionInputs};
use ort::value::{DynValue, PrimitiveTensorElementType, Shape, Tensor, Value};
use std::borrow::Cow;
use std::mem::size_of;
use std::path::Path;
use std::time::Duration;
use tracing::info;

/// Types we read from `.bin` files (manifest-described tensors and Røst's
/// `text_pos_emb.bin`). Implemented for `f32` and `i64`.
pub(super) trait FromLeBytes: Sized {
    fn read_le(bytes: &[u8]) -> Self;
}

impl FromLeBytes for f32 {
    fn read_le(bytes: &[u8]) -> Self {
        f32::from_le_bytes(bytes.try_into().expect("chunks_exact size matches"))
    }
}

impl FromLeBytes for i64 {
    fn read_le(bytes: &[u8]) -> Self {
        i64::from_le_bytes(bytes.try_into().expect("chunks_exact size matches"))
    }
}

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

/// Owned tensor contents plus shape metadata, used to pass ONNX outputs around
/// without losing the runtime-dynamic shape information.
#[derive(Clone)]
pub(super) struct TensorData<T> {
    pub data: Vec<T>,
    pub shape: Vec<usize>,
}

/// Speaker-conditioning tensors used by Chatterbox-family decoders.
#[derive(Clone)]
pub(super) struct SpeakerConditioning {
    /// Hidden-state prefix prepended to the LM input sequence.
    pub cond_emb: TensorData<f32>,
    /// Reference speech tokens concatenated in front of generated tokens
    /// before the conditional decoder stage.
    pub prompt_token: TensorData<i64>,
    /// Speaker x-vector fed to the conditional decoder.
    pub ref_x_vector: TensorData<f32>,
    /// Mel-feature prompt fed to the conditional decoder.
    pub prompt_feat: TensorData<f32>,
}

impl<T: PrimitiveTensorElementType + Clone> TensorData<T> {
    pub fn extract(value: &DynValue, name: &str) -> Result<Self, TtsError> {
        let (shape, data) = value
            .try_extract_tensor::<T>()
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
/// Missing directories or manifests mean "no precomputed conditioning"; a
/// malformed manifest is an initialization error.
pub(super) fn read_cond_manifest(dir: &Path) -> Result<Option<serde_json::Value>, TtsError> {
    let path = dir.join("manifest.json");
    if !path.exists() {
        return Ok(None);
    }
    let s = std::fs::read_to_string(&path)
        .map_err(|e| TtsError::Init(format!("failed to read {}: {e}", path.display())))?;
    let manifest = serde_json::from_str(&s)
        .map_err(|e| TtsError::Init(format!("failed to parse {}: {e}", path.display())))?;
    Ok(Some(manifest))
}

/// Load a raw little-endian `.bin` file with the shape described in the
/// manifest entry.
pub(super) fn load_tensor<T: FromLeBytes>(
    dir: &Path,
    manifest: &serde_json::Value,
    name: &str,
) -> Result<TensorData<T>, TtsError> {
    let shape = shape_from_manifest(manifest, name)?;
    let path = dir.join(format!("{name}.bin"));
    read_le_bin(&path, shape)
}

/// Load a raw little-endian `.bin` file with a known shape (no manifest).
pub(super) fn read_le_bin<T: FromLeBytes>(
    path: &Path,
    shape: Vec<usize>,
) -> Result<TensorData<T>, TtsError> {
    let element_size = size_of::<T>();
    let bytes = std::fs::read(path)
        .map_err(|e| TtsError::Init(format!("failed to read {}: {e}", path.display())))?;
    let element_count = shape.iter().try_fold(1usize, |acc, dim| {
        acc.checked_mul(*dim)
            .ok_or_else(|| TtsError::Init(format!("shape product overflows usize: {shape:?}")))
    })?;
    let expected_len = element_count
        .checked_mul(element_size)
        .ok_or_else(|| TtsError::Init(format!("byte length overflows usize: {shape:?}")))?;
    if bytes.len() != expected_len {
        return Err(TtsError::Init(format!(
            "{} length mismatch: expected {expected_len} bytes from shape {shape:?}, got {}",
            path.display(),
            bytes.len()
        )));
    }
    let data = bytes.chunks_exact(element_size).map(T::read_le).collect();
    Ok(TensorData { data, shape })
}

fn shape_from_manifest(manifest: &serde_json::Value, name: &str) -> Result<Vec<usize>, TtsError> {
    let shape = manifest
        .get(name)
        .ok_or_else(|| TtsError::Init(format!("conditioning manifest missing {name}")))?
        .get("shape")
        .ok_or_else(|| TtsError::Init(format!("conditioning manifest missing {name}.shape")))?
        .as_array()
        .ok_or_else(|| {
            TtsError::Init(format!(
                "conditioning manifest {name}.shape is not an array"
            ))
        })?
        .iter()
        .map(|v| {
            v.as_u64()
                .map(|d| d as usize)
                .ok_or_else(|| TtsError::Init(format!("invalid shape entry for {name}")))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(shape)
}

/// Shape metadata for the `past_key_values.{layer}.{key|value}` inputs used by
/// all Chatterbox-family language models.
pub(super) struct KvCacheLayout {
    pub num_layers: usize,
    pub num_kv_heads: usize,
    pub head_dim: usize,
    pub batch: usize,
}

pub(super) struct SpeechGenerationState {
    pub generated: Vec<i64>,
    pub kv_cache: Vec<Vec<f32>>,
    pub kv_seq_len: usize,
    pub attention_mask: Vec<i64>,
    pub timings: GenerationTimings,
}

impl SpeechGenerationState {
    pub fn new(
        kv_layout: &KvCacheLayout,
        start_token: i64,
        max_new_tokens: usize,
        attention_capacity: usize,
    ) -> Self {
        let mut generated = Vec::with_capacity(max_new_tokens + 1);
        generated.push(start_token);
        Self {
            generated,
            kv_cache: vec![Vec::new(); kv_layout.slot_count()],
            kv_seq_len: 0,
            attention_mask: Vec::with_capacity(attention_capacity),
            timings: GenerationTimings::default(),
        }
    }

    pub fn step_inputs(
        &self,
        step: usize,
        first_ids: &[i64],
        first_positions: &[i64],
    ) -> (Vec<i64>, Vec<i64>) {
        if step == 0 {
            (first_ids.to_vec(), first_positions.to_vec())
        } else {
            (vec![*self.generated.last().unwrap()], vec![step as i64])
        }
    }

    pub fn update_attention(&mut self, step: usize, lm_seq_len: usize) {
        if step == 0 {
            self.attention_mask = vec![1; lm_seq_len];
        } else {
            self.attention_mask.push(1);
        }
    }

    pub fn accept_token(&mut self, next_token: i64, lm_seq_len: usize, stop_token: i64) -> bool {
        self.generated.push(next_token);
        if next_token == stop_token {
            return true;
        }
        self.kv_seq_len += lm_seq_len;
        false
    }

    pub fn generated_count(&self) -> usize {
        self.generated.len().saturating_sub(1)
    }

    pub fn output_tokens(self, start_token: i64, stop_token: i64) -> Vec<i64> {
        self.generated
            .into_iter()
            .filter(|&t| t != start_token && t != stop_token)
            .collect()
    }
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

/// Call an `embed_tokens` session to get hidden-state embeddings for token IDs.
pub(super) fn run_embed(
    session: &mut Session,
    has_position_ids: bool,
    ids: &[i64],
    positions: &[i64],
    output_name: &str,
) -> Result<TensorData<f32>, TtsError> {
    let seq_len = ids.len();
    let ids_tensor = Tensor::from_array(([1, seq_len], ids.to_vec()))
        .map_err(|e| TtsError::Synthesis(format!("ids tensor: {e}")))?;

    let mut inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> = vec![(
        Cow::Borrowed("input_ids"),
        SessionInputValue::Owned(Value::from(ids_tensor)),
    )];
    if has_position_ids {
        let pos_tensor = Tensor::from_array(([1, seq_len], positions.to_vec()))
            .map_err(|e| TtsError::Synthesis(format!("pos tensor: {e}")))?;
        inputs.push((
            Cow::Borrowed("position_ids"),
            SessionInputValue::Owned(Value::from(pos_tensor)),
        ));
    }

    let outputs = session
        .run(SessionInputs::from(inputs))
        .map_err(|e| TtsError::Synthesis(format!("embed_tokens: {e}")))?;
    TensorData::<f32>::extract(&outputs[0], output_name)
}

/// Run one language-model decode step and update the KV cache in place.
#[allow(clippy::too_many_arguments)]
pub(super) fn run_language_model(
    session: &mut Session,
    kv_layout: &KvCacheLayout,
    lm_has_position_ids: bool,
    lm_embeds: Vec<f32>,
    lm_seq_len: usize,
    hidden_dim: usize,
    attention_mask: &[i64],
    kv_cache: &mut [Vec<f32>],
    kv_seq_len: usize,
) -> Result<TensorData<f32>, TtsError> {
    let batch = kv_layout.batch;

    let embeds_tensor = Tensor::from_array(([batch, lm_seq_len, hidden_dim], lm_embeds))
        .map_err(|e| TtsError::Synthesis(format!("embeds tensor: {e}")))?;

    let attn_data: Vec<i64> = (0..batch)
        .flat_map(|_| attention_mask.iter().copied())
        .collect();
    let attn_tensor = Tensor::from_array(([batch, attention_mask.len()], attn_data))
        .map_err(|e| TtsError::Synthesis(format!("attn tensor: {e}")))?;

    let mut inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> = vec![
        (
            Cow::Borrowed("inputs_embeds"),
            SessionInputValue::Owned(Value::from(embeds_tensor)),
        ),
        (
            Cow::Borrowed("attention_mask"),
            SessionInputValue::Owned(Value::from(attn_tensor)),
        ),
    ];

    if lm_has_position_ids {
        let pos_ids: Vec<i64> = (0..batch)
            .flat_map(|_| (kv_seq_len..kv_seq_len + lm_seq_len).map(|i| i as i64))
            .collect();
        let pos_tensor = Tensor::from_array(([batch, lm_seq_len], pos_ids))
            .map_err(|e| TtsError::Synthesis(format!("pos_id tensor: {e}")))?;
        inputs.push((
            Cow::Borrowed("position_ids"),
            SessionInputValue::Owned(Value::from(pos_tensor)),
        ));
    }

    kv_layout.for_each_past_input(kv_cache, kv_seq_len, |name, value| {
        inputs.push((Cow::Owned(name), SessionInputValue::Owned(value)));
        Ok(())
    })?;

    let outputs = session
        .run(SessionInputs::from(inputs))
        .map_err(|e| TtsError::Synthesis(format!("language model: {e}")))?;

    let logits = TensorData::<f32>::extract(&outputs[0], "logits")?;
    kv_layout.update_from_outputs(kv_cache, &outputs)?;
    Ok(logits)
}

/// Build conditional-decoder inputs shared by Chatterbox and Røst.
pub(super) fn decoder_inputs<'a>(
    speech_tokens: &[i64],
    cond: &SpeakerConditioning,
) -> Result<Vec<(Cow<'a, str>, SessionInputValue<'a>)>, TtsError> {
    let tokens_tensor = Tensor::from_array(([1, speech_tokens.len()], speech_tokens.to_vec()))
        .map_err(|e| TtsError::Synthesis(format!("speech tokens tensor: {e}")))?;
    let speaker_tensor = Tensor::from_array((
        cond.ref_x_vector.shape.clone(),
        cond.ref_x_vector.data.clone(),
    ))
    .map_err(|e| TtsError::Synthesis(format!("speaker tensor: {e}")))?;
    let feat_tensor = Tensor::from_array((
        cond.prompt_feat.shape.clone(),
        cond.prompt_feat.data.clone(),
    ))
    .map_err(|e| TtsError::Synthesis(format!("feat tensor: {e}")))?;

    Ok(vec![
        (
            Cow::Borrowed("speech_tokens"),
            SessionInputValue::Owned(Value::from(tokens_tensor)),
        ),
        (
            Cow::Borrowed("speaker_embeddings"),
            SessionInputValue::Owned(Value::from(speaker_tensor)),
        ),
        (
            Cow::Borrowed("speaker_features"),
            SessionInputValue::Owned(Value::from(feat_tensor)),
        ),
    ])
}

/// Single speech-token continuation, duplicated across the CFG batch when
/// enabled. Shared by Chatterbox-family language-model loops.
pub(super) fn build_continuation_embeds(
    token_embeds: &TensorData<f32>,
    use_cfg: bool,
) -> (Vec<f32>, usize, usize) {
    let hidden_dim = *token_embeds.shape.last().expect("embeds rank >= 1");
    let seq_len = token_embeds.shape[1];
    let mut data = token_embeds.data.clone();
    if use_cfg {
        data.extend_from_slice(&token_embeds.data);
    }
    (data, seq_len, hidden_dim)
}

/// Reduce the LM output to a single vocab-sized logit vector.
///
/// With CFG the output has shape `[2, seq, vocab]`: batch 0 is conditioned,
/// batch 1 is unconditioned, and the two are mixed as
/// `cond + cfg_weight * (cond - uncond)`. Without CFG this returns the last
/// position from batch 0.
pub(super) fn collapse_logits(
    logits: &TensorData<f32>,
    use_cfg: bool,
    cfg_weight: f32,
) -> Vec<f32> {
    let vocab = logits.shape[2];
    let seq = logits.shape[1];

    if use_cfg {
        let cond_start = (seq - 1) * vocab;
        let uncond_start = (seq + seq - 1) * vocab;
        let cond = &logits.data[cond_start..cond_start + vocab];
        let uncond = &logits.data[uncond_start..uncond_start + vocab];
        cond.iter()
            .zip(uncond.iter())
            .map(|(&c, &u)| c + cfg_weight * (c - u))
            .collect()
    } else {
        let start = (seq - 1) * vocab;
        logits.data[start..start + vocab].to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::atomic::{AtomicUsize, Ordering};

    static NEXT_TMP: AtomicUsize = AtomicUsize::new(0);

    fn tmp_dir(name: &str) -> PathBuf {
        let id = NEXT_TMP.fetch_add(1, Ordering::Relaxed);
        let dir = std::env::temp_dir().join(format!(
            "nobodywho_tts_ort_util_{name}_{}_{}",
            std::process::id(),
            id
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn missing_cond_manifest_is_absent() {
        let dir = tmp_dir("missing_manifest");
        let manifest = read_cond_manifest(&dir).unwrap();
        assert!(manifest.is_none());
    }

    #[test]
    fn malformed_cond_manifest_is_error() {
        let dir = tmp_dir("bad_manifest");
        fs::write(dir.join("manifest.json"), b"{not json").unwrap();
        let err = read_cond_manifest(&dir).unwrap_err().to_string();
        assert!(err.contains("failed to parse"));
    }

    #[test]
    fn missing_cond_tensor_is_error() {
        let dir = tmp_dir("missing_tensor");
        let manifest = serde_json::json!({
            "cond_emb": { "shape": [1, 2] }
        });
        let err = match load_tensor::<f32>(&dir, &manifest, "cond_emb") {
            Ok(_) => panic!("expected missing tensor to fail"),
            Err(err) => err.to_string(),
        };
        assert!(err.contains("failed to read"));
    }

    #[test]
    fn cond_tensor_length_mismatch_is_error() {
        let dir = tmp_dir("length_mismatch");
        let manifest = serde_json::json!({
            "cond_emb": { "shape": [2] }
        });
        fs::write(dir.join("cond_emb.bin"), 1.0f32.to_le_bytes()).unwrap();
        let err = match load_tensor::<f32>(&dir, &manifest, "cond_emb") {
            Ok(_) => panic!("expected tensor length mismatch to fail"),
            Err(err) => err.to_string(),
        };
        assert!(err.contains("length mismatch"));
    }

    // ── collapse_logits ────────────────────────────────────────────────────

    #[test]
    fn collapse_logits_no_cfg_takes_last_position_of_batch_zero() {
        // shape [1, 2, 3] — batch=1, seq=2, vocab=3
        let logits = TensorData {
            data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            shape: vec![1, 2, 3],
        };
        assert_eq!(collapse_logits(&logits, false, 0.0), vec![4.0, 5.0, 6.0]);
    }

    #[test]
    fn collapse_logits_with_cfg_mixes_branches_at_last_position() {
        // shape [2, 2, 3] — batch=2, seq=2, vocab=3
        let logits = TensorData {
            data: vec![
                // batch 0
                1.0, 1.0, 1.0, // pos 0 (ignored)
                10.0, 20.0, 30.0, // pos 1 — cond
                // batch 1
                2.0, 2.0, 2.0, // pos 0 (ignored)
                1.0, 2.0, 3.0, // pos 1 — uncond
            ],
            shape: vec![2, 2, 3],
        };
        // cond + 0.5 * (cond - uncond) = [14.5, 29.0, 43.5]
        assert_eq!(collapse_logits(&logits, true, 0.5), vec![14.5, 29.0, 43.5]);
    }

    #[test]
    fn collapse_logits_with_cfg_zero_weight_returns_cond_unchanged() {
        let logits = TensorData {
            data: vec![10.0, 20.0, 30.0, 100.0, 200.0, 300.0],
            shape: vec![2, 1, 3],
        };
        assert_eq!(collapse_logits(&logits, true, 0.0), vec![10.0, 20.0, 30.0]);
    }

    // ── SpeechGenerationState ──────────────────────────────────────────────

    fn layout(num_layers: usize) -> KvCacheLayout {
        KvCacheLayout {
            num_layers,
            num_kv_heads: 1,
            head_dim: 1,
            batch: 1,
        }
    }

    #[test]
    fn speech_generation_state_starts_with_start_token() {
        let s = SpeechGenerationState::new(&layout(2), 6561, 1000, 0);
        assert_eq!(s.generated, vec![6561]);
        assert_eq!(s.kv_cache.len(), 4); // 2 layers × 2 (k, v)
        assert_eq!(s.kv_seq_len, 0);
        assert!(s.attention_mask.is_empty());
    }

    #[test]
    fn step_inputs_first_step_uses_provided_sequence() {
        let s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        let (ids, pos) = s.step_inputs(0, &[10, 20, 30], &[0, 1, 2]);
        assert_eq!(ids, vec![10, 20, 30]);
        assert_eq!(pos, vec![0, 1, 2]);
    }

    #[test]
    fn step_inputs_continuation_uses_last_generated() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        s.generated.push(42);
        let (ids, pos) = s.step_inputs(5, &[10, 20], &[0, 1]);
        assert_eq!(ids, vec![42]);
        assert_eq!(pos, vec![5]);
    }

    #[test]
    fn update_attention_first_step_resets_to_seq_len() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        s.update_attention(0, 5);
        assert_eq!(s.attention_mask, vec![1, 1, 1, 1, 1]);
    }

    #[test]
    fn update_attention_continuation_appends_one() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        s.attention_mask = vec![1, 1, 1];
        s.update_attention(1, 1);
        assert_eq!(s.attention_mask, vec![1, 1, 1, 1]);
    }

    #[test]
    fn accept_token_advances_kv_seq_for_normal_token() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        let stop = s.accept_token(42, 5, 6562);
        assert!(!stop);
        assert_eq!(s.kv_seq_len, 5);
        assert_eq!(s.generated.last().copied(), Some(42));
    }

    #[test]
    fn accept_token_signals_stop_without_advancing_kv() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        let stop = s.accept_token(6562, 5, 6562);
        assert!(stop);
        assert_eq!(s.kv_seq_len, 0);
        assert_eq!(s.generated.last().copied(), Some(6562));
    }

    #[test]
    fn output_tokens_strips_start_and_stop_markers() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        s.generated = vec![6561, 100, 200, 300, 6562];
        assert_eq!(s.output_tokens(6561, 6562), vec![100, 200, 300]);
    }

    #[test]
    fn generated_count_excludes_initial_start_token() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        s.generated.extend(&[1, 2, 3]);
        assert_eq!(s.generated_count(), 3);
    }
}
