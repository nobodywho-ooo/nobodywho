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
use crate::tts::instrumentation::GenerationTimings;
use crate::tts::ort_util::{
    self, detect_num_layers, has_position_ids, KvCacheLayout, TensorData,
};
use crate::tts::sampling::{self, SamplingParams, SystemRng};
use crate::tts::TtsDevice;
use ort::session::{Session, SessionInputValue, SessionInputs};
use ort::value::{Tensor, Value};
use std::borrow::Cow;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;
use tracing::info;
use unicode_normalization::UnicodeNormalization;

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

/// Speaker-conditioning tensors for a reference voice. Either loaded from the
/// model directory's `default_cond/` or produced on the fly by the speech
/// encoder ONNX graph.
struct PrecomputedCond {
    /// Hidden-state prefix prepended to the LM input sequence.
    cond_emb: TensorData<f32>,
    /// Reference speech tokens concatenated in front of generated tokens
    /// before the conditional decoder stage.
    prompt_token: TensorData<i64>,
    /// Speaker x-vector fed to the conditional decoder.
    ref_x_vector: TensorData<f32>,
    /// Mel-feature prompt fed to the conditional decoder.
    prompt_feat: TensorData<f32>,
}

pub(crate) struct ChatterboxModel {
    speech_encoder: Mutex<Session>,
    embed_tokens: Mutex<Session>,
    language_model: Mutex<Session>,
    conditional_decoder: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    /// Optional pre-computed conditioning. When present the speech_encoder
    /// session is never run.
    precomputed_cond: Option<PrecomputedCond>,
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

        let precomputed_cond = load_precomputed_cond(&model_dir.join("default_cond"));
        let text_token_offset = read_text_token_offset(model_dir);

        info!(
            num_layers,
            num_kv_heads = NUM_KV_HEADS,
            head_dim = HEAD_DIM,
            has_precomputed = precomputed_cond.is_some(),
            "Loaded Chatterbox TTS (4 ONNX sessions)"
        );

