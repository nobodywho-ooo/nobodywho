//! Røst TTS — Danish-finetuned Chatterbox via ONNX Runtime.
//!
//! Røst reuses Chatterbox's ONNX graphs for the conditional decoder (speech
//! tokens → PCM) but has its own finetuned `embed_tokens` and `language_model`
//! exports. The finetuned `cond_enc` weights were fused into the base export
//! before the torch → ONNX conversion, so Røst cannot run the speech encoder
//! — the pre-computed conditioning in `default_cond/` is mandatory.
//!
//! Expected model directory layout:
//!
//! ```text
//! dir/
//!   tokenizer.json                 — Røst MTLTokenizer (post-processor stripped)
//!   model_config.json              — {"text_token_offset": N, "text_pos_emb_shape": [...]}
//!   text_pos_emb.bin               — learned text position embeddings (f32, row-major)
//!   default_cond/                  — pre-computed conditioning (manifest.json + .bin)
//!   onnx/embed_tokens.onnx         — exported from Røst
//!   onnx/language_model.onnx       — exported from Røst
//!   onnx/conditional_decoder.onnx  — from base Chatterbox (s3gen unchanged)
//! ```
//!
//! The generation loop also honors several debug env vars — see
//! `instrumentation.rs` for details.

use crate::errors::TtsError;
use crate::tts::instrumentation::{
    self, DebugSampler, FirstStepDump, GenerationTimings, StepDump,
};
use crate::tts::ort_util::{
    self, detect_num_layers, has_position_ids, KvCacheLayout, TensorData,
};
use crate::tts::sampling::{self, SamplingParams};
use crate::tts::TtsDevice;
use ort::session::{Session, SessionInputValue, SessionInputs};
use ort::value::{Tensor, Value};
use std::borrow::Cow;
use std::path::Path;
use std::sync::Mutex;
use std::time::Instant;
use tracing::info;
use unicode_normalization::UnicodeNormalization;

/// Sample rate of the S3Gen decoder (shared with base Chatterbox).
const S3GEN_SR: u32 = 24000;

/// Vocabulary IDs around the text span. `SOT`/`EOT` are added before the
/// `text_token_offset` shift, matching the torch multilingual wrapper.
const START_TEXT_TOKEN: i64 = 255;
const STOP_TEXT_TOKEN: i64 = 0;

/// Speech-token delimiters — the LM begins generation at `START_SPEECH_TOKEN`
/// and stops when it emits `STOP_SPEECH_TOKEN`.
const START_SPEECH_TOKEN: i64 = 6561;
const STOP_SPEECH_TOKEN: i64 = 6562;

/// Safety bound on the autoregressive loop. Real utterances stop well before
/// this via `STOP_SPEECH_TOKEN`.
const DEFAULT_MAX_NEW_TOKENS: usize = 1000;

/// Divisor applied to logits of previously generated tokens. Matches upstream.
const REPETITION_PENALTY: f32 = 2.0;

/// Both multilingual (Llama) and turbo (GPT-2) variants use 16 KV heads of
/// dim 64 — hard-coded rather than sniffed from the ONNX graph.
const NUM_KV_HEADS: usize = 16;
const HEAD_DIM: usize = 64;

/// Speaker-conditioning tensors loaded from `default_cond/`. Røst cannot
/// produce these on the fly because its speech encoder isn't exported.
struct PrecomputedCond {
    cond_emb: TensorData<f32>,
    prompt_token: TensorData<i64>,
    ref_x_vector: TensorData<f32>,
    prompt_feat: TensorData<f32>,
}

/// Fields parsed out of `model_config.json`.
struct ModelConfig {
    text_token_offset: i64,
    text_pos_emb_shape: Vec<usize>,
}

