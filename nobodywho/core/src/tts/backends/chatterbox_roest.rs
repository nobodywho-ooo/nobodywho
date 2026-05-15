//! Røst TTS — Danish-finetuned Chatterbox via ONNX Runtime.
//!
//! Røst reuses Chatterbox's ONNX graphs for the conditional decoder (speech
//! tokens → PCM) but has its own finetuned text/speech embedding and
//! language_model exports. The finetuned cond_enc weights were fused into the
//! base export before the torch → ONNX conversion, so Røst cannot run the
//! speech encoder — `conditioning.safetensors` is mandatory.
//!
//! Model-specific token IDs and generation bounds are read from
//! `model_config.json` (all fields optional, sensible defaults provided).
//!
//! Expected model directory layout:
//!
//! ```text
//! dir/
//!   tokenizer.json
//!   model_config.json              — optional token IDs and generation bounds
//!   conditioning.safetensors       — cond_emb, prompt_token, ref_x_vector,
//!                                    prompt_feat, text_pos_emb
//!   text_embed.onnx
//!   speech_embed.onnx
//!   language_model.onnx
//!   conditional_decoder.onnx
//! ```

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::ort_util::{
    self, build_continuation_embeds, collapse_logits, detect_num_layers, has_position_ids,
    KvCacheLayout, SpeakerConditioning, SpeechGenerationState, TensorData,
};
use crate::tts::sampling;
use crate::tts::{TtsDevice, TtsSampling, DEFAULT_SAMPLE_RATE};
use ort::session::SessionInputs;
use serde::Deserialize;
use std::path::{Path, PathBuf};
use tracing::{debug, info};
use unicode_normalization::UnicodeNormalization;

const TEXT_EMBED_FILE: &str = "text_embed.onnx";
const SPEECH_EMBED_FILE: &str = "speech_embed.onnx";
const LANGUAGE_MODEL_FILE: &str = "language_model.onnx";
const CONDITIONAL_DECODER_FILE: &str = "conditional_decoder.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";

/// Resolve an ONNX model file: try `model_dir/file` first, fall back to
/// `model_dir/onnx/file` to support repos that store ONNX files in a subdir.
fn onnx_path(model_dir: &Path, file: &str) -> PathBuf {
    let flat = model_dir.join(file);
    if flat.exists() {
        flat
    } else {
        model_dir.join("onnx").join(file)
    }
}

pub(in crate::tts) struct RoestBackend {
    text_embed: ort::session::Session,
    speech_embed: ort::session::Session,
    language_model: ort::session::Session,
    conditional_decoder: ort::session::Session,
    tokenizer: tokenizers::Tokenizer,
    cond: SpeakerConditioning,
    text_pos_emb: TensorData<f32>,
    language: String,
    sampling: TtsSampling,
    sample_rate: u32,
    start_text_token: i64,
    stop_text_token: i64,
    start_speech_token: i64,
    stop_speech_token: i64,
    max_new_tokens: usize,
    num_layers: usize,
    num_kv_heads: usize,
    head_dim: usize,
    text_embed_has_position_ids: bool,
    speech_embed_has_position_ids: bool,
}

