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
use ort::value::{PrimitiveTensorElementType, Shape, Tensor, Value};
use std::borrow::Cow;
use std::path::{Path, PathBuf};

/// Owned tensor contents plus shape metadata, used to pass ONNX outputs around
/// without losing the runtime-dynamic shape information.
#[derive(Clone, Debug)]
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
    pub fn extract(value: &ort::value::DynValue, name: &str) -> Result<Self, TtsError> {
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

/// A loaded `.safetensors` file. Owns the raw bytes; tensors are parsed on demand.
///
/// safetensors guarantees little-endian storage and carries dtype + shape in the
/// file header, so no separate manifest or endianness assumption is needed.
#[derive(Debug)]
pub(super) struct SafeTensorsFile {
    path: PathBuf,
    bytes: Vec<u8>,
}

impl SafeTensorsFile {
    pub fn open(path: &Path, ctx: &str) -> Result<Self, TtsError> {
        let bytes = std::fs::read(path)
            .map_err(|e| TtsError::Init(format!("{ctx}: read {}: {e}", path.display())))?;
        Ok(Self {
            path: path.to_owned(),
            bytes,
        })
    }

    fn parse(&self) -> Result<safetensors::SafeTensors<'_>, TtsError> {
        safetensors::SafeTensors::deserialize(&self.bytes)
            .map_err(|e| TtsError::Init(format!("parse {}: {e}", self.path.display())))
    }

    pub fn f32(&self, name: &str, ctx: &str) -> Result<TensorData<f32>, TtsError> {
        let st = self.parse()?;
        let t = st
            .tensor(name)
            .map_err(|e| TtsError::Init(format!("{ctx}: tensor {name:?} not found: {e}")))?;
        if t.dtype() != safetensors::Dtype::F32 {
            return Err(TtsError::Init(format!(
                "{ctx}: tensor {name:?} is {:?}, expected F32",
                t.dtype()
            )));
        }
        let data = t
            .data()
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes([b[0], b[1], b[2], b[3]]))
            .collect();
        Ok(TensorData {
            data,
            shape: t.shape().to_vec(),
        })
    }

    pub fn i64(&self, name: &str, ctx: &str) -> Result<TensorData<i64>, TtsError> {
        let st = self.parse()?;
        let t = st
            .tensor(name)
            .map_err(|e| TtsError::Init(format!("{ctx}: tensor {name:?} not found: {e}")))?;
        if t.dtype() != safetensors::Dtype::I64 {
            return Err(TtsError::Init(format!(
                "{ctx}: tensor {name:?} is {:?}, expected I64",
                t.dtype()
            )));
        }
        let data = t
            .data()
            .chunks_exact(8)
            .map(|b| i64::from_le_bytes(b.try_into().unwrap()))
            .collect();
        Ok(TensorData {
            data,
            shape: t.shape().to_vec(),
        })
    }
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

    pub fn accept_token(
        &mut self,
        next_token: i64,
        lm_seq_len: usize,
        stop_tokens: &[i64],
    ) -> bool {
        self.generated.push(next_token);
        if stop_tokens.contains(&next_token) {
            return true;
        }
        self.kv_seq_len += lm_seq_len;
        false
    }

    pub fn generated_count(&self) -> usize {
        self.generated.len().saturating_sub(1)
    }

    pub fn output_tokens(self, start_token: i64, stop_tokens: &[i64]) -> Vec<i64> {
        self.generated
            .into_iter()
            .filter(|t| *t != start_token && !stop_tokens.contains(t))
            .collect()
    }
}

impl KvCacheLayout {
    /// Number of tensor entries (2 per transformer layer: key + value).
    pub fn slot_count(&self) -> usize {
        self.num_layers * 2
    }

