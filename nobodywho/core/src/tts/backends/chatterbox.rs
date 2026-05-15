//! Chatterbox Multilingual TTS inference via ONNX Runtime.
//!
//! Four-stage pipeline:
//!
//!   1. **Speech encoder** — reference audio → speaker conditioning tensors.
//!      Skipped when pre-computed conditioning is provided.
//!   2. **Embed tokens** — text token IDs → embeddings.
//!   3. **Language model** — autoregressive Llama-style transformer with KV
//!      cache → next speech token logits.
//!   4. **Conditional decoder** — speech tokens + speaker features → PCM audio.
//!
//! Supports 23 languages including Danish, with voice cloning from a reference WAV.
//!
//! Model-specific token IDs, offsets, and generation bounds are read from
//! `model_config.json` in the model directory; see [`ChatterboxModelConfig`] for
//! the fields and their defaults.

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::ort_util::{
    self, build_continuation_embeds, collapse_logits, detect_num_layers, has_position_ids,
    KvCacheLayout, SpeakerConditioning, SpeechGenerationState, TensorData,
};
use crate::tts::sampling;
use crate::tts::{TtsDevice, TtsSampling, DEFAULT_SAMPLE_RATE};
use ort::session::{Session, SessionInputValue, SessionInputs};
use ort::value::{Tensor, Value};
use serde::Deserialize;
use std::borrow::Cow;
use std::path::{Path, PathBuf};

const SPEECH_ENCODER_FILE: &str = "speech_encoder.onnx";
const EMBED_TOKENS_FILE: &str = "embed_tokens.onnx";
const LANGUAGE_MODEL_FILE: &str = "language_model.onnx";
const CONDITIONAL_DECODER_FILE: &str = "conditional_decoder.onnx";
const TOKENIZER_FILE: &str = "tokenizer.json";
use tracing::{debug, info};
use unicode_normalization::UnicodeNormalization;

pub(in crate::tts) struct ChatterboxBackend {
    speech_encoder: Session,
    embed_tokens: Session,
    language_model: Session,
    conditional_decoder: Session,
    tokenizer: tokenizers::Tokenizer,
    precomputed_cond: Option<SpeakerConditioning>,
    reference_audio: Option<Vec<f32>>,
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
    embed_has_position_ids: bool,
    embed_has_exaggeration: bool,
    lm_has_position_ids: bool,
}

impl ChatterboxBackend {
    pub fn new(
        model_dir: &Path,
        reference_wav: Option<&Path>,
        language: String,
        sampling: TtsSampling,
        sample_rate: u32,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let model_config = read_model_config(model_dir);

        let tokenizer = tokenizers::Tokenizer::from_file(model_dir.join(TOKENIZER_FILE))
            .map_err(|e| TtsError::Init(format!("chatterbox: load tokenizer: {e}")))?;

        let speech_encoder =
            ort_util::load_session(&model_dir.join(SPEECH_ENCODER_FILE), device, false)?;
        let embed_tokens =
            ort_util::load_session(&model_dir.join(EMBED_TOKENS_FILE), device, false)?;
        let language_model =
            ort_util::load_session(&model_dir.join(LANGUAGE_MODEL_FILE), device, false)?;
        let conditional_decoder =
            ort_util::load_session(&model_dir.join(CONDITIONAL_DECODER_FILE), device, false)?;

        let num_layers = detect_num_layers(&language_model);
        let (num_kv_heads, head_dim) = ort_util::detect_kv_dims(&language_model)?;
        let embed_has_position_ids = has_position_ids(&embed_tokens);
        let embed_has_exaggeration = ort_util::has_exaggeration(&embed_tokens);
        let lm_has_position_ids = has_position_ids(&language_model);

        let precomputed_cond = load_precomputed_cond(&model_dir.join("conditioning.safetensors"))?;
        let reference_audio = resolve_reference_audio(model_dir, reference_wav, sample_rate)?;

        info!(
            num_layers,
            num_kv_heads,
            head_dim,
            start_text_token = model_config.start_text_token,
            stop_text_token = model_config.stop_text_token,
            start_speech_token = model_config.start_speech_token,
            stop_speech_token = model_config.stop_speech_token,
            has_precomputed = precomputed_cond.is_some(),
            "Loaded Chatterbox TTS"
        );

        Ok(Self {
            speech_encoder,
            embed_tokens,
            language_model,
            conditional_decoder,
            tokenizer,
            precomputed_cond,
            reference_audio,
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
            embed_has_position_ids,
            embed_has_exaggeration,
            lm_has_position_ids,
        })
    }
}