impl RoestBackend {
    pub fn new(
        model_dir: &Path,
        language: String,
        sampling: TtsSampling,
        sample_rate: u32,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let model_config = load_model_config(model_dir)?;

        let tokenizer = tokenizers::Tokenizer::from_file(model_dir.join(TOKENIZER_FILE))
            .map_err(|e| TtsError::Init(format!("roest: load tokenizer: {e}")))?;

        let text_embed =
            ort_util::load_session(&onnx_path(model_dir, TEXT_EMBED_FILE), device, false)?;
        let speech_embed =
            ort_util::load_session(&onnx_path(model_dir, SPEECH_EMBED_FILE), device, false)?;
        let language_model =
            ort_util::load_session(&onnx_path(model_dir, LANGUAGE_MODEL_FILE), device, true)?;
        let conditional_decoder = ort_util::load_session(
            &onnx_path(model_dir, CONDITIONAL_DECODER_FILE),
            device,
            false,
        )?;

        let num_layers = detect_num_layers(&language_model);
        let (num_kv_heads, head_dim) = ort_util::detect_kv_dims(&language_model)?;
        let text_embed_has_position_ids = has_position_ids(&text_embed);
        let speech_embed_has_position_ids = has_position_ids(&speech_embed);

        let cond = load_bin_conditioning(&model_dir.join("default_cond"))?;
        let text_pos_emb = load_text_pos_emb(
            &model_dir.join("text_pos_emb.bin"),
            &model_config.text_pos_emb_shape,
        )?;

        info!(
            num_layers,
            num_kv_heads,
            head_dim,
            start_text_token = model_config.start_text_token,
            stop_text_token = model_config.stop_text_token,
            start_speech_token = model_config.start_speech_token,
            stop_speech_token = model_config.stop_speech_token,
            cond_seq = cond.cond_emb.shape[1],
            "Loaded Røst TTS"
        );

        Ok(Self {
            text_embed,
            speech_embed,
            language_model,
            conditional_decoder,
            tokenizer,
            cond,
            text_pos_emb,
            language,
            sampling,
            sample_rate,
            start_text_token: model_config.start_text_token,
            stop_text_token: model_config.stop_text_token,
            start_speech_token: model_config.start_speech_token,
            stop_speech_token: model_config.stop_speech_token,
            max_new_tokens: model_config.max_new_tokens,
            num_layers,
            num_kv_heads,
            head_dim,
            text_embed_has_position_ids,
            speech_embed_has_position_ids,
        })
    }
}

impl TtsBackendImpl for RoestBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        let sampling = self.sampling.clone();
        let prepared = prepare_text_for_mtl_tokenizer(text, &self.language);
        let (text_input_ids, text_position_ids) = self.tokenize_for_lm(&prepared)?;
        let speech_tokens =
            self.generate_speech_tokens(&text_input_ids, &text_position_ids, &sampling)?;

        let prompt_token_count = self.cond.prompt_token.data.len();
        let mut full_tokens: Vec<i64> = self.cond.prompt_token.data.clone();
        full_tokens.extend_from_slice(&speech_tokens);

        let pcm = self.decode_speech(&full_tokens)?;

        debug!(
            text_tokens = text_input_ids.len(),
            prompt_tokens = prompt_token_count,
            generated_tokens = speech_tokens.len(),
            pcm_samples = pcm.len(),
            pcm_duration_s = pcm.len() as f32 / self.sample_rate as f32,
            "Røst: decode complete"
        );

        Ok((pcm, self.sample_rate))
    }
}

impl RoestBackend {
    /// Build the `[SOT, text..., EOT]` token sequence plus parallel position IDs.
    fn tokenize_for_lm(&self, prepared_text: &str) -> Result<(Vec<i64>, Vec<i64>), TtsError> {
        let encoding = self
            .tokenizer
            .encode(prepared_text, false)
            .map_err(|e| TtsError::Synthesis(format!("tokenization failed: {e}")))?;
        let raw_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();

        let mut ids: Vec<i64> = Vec::with_capacity(raw_ids.len() + 2);
        ids.push(self.start_text_token);
        ids.extend(&raw_ids);
        ids.push(self.stop_text_token);

        let positions: Vec<i64> = (0..ids.len() as i64).collect();
        Ok((ids, positions))
    }

