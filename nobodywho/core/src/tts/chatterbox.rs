/// Chatterbox Multilingual TTS inference via ONNX Runtime.
///
/// Implements the full 4-stage pipeline:
///   1. Speech encoder — reference audio → speaker embeddings
///   2. Embed tokens — text token IDs → embeddings
///   3. Language model — autoregressive Llama with KV cache → speech tokens
///   4. Conditional decoder — speech tokens + speaker embeddings → PCM audio
///
/// Supports 23 languages including Danish, with voice cloning from a reference WAV.
use crate::errors::TtsError;
use ort::memory::Allocator;
use ort::session::builder::SessionBuilder;
use ort::session::Session;
use ort::value::Shape;
use ort::value::Tensor;
use ort::value::Value;
use std::borrow::Cow;
use std::path::Path;
use std::sync::Mutex;
use tracing::info;
use unicode_normalization::UnicodeNormalization;

const S3GEN_SR: u32 = 24000;
const START_SPEECH_TOKEN: i64 = 6561;
const STOP_SPEECH_TOKEN: i64 = 6562;
const DEFAULT_MAX_NEW_TOKENS: usize = 1000;
const REPETITION_PENALTY: f32 = 2.0;

pub(crate) struct SamplingParams {
    pub temperature: f32,
    pub top_k: usize,
    pub top_p: f32,
    pub min_p: f32,
    pub cfg_weight: f32,
}
/// Default text token offset (turbo = 6563, multilingual = 8194).
/// Loaded from model_config.json if present.
const DEFAULT_TEXT_TOKEN_OFFSET: i64 = 6563;

/// Pre-computed conditioning tensors (from Python, avoids speech_encoder ONNX).
struct PrecomputedCond {
    cond_emb: TensorData<f32>,
    prompt_token: TensorData<i64>,
    ref_x_vector: TensorData<f32>,
    prompt_feat: TensorData<f32>,
}

pub(crate) struct ChatterboxModel {
    speech_encoder: Mutex<Session>,
    embed_tokens: Mutex<Session>,
    language_model: Mutex<Session>,
    conditional_decoder: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    /// Pre-computed conditioning (if available, bypasses speech_encoder).
    precomputed_cond: Option<PrecomputedCond>,
    /// Offset for text token IDs so embed_tokens routes them correctly.
    text_token_offset: i64,
    /// Number of transformer layers (30 for multilingual/Llama, 24 for turbo/GPT-2)
    num_layers: usize,
    num_kv_heads: usize,
    head_dim: usize,
    /// Whether embed_tokens accepts position_ids input
    embed_has_position_ids: bool,
    /// Whether language_model accepts position_ids input
    lm_has_position_ids: bool,
}