impl TtsBackendImpl for ChatterboxBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        let sampling = self.sampling.clone();
        let prepared_text = prepare_text(text, &self.language);
        let (input_ids, position_ids) = self.tokenize_for_lm(&prepared_text)?;
        let cond = obtain_conditioning(
            &self.precomputed_cond,
            &mut self.speech_encoder,
            self.reference_audio.as_deref(),
        )?;
        let speech_tokens =
            self.generate_speech_tokens(&input_ids, &position_ids, &cond, &sampling)?;

        let prompt_token_count = cond.prompt_token.data.len();
        let mut full_speech_tokens = cond.prompt_token.data.clone();
        full_speech_tokens.extend_from_slice(&speech_tokens);

        let pcm = self.decode_speech(&full_speech_tokens, &cond)?;

        debug!(
            text_tokens = input_ids.len(),
            prompt_tokens = prompt_token_count,
            generated_tokens = speech_tokens.len(),
            pcm_samples = pcm.len(),
            pcm_duration_s = pcm.len() as f32 / self.sample_rate as f32,
            "Chatterbox: decode complete"
        );

        Ok((pcm, self.sample_rate))
    }
}

impl ChatterboxBackend {
    fn tokenize_for_lm(&self, prepared_text: &str) -> Result<(Vec<i64>, Vec<i64>), TtsError> {
        let encoding = self
            .tokenizer
            .encode(prepared_text, true)
            .map_err(|e| TtsError::Synthesis(format!("tokenization failed: {e}")))?;

        // Mirror the reference tts.py padding:
        //   text_tokens = F.pad(text_tokens, (1, 0), value=sot)   # prepend 255
        //   text_tokens = F.pad(text_tokens, (0, 1), value=eot)   # append 0
        // Followed by start_speech_token (6561) to trigger speech generation.
        let mut input_ids: Vec<i64> = vec![self.start_text_token];
        input_ids.extend(encoding.get_ids().iter().map(|&id| id as i64));
        input_ids.push(self.stop_text_token);
        input_ids.push(self.start_speech_token);

        // Text tokens (including SOT/EOT) get sequential positions.
        // start_speech_token is the first speech token, position 0 in speech space.
        let n_text = input_ids.len() - 1;
        let mut position_ids: Vec<i64> = (0..n_text as i64).collect();
        position_ids.push(0);

        Ok((input_ids, position_ids))
    }