    fn generate_speech_tokens(
        &mut self,
        text_input_ids: &[i64],
        text_position_ids: &[i64],
        sampling_params: &TtsSampling,
    ) -> Result<Vec<i64>, TtsError> {
        let use_cfg = sampling_params.cfg_weight > 0.0;
        let kv_layout = KvCacheLayout {
            num_layers: self.num_layers,
            num_kv_heads: self.num_kv_heads,
            head_dim: self.head_dim,
            batch: if use_cfg { 2 } else { 1 },
        };

        let cond_emb_seq = self.cond.cond_emb.shape[1];
        let cond_emb = &self.cond.cond_emb;
        let text_pos_emb = &self.text_pos_emb;
        let text_embed_session = &mut self.text_embed;
        let speech_embed_session = &mut self.speech_embed;
        let lm_session = &mut self.language_model;

        let bos_embeds = embed_speech_token(
            speech_embed_session,
            self.speech_embed_has_position_ids,
            self.start_speech_token,
            0,
        )?;

        let mut state = SpeechGenerationState::new(
            &kv_layout,
            self.start_speech_token,
            self.max_new_tokens,
            cond_emb_seq + text_input_ids.len() + self.max_new_tokens,
        );

        for step in 0..self.max_new_tokens {
            let token_embeds = if step == 0 {
                ort_util::run_embed(
                    text_embed_session,
                    self.text_embed_has_position_ids,
                    false,
                    text_input_ids,
                    text_position_ids,
                    "text_embeds",
                )?
            } else {
                // Speech position counter advances by 1 per loop iteration —
                // the duplicated BOS in the first-step pack resolves to position 0
                // (same embedding twice), so step 1 → position 1.
                embed_speech_token(
                    speech_embed_session,
                    self.speech_embed_has_position_ids,
                    *state.generated.last().unwrap(),
                    step as i64,
                )?
            };

            let (lm_embeds, lm_seq_len, hidden_dim) = if step == 0 {
                build_first_step_embeds(
                    cond_emb,
                    text_pos_emb,
                    &token_embeds,
                    &bos_embeds,
                    use_cfg,
                )?
            } else {
                build_continuation_embeds(&token_embeds, use_cfg)
            };

            state.update_attention(step, lm_seq_len);

            let logits = ort_util::run_language_model(
                lm_session,
                &kv_layout,
                false,
                lm_embeds,
                lm_seq_len,
                hidden_dim,
                &state.attention_mask,
                &mut state.kv_cache,
                state.kv_seq_len,
            )?;

            let mut final_logits = collapse_logits(&logits, use_cfg, sampling_params.cfg_weight);

            if (self.stop_speech_token as usize) < final_logits.len() {
                tracing::trace!(
                    step,
                    stop_logit = final_logits[self.stop_speech_token as usize],
                    "roest: stop speech token logit (pre-penalty)"
                );
            }

            sampling::apply_repetition_penalty(
                &mut final_logits,
                &state.generated,
                sampling_params.repetition_penalty,
                true,
            );
            let next_token = sampling::sample_token(&mut final_logits, sampling_params) as i64;

            if state.accept_token(next_token, lm_seq_len, &[self.stop_speech_token]) {
                info!(steps = step + 1, "Røst: EOS reached");
                break;
            }
        }

        if state.generated_count() >= self.max_new_tokens {
            tracing::warn!(
                max_new_tokens = self.max_new_tokens,
                "Røst: reached max_new_tokens without EOS — output is truncated"
            );
        }

        Ok(state.output_tokens(self.start_speech_token, &[self.stop_speech_token]))
    }

    fn decode_speech(&mut self, speech_tokens: &[i64]) -> Result<Vec<f32>, TtsError> {
        let inputs = ort_util::decoder_inputs(speech_tokens, &self.cond)?;
        let outputs = self
            .conditional_decoder
            .run(SessionInputs::from(inputs))
            .map_err(|e| TtsError::Synthesis(format!("conditional decoder: {e}")))?;
        Ok(TensorData::<f32>::extract(&outputs[0], "wav")?.data)
    }
}

fn embed_speech_token(
    session: &mut ort::session::Session,
    has_position_ids: bool,
    token: i64,
    position: i64,
) -> Result<TensorData<f32>, TtsError> {
    ort_util::run_embed(
        session,
        has_position_ids,
        false,
        &[token],
        &[position],
        "speech_embeds",
    )
}

