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

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::ort_util::{
    self, build_continuation_embeds, collapse_logits, detect_num_layers, has_position_ids,
    KvCacheLayout, SpeakerConditioning, SpeechGenerationState, TensorData,
};
use crate::tts::sampling::{self, SamplingParams};
use crate::tts::{TtsDevice, TtsSampling, DEFAULT_SAMPLE_RATE};
use ort::session::SessionInputs;
use std::path::Path;
use std::time::Instant;
use tracing::info;
use unicode_normalization::UnicodeNormalization;

pub(super) struct RoestBackend {
    model: RoestModel,
    language: String,
    sampling: TtsSampling,
}

impl RoestBackend {
    pub fn new(model: RoestModel, language: String, sampling: TtsSampling) -> Self {
        Self {
            model,
            language,
            sampling,
        }
    }
}

impl TtsBackendImpl for RoestBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        let sampling = SamplingParams::from(&self.sampling);
        let samples = self.model.synthesize(text, &self.language, &sampling)?;
        Ok((samples, DEFAULT_SAMPLE_RATE))
    }
}

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

/// Fields parsed out of `model_config.json`.
struct ModelConfig {
    text_token_offset: i64,
    text_pos_emb_shape: Vec<usize>,
}

pub(crate) struct RoestModel {
    embed_tokens: ort::session::Session,
    language_model: ort::session::Session,
    conditional_decoder: ort::session::Session,
    tokenizer: tokenizers::Tokenizer,
    cond: SpeakerConditioning,
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
        let embed_tokens =
            ort_util::load_session(&onnx_dir.join("embed_tokens.onnx"), device, true)?;
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

        let cond = load_precomputed_cond(&model_dir.join("default_cond"))?.ok_or_else(|| {
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
            embed_tokens,
            language_model,
            conditional_decoder,
            tokenizer,
            cond,
            text_token_offset: config.text_token_offset,
            text_pos_emb,
            num_layers,
            embed_has_position_ids,
        })
    }

    pub(super) fn synthesize(
        &mut self,
        text: &str,
        language: &str,
        sampling: &SamplingParams,
    ) -> Result<Vec<f32>, TtsError> {
        let start = Instant::now();
        let prepared = prepare_text_for_mtl_tokenizer(text, language);
        let (text_input_ids, text_position_ids) = self.tokenize_for_lm(&prepared)?;
        let speech_tokens =
            self.generate_speech_tokens(&text_input_ids, &text_position_ids, sampling)?;

        let mut full_tokens: Vec<i64> = self.cond.prompt_token.data.clone();
        full_tokens.extend_from_slice(&speech_tokens);

        let pcm = self.decode_speech(&full_tokens)?;

        info!(
            input_tokens = text_input_ids.len(),
            speech_tokens = speech_tokens.len(),
            samples = pcm.len(),
            duration_secs = pcm.len() as f32 / S3GEN_SR as f32,
            elapsed = ?start.elapsed(),
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
        &mut self,
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

        let embed_has_position_ids = self.embed_has_position_ids;
        let cond_emb_seq = self.cond.cond_emb.shape[1];
        let cond_emb = &self.cond.cond_emb;
        let text_pos_emb = &self.text_pos_emb;
        let embed_session = &mut self.embed_tokens;
        let lm_session = &mut self.language_model;

        let bos_embeds = embed_bos_token(embed_session, embed_has_position_ids)?;

        let mut state = SpeechGenerationState::new(
            &kv_layout,
            START_SPEECH_TOKEN,
            DEFAULT_MAX_NEW_TOKENS,
            cond_emb_seq + text_input_ids.len() + DEFAULT_MAX_NEW_TOKENS,
        );

        for step in 0..DEFAULT_MAX_NEW_TOKENS {
            let step_start = Instant::now();

            let (ids, positions) = state.step_inputs(step, text_input_ids, text_position_ids);

            let token_embeds = ort_util::run_embed(
                embed_session,
                embed_has_position_ids,
                &ids,
                &positions,
                "inputs_embeds",
            )?;

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
            state.timings.embed += step_start.elapsed();

            state.update_attention(step, lm_seq_len);

            let lm_start = Instant::now();
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
            state.timings.lm += lm_start.elapsed();

            let sample_start = Instant::now();
            let mut final_logits = collapse_logits(&logits, use_cfg, sampling_params.cfg_weight);

            sampling::apply_repetition_penalty(
                &mut final_logits,
                &state.generated,
                REPETITION_PENALTY,
                true,
            );

            let next_token = sampling::sample_token(&mut final_logits, sampling_params) as i64;
            state.timings.sample += sample_start.elapsed();

            if state.accept_token(next_token, lm_seq_len, STOP_SPEECH_TOKEN) {
                break;
            }
        }

        let generated_count = state.generated_count();
        let loop_elapsed = state.timings.total();
        info!(
            generated = generated_count,
            embed_elapsed = ?state.timings.embed,
            lm_elapsed = ?state.timings.lm,
            sample_elapsed = ?state.timings.sample,
            tokens_per_sec = if loop_elapsed.is_zero() {
                0.0
            } else {
                generated_count as f64 / loop_elapsed.as_secs_f64()
            },
            "Røst: generation timings"
        );

        Ok(state.output_tokens(START_SPEECH_TOKEN, STOP_SPEECH_TOKEN))
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

/// Compute the embedding for the standalone `START_SPEECH` token. Used
/// both as the trailing entry in the first-step sequence and as a
/// replacement for the unconditioned text embedding on the CFG branch.
fn embed_bos_token(
    session: &mut ort::session::Session,
    embed_has_position_ids: bool,
) -> Result<TensorData<f32>, TtsError> {
    ort_util::run_embed(
        session,
        embed_has_position_ids,
        &[START_SPEECH_TOKEN],
        &[0_i64],
        "bos_inputs_embeds",
    )
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
/// position embeddings, after validating the saved shape.
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
    ort_util::read_le_bin(&model_dir.join("text_pos_emb.bin"), shape.to_vec())
}