    fn generate_speech_tokens(
        &mut self,
        input_ids: &[i64],
        position_ids: &[i64],
        cond: &SpeakerConditioning,
        sampling_params: &TtsSampling,
    ) -> Result<Vec<i64>, TtsError> {
        let use_cfg = sampling_params.cfg_weight > 0.0;
        let kv_layout = KvCacheLayout {
            num_layers: self.num_layers,
            num_kv_heads: self.num_kv_heads,
            head_dim: self.head_dim,
            batch: if use_cfg { 2 } else { 1 },
        };

        let embed_has_position_ids = self.embed_has_position_ids;
        let embed_has_exaggeration = self.embed_has_exaggeration;
        let lm_has_position_ids = self.lm_has_position_ids;
        let embed_session = &mut self.embed_tokens;
        let lm_session = &mut self.language_model;

        let mut state =
            SpeechGenerationState::new(&kv_layout, self.start_speech_token, self.max_new_tokens, 0);

        for step in 0..self.max_new_tokens {
            let (ids, positions) = state.step_inputs(step, input_ids, position_ids);

            let token_embeds = ort_util::run_embed(
                embed_session,
                embed_has_position_ids,
                embed_has_exaggeration,
                &ids,
                &positions,
                "inputs_embeds",
            )?;

            let (lm_embeds, lm_seq_len, hidden_dim) = if step == 0 {
                build_first_step_embeds(&cond.cond_emb, &token_embeds, use_cfg)
            } else {
                build_continuation_embeds(&token_embeds, use_cfg)
            };

            state.update_attention(step, lm_seq_len);

            let logits = ort_util::run_language_model(
                lm_session,
                &kv_layout,
                lm_has_position_ids,
                lm_embeds,
                lm_seq_len,
                hidden_dim,
                &state.attention_mask,
                &mut state.kv_cache,
                state.kv_seq_len,
            )?;

            let mut final_logits = collapse_logits(&logits, use_cfg, sampling_params.cfg_weight);

            // Trace the stop-token logit before penalty so we can see if it is
            // ever close to being sampled.
            if (self.stop_speech_token as usize) < final_logits.len() {
                tracing::trace!(
                    step,
                    stop_logit = final_logits[self.stop_speech_token as usize],
                    "chatterbox: stop speech token logit (pre-penalty)"
                );
            }

            sampling::apply_repetition_penalty(
                &mut final_logits,
                &state.generated,
                sampling_params.repetition_penalty,
                false,
            );
            let next_token = sampling::sample_token(&mut final_logits, sampling_params) as i64;

            if state.accept_token(next_token, lm_seq_len, &[self.stop_speech_token]) {
                info!(steps = step + 1, "Chatterbox: EOS reached");
                break;
            }
        }

        if state.generated_count() >= self.max_new_tokens {
            tracing::warn!(
                max_new_tokens = self.max_new_tokens,
                "Chatterbox: reached max_new_tokens without EOS — output is truncated"
            );
        }

        Ok(state.output_tokens(self.start_speech_token, &[self.stop_speech_token]))
    }

    fn decode_speech(
        &mut self,
        speech_tokens: &[i64],
        cond: &SpeakerConditioning,
    ) -> Result<Vec<f32>, TtsError> {
        let inputs = ort_util::decoder_inputs(speech_tokens, cond)?;
        let outputs = self
            .conditional_decoder
            .run(SessionInputs::from(inputs))
            .map_err(|e| TtsError::Synthesis(format!("conditional decoder: {e}")))?;
        Ok(TensorData::<f32>::extract(&outputs[0], "wav")?.data)
    }
}

/// Preprocess text to match the MTLTokenizer pipeline used during training:
/// NFKD-normalized lowercase, language-tagged, and with ASCII spaces replaced
/// by `[SPACE]` https://github.com/resemble-ai/chatterbox/blob/3f35dfc8fbe63e5b29793289dc68f1875bb317a5/src/chatterbox/models/tokenizers/tokenizer.py#L286
fn prepare_text(text: &str, language: &str) -> String {
    let normalized: String = text.to_lowercase().nfkd().collect();
    let with_lang = if language.is_empty() {
        normalized
    } else {
        format!("[{}]{}", language.to_lowercase(), normalized)
    };
    with_lang.replace(' ', "[SPACE]")
}

/// First LM step: `[cond_emb | text_embeds]` for batch 0, and when CFG is
/// enabled `[cond_emb | 0...0]` for batch 1 (unconditioned). Returns
/// `(packed_data, seq_len, hidden_dim)`.
fn build_first_step_embeds(
    cond_emb: &TensorData<f32>,
    text_embeds: &TensorData<f32>,
    use_cfg: bool,
) -> (Vec<f32>, usize, usize) {
    let hidden_dim = *text_embeds.shape.last().expect("embeds must be rank >= 1");
    let cond_seq = cond_emb.shape[1];
    let text_seq = text_embeds.shape[1];
    let total_seq = cond_seq + text_seq;
    let batch = if use_cfg { 2 } else { 1 };

    let mut data = Vec::with_capacity(batch * total_seq * hidden_dim);
    data.extend_from_slice(&cond_emb.data);
    data.extend_from_slice(&text_embeds.data);

    if use_cfg {
        data.extend_from_slice(&cond_emb.data);
        data.extend(std::iter::repeat_n(0.0f32, text_seq * hidden_dim));
    }

    (data, total_seq, hidden_dim)
}