/// First LM step — mirrors the torch T3 inference path.
///
/// Batch 0 (conditioned):   `cond_emb | text_embeds | BOS | BOS`
/// Batch 1 (unconditioned): `cond_emb | text_pos_emb | BOS | BOS`
/// https://github.com/resemble-ai/chatterbox/blob/master/src/chatterbox/models/t3/t3.py#L310
fn build_first_step_embeds(
    cond_emb: &TensorData<f32>,
    text_pos_emb: &TensorData<f32>,
    text_embeds: &TensorData<f32>,
    bos_embeds: &TensorData<f32>,
    use_cfg: bool,
) -> Result<(Vec<f32>, usize, usize), TtsError> {
    let hidden_dim = *text_embeds.shape.last().expect("embeds rank >= 1");
    let cond_seq = cond_emb.shape[1];
    let text_seq = text_embeds.shape[1];
    let bos_seq = bos_embeds.shape[1];
    let total_seq = cond_seq + text_seq + bos_seq + bos_seq;
    let batch = if use_cfg { 2 } else { 1 };

    let mut data = Vec::with_capacity(batch * total_seq * hidden_dim);
    data.extend_from_slice(&cond_emb.data);
    data.extend_from_slice(&text_embeds.data);
    data.extend_from_slice(&bos_embeds.data);
    data.extend_from_slice(&bos_embeds.data);

    if use_cfg {
        data.extend_from_slice(&cond_emb.data);
        data.extend_from_slice(text_position_slice(text_pos_emb, text_seq, hidden_dim)?);
        data.extend_from_slice(&bos_embeds.data);
        data.extend_from_slice(&bos_embeds.data);
    }

    Ok((data, total_seq, hidden_dim))
}

/// Return the leading `text_seq × hidden_dim` chunk of learned text
/// position embeddings, validating the shape first.
/// https://github.com/resemble-ai/chatterbox/blob/master/src/chatterbox/models/t3/t3.py#L118
fn text_position_slice(
    text_pos_emb: &TensorData<f32>,
    text_seq: usize,
    hidden_dim: usize,
) -> Result<&[f32], TtsError> {
    if text_pos_emb.shape.len() != 2 {
        return Err(TtsError::Init("text_pos_emb must be rank-2".into()));
    }
    if text_pos_emb.shape[1] != hidden_dim {
        return Err(TtsError::Init(format!(
            "text_pos_emb hidden size mismatch: {} != {}",
            text_pos_emb.shape[1], hidden_dim
        )));
    }
    if text_pos_emb.shape[0] < text_seq {
        return Err(TtsError::Synthesis(format!(
            "text_pos_emb too short: need {text_seq}, have {}",
            text_pos_emb.shape[0]
        )));
    }
    Ok(&text_pos_emb.data[..text_seq * hidden_dim])
}

fn prepare_text_for_mtl_tokenizer(text: &str, language: &str) -> String {
    let punctuated = punc_norm(text);
    let normalized: String = punctuated.to_lowercase().nfkd().collect();
    let replaced_spaces = normalized.replace(' ', "[SPACE]");
    let language = if language.is_empty() { "da" } else { language }.to_lowercase();
    format!("[{}]{}", language, replaced_spaces)
}

/// Normalize whitespace and punctuation to match the upstream Chatterbox
/// dataset preprocessing. Applied before `nfkd` + lowercasing.
/// https://github.com/resemble-ai/chatterbox/blob/master/src/chatterbox/tts.py#L18
fn punc_norm(text: &str) -> String {
    if text.is_empty() {
        return "You need to add some text for me to talk.".into();
    }

    let mut text = text.to_string();
    if let Some(first) = text.chars().next() {
        if first.is_lowercase() {
            let first_upper: String = first.to_uppercase().collect();
            text.replace_range(0..first.len_utf8(), &first_upper);
        }
    }

    text = text.split_whitespace().collect::<Vec<_>>().join(" ");

    for (from, to) in [
        ("...", ", "),
        ("…", ", "),
        (":", ","),
        (" - ", ", "),
        (";", ", "),
        ("—", "-"),
        ("–", "-"),
        (" ,", ","),
        ("\u{201C}", "\""),
        ("\u{201D}", "\""),
        ("\u{2018}", "'"),
        ("\u{2019}", "'"),
    ] {
        text = text.replace(from, to);
    }

    while text.ends_with(' ') {
        text.pop();
    }

    if !text.ends_with(['.', '!', '?', '-', ',', '、', '，', '。', '？', '！']) {
        text.push('.');
    }

    text
}

