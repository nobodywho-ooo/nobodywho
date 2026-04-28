//! Chatterbox Multilingual TTS inference via ONNX Runtime.
//!
//! Four-stage pipeline:
//!
//!   1. **Speech encoder** — reference audio → speaker conditioning tensors.
//!      Skipped when pre-computed conditioning is provided.
//!   2. **Embed tokens** — text token IDs → hidden-state embeddings.
//!   3. **Language model** — autoregressive Llama-style transformer with KV
//!      cache → next speech token logits.
//!   4. **Conditional decoder** — speech tokens + speaker features → PCM audio.
//!
//! Supports 23 languages including Danish, with voice cloning from a reference WAV.

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::ort_util::{
    self, build_continuation_embeds, collapse_logits, detect_num_layers, has_position_ids,
    KvCacheLayout, SpeakerConditioning, SpeechGenerationState, TensorData,
};
use crate::tts::sampling::{self, SamplingParams};
use crate::tts::{TtsDevice, TtsSampling, DEFAULT_SAMPLE_RATE};
use ort::session::{SessionInputValue, SessionInputs};
use ort::value::{Tensor, Value};
use std::borrow::Cow;
use std::path::Path;
use std::time::Instant;
use tracing::info;
use unicode_normalization::UnicodeNormalization;

pub(super) struct ChatterboxBackend {
    model: ChatterboxModel,
    reference_audio: Option<Vec<f32>>,
    language: String,
    sampling: TtsSampling,
}

impl ChatterboxBackend {
    pub fn new(
        model: ChatterboxModel,
        reference_audio: Option<Vec<f32>>,
        language: String,
        sampling: TtsSampling,
    ) -> Self {
        Self {
            model,
            reference_audio,
            language,
            sampling,
        }
    }
}

impl TtsBackendImpl for ChatterboxBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        let sampling = SamplingParams::from(&self.sampling);
        let samples = self.model.synthesize(
            text,
            &self.language,
            self.reference_audio.as_deref(),
            &sampling,
        )?;
        Ok((samples, DEFAULT_SAMPLE_RATE))
    }
}

/// Sample rate of the S3Gen conditional decoder — all Chatterbox audio is
/// produced at 24 kHz regardless of reference audio sample rate.
const S3GEN_SR: u32 = 24000;

/// Vocabulary ID marking the start of the speech-token span. The LM is
/// primed with `[cond | text | START_SPEECH]` and generation begins from there.
const START_SPEECH_TOKEN: i64 = 6561;
/// Vocabulary ID that terminates speech-token generation.
const STOP_SPEECH_TOKEN: i64 = 6562;

/// Safety bound on the autoregressive loop. Real utterances stop well before
/// this via `STOP_SPEECH_TOKEN`.
const DEFAULT_MAX_NEW_TOKENS: usize = 1000;

/// Divisor applied to logits of previously generated tokens. Matches the
/// upstream Chatterbox repro (2.0) — strong enough to kill verbatim repeats
/// without suppressing natural phoneme reuse.
const REPETITION_PENALTY: f32 = 2.0;

/// Default text-token offset for turbo / GPT-2 based models (multilingual
/// uses 8194; overridden via `model_config.json`).
const DEFAULT_TEXT_TOKEN_OFFSET: i64 = 6563;

/// Both multilingual (Llama) and turbo (GPT-2) variants use 16 KV heads of
/// dim 64 — hard-coded here rather than sniffed from the ONNX graph.
const NUM_KV_HEADS: usize = 16;
const HEAD_DIM: usize = 64;

pub(crate) struct ChatterboxModel {
    speech_encoder: ort::session::Session,
    embed_tokens: ort::session::Session,
    language_model: ort::session::Session,
    conditional_decoder: ort::session::Session,
    tokenizer: tokenizers::Tokenizer,
    /// Optional pre-computed conditioning. When present the speech_encoder
    /// session is never run.
    precomputed_cond: Option<SpeakerConditioning>,
    /// Offset added to text token IDs so `embed_tokens` routes them through
    /// the text embedding table instead of the speech embedding table.
    text_token_offset: i64,
    num_layers: usize,
    embed_has_position_ids: bool,
    lm_has_position_ids: bool,
}

