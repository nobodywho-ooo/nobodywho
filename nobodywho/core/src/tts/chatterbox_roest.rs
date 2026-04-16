/// Røst TTS — Danish finetuned Chatterbox via ONNX Runtime.
///
/// Uses pre-computed conditioning (from Python export script) instead of
/// running the speech_encoder ONNX, since the finetuned cond_enc weights
/// were fused during the base ONNX export and can't be swapped.
///
/// Expected model directory layout:
///   dir/
///     tokenizer.json           — Røst MTLTokenizer (post-processor stripped)
///     model_config.json        — {"text_token_offset": 8194}
///     default_cond/            — pre-computed conditioning (manifest.json + .bin files)
///     onnx/embed_tokens.onnx   — exported from Røst via torch.onnx.export
///     onnx/language_model.onnx — exported from Røst via torch.onnx.export
///     onnx/conditional_decoder.onnx — from base chatterbox (s3gen unchanged)
///     onnx/speech_encoder.onnx      — from base chatterbox (unused, kept for compat)
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
const START_TEXT_TOKEN: i64 = 255; // SOT — prepended to text tokens
const STOP_TEXT_TOKEN: i64 = 0;   // EOT — appended to text tokens
const START_SPEECH_TOKEN: i64 = 6561;
const STOP_SPEECH_TOKEN: i64 = 6562;
const DEFAULT_MAX_NEW_TOKENS: usize = 1000;
const REPETITION_PENALTY: f32 = 2.0;

pub(crate) struct SamplingParams {
    pub temperature: f32,
    pub top_k: usize,
    pub top_p: f32,
    pub min_p: f32,
    #[allow(dead_code)]
    pub cfg_weight: f32,
}

pub(crate) struct RoestModel {
    embed_tokens: Mutex<Session>,
    language_model: Mutex<Session>,
    conditional_decoder: Mutex<Session>,
    tokenizer: tokenizers::Tokenizer,
    cond: PrecomputedCond,
    text_token_offset: i64,
    num_layers: usize,
    num_kv_heads: usize,
    head_dim: usize,
    embed_has_position_ids: bool,
}

struct PrecomputedCond {
    cond_emb: TensorData<f32>,
    prompt_token: TensorData<i64>,
    ref_x_vector: TensorData<f32>,
    prompt_feat: TensorData<f32>,
}

impl RoestModel {
    pub fn new(model_dir: &Path) -> Result<Self, TtsError> {
        let onnx_dir = model_dir.join("onnx");

        let tokenizer = tokenizers::Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|e| TtsError::Init(format!("failed to load tokenizer: {e}")))?;

        let embed_tokens = load_session(&onnx_dir.join("embed_tokens.onnx"))?;
        let language_model = find_and_load_language_model(&onnx_dir)?;
        let conditional_decoder = load_session(&onnx_dir.join("conditional_decoder.onnx"))?;

        let num_layers = language_model
            .inputs()
            .iter()
            .filter(|i| i.name().starts_with("past_key_values.") && i.name().ends_with(".key"))
            .count();
        let num_kv_heads = 16;
        let head_dim = 64;

        let embed_has_position_ids = embed_tokens
            .inputs()
            .iter()
            .any(|i| i.name() == "position_ids");

        let cond = load_precomputed_cond(&model_dir.join("default_cond"))
            .ok_or_else(|| TtsError::Init("missing default_cond/ directory with pre-computed conditioning".into()))?;

        let text_token_offset = load_config_offset(model_dir)?;

        info!(
            num_layers,
            text_token_offset,
            cond_seq = cond.cond_emb.shape[1],
            "Loaded Røst TTS"
        );