/// All fields are optional and default to the upstream Røst export values.
#[derive(Deserialize, Default)]
struct RoestModelConfig {
    #[serde(default = "default_start_text_token")]
    start_text_token: i64,
    #[serde(default = "default_stop_text_token")]
    stop_text_token: i64,
    #[serde(default = "default_start_speech_token")]
    start_speech_token: i64,
    #[serde(default = "default_stop_speech_token")]
    stop_speech_token: i64,
    #[serde(default = "default_max_new_tokens")]
    max_new_tokens: usize,
    #[serde(default = "default_text_pos_emb_shape")]
    text_pos_emb_shape: [usize; 2],
}

fn default_text_pos_emb_shape() -> [usize; 2] {
    [2050, 1024]
}

fn default_start_text_token() -> i64 {
    255
}
fn default_stop_text_token() -> i64 {
    0
}
fn default_start_speech_token() -> i64 {
    6561
}
fn default_stop_speech_token() -> i64 {
    6562
}
fn default_max_new_tokens() -> usize {
    1000
}

/// Load pre-computed conditioning from a directory of raw-binary tensors.
/// The directory must contain `manifest.json` (mapping name → shape + dtype)
/// and one `<name>.bin` per tensor (row-major, little-endian).
fn load_bin_conditioning(cond_dir: &Path) -> Result<SpeakerConditioning, TtsError> {
    let manifest_path = cond_dir.join("manifest.json");
    let raw = std::fs::read_to_string(&manifest_path)
        .map_err(|e| TtsError::Init(format!("roest: read {}: {e}", manifest_path.display())))?;
    let manifest: std::collections::HashMap<String, serde_json::Value> = serde_json::from_str(&raw)
        .map_err(|e| TtsError::Init(format!("roest: parse {}: {e}", manifest_path.display())))?;

    fn load_f32(
        cond_dir: &Path,
        name: &str,
        manifest: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<TensorData<f32>, TtsError> {
        let meta = manifest
            .get(name)
            .ok_or_else(|| TtsError::Init(format!("roest: {name} missing from manifest")))?;
        let shape: Vec<usize> = meta["shape"]
            .as_array()
            .ok_or_else(|| TtsError::Init(format!("roest: {name}.shape not an array")))?
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) as usize)
            .collect();
        let bytes = std::fs::read(cond_dir.join(format!("{name}.bin")))
            .map_err(|e| TtsError::Init(format!("roest: read {name}.bin: {e}")))?;
        let data = bytes
            .chunks_exact(4)
            .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        Ok(TensorData { data, shape })
    }

    fn load_i64(
        cond_dir: &Path,
        name: &str,
        manifest: &std::collections::HashMap<String, serde_json::Value>,
    ) -> Result<TensorData<i64>, TtsError> {
        let meta = manifest
            .get(name)
            .ok_or_else(|| TtsError::Init(format!("roest: {name} missing from manifest")))?;
        let shape: Vec<usize> = meta["shape"]
            .as_array()
            .ok_or_else(|| TtsError::Init(format!("roest: {name}.shape not an array")))?
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) as usize)
            .collect();
        let bytes = std::fs::read(cond_dir.join(format!("{name}.bin")))
            .map_err(|e| TtsError::Init(format!("roest: read {name}.bin: {e}")))?;
        let data = bytes
            .chunks_exact(8)
            .map(|b| i64::from_le_bytes(b.try_into().unwrap()))
            .collect();
        Ok(TensorData { data, shape })
    }

    Ok(SpeakerConditioning {
        cond_emb: load_f32(cond_dir, "cond_emb", &manifest)?,
        prompt_token: load_i64(cond_dir, "prompt_token", &manifest)?,
        ref_x_vector: load_f32(cond_dir, "ref_x_vector", &manifest)?,
        prompt_feat: load_f32(cond_dir, "prompt_feat", &manifest)?,
    })
}