/// Reuse pre-computed conditioning if present,
/// otherwise run the speech encoder on the provided reference audio.
fn obtain_conditioning(
    precomputed_cond: &Option<SpeakerConditioning>,
    speech_encoder: &mut Session,
    reference_audio: Option<&[f32]>,
) -> Result<SpeakerConditioning, TtsError> {
    if let Some(pc) = precomputed_cond {
        return Ok(pc.clone());
    }

    let samples = reference_audio.ok_or_else(|| {
        TtsError::Synthesis(
            "reference audio is required for Chatterbox \
             (pass reference_audio or place default_voice.wav in model dir)"
                .into(),
        )
    })?;

    let audio_tensor = Tensor::from_array(([1, samples.len()], samples.to_vec()))
        .map_err(|e| TtsError::Synthesis(format!("audio tensor: {e}")))?;

    let outputs = speech_encoder
        .run(SessionInputs::from(vec![(
            Cow::Borrowed("audio_values"),
            SessionInputValue::Owned(Value::from(audio_tensor)),
        )]))
        .map_err(|e| TtsError::Synthesis(format!("speech encoder: {e}")))?;

    Ok(SpeakerConditioning {
        cond_emb: TensorData::<f32>::extract(&outputs[0], "cond_emb")?,
        prompt_token: TensorData::<i64>::extract(&outputs[1], "prompt_token")?,
        ref_x_vector: TensorData::<f32>::extract(&outputs[2], "ref_x_vector")?,
        prompt_feat: TensorData::<f32>::extract(&outputs[3], "prompt_feat")?,
    })
}

fn load_precomputed_cond(path: &Path) -> Result<Option<SpeakerConditioning>, TtsError> {
    if !path.exists() {
        return Ok(None);
    }
    let st = ort_util::SafeTensorsFile::open(path, "chatterbox")?;
    Ok(Some(SpeakerConditioning {
        cond_emb: st.f32("cond_emb", "chatterbox")?,
        prompt_token: st.i64("prompt_token", "chatterbox")?,
        ref_x_vector: st.f32("ref_x_vector", "chatterbox")?,
        prompt_feat: st.f32("prompt_feat", "chatterbox")?,
    }))
}

/// Resolve the reference audio for a Chatterbox load: prefer an explicitly
/// provided WAV, otherwise fall back to `default_voice.wav` in the model dir.
pub(super) fn resolve_reference_audio(
    model_dir: &Path,
    reference_wav: Option<&Path>,
    sample_rate: u32,
) -> Result<Option<Vec<f32>>, TtsError> {
    if let Some(path) = reference_wav {
        let samples = load_reference_audio(path, sample_rate)?;
        tracing::info!(
            samples = samples.len(),
            "Loaded reference audio for voice cloning"
        );
        return Ok(Some(samples));
    }

    let default_path = model_dir.join("default_voice.wav");
    if default_path.exists() {
        let samples = load_reference_audio(&default_path, sample_rate)?;
        tracing::info!(samples = samples.len(), "Loaded default reference voice");
        return Ok(Some(samples));
    }

    Ok(None)
}

/// Load a WAV file as mono f32 samples. The file must be at the expected `sample_rate`
/// and either mono or stereo (stereo is downmixed by averaging channels).
fn load_reference_audio(wav_path: &Path, sample_rate: u32) -> Result<Vec<f32>, TtsError> {
    let reader = hound::WavReader::open(wav_path)
        .map_err(|e| TtsError::Init(format!("chatterbox: open reference WAV: {e}")))?;

    let spec = reader.spec();

    if spec.sample_rate != sample_rate {
        return Err(TtsError::Init(format!(
            "chatterbox: reference WAV must be {} Hz, got {} Hz (resample before use)",
            sample_rate, spec.sample_rate
        )));
    }

    let channels = spec.channels as usize;

    let samples: Vec<f32> = match spec.sample_format {
        hound::SampleFormat::Int => {
            let max_val = (1i64 << (spec.bits_per_sample - 1)) as f32;
            reader
                .into_samples::<i32>()
                .filter_map(|s| s.ok())
                .map(|s| s as f32 / max_val)
                .collect()
        }
        hound::SampleFormat::Float => reader
            .into_samples::<f32>()
            .filter_map(|s| s.ok())
            .collect(),
    };

    if channels > 1 {
        Ok(samples
            .chunks(channels)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect())
    } else {
        Ok(samples)
    }
}