pub(crate) struct RoestModel {
    embed_tokens: Mutex<Session>,
    language_model: Mutex<Session>,
    conditional_decoder: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    cond: PrecomputedCond,
    text_token_offset: i64,
    /// Pre-loaded text position embeddings, used to build the unconditioned
    /// CFG branch (the torch reference reads these from a learned embedding
    /// layer that was dropped from the ONNX export).
    text_pos_emb: TensorData<f32>,
    num_layers: usize,
    embed_has_position_ids: bool,
}

impl RoestModel {
    pub fn new(model_dir: &Path, device: TtsDevice) -> Result<Self, TtsError> {
        let onnx_dir = model_dir.join("onnx");

        let tokenizer = tokenizers::Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|e| TtsError::Init(format!("failed to load tokenizer: {e}")))?;

        // Røst's ONNX graphs break under ORT's default graph fusion passes —
        // load them with optimization disabled.
        let embed_tokens = ort_util::load_session(&onnx_dir.join("embed_tokens.onnx"), device, true)?;
        let language_model = ort_util::find_language_model(
            &onnx_dir,
            device,
            true,
            &[
                "language_model.onnx",
                "language_model_q4.onnx",
                "language_model_fp16.onnx",
            ],
        )?;
        let conditional_decoder =
            ort_util::load_session(&onnx_dir.join("conditional_decoder.onnx"), device, true)?;

        let num_layers = detect_num_layers(&language_model);
        let embed_has_position_ids = has_position_ids(&embed_tokens);

        let cond = load_precomputed_cond(&model_dir.join("default_cond")).ok_or_else(|| {
            TtsError::Init("missing default_cond/ directory with pre-computed conditioning".into())
        })?;
        let config = load_model_config(model_dir)?;
        let text_pos_emb = load_text_pos_emb(model_dir, &config.text_pos_emb_shape)?;

        info!(
            num_layers,
            text_token_offset = config.text_token_offset,
            cond_seq = cond.cond_emb.shape[1],
            "Loaded Røst TTS"
        );