impl ChatterboxModel {
    /// Load all 4 ONNX models + tokenizer from a directory.
    ///
    /// Expected layout:
    /// ```text
    /// dir/
    ///   tokenizer.json
    ///   model_config.json              (optional — overrides text_token_offset)
    ///   default_cond/                  (optional — pre-computed conditioning)
    ///   onnx/speech_encoder.onnx
    ///   onnx/embed_tokens.onnx
    ///   onnx/language_model*.onnx      (any quantization variant)
    ///   onnx/conditional_decoder.onnx
    /// ```
    pub fn new(model_dir: &Path, device: TtsDevice) -> Result<Self, TtsError> {
        let onnx_dir = model_dir.join("onnx");

        let tokenizer = tokenizers::Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|e| TtsError::Init(format!("failed to load tokenizer: {e}")))?;

        let speech_encoder =
            ort_util::load_session(&onnx_dir.join("speech_encoder.onnx"), device, false)?;
        let embed_tokens =
            ort_util::load_session(&onnx_dir.join("embed_tokens.onnx"), device, false)?;
        let language_model = ort_util::find_language_model(
            &onnx_dir,
            device,
            false,
            &[
                "language_model.onnx",
                "language_model_q4.onnx",
                "language_model_fp16.onnx",
                "language_model_q4f16.onnx",
            ],
        )?;
        let conditional_decoder =
            ort_util::load_session(&onnx_dir.join("conditional_decoder.onnx"), device, false)?;

        let num_layers = detect_num_layers(&language_model);
        let embed_has_position_ids = has_position_ids(&embed_tokens);
        let lm_has_position_ids = has_position_ids(&language_model);

        let precomputed_cond = load_precomputed_cond(&model_dir.join("default_cond"))?;
        let text_token_offset = read_text_token_offset(model_dir);

        info!(
            num_layers,
            num_kv_heads = NUM_KV_HEADS,
            head_dim = HEAD_DIM,
            has_precomputed = precomputed_cond.is_some(),
            "Loaded Chatterbox TTS (4 ONNX sessions)"
        );