        Ok(Self {
            embed_tokens: Mutex::new(embed_tokens),
            language_model: Mutex::new(language_model),
            conditional_decoder: Mutex::new(conditional_decoder),
            tokenizer,
            cond,
            text_token_offset,
            num_layers,
            num_kv_heads,
            head_dim,
            embed_has_position_ids,
        })
    }

    pub fn synthesize(
        &self,
        text: &str,
        sampling: &SamplingParams,
    ) -> Result<Vec<f32>, TtsError> {
        // Preprocess: lowercase → NFKD → [da] tag → [SPACE] replacement
        // The [da] language tag is required by the multilingual Llama backbone.
        let normalized: String = text.to_lowercase().nfkd().collect();
        let prepared = format!("[da]{}", normalized).replace(' ', "[SPACE]");

        // Tokenize
        let encoding = self
            .tokenizer
            .encode(prepared.as_str(), true)
            .map_err(|e| TtsError::Synthesis(format!("tokenization failed: {e}")))?;
        let raw_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();

        // Match the torch multilingual wrapper:
        // text_tokens = [SOT, text_tokens(offset), EOT]
        // START_SPEECH is embedded separately and appended after conditioning/text.
        let mut text_input_ids: Vec<i64> = Vec::with_capacity(raw_ids.len() + 2);
        text_input_ids.push(START_TEXT_TOKEN); // SOT
        for &id in &raw_ids {
            text_input_ids.push(id + self.text_token_offset);
        }
        text_input_ids.push(STOP_TEXT_TOKEN); // EOT

        // The ONNX export keeps position_ids in the signature for compatibility,
        // but the Røst embed wrapper ignores them. We still pass the same values
        // the torch path would conceptually use for text tokens.
        let text_position_ids: Vec<i64> = (0..text_input_ids.len() as i64).collect();

        info!(tokens = text_input_ids.len(), text, "Røst: tokenized");

        // Generate speech tokens
        let speech_tokens = self.generate_speech_tokens(&text_input_ids, &text_position_ids, sampling)?;

        // Prepend prompt tokens, decode to audio
        let mut full_tokens: Vec<i64> = self.cond.prompt_token.data.clone();
        full_tokens.extend_from_slice(&speech_tokens);

        info!(generated = speech_tokens.len(), total = full_tokens.len(), "Røst: speech tokens");

        let pcm = self.decode_speech(&full_tokens)?;

        info!(
            samples = pcm.len(),
            duration_secs = pcm.len() as f32 / S3GEN_SR as f32,
            "Røst: synthesis complete"
        );
        Ok(pcm)
    }

    fn generate_speech_tokens(
        &self,
        text_input_ids: &[i64],
        text_position_ids: &[i64],
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

        let mut embed_session = self.embed_tokens.lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;
        let mut lm_session = self.language_model.lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;

        for step in 0..DEFAULT_MAX_NEW_TOKENS {
            let (current_ids, current_pos) = if step == 0 {
                (text_input_ids.to_vec(), text_position_ids.to_vec())
            } else {
                let last_token = *generated.last().unwrap();
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
                embed_inputs.push((Cow::Borrowed("position_ids"),
                    ort::session::SessionInputValue::Owned(Value::from(pos_tensor))));
            }

            let embeds = extract_f32_tensor(
                &embed_session.run(ort::session::SessionInputs::from(embed_inputs))
                    .map_err(|e| TtsError::Synthesis(format!("embed_tokens: {e}")))?[0],
                "inputs_embeds",
            )?;
            let hidden_dim = embeds.shape[embeds.shape.len() - 1];

            // Match torch T3 inference:
            // 1. Build cond_emb + text_emb for batch 0.
            // 2. For CFG, batch 1 gets the same cond_emb but zeroed text embeddings.
            // 3. Append the same START_SPEECH embedding to both batches.
            let (lm_embeds_data, lm_seq_len) = if step == 0 {
                let cond_seq = self.cond.cond_emb.shape[1];
                let text_seq = embeds.shape[1];

                let bos_ids = vec![START_SPEECH_TOKEN];
                let bos_pos = vec![0_i64];
                let bos_tensor = Tensor::from_array(([1, 1], bos_ids))
                    .map_err(|e| TtsError::Synthesis(format!("bos ids tensor: {e}")))?;
                let mut bos_inputs: Vec<(Cow<'_, str>, ort::session::SessionInputValue<'_>)> = vec![
                    (Cow::Borrowed("input_ids"), ort::session::SessionInputValue::Owned(Value::from(bos_tensor))),
                ];
                if self.embed_has_position_ids {
                    let bos_pos_tensor = Tensor::from_array(([1, 1], bos_pos))
                        .map_err(|e| TtsError::Synthesis(format!("bos pos tensor: {e}")))?;
                    bos_inputs.push((
                        Cow::Borrowed("position_ids"),
                        ort::session::SessionInputValue::Owned(Value::from(bos_pos_tensor)),
                    ));
                }
                let bos_embeds = extract_f32_tensor(
                    &embed_session
                        .run(ort::session::SessionInputs::from(bos_inputs))
                        .map_err(|e| TtsError::Synthesis(format!("embed_tokens bos: {e}")))?[0],
                    "bos_inputs_embeds",
                )?;
                let bos_seq = bos_embeds.shape[1];
                let total = cond_seq + text_seq + bos_seq;
                let mut data = Vec::with_capacity(batch_size * total * hidden_dim);

                // Batch 0: cond_emb + text embeddings + BOS speech embedding.
                data.extend_from_slice(&self.cond.cond_emb.data);
                data.extend_from_slice(&embeds.data);
                data.extend_from_slice(&bos_embeds.data);

                if use_cfg {
                    // Batch 1: same conditioning, zeroed text embeddings, same BOS speech embedding.
                    data.extend_from_slice(&self.cond.cond_emb.data);
                    data.extend(std::iter::repeat(0.0f32).take(text_seq * hidden_dim));
                    data.extend_from_slice(&bos_embeds.data);
                }
                (data, total)
            } else {
                // Speech token embedding — same for both batches
                let mut data = embeds.data.clone();
                if use_cfg {
                    data.extend_from_slice(&embeds.data);
                }
                (data, embeds.shape[1])
            };

            if step == 0 {
                attention_mask = vec![1i64; lm_seq_len];
            } else {
                attention_mask.push(1);
            }

            let embeds_tensor = Tensor::from_array(([batch_size, lm_seq_len, hidden_dim], lm_embeds_data))
                .map_err(|e| TtsError::Synthesis(format!("embeds tensor: {e}")))?;
            let attn_data: Vec<i64> = (0..batch_size).flat_map(|_| attention_mask.iter().copied()).collect();
            let attn_tensor = Tensor::from_array(([batch_size, attention_mask.len()], attn_data))
                .map_err(|e| TtsError::Synthesis(format!("attn tensor: {e}")))?;

            let mut lm_inputs: Vec<(Cow<'_, str>, ort::session::SessionInputValue<'_>)> = vec![
                (Cow::Borrowed("inputs_embeds"), ort::session::SessionInputValue::Owned(Value::from(embeds_tensor))),
                (Cow::Borrowed("attention_mask"), ort::session::SessionInputValue::Owned(Value::from(attn_tensor))),
            ];

            // KV cache
            for layer in 0..self.num_layers {
                for (kv_idx, kv_name) in ["key", "value"].iter().enumerate() {
                    let cache_idx = layer * 2 + kv_idx;
                    let name = format!("past_key_values.{layer}.{kv_name}");
                    let tensor = if kv_seq_len == 0 {
                        Tensor::<f32>::new(
                            &Allocator::default(),
                            Shape::new([batch_size as i64, self.num_kv_heads as i64, 0, self.head_dim as i64]),
                        ).map_err(|e| TtsError::Synthesis(format!("empty kv {name}: {e}")))?
                    } else {
                        Tensor::from_array((
                            [batch_size, self.num_kv_heads, kv_seq_len, self.head_dim],
                            kv_cache[cache_idx].clone(),
                        )).map_err(|e| TtsError::Synthesis(format!("kv {name}: {e}")))?
                    };
                    lm_inputs.push((Cow::Owned(name), ort::session::SessionInputValue::Owned(Value::from(tensor))));
                }
            }

            let lm_outputs = lm_session
                .run(ort::session::SessionInputs::from(lm_inputs))
                .map_err(|e| TtsError::Synthesis(format!("language model step {step}: {e}")))?;

            let logits = extract_f32_tensor(&lm_outputs[0], "logits")?;
            let vocab_size = logits.shape[2];
            let out_seq = logits.shape[1];
            let mut final_logits = if use_cfg {
                // Match torch T3 CFG exactly:
                // logits = cond + cfg_weight * (cond - uncond)
                let cond_offset = (out_seq - 1) * vocab_size;
                let uncond_offset = (out_seq + out_seq - 1) * vocab_size;
                let cond_logits = &logits.data[cond_offset..cond_offset + vocab_size];
                let uncond_logits = &logits.data[uncond_offset..uncond_offset + vocab_size];
                cond_logits
                    .iter()
                    .zip(uncond_logits.iter())
                    .map(|(&c, &u)| c + sampling.cfg_weight * (c - u))
                    .collect::<Vec<f32>>()
            } else {
                let offset = (out_seq - 1) * vocab_size;
                logits.data[offset..offset + vocab_size].to_vec()
            };

            apply_repetition_penalty(&mut final_logits, &generated, REPETITION_PENALTY);
            let next_token = sample_token(&mut final_logits, sampling) as i64;
            generated.push(next_token);

            if next_token == STOP_SPEECH_TOKEN {
                break;
            }

            let new_kv_seq_len = kv_seq_len + lm_seq_len;
            for i in 0..(self.num_layers * 2) {
                kv_cache[i] = extract_f32_tensor(&lm_outputs[1 + i], &format!("kv_{i}"))?.data;
            }
            kv_seq_len = new_kv_seq_len;

            if step % 50 == 0 && step > 0 {
                info!(step, "Røst: generation progress");
            }
        }

        Ok(generated.into_iter()
            .filter(|&t| t != START_SPEECH_TOKEN && t != STOP_SPEECH_TOKEN)
            .collect())
    }

    fn decode_speech(&self, speech_tokens: &[i64]) -> Result<Vec<f32>, TtsError> {
        let tokens_tensor = Tensor::from_array(([1, speech_tokens.len()], speech_tokens.to_vec()))
            .map_err(|e| TtsError::Synthesis(format!("speech tokens tensor: {e}")))?;
        let speaker_tensor = Tensor::from_array((
            self.cond.ref_x_vector.shape.clone(), self.cond.ref_x_vector.data.clone(),
        )).map_err(|e| TtsError::Synthesis(format!("speaker tensor: {e}")))?;
        let feat_tensor = Tensor::from_array((
            self.cond.prompt_feat.shape.clone(), self.cond.prompt_feat.data.clone(),
        )).map_err(|e| TtsError::Synthesis(format!("feat tensor: {e}")))?;

        let inputs: Vec<(Cow<'_, str>, ort::session::SessionInputValue<'_>)> = vec![
            (Cow::Borrowed("speech_tokens"), ort::session::SessionInputValue::Owned(Value::from(tokens_tensor))),
            (Cow::Borrowed("speaker_embeddings"), ort::session::SessionInputValue::Owned(Value::from(speaker_tensor))),
            (Cow::Borrowed("speaker_features"), ort::session::SessionInputValue::Owned(Value::from(feat_tensor))),
        ];

        let mut session = self.conditional_decoder.lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;
        let outputs = session.run(ort::session::SessionInputs::from(inputs))
            .map_err(|e| TtsError::Synthesis(format!("conditional decoder: {e}")))?;

        Ok(extract_f32_tensor(&outputs[0], "wav")?.data)
    }
}