/// Missing file or missing keys use the defaults
/// matching the upstream multilingual Chatterbox export.
#[derive(Deserialize)]
#[serde(default)]
struct ChatterboxModelConfig {
    start_text_token: i64,
    stop_text_token: i64,
    start_speech_token: i64,
    stop_speech_token: i64,
    max_new_tokens: usize,
}

impl Default for ChatterboxModelConfig {
    fn default() -> Self {
        Self {
            start_text_token: 255,
            stop_text_token: 0,
            start_speech_token: 6561,
            stop_speech_token: 6562,
            max_new_tokens: 1000,
        }
    }
}

fn read_model_config(model_dir: &Path) -> ChatterboxModelConfig {
    let Ok(s) = std::fs::read_to_string(model_dir.join("model_config.json")) else {
        return ChatterboxModelConfig::default();
    };
    serde_json::from_str(&s).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn prepare_text_lowercases_and_tags_language() {
        assert_eq!(prepare_text("Hej Verden", "DA"), "[da]hej[SPACE]verden");
    }

    #[test]
    fn prepare_text_no_tag_when_language_empty() {
        assert_eq!(prepare_text("Hej", ""), "hej");
    }

    #[test]
    fn prepare_text_replaces_spaces_with_marker() {
        assert_eq!(prepare_text("a b c", "en"), "[en]a[SPACE]b[SPACE]c");
    }

    #[test]
    fn build_first_step_embeds_no_cfg_packs_cond_text() {
        let cond = TensorData {
            data: vec![1.0; 8],
            shape: vec![1, 2, 4],
        };
        let text = TensorData {
            data: vec![2.0; 12],
            shape: vec![1, 3, 4],
        };
        let (data, seq, hidden) = build_first_step_embeds(&cond, &text, false);
        assert_eq!(seq, 5);
        assert_eq!(hidden, 4);
        assert_eq!(data.len(), 20);
        assert_eq!(&data[..8], &[1.0; 8]);
        assert_eq!(&data[8..20], &[2.0; 12]);
    }

    #[test]
    fn build_first_step_embeds_with_cfg_zeros_uncond_text() {
        let cond = TensorData {
            data: vec![1.0; 8],
            shape: vec![1, 2, 4],
        };
        let text = TensorData {
            data: vec![2.0; 12],
            shape: vec![1, 3, 4],
        };
        let (data, seq, hidden) = build_first_step_embeds(&cond, &text, true);
        assert_eq!(seq, 5);
        assert_eq!(hidden, 4);
        assert_eq!(data.len(), 40);
        assert_eq!(&data[..8], &[1.0; 8]);
        assert_eq!(&data[8..20], &[2.0; 12]);
        assert_eq!(&data[20..28], &[1.0; 8]);
        assert_eq!(&data[28..40], &[0.0; 12]);
    }
}

#[derive(Clone, Debug)]
pub struct ChatterboxConfig {
    pub model_dir: PathBuf,
    pub reference_wav: Option<PathBuf>,
    pub language: String,
    pub sampling: TtsSampling,
    pub sample_rate: u32,
}

impl ChatterboxConfig {
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        Self {
            model_dir: model_dir.into(),
            reference_wav: None,
            language: "en-us".into(),
            sampling: TtsSampling::default(),
            sample_rate: DEFAULT_SAMPLE_RATE,
        }
    }

    pub fn with_sampling(mut self, sampling: TtsSampling) -> Self {
        self.sampling = sampling;
        self
    }
}