        Ok(Self {
            speech_encoder,
            embed_tokens,
            language_model,
            conditional_decoder,
            tokenizer,
            precomputed_cond,
            text_token_offset,
            num_layers,
            embed_has_position_ids,
            lm_has_position_ids,
        })
    }

    pub(super) fn synthesize(
        &mut self,
        text: &str,
        language: &str,
        reference_audio: Option<&[f32]>,
        sampling: &SamplingParams,
    ) -> Result<Vec<f32>, TtsError> {
        let start = Instant::now();
        let prepared_text = prepare_text(text, language);
        let (input_ids, position_ids) = self.tokenize_for_lm(&prepared_text)?;
        let cond = self.obtain_conditioning(reference_audio)?;
        let speech_tokens =
            self.generate_speech_tokens(&input_ids, &position_ids, &cond, sampling)?;

        let mut full_speech_tokens = cond.prompt_token.data.clone();
        full_speech_tokens.extend_from_slice(&speech_tokens);

        let pcm = self.decode_speech(&full_speech_tokens, &cond)?;

        info!(
            input_tokens = input_ids.len(),
            speech_tokens = speech_tokens.len(),
            samples = pcm.len(),
            duration_secs = pcm.len() as f32 / S3GEN_SR as f32,
            elapsed = ?start.elapsed(),
            "Chatterbox: synthesis complete"
        );

        Ok(pcm)
    }

    /// Encode `prepared_text` into the `[text_ids..., START_SPEECH]` sequence
    /// the LM expects, plus the parallel position-id vector.
    ///
    /// Text tokens get positions `[0, N-1]` from the learned text embedding;
    /// the trailing `START_SPEECH` restarts at speech-position 0.
    fn tokenize_for_lm(&self, prepared_text: &str) -> Result<(Vec<i64>, Vec<i64>), TtsError> {
        let encoding = self
            .tokenizer
            .encode(prepared_text, true)
            .map_err(|e| TtsError::Synthesis(format!("tokenization failed: {e}")))?;
        let raw_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();

        // If any tokenizer output already lands in the speech range (multilingual
        // tokenizer), no offset is needed. Otherwise (turbo / GPT-2 tokenizer)
        // every ID must be shifted into the text range.
        let already_offset = raw_ids.iter().any(|&id| id >= self.text_token_offset);
        let mut input_ids: Vec<i64> = if already_offset {
            raw_ids
        } else {
            raw_ids
                .iter()
                .map(|&id| id + self.text_token_offset)
                .collect()
        };
        input_ids.push(START_SPEECH_TOKEN);

        let n_text = input_ids.len() - 1;
        let mut position_ids: Vec<i64> = (0..n_text as i64).collect();
        position_ids.push(0);

        Ok((input_ids, position_ids))
    }

    /// Reuse pre-computed conditioning if present, otherwise run the speech
    /// encoder session on the caller-provided reference audio.
    fn obtain_conditioning(
        &mut self,
        reference_audio: Option<&[f32]>,
    ) -> Result<SpeakerConditioning, TtsError> {
        if let Some(pc) = &self.precomputed_cond {
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

        let outputs = self
            .speech_encoder
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

    fn generate_speech_tokens(
        &mut self,
        input_ids: &[i64],
        position_ids: &[i64],
        cond: &SpeakerConditioning,
        sampling_params: &SamplingParams,
    ) -> Result<Vec<i64>, TtsError> {
        let use_cfg = sampling_params.cfg_weight > 0.0;
        let kv_layout = KvCacheLayout {
            num_layers: self.num_layers,
            num_kv_heads: NUM_KV_HEADS,
            head_dim: HEAD_DIM,
            batch: if use_cfg { 2 } else { 1 },
        };

        let embed_has_position_ids = self.embed_has_position_ids;
        let lm_has_position_ids = self.lm_has_position_ids;
        let embed_session = &mut self.embed_tokens;
        let lm_session = &mut self.language_model;

        let mut state =
            SpeechGenerationState::new(&kv_layout, START_SPEECH_TOKEN, DEFAULT_MAX_NEW_TOKENS, 0);

        for step in 0..DEFAULT_MAX_NEW_TOKENS {
            let step_start = Instant::now();

            let (ids, positions) = state.step_inputs(step, input_ids, position_ids);

            let token_embeds = ort_util::run_embed(
                embed_session,
                embed_has_position_ids,
                &ids,
                &positions,
                "inputs_embeds",
            )?;
            state.timings.embed += step_start.elapsed();

            let (lm_embeds, lm_seq_len, hidden_dim) = if step == 0 {
                build_first_step_embeds(&cond.cond_emb, &token_embeds, use_cfg)
            } else {
                build_continuation_embeds(&token_embeds, use_cfg)
            };

            state.update_attention(step, lm_seq_len);

            let lm_start = Instant::now();
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
            state.timings.lm += lm_start.elapsed();

            let sample_start = Instant::now();
            let mut final_logits = collapse_logits(&logits, use_cfg, sampling_params.cfg_weight);
            sampling::apply_repetition_penalty(
                &mut final_logits,
                &state.generated,
                REPETITION_PENALTY,
                false,
            );
            let next_token = sampling::sample_token(&mut final_logits, sampling_params) as i64;
            state.timings.sample += sample_start.elapsed();

            if state.accept_token(next_token, lm_seq_len, STOP_SPEECH_TOKEN) {
                break;
            }
        }

        info!(
            generated = state.generated_count(),
            embed_elapsed = ?state.timings.embed,
            lm_elapsed = ?state.timings.lm,
            sample_elapsed = ?state.timings.sample,
            "Chatterbox: generation timings"
        );

        Ok(state.output_tokens(START_SPEECH_TOKEN, STOP_SPEECH_TOKEN))
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
/// by the explicit `[SPACE]` token.
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

fn load_precomputed_cond(dir: &Path) -> Result<Option<SpeakerConditioning>, TtsError> {
    let Some(manifest) = ort_util::read_cond_manifest(dir)? else {
        return Ok(None);
    };
    Ok(Some(SpeakerConditioning {
        cond_emb: ort_util::load_tensor(dir, &manifest, "cond_emb")?,
        prompt_token: ort_util::load_tensor(dir, &manifest, "prompt_token")?,
        ref_x_vector: ort_util::load_tensor(dir, &manifest, "ref_x_vector")?,
        prompt_feat: ort_util::load_tensor(dir, &manifest, "prompt_feat")?,
    }))
}

fn read_text_token_offset(model_dir: &Path) -> i64 {
    let Ok(s) = std::fs::read_to_string(model_dir.join("model_config.json")) else {
        return DEFAULT_TEXT_TOKEN_OFFSET;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else {
        return DEFAULT_TEXT_TOKEN_OFFSET;
    };
    v["text_token_offset"]
        .as_i64()
        .unwrap_or(DEFAULT_TEXT_TOKEN_OFFSET)
}

/// Load a WAV file and resample to `S3GEN_SR` mono f32 samples.
pub(crate) fn load_reference_audio(wav_path: &Path) -> Result<Vec<f32>, TtsError> {
    let reader = hound::WavReader::open(wav_path)
        .map_err(|e| TtsError::Init(format!("failed to open reference WAV: {e}")))?;

    let spec = reader.spec();
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

    let mono: Vec<f32> = if channels > 1 {
        samples
            .chunks(channels)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        samples
    };

    if spec.sample_rate == S3GEN_SR {
        Ok(mono)
    } else {
        Ok(resample(&mono, spec.sample_rate, S3GEN_SR))
    }
}

/// Linear-interpolation resampler. Sufficient for reference audio, which is
/// short and only feeds the speech encoder.
fn resample(samples: &[f32], from_rate: u32, to_rate: u32) -> Vec<f32> {
    if from_rate == to_rate || samples.is_empty() {
        return samples.to_vec();
    }
    let ratio = from_rate as f64 / to_rate as f64;
    let out_len = ((samples.len() as f64) / ratio).ceil() as usize;
    let mut output = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let src_pos = i as f64 * ratio;
        let idx = src_pos as usize;
        let frac = src_pos - idx as f64;
        let sample = if idx + 1 < samples.len() {
            samples[idx] as f64 * (1.0 - frac) + samples[idx + 1] as f64 * frac
        } else {
            samples[idx.min(samples.len() - 1)] as f64
        };
        output.push(sample as f32);
    }
    output
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
        // batch=2 → 40 floats.
        assert_eq!(data.len(), 40);
        // Conditioned branch.
        assert_eq!(&data[..8], &[1.0; 8]);
        assert_eq!(&data[8..20], &[2.0; 12]);
        // Unconditioned branch: cond + zeros for text positions.
        assert_eq!(&data[20..28], &[1.0; 8]);
        assert_eq!(&data[28..40], &[0.0; 12]);
    }

    #[test]
    fn resample_passthrough_when_rates_equal() {
        let samples = vec![0.1, 0.2, 0.3];
        assert_eq!(resample(&samples, 16000, 16000), samples);
    }

    #[test]
    fn resample_empty_returns_empty() {
        assert!(resample(&[], 16000, 24000).is_empty());
    }
}