// ── Helpers ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct TensorData<T> {
    data: Vec<T>,
    shape: Vec<usize>,
}

fn extract_f32_tensor(value: &ort::value::DynValue, name: &str) -> Result<TensorData<f32>, TtsError> {
    let (shape, data) = value.try_extract_tensor::<f32>()
        .map_err(|e| TtsError::Synthesis(format!("extract {name}: {e}")))?;
    Ok(TensorData { data: data.to_vec(), shape: shape.iter().map(|&d| d as usize).collect() })
}

#[allow(dead_code)]
fn extract_i64_tensor(value: &ort::value::DynValue, name: &str) -> Result<TensorData<i64>, TtsError> {
    let (shape, data) = value.try_extract_tensor::<i64>()
        .map_err(|e| TtsError::Synthesis(format!("extract {name}: {e}")))?;
    Ok(TensorData { data: data.to_vec(), shape: shape.iter().map(|&d| d as usize).collect() })
}

fn load_precomputed_cond(dir: &Path) -> Option<PrecomputedCond> {
    let manifest_str = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_str).ok()?;

    let load_f32 = |name: &str| -> Option<TensorData<f32>> {
        let shape: Vec<usize> = manifest[name]["shape"].as_array()?
            .iter().map(|v| v.as_u64().unwrap_or(0) as usize).collect();
        let bytes = std::fs::read(dir.join(format!("{name}.bin"))).ok()?;
        let data: Vec<f32> = bytes.chunks_exact(4)
            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]])).collect();
        Some(TensorData { data, shape })
    };
    let load_i64 = |name: &str| -> Option<TensorData<i64>> {
        let shape: Vec<usize> = manifest[name]["shape"].as_array()?
            .iter().map(|v| v.as_u64().unwrap_or(0) as usize).collect();
        let bytes = std::fs::read(dir.join(format!("{name}.bin"))).ok()?;
        let data: Vec<i64> = bytes.chunks_exact(8)
            .map(|c| i64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])).collect();
        Some(TensorData { data, shape })
    };

    Some(PrecomputedCond {
        cond_emb: load_f32("cond_emb")?,
        prompt_token: load_i64("prompt_token")?,
        ref_x_vector: load_f32("ref_x_vector")?,
        prompt_feat: load_f32("prompt_feat")?,
    })
}