impl ChatterboxModel {
    /// Load all 4 ONNX models + tokenizer from a directory.
    ///
    /// Expected directory layout:
    ///   dir/
    ///     tokenizer.json
    ///     onnx/speech_encoder.onnx (+.onnx_data)
    ///     onnx/embed_tokens.onnx (+.onnx_data)
    ///     onnx/language_model*.onnx (+.onnx_data)   — any quantization variant
    ///     onnx/conditional_decoder.onnx (+.onnx_data)
    pub fn new(model_dir: &Path) -> Result<Self, TtsError> {
        let onnx_dir = model_dir.join("onnx");

        let tokenizer_path = model_dir.join("tokenizer.json");
        let tokenizer = tokenizers::Tokenizer::from_file(&tokenizer_path)
            .map_err(|e| TtsError::Init(format!("failed to load tokenizer: {e}")))?;

        let speech_encoder = load_session(&onnx_dir.join("speech_encoder.onnx"))?;
        let embed_tokens = load_session(&onnx_dir.join("embed_tokens.onnx"))?;
        let language_model = find_and_load_language_model(&onnx_dir)?;
        let conditional_decoder = load_session(&onnx_dir.join("conditional_decoder.onnx"))?;

        // Detect layer count from LM input names: past_key_values.{N}.key
        let num_layers = language_model
            .inputs()
            .iter()
            .filter(|i| i.name().starts_with("past_key_values.") && i.name().ends_with(".key"))
            .count();
        // Default head config — both multilingual (Llama) and turbo (GPT-2) use 16 heads, 64 dim
        let num_kv_heads = 16;
        let head_dim = 64;

        let embed_has_position_ids = embed_tokens
            .inputs()
            .iter()
            .any(|i| i.name() == "position_ids");
        let lm_has_position_ids = language_model
            .inputs()
            .iter()
            .any(|i| i.name() == "position_ids");

        // Load pre-computed conditioning if available (bypasses speech_encoder)
        let precomputed_cond = load_precomputed_cond(&model_dir.join("default_cond"));

        // Load text_token_offset from model_config.json if present
        let text_token_offset = model_dir
            .join("model_config.json")
            .exists()
            .then(|| {
                let s = std::fs::read_to_string(model_dir.join("model_config.json")).ok()?;
                let v: serde_json::Value = serde_json::from_str(&s).ok()?;
                v["text_token_offset"].as_i64()
            })
            .flatten()
            .unwrap_or(DEFAULT_TEXT_TOKEN_OFFSET);

        info!(
            num_layers,
            num_kv_heads,
            head_dim,
            has_precomputed = precomputed_cond.is_some(),
            "Loaded Chatterbox TTS (4 ONNX sessions)"
        );

        Ok(Self {
            speech_encoder: Mutex::new(speech_encoder),
            embed_tokens: Mutex::new(embed_tokens),
            language_model: Mutex::new(language_model),
            conditional_decoder: Mutex::new(conditional_decoder),
            tokenizer,
            num_layers,
            num_kv_heads,
            head_dim,
            precomputed_cond,
            text_token_offset,
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
        // Preprocess text to match the MTLTokenizer pipeline used during training:
        // 1. Lowercase + NFKD normalize
        // 2. Prepend language tag [da] if specified (required by multilingual models)
        // 3. Replace spaces with [SPACE] token
        let normalized: String = text.to_lowercase().nfkd().collect();
        let with_lang = if language.is_empty() {
            normalized
        } else {
            format!("[{}]{}", language.to_lowercase(), normalized)
        };
        let prepared_text = with_lang.replace(' ', "[SPACE]");

        // Step 0b: Load reference audio (or use provided samples)
        let audio_samples = match reference_audio {
            Some(samples) => samples.to_vec(),
            None => {
                // Try to load default_voice.wav from model dir
                return Err(TtsError::Synthesis(
                    "reference audio is required for Chatterbox (pass reference_audio or place default_voice.wav in model dir)".into(),
                ));
            }
        };

        // Step 1: Tokenize
        let encoding = self
            .tokenizer
            .encode(prepared_text.as_str(), true)
            .map_err(|e| TtsError::Synthesis(format!("tokenization failed: {e}")))?;
        let raw_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();

        // Offset text tokens so embed_tokens routes them to text_emb.
        // Tokens from the tokenizer are text tokens; speech tokens (START/STOP) are < 6563.
        // If the tokenizer already produces speech-range IDs (multilingual model), no offset needed.
        let has_text_tokens = raw_ids.iter().any(|&id| id >= self.text_token_offset);
        let mut input_ids: Vec<i64> = if has_text_tokens {
            // Multilingual tokenizer — IDs already include speech tokens, no offset
            raw_ids
        } else {
            // Turbo/GPT-2 tokenizer — all IDs are text tokens, offset them
            raw_ids.iter().map(|&id| id + self.text_token_offset).collect()
        };
        // Append START_SPEECH token — the LM expects [cond | text | START_SPEECH]
        input_ids.push(START_SPEECH_TOKEN);
        let seq_len = input_ids.len();

        // Position IDs: text tokens get [0, 1, 2, ..., N_text-1],
        // speech tokens (START_SPEECH at end) get [0].
        // In Python: text_pos_emb uses arange(0, text_len), speech_pos_emb uses get_fixed_embedding(step).
        let n_text = seq_len - 1; // everything except the trailing START_SPEECH
        let mut position_ids: Vec<i64> = (0..n_text as i64).collect();
        position_ids.push(0); // speech position 0 for START_SPEECH

        info!(
            tokens = seq_len,
            text = prepared_text.as_str(),
            "Chatterbox: tokenized"
        );

        // Step 2: Get conditioning — either from pre-computed data or speech encoder
        let (cond_emb, prompt_token, ref_x_vector, prompt_feat) =
            if let Some(pc) = &self.precomputed_cond {
                info!("Using pre-computed conditioning");
                (
                    pc.cond_emb.clone(),
                    pc.prompt_token.clone(),
                    pc.ref_x_vector.clone(),
                    pc.prompt_feat.clone(),
                )
            } else {
                let audio_tensor =
                    Tensor::from_array(([1, audio_samples.len()], audio_samples))
                        .map_err(|e| TtsError::Synthesis(format!("audio tensor: {e}")))?;

                let mut session = self
                    .speech_encoder
                    .lock()
                    .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;

                let outputs = session
                    .run(
                        ort::session::SessionInputs::from(vec![(
                            Cow::Borrowed("audio_values"),
                            ort::session::SessionInputValue::Owned(Value::from(audio_tensor)),
                        )]),
                    )
                    .map_err(|e| TtsError::Synthesis(format!("speech encoder: {e}")))?;

                let cond_emb = extract_f32_tensor(&outputs[0], "cond_emb")?;
                let prompt_token = extract_i64_tensor(&outputs[1], "prompt_token")?;
                let ref_x_vector = extract_f32_tensor(&outputs[2], "ref_x_vector")?;
                let prompt_feat = extract_f32_tensor(&outputs[3], "prompt_feat")?;

                (cond_emb, prompt_token, ref_x_vector, prompt_feat)
            };

        info!(
            cond_emb_shape = ?cond_emb.shape,
            prompt_token_shape = ?prompt_token.shape,
            "Chatterbox: speech encoder done"
        );

        // Step 3: Autoregressive generation loop
        let speech_tokens = self.generate_speech_tokens(
            &input_ids,
            &position_ids,
            &cond_emb,
            sampling,
        )?;

        // Prepend prompt tokens to generated speech tokens
        let mut full_speech_tokens: Vec<i64> = prompt_token.data.clone();
        full_speech_tokens.extend_from_slice(&speech_tokens);

        info!(
            generated = speech_tokens.len(),
            total = full_speech_tokens.len(),
            "Chatterbox: speech tokens generated"
        );

        // Step 4: Conditional decoder — speech tokens → PCM
        let pcm = self.decode_speech(
            &full_speech_tokens,
            &ref_x_vector,
            &prompt_feat,
        )?;

        info!(
            samples = pcm.len(),
            duration_secs = pcm.len() as f32 / S3GEN_SR as f32,
            "Chatterbox: synthesis complete"
        );

        Ok(pcm)
    }

    fn generate_speech_tokens(
        &self,
        input_ids: &[i64],
        position_ids: &[i64],
        cond_emb: &TensorData<f32>,
        sampling: &SamplingParams,
    ) -> Result<Vec<i64>, TtsError> {
        let use_cfg = sampling.cfg_weight > 0.0;
        let batch_size = if use_cfg { 2usize } else { 1usize };
        let mut generated: Vec<i64> = vec![START_SPEECH_TOKEN];

        let mut kv_cache: Vec<Vec<f32>> = (0..self.num_layers * 2)
            .map(|_| Vec::new())
            .collect();
        let mut kv_seq_len: usize = 0;
        let mut attention_mask: Vec<i64> = Vec::new();

        let mut embed_session = self
            .embed_tokens
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;
        let mut lm_session = self
            .language_model
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;

        for step in 0..DEFAULT_MAX_NEW_TOKENS {
            // Get embeddings for current input
            let (current_ids, current_pos) = if step == 0 {
                (input_ids.to_vec(), position_ids.to_vec())
            } else {
                let last_token = *generated.last().unwrap();
                // Speech position: step 1 → pos 1, step 2 → pos 2, etc.
                // (step 0 prefill used pos 0 for START_SPEECH)
                (vec![last_token], vec![step as i64])
            };

            let cur_seq_len = current_ids.len();
            let ids_tensor = Tensor::from_array(([1, cur_seq_len], current_ids))
                .map_err(|e| TtsError::Synthesis(format!("ids tensor: {e}")))?;

            let mut embed_inputs: Vec<(Cow<'_, str>, ort::session::SessionInputValue<'_>)> = vec![
                (Cow::Borrowed("input_ids"), ort::session::SessionInputValue::Owned(Value::from(ids_tensor))),
            ];
            if self.embed_has_position_ids {
                let pos_tensor = Tensor::from_array(([1, cur_seq_len], current_pos))
                    .map_err(|e| TtsError::Synthesis(format!("pos tensor: {e}")))?;
                embed_inputs.push((Cow::Borrowed("position_ids"), ort::session::SessionInputValue::Owned(Value::from(pos_tensor))));
            }

            let embed_outputs = embed_session
                .run(ort::session::SessionInputs::from(embed_inputs))
                .map_err(|e| TtsError::Synthesis(format!("embed_tokens: {e}")))?;
            let embeds = extract_f32_tensor(&embed_outputs[0], "inputs_embeds")?;
            let hidden_dim = embeds.shape[embeds.shape.len() - 1];

            // Build inputs_embeds: on first step, prepend cond_emb
            // For CFG: batch[0] = conditioned, batch[1] = unconditioned (zeroed text)
            let (inputs_embeds_data, inputs_embeds_seq_len) = if step == 0 {
                let cond_seq = cond_emb.shape[1];
                let text_seq = embeds.shape[1];
                let total_seq = cond_seq + text_seq;

                // Batch 0: cond_emb + text embeddings (conditioned)
                let mut data = Vec::with_capacity(batch_size * total_seq * hidden_dim);
                data.extend_from_slice(&cond_emb.data);
                data.extend_from_slice(&embeds.data);

                if use_cfg {
                    // Batch 1: cond_emb + ZEROED text embeddings (unconditioned)
                    data.extend_from_slice(&cond_emb.data);
                    data.extend(std::iter::repeat(0.0f32).take(text_seq * hidden_dim));
                }
                (data, total_seq)
            } else {
                // Speech token embedding — same for both batches
                let mut data = embeds.data.clone();
                if use_cfg {
                    data.extend_from_slice(&embeds.data);
                }
                (data, embeds.shape[1])
            };

            // Attention mask
            if step == 0 {
                attention_mask = vec![1i64; inputs_embeds_seq_len];
            } else {
                attention_mask.push(1);
            }

            // Build LM inputs
            let embeds_tensor = Tensor::from_array((
                [batch_size, inputs_embeds_seq_len, hidden_dim],
                inputs_embeds_data,
            ))
            .map_err(|e| TtsError::Synthesis(format!("embeds tensor: {e}")))?;

            // Attention mask: replicate for batch_size
            let attn_data: Vec<i64> = (0..batch_size).flat_map(|_| attention_mask.iter().copied()).collect();
            let attn_tensor = Tensor::from_array(([batch_size, attention_mask.len()], attn_data))
                .map_err(|e| TtsError::Synthesis(format!("attn tensor: {e}")))?;

            let mut lm_inputs: Vec<(Cow<'_, str>, ort::session::SessionInputValue<'_>)> = vec![
                (Cow::Borrowed("inputs_embeds"), ort::session::SessionInputValue::Owned(Value::from(embeds_tensor))),
                (Cow::Borrowed("attention_mask"), ort::session::SessionInputValue::Owned(Value::from(attn_tensor))),
            ];

            if self.lm_has_position_ids {
                let pos_ids: Vec<i64> = (0..batch_size)
                    .flat_map(|_| (kv_seq_len..kv_seq_len + inputs_embeds_seq_len).map(|i| i as i64))
                    .collect();
                let pos_tensor = Tensor::from_array(([batch_size, inputs_embeds_seq_len], pos_ids))
                    .map_err(|e| TtsError::Synthesis(format!("pos_id tensor: {e}")))?;
                lm_inputs.push((Cow::Borrowed("position_ids"), ort::session::SessionInputValue::Owned(Value::from(pos_tensor))));
            }

            // KV cache
            for layer in 0..self.num_layers {
                for (kv_idx, kv_name) in ["key", "value"].iter().enumerate() {
                    let cache_idx = layer * 2 + kv_idx;
                    let name = format!("past_key_values.{layer}.{kv_name}");
                    let tensor = if kv_seq_len == 0 {
                        Tensor::<f32>::new(
                            &Allocator::default(),
                            Shape::new([batch_size as i64, self.num_kv_heads as i64, 0, self.head_dim as i64]),
                        )
                        .map_err(|e| TtsError::Synthesis(format!("empty kv {name}: {e}")))?
                    } else {
                        Tensor::from_array((
                            [batch_size, self.num_kv_heads, kv_seq_len, self.head_dim],
                            kv_cache[cache_idx].clone(),
                        ))
                        .map_err(|e| TtsError::Synthesis(format!("kv {name}: {e}")))?
                    };
                    lm_inputs.push((Cow::Owned(name), ort::session::SessionInputValue::Owned(Value::from(tensor))));
                }
            }

            let lm_outputs = lm_session
                .run(ort::session::SessionInputs::from(lm_inputs))
                .map_err(|e| TtsError::Synthesis(format!("language model step {step}: {e}")))?;

            // Extract logits [batch_size, seq_len, vocab_size]
            let logits = extract_f32_tensor(&lm_outputs[0], "logits")?;
            let vocab_size = logits.shape[2];
            let seq_len = logits.shape[1];

            // Get last-position logits, apply CFG if enabled
            let mut final_logits = if use_cfg {
                // Batch 0 = conditioned, batch 1 = unconditioned
                let cond_offset = (seq_len - 1) * vocab_size;
                let uncond_offset = (seq_len + seq_len - 1) * vocab_size;
                let cond_logits = &logits.data[cond_offset..cond_offset + vocab_size];
                let uncond_logits = &logits.data[uncond_offset..uncond_offset + vocab_size];
                cond_logits
                    .iter()
                    .zip(uncond_logits.iter())
                    .map(|(&c, &u)| c + sampling.cfg_weight * (c - u))
                    .collect::<Vec<f32>>()
            } else {
                let offset = (seq_len - 1) * vocab_size;
                logits.data[offset..offset + vocab_size].to_vec()
            };

            // Repetition penalty + sampling
            apply_repetition_penalty(&mut final_logits, &generated, REPETITION_PENALTY);
            let next_token = sample_token(&mut final_logits, sampling) as i64;
            generated.push(next_token);

            if next_token == STOP_SPEECH_TOKEN {
                break;
            }

            // Update KV cache
            let new_kv_seq_len = kv_seq_len + inputs_embeds_seq_len;
            for i in 0..(self.num_layers * 2) {
                let kv = extract_f32_tensor(&lm_outputs[1 + i], &format!("kv_{i}"))?;
                kv_cache[i] = kv.data;
            }
            kv_seq_len = new_kv_seq_len;

            if step % 50 == 0 && step > 0 {
                info!(step, "Chatterbox: generation progress");
            }
        }

        let speech_tokens: Vec<i64> = generated
            .iter()
            .copied()
            .filter(|&t| t != START_SPEECH_TOKEN && t != STOP_SPEECH_TOKEN)
            .collect();

        Ok(speech_tokens)
    }

    fn decode_speech(
        &self,
        speech_tokens: &[i64],
        ref_x_vector: &TensorData<f32>,
        prompt_feat: &TensorData<f32>,
    ) -> Result<Vec<f32>, TtsError> {
        let num_tokens = speech_tokens.len();

        let tokens_tensor = Tensor::from_array(([1, num_tokens], speech_tokens.to_vec()))
            .map_err(|e| TtsError::Synthesis(format!("speech tokens tensor: {e}")))?;

        let speaker_tensor = Tensor::from_array((
            ref_x_vector.shape.clone(),
            ref_x_vector.data.clone(),
        ))
        .map_err(|e| TtsError::Synthesis(format!("speaker tensor: {e}")))?;

        let feat_tensor = Tensor::from_array((
            prompt_feat.shape.clone(),
            prompt_feat.data.clone(),
        ))
        .map_err(|e| TtsError::Synthesis(format!("feat tensor: {e}")))?;

        let inputs: Vec<(Cow<'_, str>, ort::session::SessionInputValue<'_>)> = vec![
            (
                Cow::Borrowed("speech_tokens"),
                ort::session::SessionInputValue::Owned(Value::from(tokens_tensor)),
            ),
            (
                Cow::Borrowed("speaker_embeddings"),
                ort::session::SessionInputValue::Owned(Value::from(speaker_tensor)),
            ),
            (
                Cow::Borrowed("speaker_features"),
                ort::session::SessionInputValue::Owned(Value::from(feat_tensor)),
            ),
        ];

        let mut session = self
            .conditional_decoder
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;

        let outputs = session
            .run(ort::session::SessionInputs::from(inputs))
            .map_err(|e| TtsError::Synthesis(format!("conditional decoder: {e}")))?;

        let wav = extract_f32_tensor(&outputs[0], "wav")?;
        Ok(wav.data)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

/// Simple container for extracted tensor data + shape.
#[derive(Clone)]
struct TensorData<T> {
    data: Vec<T>,
    shape: Vec<usize>,
}

fn extract_f32_tensor(
    value: &ort::value::DynValue,
    name: &str,
) -> Result<TensorData<f32>, TtsError> {
    let (shape, data) = value
        .try_extract_tensor::<f32>()
        .map_err(|e| TtsError::Synthesis(format!("extract {name}: {e}")))?;
    Ok(TensorData {
        data: data.to_vec(),
        shape: shape.iter().map(|&d| d as usize).collect(),
    })
}

fn extract_i64_tensor(
    value: &ort::value::DynValue,
    name: &str,
) -> Result<TensorData<i64>, TtsError> {
    let (shape, data) = value
        .try_extract_tensor::<i64>()
        .map_err(|e| TtsError::Synthesis(format!("extract {name}: {e}")))?;
    Ok(TensorData {
        data: data.to_vec(),
        shape: shape.iter().map(|&d| d as usize).collect(),
    })
}

/// Load pre-computed conditioning from a directory with manifest.json + .bin files.
fn load_precomputed_cond(dir: &Path) -> Option<PrecomputedCond> {
    let manifest_path = dir.join("manifest.json");
    if !manifest_path.exists() {
        return None;
    }

    let manifest_str = std::fs::read_to_string(&manifest_path).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_str).ok()?;

    let load_f32 = |name: &str| -> Option<TensorData<f32>> {
        let entry = manifest.get(name)?;
        let shape: Vec<usize> = entry["shape"]
            .as_array()?
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) as usize)
            .collect();
        let bytes = std::fs::read(dir.join(format!("{name}.bin"))).ok()?;
        let data: Vec<f32> = bytes
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
            .collect();
        Some(TensorData { data, shape })
    };

    let load_i64 = |name: &str| -> Option<TensorData<i64>> {
        let entry = manifest.get(name)?;
        let shape: Vec<usize> = entry["shape"]
            .as_array()?
            .iter()
            .map(|v| v.as_u64().unwrap_or(0) as usize)
            .collect();
        let bytes = std::fs::read(dir.join(format!("{name}.bin"))).ok()?;
        let data: Vec<i64> = bytes
            .chunks_exact(8)
            .map(|c| i64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]]))
            .collect();
        Some(TensorData { data, shape })
    };

    Some(PrecomputedCond {
        cond_emb: load_f32("cond_emb")?,
        prompt_token: load_i64("prompt_token")?,
        ref_x_vector: load_f32("ref_x_vector")?,
        prompt_feat: load_f32("prompt_feat")?,
    })
}