    /// Build the `past_key_values.{layer}.{key|value}` input entries.
    /// On the first step (`past_seq_len == 0`) tensors are ORT-allocated with a
    /// zero sequence dimension (requires `Tensor::new`; `from_array` rejects
    /// zero-sized dims). On subsequent steps cached data is moved into owned tensors.
    pub fn past_inputs(
        &self,
        past: &mut [Vec<f32>],
        past_seq_len: usize,
    ) -> Result<Vec<(Cow<'static, str>, SessionInputValue<'static>)>, TtsError> {
        let mut inputs = Vec::with_capacity(self.slot_count());
        for layer in 0..self.num_layers {
            for (kv_idx, kv_name) in ["key", "value"].iter().enumerate() {
                let cache_idx = layer * 2 + kv_idx;
                let name = format!("past_key_values.{layer}.{kv_name}");
                let tensor: Tensor<f32> = if past_seq_len == 0 {
                    Tensor::new(
                        &Allocator::default(),
                        Shape::new([
                            self.batch as i64,
                            self.num_kv_heads as i64,
                            0i64,
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
                inputs.push((
                    Cow::Owned(name),
                    SessionInputValue::Owned(Value::from(tensor)),
                ));
            }
        }
        Ok(inputs)
    }

    /// Replace the in-memory cache with the LM's `present_*` outputs, which are
    /// laid out as `[logits, kv_0, kv_1, ...]` in the session output list.
    pub fn update_from_outputs(
        &self,
        past: &mut [Vec<f32>],
        lm_outputs: &ort::session::SessionOutputs,
    ) -> Result<(), TtsError> {
        for i in 0..self.slot_count() {
            past[i] = lm_outputs[1 + i]
                .try_extract_tensor::<f32>()
                .map_err(|e| TtsError::Synthesis(format!("extract kv_{i}: {e}")))?
                .1
                .to_vec();
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

/// Read `num_kv_heads` and `head_dim` from the `past_key_values.0.key` input
/// shape `[batch, num_kv_heads, seq_len, head_dim]`. Errors if the input is
/// absent, not a tensor, or has dynamic heads/dim.
pub(super) fn detect_kv_dims(session: &Session) -> Result<(usize, usize), TtsError> {
    let kv_input = session
        .inputs()
        .iter()
        .find(|i| i.name() == "past_key_values.0.key")
        .ok_or_else(|| {
            TtsError::Init(
                "language model has no past_key_values.0.key input; unsupported export".into(),
            )
        })?;
    let shape = kv_input
        .dtype()
        .tensor_shape()
        .ok_or_else(|| TtsError::Init("past_key_values.0.key is not a tensor type".into()))?;
    if shape.len() < 4 {
        return Err(TtsError::Init(format!(
            "past_key_values.0.key has rank {}; expected [batch, heads, seq, dim]",
            shape.len()
        )));
    }
    let num_kv_heads = shape[1];
    let head_dim = shape[3];
    if num_kv_heads <= 0 || head_dim <= 0 {
        return Err(TtsError::Init(format!(
            "dynamic KV dims (num_kv_heads={num_kv_heads}, head_dim={head_dim}); concrete values required"
        )));
    }
    Ok((num_kv_heads as usize, head_dim as usize))
}

/// Return true if `session` accepts an input named `position_ids`.
pub(super) fn has_position_ids(session: &Session) -> bool {
    session.inputs().iter().any(|i| i.name() == "position_ids")
}

/// Return true if `session` accepts an input named `exaggeration` (onnx-community
/// Chatterbox export added this scalar control for speech expressiveness).
pub(super) fn has_exaggeration(session: &Session) -> bool {
    session.inputs().iter().any(|i| i.name() == "exaggeration")
}

/// Call an embed session to get hidden-state embeddings for token IDs.
pub(super) fn run_embed(
    session: &mut Session,
    has_position_ids: bool,
    has_exaggeration: bool,
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
    if has_exaggeration {
        let exag_tensor = Tensor::from_array(([1usize], vec![1.0f32]))
            .map_err(|e| TtsError::Synthesis(format!("exaggeration tensor: {e}")))?;
        inputs.push((
            Cow::Borrowed("exaggeration"),
            SessionInputValue::Owned(Value::from(exag_tensor)),
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

    inputs.extend(kv_layout.past_inputs(kv_cache, kv_seq_len)?);

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
    fn safetensors_missing_file_is_error() {
        let dir = tmp_dir("missing_st");
        let err = SafeTensorsFile::open(&dir.join("conditioning.safetensors"), "test")
            .unwrap_err()
            .to_string();
        assert!(err.contains("read"));
    }

    #[test]
    fn safetensors_wrong_dtype_is_error() {
        use safetensors::{serialize, tensor::TensorView, Dtype};
        let dir = tmp_dir("wrong_dtype");
        let data: Vec<f32> = vec![1.0, 2.0];
        let bytes: Vec<u8> = data.iter().flat_map(|f| f.to_le_bytes()).collect();
        let view = TensorView::new(Dtype::F32, vec![2], &bytes).unwrap();
        let tensors = std::collections::HashMap::from([("cond_emb", view)]);
        let buf = serialize(tensors, &None).unwrap();
        let path = dir.join("conditioning.safetensors");
        fs::write(&path, &buf).unwrap();
        let st = SafeTensorsFile::open(&path, "test").unwrap();
        let err = st.i64("cond_emb", "test").unwrap_err().to_string();
        assert!(err.contains("F32") || err.contains("I64"));
    }

    #[test]
    fn safetensors_missing_tensor_is_error() {
        use safetensors::serialize;
        let dir = tmp_dir("missing_tensor_st");
        let tensors: std::collections::HashMap<&str, safetensors::tensor::TensorView<'_>> =
            std::collections::HashMap::new();
        let buf = serialize(tensors, &None).unwrap();
        let path = dir.join("conditioning.safetensors");
        fs::write(&path, &buf).unwrap();
        let st = SafeTensorsFile::open(&path, "test").unwrap();
        let err = st.f32("cond_emb", "test").unwrap_err().to_string();
        assert!(err.contains("cond_emb"));
    }

    // ── collapse_logits ────────────────────────────────────────────────────

    #[test]
    fn collapse_logits_no_cfg_takes_last_position_of_batch_zero() {
        let logits = TensorData {
            data: vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0],
            shape: vec![1, 2, 3],
        };
        assert_eq!(collapse_logits(&logits, false, 0.0), vec![4.0, 5.0, 6.0]);
    }

    #[test]
    fn collapse_logits_with_cfg_mixes_branches_at_last_position() {
        let logits = TensorData {
            data: vec![
                1.0, 1.0, 1.0, 10.0, 20.0, 30.0, 2.0, 2.0, 2.0, 1.0, 2.0, 3.0,
            ],
            shape: vec![2, 2, 3],
        };
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
        assert_eq!(s.kv_cache.len(), 4);
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
        let stop = s.accept_token(42, 5, &[6562]);
        assert!(!stop);
        assert_eq!(s.kv_seq_len, 5);
        assert_eq!(s.generated.last().copied(), Some(42));
    }

    #[test]
    fn accept_token_signals_stop_without_advancing_kv() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        let stop = s.accept_token(6562, 5, &[6562]);
        assert!(stop);
        assert_eq!(s.kv_seq_len, 0);
        assert_eq!(s.generated.last().copied(), Some(6562));
    }

    #[test]
    fn accept_token_stops_on_any_of_multiple_stop_tokens() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        let stop = s.accept_token(2, 5, &[6562, 2]);
        assert!(stop);
        assert_eq!(s.kv_seq_len, 0);
    }

    #[test]
    fn output_tokens_strips_start_and_stop_markers() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        s.generated = vec![6561, 100, 200, 300, 6562];
        assert_eq!(s.output_tokens(6561, &[6562]), vec![100, 200, 300]);
    }

    #[test]
    fn output_tokens_strips_multiple_stop_tokens() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        s.generated = vec![6561, 100, 2, 200, 300, 6562];
        assert_eq!(s.output_tokens(6561, &[6562, 2]), vec![100, 200, 300]);
    }

    #[test]
    fn generated_count_excludes_initial_start_token() {
        let mut s = SpeechGenerationState::new(&layout(1), 6561, 1000, 0);
        s.generated.extend(&[1, 2, 3]);
        assert_eq!(s.generated_count(), 3);
    }
}