fn load_config_offset(model_dir: &Path) -> Result<i64, TtsError> {
    let path = model_dir.join("model_config.json");
    let s = std::fs::read_to_string(&path)
        .map_err(|e| TtsError::Init(format!("missing model_config.json: {e}")))?;
    let v: serde_json::Value = serde_json::from_str(&s)
        .map_err(|e| TtsError::Init(format!("invalid model_config.json: {e}")))?;
    v["text_token_offset"]
        .as_i64()
        .ok_or_else(|| TtsError::Init("model_config.json missing text_token_offset".into()))
}

fn load_session(path: &Path) -> Result<Session, TtsError> {
    SessionBuilder::new()
        .map_err(|e| TtsError::Init(format!("ort session builder: {e}")))?
        .with_execution_providers([ort::ep::CPU::default().build()])
        .map_err(|e| TtsError::Init(format!("ort execution providers: {e}")))?
        .commit_from_file(path)
        .map_err(|e| TtsError::Init(format!("ort load model {}: {e}", path.display())))
}

fn find_and_load_language_model(onnx_dir: &Path) -> Result<Session, TtsError> {
    for name in ["language_model.onnx", "language_model_q4.onnx", "language_model_fp16.onnx"] {
        let path = onnx_dir.join(name);
        if path.exists() {
            info!(model = name, "Loading language model");
            return load_session(&path);
        }
    }
    Err(TtsError::Init("no language model ONNX file found".into()))
}