/// Load the text position embedding from a raw f32 binary file.
fn load_text_pos_emb(path: &Path, shape: &[usize; 2]) -> Result<TensorData<f32>, TtsError> {
    let bytes = std::fs::read(path)
        .map_err(|e| TtsError::Init(format!("roest: read {}: {e}", path.display())))?;
    let data = bytes
        .chunks_exact(4)
        .map(|b| f32::from_le_bytes(b.try_into().unwrap()))
        .collect();
    Ok(TensorData {
        data,
        shape: shape.to_vec(),
    })
}

fn load_model_config(model_dir: &Path) -> Result<RoestModelConfig, TtsError> {
    let path = model_dir.join("model_config.json");
    let Ok(s) = std::fs::read_to_string(&path) else {
        return Ok(RoestModelConfig::default());
    };
    serde_json::from_str(&s)
        .map_err(|e| TtsError::Init(format!("roest: parse {}: {e}", path.display())))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn punc_norm_empty_returns_default_message() {
        assert_eq!(punc_norm(""), "You need to add some text for me to talk.");
    }

    #[test]
    fn punc_norm_capitalizes_first_letter() {
        assert_eq!(punc_norm("hej"), "Hej.");
    }

    #[test]
    fn punc_norm_keeps_already_capital() {
        assert_eq!(punc_norm("Hej."), "Hej.");
    }

    #[test]
    fn punc_norm_collapses_extra_whitespace() {
        assert_eq!(punc_norm("hej   verden"), "Hej verden.");
    }

    #[test]
    fn punc_norm_replaces_ellipsis_with_comma() {
        assert_eq!(punc_norm("hej..."), "Hej,");
    }

    #[test]
    fn punc_norm_replaces_unicode_ellipsis() {
        assert_eq!(punc_norm("hej\u{2026}"), "Hej,");
    }

    #[test]
    fn punc_norm_replaces_em_dash_with_hyphen() {
        assert_eq!(punc_norm("hej\u{2014}verden"), "Hej-verden.");
    }

    #[test]
    fn punc_norm_replaces_dashed_phrase_with_comma() {
        assert_eq!(punc_norm("hej - verden"), "Hej, verden.");
    }

    #[test]
    fn punc_norm_replaces_smart_double_quotes() {
        assert_eq!(punc_norm("\u{201C}hej\u{201D}"), "\"hej\".");
    }

    #[test]
    fn punc_norm_appends_period_when_missing_terminator() {
        assert_eq!(punc_norm("hej"), "Hej.");
    }

    #[test]
    fn punc_norm_keeps_existing_terminator() {
        assert_eq!(punc_norm("hej!"), "Hej!");
        assert_eq!(punc_norm("hej?"), "Hej?");
    }

    #[test]
    fn prepare_text_for_mtl_default_language_is_da() {
        assert_eq!(prepare_text_for_mtl_tokenizer("Hej.", ""), "[da]hej.");
    }

    #[test]
    fn prepare_text_for_mtl_lowercases_language_tag() {
        assert_eq!(prepare_text_for_mtl_tokenizer("Hej.", "DA"), "[da]hej.");
    }

    #[test]
    fn prepare_text_for_mtl_replaces_spaces_with_marker() {
        assert_eq!(
            prepare_text_for_mtl_tokenizer("a b c", "en"),
            "[en]a[SPACE]b[SPACE]c."
        );
    }

    fn tensor_3d(batch: usize, seq: usize, hidden: usize, fill: f32) -> TensorData<f32> {
        TensorData {
            data: vec![fill; batch * seq * hidden],
            shape: vec![batch, seq, hidden],
        }
    }

    fn pos_emb(rows: usize, hidden: usize, fill: f32) -> TensorData<f32> {
        TensorData {
            data: vec![fill; rows * hidden],
            shape: vec![rows, hidden],
        }
    }

    #[test]
    fn build_first_step_embeds_no_cfg_packs_cond_text_bos_bos() {
        let cond = tensor_3d(1, 3, 4, 1.0);
        let text_pos = pos_emb(8, 4, 0.0);
        let text = tensor_3d(1, 2, 4, 2.0);
        let bos = tensor_3d(1, 1, 4, 3.0);

        let (data, seq, hidden) =
            build_first_step_embeds(&cond, &text_pos, &text, &bos, false).unwrap();

        assert_eq!(seq, 7);
        assert_eq!(hidden, 4);
        assert_eq!(data.len(), 28);
        assert_eq!(&data[..12], &[1.0; 12]);
        assert_eq!(&data[12..20], &[2.0; 8]);
        assert_eq!(&data[20..28], &[3.0; 8]);
    }

    #[test]
    fn build_first_step_embeds_with_cfg_substitutes_text_pos_in_uncond() {
        let cond = tensor_3d(1, 3, 4, 1.0);
        let mut text_pos = pos_emb(8, 4, 0.0);
        for v in &mut text_pos.data[..8] {
            *v = 7.0;
        }
        let text = tensor_3d(1, 2, 4, 2.0);
        let bos = tensor_3d(1, 1, 4, 3.0);

        let (data, seq, hidden) =
            build_first_step_embeds(&cond, &text_pos, &text, &bos, true).unwrap();

        assert_eq!(seq, 7);
        assert_eq!(hidden, 4);
        assert_eq!(data.len(), 56);
        assert_eq!(&data[..12], &[1.0; 12]);
        assert_eq!(&data[12..20], &[2.0; 8]);
        assert_eq!(&data[20..28], &[3.0; 8]);
        assert_eq!(&data[28..40], &[1.0; 12]);
        assert_eq!(&data[40..48], &[7.0; 8]);
        assert_eq!(&data[48..56], &[3.0; 8]);
    }

    #[test]
    fn text_position_slice_returns_leading_chunk() {
        let pos = pos_emb(10, 4, 0.5);
        let s = text_position_slice(&pos, 3, 4).unwrap();
        assert_eq!(s.len(), 12);
        assert!(s.iter().all(|&v| v == 0.5));
    }

    #[test]
    fn text_position_slice_rejects_non_rank_2() {
        let pos = TensorData {
            data: vec![0.0; 8],
            shape: vec![8],
        };
        let err = text_position_slice(&pos, 1, 4).unwrap_err().to_string();
        assert!(err.contains("rank-2"));
    }

    #[test]
    fn text_position_slice_rejects_hidden_dim_mismatch() {
        let pos = pos_emb(10, 8, 0.0);
        let err = text_position_slice(&pos, 1, 4).unwrap_err().to_string();
        assert!(err.contains("hidden size mismatch"));
    }

    #[test]
    fn text_position_slice_rejects_too_short() {
        let pos = pos_emb(2, 4, 0.0);
        let err = text_position_slice(&pos, 5, 4).unwrap_err().to_string();
        assert!(err.contains("too short"));
    }
}

#[derive(Clone, Debug)]
pub struct RoestConfig {
    pub model_dir: PathBuf,
    pub language: String,
    pub sampling: TtsSampling,
    pub sample_rate: u32,
}

impl RoestConfig {
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        Self {
            model_dir: model_dir.into(),
            language: "da".into(),
            sampling: TtsSampling::default(),
            sample_rate: DEFAULT_SAMPLE_RATE,
        }
    }

    pub fn with_sampling(mut self, sampling: TtsSampling) -> Self {
        self.sampling = sampling;
        self
    }
}