        Ok(Self {
            embed_tokens: Mutex::new(embed_tokens),
            language_model: Mutex::new(language_model),
            conditional_decoder: Mutex::new(conditional_decoder),
            tokenizer,
            cond,
            text_token_offset: config.text_token_offset,
            text_pos_emb,
            num_layers,
            embed_has_position_ids,
        })
    }

    pub fn synthesize(
        &self,
        text: &str,
        language: &str,
        sampling: &SamplingParams,
    ) -> Result<Vec<f32>, TtsError> {
        let synth_start = Instant::now();

        let prepared = prepare_text_for_mtl_tokenizer(text, language);
        let (text_input_ids, text_position_ids) = self.tokenize_for_lm(&prepared)?;
        let tokenized_at = Instant::now();

        info!(
            tokens = text_input_ids.len(),
            text,
            prepared = prepared.as_str(),
            elapsed = ?tokenized_at.duration_since(synth_start),
            "Røst: tokenized"
        );

        let generate_start = Instant::now();
        let speech_tokens =
            self.generate_speech_tokens(&text_input_ids, &text_position_ids, sampling)?;
        let generated_at = Instant::now();

        let mut full_tokens: Vec<i64> = self.cond.prompt_token.data.clone();
        full_tokens.extend_from_slice(&speech_tokens);

        info!(
            generated = speech_tokens.len(),
            total = full_tokens.len(),
            elapsed = ?generated_at.duration_since(generate_start),
            "Røst: speech tokens"
        );
        if std::env::var_os("NOBODYWHO_TTS_DEBUG_TOKENS").is_some() {
            info!(tokens = ?speech_tokens, "Røst: debug generated tokens");
        }

        let decode_start = Instant::now();
        let pcm = self.decode_speech(&full_tokens)?;
        let decoded_at = Instant::now();

        info!(
            samples = pcm.len(),
            duration_secs = pcm.len() as f32 / S3GEN_SR as f32,
            decode_elapsed = ?decoded_at.duration_since(decode_start),
            total_elapsed = ?decoded_at.duration_since(synth_start),
            "Røst: synthesis complete"
        );
        Ok(pcm)
    }

    /// Build the `[SOT, text..., EOT]` token sequence (each shifted by
    /// `text_token_offset`) plus its parallel position IDs. The speech span
    /// is appended inside `generate_speech_tokens` as an explicit BOS embed.
    fn tokenize_for_lm(&self, prepared_text: &str) -> Result<(Vec<i64>, Vec<i64>), TtsError> {
        let encoding = self
            .tokenizer
            .encode(prepared_text, false)
            .map_err(|e| TtsError::Synthesis(format!("tokenization failed: {e}")))?;
        let raw_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();

        let mut ids: Vec<i64> = Vec::with_capacity(raw_ids.len() + 2);
        ids.push(START_TEXT_TOKEN + self.text_token_offset);
        for &id in &raw_ids {
            ids.push(id + self.text_token_offset);
        }
        ids.push(STOP_TEXT_TOKEN + self.text_token_offset);

        let positions: Vec<i64> = (0..ids.len() as i64).collect();
        Ok((ids, positions))
    }

    fn generate_speech_tokens(
        &self,
        text_input_ids: &[i64],
        text_position_ids: &[i64],
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

        let bos_start = Instant::now();
        let bos_embeds = self.embed_bos_token(&mut embed_session)?;
        let bos_elapsed = bos_start.elapsed();

        let mut generated: Vec<i64> = Vec::with_capacity(DEFAULT_MAX_NEW_TOKENS + 1);
        generated.push(START_SPEECH_TOKEN);
        let mut kv_cache: Vec<Vec<f32>> = vec![Vec::new(); kv_layout.slot_count()];
        let mut kv_seq_len: usize = 0;
        let mut attention_mask: Vec<i64> = Vec::with_capacity(
            self.cond.cond_emb.shape[1] + text_input_ids.len() + DEFAULT_MAX_NEW_TOKENS,
        );
        let mut debug_sampler = DebugSampler::from_env()?;
        let mut timings = GenerationTimings::default();

        for step in 0..DEFAULT_MAX_NEW_TOKENS {
            let step_start = Instant::now();

            let (ids, positions) = if step == 0 {
                (text_input_ids.to_vec(), text_position_ids.to_vec())
            } else {
                (vec![*generated.last().unwrap()], vec![step as i64])
            };

            let token_embeds = self.run_embed(&mut embed_session, &ids, &positions)?;

            let (lm_embeds, lm_seq_len, hidden_dim) = if step == 0 {
                self.build_first_step_embeds(&token_embeds, &bos_embeds, use_cfg)?
            } else {
                build_continuation_embeds(&token_embeds, use_cfg)
            };
            timings.embed += step_start.elapsed();

            if step == 0 {
                attention_mask = vec![1; lm_seq_len];
            } else {
                attention_mask.push(1);
            }

            let dump_first_step = step == 0 && std::env::var_os("NOBODYWHO_TTS_DUMP_DIR").is_some();
            let dump_inputs_embeds = dump_first_step.then(|| lm_embeds.clone());
            let dump_attention_mask = dump_first_step.then(|| {
                (0..kv_layout.batch)
                    .flat_map(|_| attention_mask.iter().copied())
                    .collect::<Vec<i64>>()
            });

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

            if dump_first_step {
                instrumentation::maybe_dump_first_step(&FirstStepDump {
                    inputs_embeds: dump_inputs_embeds.as_deref().unwrap_or(&[]),
                    inputs_embeds_shape: [kv_layout.batch, lm_seq_len, hidden_dim],
                    attention_mask: dump_attention_mask.as_deref().unwrap_or(&[]),
                    attention_mask_shape: [kv_layout.batch, attention_mask.len()],
                    logits: &logits.data,
                    logits_shape: [kv_layout.batch, logits.shape[1], logits.shape[2]],
                    final_logits: &final_logits,
                })?;
            }

            sampling::apply_repetition_penalty(
                &mut final_logits,
                &generated,
                REPETITION_PENALTY,
                true,
            );

            let processed_logits = preview_sampled_logits(&final_logits, sampling_params);
            instrumentation::maybe_dump_step(&StepDump {
                step,
                logits: &logits.data,
                logits_shape: [kv_layout.batch, logits.shape[1], logits.shape[2]],
                processed_logits: &processed_logits,
                generated: &generated,
            })?;

            let next_token = if let Some(token) = debug_sampler.forced_token(step) {
                token
            } else {
                sampling::sample_token(&mut final_logits, sampling_params, &mut debug_sampler) as i64
            };
            generated.push(next_token);
            timings.sample += sample_start.elapsed();

            if next_token == STOP_SPEECH_TOKEN {
                break;
            }

            kv_seq_len += lm_seq_len;

            if step % 50 == 0 && step > 0 {
                info!(
                    step,
                    generated = generated.len().saturating_sub(1),
                    embed_elapsed = ?timings.embed,
                    lm_elapsed = ?timings.lm,
                    sample_elapsed = ?timings.sample,
                    "Røst: generation progress"
                );
            }
        }

        let generated_count = generated.len().saturating_sub(1);
        let loop_elapsed = timings.total();
        info!(
            generated = generated_count,
            bos_elapsed = ?bos_elapsed,
            embed_elapsed = ?timings.embed,
            lm_elapsed = ?timings.lm,
            sample_elapsed = ?timings.sample,
            loop_elapsed = ?loop_elapsed,
            tokens_per_sec = if loop_elapsed.is_zero() {
                0.0
            } else {
                generated_count as f64 / loop_elapsed.as_secs_f64()
            },
            "Røst: generation timings"
        );

        Ok(generated
            .into_iter()
            .filter(|&t| t != START_SPEECH_TOKEN && t != STOP_SPEECH_TOKEN)
            .collect())
    }

    /// Compute the embedding for the standalone `START_SPEECH` token. Used
    /// both as the trailing entry in the first-step sequence and as a
    /// replacement for the unconditioned text embedding on the CFG branch.
    fn embed_bos_token(&self, session: &mut Session) -> Result<TensorData<f32>, TtsError> {
        let ids_tensor = Tensor::from_array(([1, 1], vec![START_SPEECH_TOKEN]))
            .map_err(|e| TtsError::Synthesis(format!("bos ids tensor: {e}")))?;

        let mut inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> = vec![(
            Cow::Borrowed("input_ids"),
            SessionInputValue::Owned(Value::from(ids_tensor)),
        )];
        if self.embed_has_position_ids {
            let pos_tensor = Tensor::from_array(([1, 1], vec![0_i64]))
                .map_err(|e| TtsError::Synthesis(format!("bos pos tensor: {e}")))?;
            inputs.push((
                Cow::Borrowed("position_ids"),
                SessionInputValue::Owned(Value::from(pos_tensor)),
            ));
        }

        let outputs = session
            .run(SessionInputs::from(inputs))
            .map_err(|e| TtsError::Synthesis(format!("embed_tokens bos: {e}")))?;

        TensorData::<f32>::extract(&outputs[0], "bos_inputs_embeds")
    }

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

    /// First LM step: mirrors the torch T3 inference path.
    ///
    /// Batch 0 (conditioned):   `cond_emb | text_embeds | BOS | BOS`
    /// Batch 1 (unconditioned): `cond_emb | text_pos_emb | BOS | BOS`
    ///
    /// Upstream's `prepare_input_embeds` already includes one `START_SPEECH`,
    /// and `inference()` then appends another BOS before the first LM call,
    /// hence the BOS appears twice. The unconditioned branch substitutes the
    /// learned text position embeddings for the real text embeddings.
    fn build_first_step_embeds(
        &self,
        text_embeds: &TensorData<f32>,
        bos_embeds: &TensorData<f32>,
        use_cfg: bool,
    ) -> Result<(Vec<f32>, usize, usize), TtsError> {
        let hidden_dim = *text_embeds.shape.last().expect("embeds rank >= 1");
        let cond_seq = self.cond.cond_emb.shape[1];
        let text_seq = text_embeds.shape[1];
        let bos_seq = bos_embeds.shape[1];
        let total_seq = cond_seq + text_seq + bos_seq + bos_seq;
        let batch = if use_cfg { 2 } else { 1 };

        let mut data = Vec::with_capacity(batch * total_seq * hidden_dim);

        data.extend_from_slice(&self.cond.cond_emb.data);
        data.extend_from_slice(&text_embeds.data);
        data.extend_from_slice(&bos_embeds.data);
        data.extend_from_slice(&bos_embeds.data);

        if use_cfg {
            data.extend_from_slice(&self.cond.cond_emb.data);
            data.extend_from_slice(self.text_position_slice(text_seq, hidden_dim)?);
            data.extend_from_slice(&bos_embeds.data);
            data.extend_from_slice(&bos_embeds.data);
        }

        Ok((data, total_seq, hidden_dim))
    }

    /// Return the leading `text_seq × hidden_dim` chunk of learned text
    /// position embeddings, after validating the saved shape.
    fn text_position_slice(&self, text_seq: usize, hidden_dim: usize) -> Result<&[f32], TtsError> {
        if self.text_pos_emb.shape.len() != 2 {
            return Err(TtsError::Init("text_pos_emb must be rank-2".into()));
        }
        if self.text_pos_emb.shape[1] != hidden_dim {
            return Err(TtsError::Init(format!(
                "text_pos_emb hidden size mismatch: {} != {}",
                self.text_pos_emb.shape[1], hidden_dim
            )));
        }
        if self.text_pos_emb.shape[0] < text_seq {
            return Err(TtsError::Synthesis(format!(
                "text_pos_emb too short: need {text_seq}, have {}",
                self.text_pos_emb.shape[0]
            )));
        }
        Ok(&self.text_pos_emb.data[..text_seq * hidden_dim])
    }

    /// Run one LM step, updating `kv_cache` in place and returning logits.
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

    fn decode_speech(&self, speech_tokens: &[i64]) -> Result<Vec<f32>, TtsError> {
        let decode_start = Instant::now();
        let tokens_tensor = Tensor::from_array(([1, speech_tokens.len()], speech_tokens.to_vec()))
            .map_err(|e| TtsError::Synthesis(format!("speech tokens tensor: {e}")))?;
        let speaker_tensor = Tensor::from_array((
            self.cond.ref_x_vector.shape.clone(),
            self.cond.ref_x_vector.data.clone(),
        ))
        .map_err(|e| TtsError::Synthesis(format!("speaker tensor: {e}")))?;
        let feat_tensor = Tensor::from_array((
            self.cond.prompt_feat.shape.clone(),
            self.cond.prompt_feat.data.clone(),
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
        let input_prep_elapsed = decode_start.elapsed();

        let mut session = self
            .conditional_decoder
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;
        let run_start = Instant::now();
        let outputs = session
            .run(SessionInputs::from(inputs))
            .map_err(|e| TtsError::Synthesis(format!("conditional decoder: {e}")))?;
        let run_elapsed = run_start.elapsed();

        let extract_start = Instant::now();
        let wav = TensorData::<f32>::extract(&outputs[0], "wav")?.data;
        let extract_elapsed = extract_start.elapsed();

        info!(
            tokens = speech_tokens.len(),
            input_prep_elapsed = ?input_prep_elapsed,
            run_elapsed = ?run_elapsed,
            extract_elapsed = ?extract_elapsed,
            "Røst: decoder timings"
        );

        Ok(wav)
    }
}

/// Single speech-token continuation, duplicated across the CFG batch when
/// enabled. Identical to Chatterbox's continuation layout.
fn build_continuation_embeds(
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

/// Reduce the LM output to a single vocab-sized logit vector. See
/// `chatterbox::collapse_logits` for the CFG math.
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

/// Apply the sampler's pre-multinomial warpers to a clone of `logits`, for
/// use with `NOBODYWHO_TTS_DUMP_STEP`. The real sampling step uses the
/// untouched `logits` directly so this preview can't affect token choice.
fn preview_sampled_logits(logits: &[f32], params: &SamplingParams) -> Vec<f32> {
    let mut preview = logits.to_vec();
    if params.temperature <= 1e-6 {
        return preview;
    }
    if params.temperature != 1.0 {
        for score in preview.iter_mut() {
            *score /= params.temperature;
        }
    }
    sampling::apply_top_k(&mut preview, params.top_k);
    sampling::apply_min_p(&mut preview, params.min_p);
    sampling::apply_top_p(&mut preview, params.top_p);
    preview
}

fn prepare_text_for_mtl_tokenizer(text: &str, language: &str) -> String {
    let punctuated = punc_norm(text);
    let normalized: String = punctuated.to_lowercase().nfkd().collect();
    let language = if language.is_empty() { "da" } else { language }.to_lowercase();
    format!("[{}]{}", language, normalized).replace(' ', "[SPACE]")
}

/// Normalize whitespace and punctuation to match the upstream Chatterbox
/// dataset preprocessing. Applied before `nfkd` + lowercasing.
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
        ("“", "\""),
        ("”", "\""),
        ("‘", "'"),
        ("’", "'"),
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

fn load_precomputed_cond(dir: &Path) -> Option<PrecomputedCond> {
    let manifest = ort_util::read_cond_manifest(dir)?;
    Some(PrecomputedCond {
        cond_emb: ort_util::load_f32_tensor(dir, &manifest, "cond_emb")?,
        prompt_token: ort_util::load_i64_tensor(dir, &manifest, "prompt_token")?,
        ref_x_vector: ort_util::load_f32_tensor(dir, &manifest, "ref_x_vector")?,
        prompt_feat: ort_util::load_f32_tensor(dir, &manifest, "prompt_feat")?,
    })
}

fn load_model_config(model_dir: &Path) -> Result<ModelConfig, TtsError> {
    let path = model_dir.join("model_config.json");
    let s = std::fs::read_to_string(&path)
        .map_err(|e| TtsError::Init(format!("missing model_config.json: {e}")))?;
    let v: serde_json::Value = serde_json::from_str(&s)
        .map_err(|e| TtsError::Init(format!("invalid model_config.json: {e}")))?;
    let text_token_offset = v["text_token_offset"]
        .as_i64()
        .ok_or_else(|| TtsError::Init("model_config.json missing text_token_offset".into()))?;
    let text_pos_emb_shape = v["text_pos_emb_shape"]
        .as_array()
        .ok_or_else(|| TtsError::Init("model_config.json missing text_pos_emb_shape".into()))?
        .iter()
        .map(|value| {
            value
                .as_u64()
                .map(|dim| dim as usize)
                .ok_or_else(|| TtsError::Init("invalid text_pos_emb_shape entry".into()))
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(ModelConfig {
        text_token_offset,
        text_pos_emb_shape,
    })
}

fn load_text_pos_emb(model_dir: &Path, shape: &[usize]) -> Result<TensorData<f32>, TtsError> {
    let path = model_dir.join("text_pos_emb.bin");
    let bytes = std::fs::read(&path)
        .map_err(|e| TtsError::Init(format!("missing text_pos_emb.bin: {e}")))?;
    let data: Vec<f32> = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
        .collect();
    let expected_len: usize = shape.iter().product();
    if data.len() != expected_len {
        return Err(TtsError::Init(format!(
            "text_pos_emb.bin length mismatch: expected {expected_len} floats, got {}",
            data.len()
        )));
    }
    Ok(TensorData {
        data,
        shape: shape.to_vec(),
    })
}