fn apply_repetition_penalty(logits: &mut [f32], generated: &[i64], penalty: f32) {
    for &token_id in generated {
        if let Some(score) = logits.get_mut(token_id as usize) {
            if *score < 0.0 { *score *= penalty; } else { *score /= penalty; }
        }
    }
}

fn argmax(values: &[f32]) -> usize {
    values.iter().enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i).unwrap_or(0)
}

fn sample_token(logits: &mut [f32], params: &SamplingParams) -> usize {
    if params.temperature <= 1e-6 { return argmax(logits); }

    for v in logits.iter_mut() { *v /= params.temperature; }

    let mut indexed: Vec<(usize, f32)> = logits.iter().copied().enumerate().collect();
    indexed.sort_unstable_by(|(_, a), (_, b)| b.partial_cmp(a).unwrap_or(std::cmp::Ordering::Equal));

    if params.top_k > 0 { indexed.truncate(params.top_k.min(indexed.len())); }

    let max_val = indexed[0].1;
    let mut probs: Vec<(usize, f32)> = indexed.iter()
        .map(|&(idx, v)| (idx, (v - max_val).exp())).collect();
    let sum: f32 = probs.iter().map(|(_, p)| p).sum();
    for (_, p) in probs.iter_mut() { *p /= sum; }

    if params.min_p > 0.0 {
        let max_prob = probs.iter().map(|(_, p)| *p).fold(0.0f32, f32::max);
        let threshold = params.min_p * max_prob;
        probs.retain(|(_, p)| *p >= threshold);
        let sum: f32 = probs.iter().map(|(_, p)| p).sum();
        if sum > 0.0 { for (_, p) in probs.iter_mut() { *p /= sum; } }
    }

    if params.top_p < 1.0 {
        let mut cum = 0.0;
        let mut cutoff = probs.len();
        for (i, (_, p)) in probs.iter().enumerate() {
            cum += p;
            if cum >= params.top_p { cutoff = i + 1; break; }
        }
        probs.truncate(cutoff);
        let sum: f32 = probs.iter().map(|(_, p)| p).sum();
        for (_, p) in probs.iter_mut() { *p /= sum; }
    }

    let mut rng: f32 = rand::random();
    for (idx, prob) in &probs {
        rng -= prob;
        if rng <= 0.0 { return *idx; }
    }
    probs.last().map(|(idx, _)| *idx).unwrap_or(0)
}