fn load_session(path: &Path) -> Result<Session, TtsError> {
    SessionBuilder::new()
        .map_err(|e| TtsError::Init(format!("ort session builder: {e}")))?
        .with_execution_providers([ort::ep::CPU::default().build()])
        .map_err(|e| TtsError::Init(format!("ort execution providers: {e}")))?
        .commit_from_file(path)
        .map_err(|e| TtsError::Init(format!("ort load model {}: {e}", path.display())))
}

/// Find the language model ONNX file, preferring quantized variants.
fn find_and_load_language_model(onnx_dir: &Path) -> Result<Session, TtsError> {
    // fp32 is safest (fp16/q4 need matching tensor types or can segfault)
    let candidates = [
        "language_model.onnx",
        "language_model_q4.onnx",
        "language_model_fp16.onnx",
        "language_model_q4f16.onnx",
    ];

    for name in &candidates {
        let path = onnx_dir.join(name);
        if path.exists() {
            info!(model = name, "Loading language model");
            return load_session(&path);
        }
    }

    Err(TtsError::Init(
        "no language model ONNX file found in onnx/ directory".into(),
    ))
}

fn apply_repetition_penalty(logits: &mut [f32], generated: &[i64], penalty: f32) {
    for &token_id in generated {
        if let Some(score) = logits.get_mut(token_id as usize) {
            if *score < 0.0 {
                *score *= penalty;
            } else {
                *score /= penalty;
            }
        }
    }
}