        Ok(Self {
            speech_encoder: Mutex::new(speech_encoder),
            embed_tokens: Mutex::new(embed_tokens),
            language_model: Mutex::new(language_model),
            conditional_decoder: Mutex::new(conditional_decoder),
            tokenizer,
            precomputed_cond,
            text_token_offset,
            num_layers,
            embed_has_position_ids,
            lm_has_position_ids,
        })
    }

    pub fn synthesize(
        &self,
        text: &str,
        language: &str,
        reference_audio: Option<&[f32]>,
        sampling: &SamplingParams,
    ) -> Result<Vec<f32>, TtsError> {
        let prepared_text = prepare_text(text, language);
        let (input_ids, position_ids) = self.tokenize_for_lm(&prepared_text)?;

        info!(tokens = input_ids.len(), text = prepared_text, "Chatterbox: tokenized");

        let cond = self.obtain_conditioning(reference_audio)?;

        info!(
            cond_emb_shape = ?cond.cond_emb.shape,
            prompt_token_shape = ?cond.prompt_token.shape,
            "Chatterbox: speaker conditioning ready"
        );

        let speech_tokens = self.generate_speech_tokens(&input_ids, &position_ids, &cond, sampling)?;

        let mut full_speech_tokens = cond.prompt_token.data.clone();
        full_speech_tokens.extend_from_slice(&speech_tokens);

        info!(
            generated = speech_tokens.len(),
            total = full_speech_tokens.len(),
            "Chatterbox: speech tokens generated"
        );

        let pcm = self.decode_speech(&full_speech_tokens, &cond)?;

        info!(
            samples = pcm.len(),
            duration_secs = pcm.len() as f32 / S3GEN_SR as f32,
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
        &self,
        reference_audio: Option<&[f32]>,
    ) -> Result<PrecomputedCond, TtsError> {
        if let Some(pc) = &self.precomputed_cond {
            info!("Using pre-computed conditioning");
            return Ok(PrecomputedCond {
                cond_emb: pc.cond_emb.clone(),
                prompt_token: pc.prompt_token.clone(),
                ref_x_vector: pc.ref_x_vector.clone(),
                prompt_feat: pc.prompt_feat.clone(),
            });
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

        let mut session = self
            .speech_encoder
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;

        let outputs = session
            .run(SessionInputs::from(vec![(
                Cow::Borrowed("audio_values"),
                SessionInputValue::Owned(Value::from(audio_tensor)),
            )]))
            .map_err(|e| TtsError::Synthesis(format!("speech encoder: {e}")))?;

        Ok(PrecomputedCond {
            cond_emb: TensorData::<f32>::extract(&outputs[0], "cond_emb")?,
            prompt_token: TensorData::<i64>::extract(&outputs[1], "prompt_token")?,
            ref_x_vector: TensorData::<f32>::extract(&outputs[2], "ref_x_vector")?,
            prompt_feat: TensorData::<f32>::extract(&outputs[3], "prompt_feat")?,
        })
    }

    fn generate_speech_tokens(
        &self,
        input_ids: &[i64],
        position_ids: &[i64],
        cond: &PrecomputedCond,
        sampling_params: &SamplingParams,
    ) -> Result<Vec<i64>, TtsError> {
        let use_cfg = sampling_params.cfg_weight > 0.0;
        let kv_layout = KvCacheLayout {
            num_layers: self.num_layers,
            num_kv_heads: NUM_KV_HEADS,
            head_dim: HEAD_DIM,
            batch: if use_cfg { 2 } else { 1 },
        };

        let mut embed_session = self
            .embed_tokens
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;
        let mut lm_session = self
            .language_model
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;

        let mut generated: Vec<i64> = vec![START_SPEECH_TOKEN];
        let mut kv_cache: Vec<Vec<f32>> = vec![Vec::new(); kv_layout.slot_count()];
        let mut kv_seq_len: usize = 0;
        let mut attention_mask: Vec<i64> = Vec::new();
        let mut rng = SystemRng;
        let mut timings = GenerationTimings::default();

        for step in 0..DEFAULT_MAX_NEW_TOKENS {
            let step_start = Instant::now();

            let (ids, positions) = if step == 0 {
                (input_ids.to_vec(), position_ids.to_vec())
            } else {
                (vec![*generated.last().unwrap()], vec![step as i64])
            };

            let token_embeds = self.run_embed(&mut embed_session, &ids, &positions)?;
            timings.embed += step_start.elapsed();

            let (lm_embeds, lm_seq_len, hidden_dim) = if step == 0 {
                build_first_step_embeds(&cond.cond_emb, &token_embeds, use_cfg)
            } else {
                build_continuation_embeds(&token_embeds, use_cfg)
            };

            if step == 0 {
                attention_mask = vec![1; lm_seq_len];
            } else {
                attention_mask.push(1);
            }

            let lm_start = Instant::now();
            let logits = self.run_language_model(
                &mut lm_session,
                &kv_layout,
                lm_embeds,
                lm_seq_len,
                hidden_dim,
                &attention_mask,
                &mut kv_cache,
                kv_seq_len,
            )?;
            timings.lm += lm_start.elapsed();

            let sample_start = Instant::now();
            let mut final_logits = collapse_logits(&logits, use_cfg, sampling_params.cfg_weight);
            sampling::apply_repetition_penalty(
                &mut final_logits,
                &generated,
                REPETITION_PENALTY,
                false,
            );
            let next_token = sampling::sample_token(&mut final_logits, sampling_params, &mut rng) as i64;
            generated.push(next_token);
            timings.sample += sample_start.elapsed();

            if next_token == STOP_SPEECH_TOKEN {
                break;
            }

            kv_seq_len += lm_seq_len;

            if step % 50 == 0 && step > 0 {
                info!(step, "Chatterbox: generation progress");
            }
        }

        info!(
            generated = generated.len().saturating_sub(1),
            embed_elapsed = ?timings.embed,
            lm_elapsed = ?timings.lm,
            sample_elapsed = ?timings.sample,
            "Chatterbox: generation timings"
        );

        Ok(generated
            .into_iter()
            .filter(|&t| t != START_SPEECH_TOKEN && t != STOP_SPEECH_TOKEN)
            .collect())
    }

    /// Call the `embed_tokens` session to get hidden-state embeddings for a
    /// small batch of token IDs.
    fn run_embed(
        &self,
        session: &mut Session,
        ids: &[i64],
        positions: &[i64],
    ) -> Result<TensorData<f32>, TtsError> {
        let seq_len = ids.len();
        let ids_tensor = Tensor::from_array(([1, seq_len], ids.to_vec()))
            .map_err(|e| TtsError::Synthesis(format!("ids tensor: {e}")))?;

        let mut inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> = vec![(
            Cow::Borrowed("input_ids"),
            SessionInputValue::Owned(Value::from(ids_tensor)),
        )];
        if self.embed_has_position_ids {
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
        TensorData::<f32>::extract(&outputs[0], "inputs_embeds")
    }

    /// Run one LM step, updating `kv_cache` in place with the new `present_*`
    /// tensors and returning the freshly-extracted logits. Takes ownership of
    /// `lm_embeds` to avoid a final clone.
    #[allow(clippy::too_many_arguments)]
    fn run_language_model(
        &self,
        session: &mut Session,
        kv_layout: &KvCacheLayout,
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

        if self.lm_has_position_ids {
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

    fn decode_speech(
        &self,
        speech_tokens: &[i64],
        cond: &PrecomputedCond,
    ) -> Result<Vec<f32>, TtsError> {
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

        let inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> = vec![
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
        ];

        let mut session = self
            .conditional_decoder
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;

        let outputs = session
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

/// Continuation LM step: a single speech-token embedding, duplicated once
/// more for batch 1 when CFG is enabled.
fn build_continuation_embeds(
    token_embeds: &TensorData<f32>,
    use_cfg: bool,
) -> (Vec<f32>, usize, usize) {
    let hidden_dim = *token_embeds.shape.last().expect("embeds must be rank >= 1");
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
/// batch 1 is unconditioned, and we mix `cond + w × (cond − uncond)`.
/// Without CFG we simply take the last-position logits from batch 0.
fn collapse_logits(logits: &TensorData<f32>, use_cfg: bool, cfg_weight: f32) -> Vec<f32> {
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

fn load_precomputed_cond(dir: &Path) -> Option<PrecomputedCond> {
    let manifest = ort_util::read_cond_manifest(dir)?;
    Some(PrecomputedCond {
        cond_emb: ort_util::load_f32_tensor(dir, &manifest, "cond_emb")?,
        prompt_token: ort_util::load_i64_tensor(dir, &manifest, "prompt_token")?,
        ref_x_vector: ort_util::load_f32_tensor(dir, &manifest, "ref_x_vector")?,
        prompt_feat: ort_util::load_f32_tensor(dir, &manifest, "prompt_feat")?,
    })
}

fn read_text_token_offset(model_dir: &Path) -> i64 {
    let Ok(s) = std::fs::read_to_string(model_dir.join("model_config.json")) else {
        return DEFAULT_TEXT_TOKEN_OFFSET;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) else {
        return DEFAULT_TEXT_TOKEN_OFFSET;
    };
    v["text_token_offset"].as_i64().unwrap_or(DEFAULT_TEXT_TOKEN_OFFSET)
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