fn argmax(values: &[f32]) -> usize {
    values
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i)
        .unwrap_or(0)
}

/// Sample a token from logits using temperature, top-k, and top-p.
fn sample_token(logits: &mut [f32], params: &SamplingParams) -> usize {
    // Temperature = 0 or very low → greedy
    if params.temperature <= 1e-6 {
        return argmax(logits);
    }

    // Apply temperature
    for v in logits.iter_mut() {
        *v /= params.temperature;
    }

    // Build sorted index for top-k / top-p filtering
    let mut indexed: Vec<(usize, f32)> = logits.iter().copied().enumerate().collect();
    indexed.sort_unstable_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    // Top-k: keep only the top k candidates
    let k = if params.top_k > 0 {
        params.top_k.min(indexed.len())
    } else {
        indexed.len()
    };
    indexed.truncate(k);

    // Softmax over the kept candidates
    let max_val = indexed[0].1;
    let mut probs: Vec<(usize, f32)> = indexed
        .iter()
        .map(|&(idx, v)| (idx, (v - max_val).exp()))
        .collect();
    let sum: f32 = probs.iter().map(|(_, p)| p).sum();
    for (_, p) in probs.iter_mut() {
        *p /= sum;
    }

    // Min-p: remove candidates with prob < min_p * max_prob
    if params.min_p > 0.0 {
        let max_prob = probs.iter().map(|(_, p)| *p).fold(0.0f32, f32::max);
        let threshold = params.min_p * max_prob;
        probs.retain(|(_, p)| *p >= threshold);
        // Renormalize
        let sum: f32 = probs.iter().map(|(_, p)| p).sum();
        if sum > 0.0 {
            for (_, p) in probs.iter_mut() {
                *p /= sum;
            }
        }
    }

    // Top-p / nucleus: keep smallest set with cumulative prob >= top_p
    if params.top_p < 1.0 {
        let mut cumulative = 0.0;
        let mut cutoff = probs.len();
        for (i, (_, p)) in probs.iter().enumerate() {
            cumulative += p;
            if cumulative >= params.top_p {
                cutoff = i + 1;
                break;
            }
        }
        probs.truncate(cutoff);
        // Renormalize
        let sum: f32 = probs.iter().map(|(_, p)| p).sum();
        for (_, p) in probs.iter_mut() {
            *p /= sum;
        }
    }

    // Multinomial sample
    let mut rng_val: f32 = rand::random();
    for (idx, prob) in &probs {
        rng_val -= prob;
        if rng_val <= 0.0 {
            return *idx;
        }
    }
    probs.last().map(|(idx, _)| *idx).unwrap_or(0)
}

/// Load a WAV file and resample to 24kHz mono f32 samples.
pub(crate) fn load_reference_audio(wav_path: &Path) -> Result<Vec<f32>, TtsError> {
    let reader = hound::WavReader::open(wav_path)
        .map_err(|e| TtsError::Init(format!("failed to open reference WAV: {e}")))?;

    let spec = reader.spec();
    let channels = spec.channels as usize;

    // Read samples as f32
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

    // Mix down to mono if stereo
    let mono: Vec<f32> = if channels > 1 {
        samples
            .chunks(channels)
            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
            .collect()
    } else {
        samples
    };

    // Resample to 24kHz if needed
    let source_rate = spec.sample_rate;
    if source_rate == S3GEN_SR {
        return Ok(mono);
    }

    Ok(resample(&mono, source_rate, S3GEN_SR))
}

/// Simple linear-interpolation resampler. Good enough for reference audio.
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
