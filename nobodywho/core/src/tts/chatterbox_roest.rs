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
use crate::tts::{ort_execution_providers, TtsDevice};
use ort::memory::Allocator;
use ort::session::builder::GraphOptimizationLevel;
use ort::session::builder::SessionBuilder;
use ort::session::Session;
use ort::value::Shape;
use ort::value::Tensor;
use ort::value::Value;
use std::borrow::Cow;
use std::path::Path;
use std::sync::Mutex;
use std::time::{Duration, Instant};
use tracing::info;
use unicode_normalization::UnicodeNormalization;

const S3GEN_SR: u32 = 24000;
const START_TEXT_TOKEN: i64 = 255; // SOT — prepended to text tokens
const STOP_TEXT_TOKEN: i64 = 0; // EOT — appended to text tokens
const START_SPEECH_TOKEN: i64 = 6561;
const STOP_SPEECH_TOKEN: i64 = 6562;
const DEFAULT_MAX_NEW_TOKENS: usize = 1000;
const REPETITION_PENALTY: f32 = 2.0;

pub(crate) struct SamplingParams {
    pub temperature: f32,
    #[allow(dead_code)]
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
    text_pos_emb: TensorData<f32>,
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

struct ModelConfig {
    text_token_offset: i64,
    text_pos_emb_shape: Vec<usize>,
}

struct DebugSampler {
    uniforms: Option<Vec<f32>>,
    next_idx: usize,
    forced_tokens: Vec<i64>,
}

struct FirstStepDump<'a> {
    inputs_embeds: &'a [f32],
    inputs_embeds_shape: [usize; 3],
    attention_mask: &'a [i64],
    attention_mask_shape: [usize; 2],
    logits: &'a [f32],
    logits_shape: [usize; 3],
    final_logits: &'a [f32],
}

struct StepDump<'a> {
    step: usize,
    logits: &'a [f32],
    logits_shape: [usize; 3],
    processed_logits: &'a [f32],
    generated: &'a [i64],
}

impl RoestModel {
    pub fn new(model_dir: &Path, device: TtsDevice) -> Result<Self, TtsError> {
        let onnx_dir = model_dir.join("onnx");

        let tokenizer = tokenizers::Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|e| TtsError::Init(format!("failed to load tokenizer: {e}")))?;

        let embed_tokens = load_session(&onnx_dir.join("embed_tokens.onnx"), device)?;
        let language_model = find_and_load_language_model(&onnx_dir, device)?;
        let conditional_decoder = load_session(&onnx_dir.join("conditional_decoder.onnx"), device)?;

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

        let cond = load_precomputed_cond(&model_dir.join("default_cond")).ok_or_else(|| {
            TtsError::Init("missing default_cond/ directory with pre-computed conditioning".into())
        })?;
        let config = load_model_config(model_dir)?;
        let text_pos_emb = load_text_pos_emb(model_dir, &config.text_pos_emb_shape)?;
        let text_token_offset = config.text_token_offset;

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
            text_pos_emb,
            num_layers,
            num_kv_heads,
            head_dim,
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

        // Tokenize
        let encoding = self
            .tokenizer
            .encode(prepared.as_str(), false)
            .map_err(|e| TtsError::Synthesis(format!("tokenization failed: {e}")))?;
        let raw_ids: Vec<i64> = encoding.get_ids().iter().map(|&id| id as i64).collect();
        let tokenized_at = Instant::now();

        // Match the torch multilingual wrapper:
        // text_tokens = [SOT, text_tokens(offset), EOT]
        // START_SPEECH is embedded separately and appended after conditioning/text.
        let mut text_input_ids: Vec<i64> = Vec::with_capacity(raw_ids.len() + 2);
        text_input_ids.push(START_TEXT_TOKEN + self.text_token_offset); // SOT
        for &id in &raw_ids {
            text_input_ids.push(id + self.text_token_offset);
        }
        text_input_ids.push(STOP_TEXT_TOKEN + self.text_token_offset); // EOT

        // The ONNX embed export uses explicit position_ids; pass the same
        // text positions the torch path would use for learned text_pos_emb.
        let text_position_ids: Vec<i64> = (0..text_input_ids.len() as i64).collect();

        info!(
            tokens = text_input_ids.len(),
            text,
            prepared = prepared.as_str(),
            elapsed = ?tokenized_at.duration_since(synth_start),
            "Røst: tokenized"
        );

        // Generate speech tokens
        let generate_start = Instant::now();
        let speech_tokens =
            self.generate_speech_tokens(&text_input_ids, &text_position_ids, sampling)?;
        let generated_at = Instant::now();

        // Prepend prompt tokens, decode to audio
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

    fn generate_speech_tokens(
        &self,
        text_input_ids: &[i64],
        text_position_ids: &[i64],
        sampling: &SamplingParams,
    ) -> Result<Vec<i64>, TtsError> {
        let use_cfg = sampling.cfg_weight > 0.0;
        let batch_size = if use_cfg { 2usize } else { 1usize };
        let mut generated: Vec<i64> = Vec::with_capacity(DEFAULT_MAX_NEW_TOKENS + 1);
        generated.push(START_SPEECH_TOKEN);
        let mut debug_sampler = DebugSampler::from_env()?;

        let mut kv_cache: Vec<Vec<f32>> = (0..self.num_layers * 2).map(|_| Vec::new()).collect();
        let mut kv_seq_len: usize = 0;
        let mut attention_mask: Vec<i64> = Vec::with_capacity(
            self.cond.cond_emb.shape[1] + text_input_ids.len() + DEFAULT_MAX_NEW_TOKENS,
        );
        let mut embed_time = Duration::ZERO;
        let mut lm_time = Duration::ZERO;
        let mut sample_time = Duration::ZERO;

        let mut embed_session = self
            .embed_tokens
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;
        let mut lm_session = self
            .language_model
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;

        let bos_start = Instant::now();
        let bos_embeds = {
            let bos_tensor = Tensor::from_array(([1, 1], vec![START_SPEECH_TOKEN]))
                .map_err(|e| TtsError::Synthesis(format!("bos ids tensor: {e}")))?;
            let mut bos_inputs: Vec<(Cow<'_, str>, ort::session::SessionInputValue<'_>)> = vec![(
                Cow::Borrowed("input_ids"),
                ort::session::SessionInputValue::Owned(Value::from(bos_tensor)),
            )];
            if self.embed_has_position_ids {
                let bos_pos_tensor = Tensor::from_array(([1, 1], vec![0_i64]))
                    .map_err(|e| TtsError::Synthesis(format!("bos pos tensor: {e}")))?;
                bos_inputs.push((
                    Cow::Borrowed("position_ids"),
                    ort::session::SessionInputValue::Owned(Value::from(bos_pos_tensor)),
                ));
            }
            extract_f32_tensor(
                &embed_session
                    .run(ort::session::SessionInputs::from(bos_inputs))
                    .map_err(|e| TtsError::Synthesis(format!("embed_tokens bos: {e}")))?[0],
                "bos_inputs_embeds",
            )?
        };
        let bos_elapsed = bos_start.elapsed();

        for step in 0..DEFAULT_MAX_NEW_TOKENS {
            let embed_start = Instant::now();
            let (current_ids, current_pos) = if step == 0 {
                (text_input_ids.to_vec(), text_position_ids.to_vec())
            } else {
                let last_token = *generated.last().unwrap();
                (vec![last_token], vec![step as i64])
            };

            let cur_seq_len = current_ids.len();
            let ids_tensor = Tensor::from_array(([1, cur_seq_len], current_ids))
                .map_err(|e| TtsError::Synthesis(format!("ids tensor: {e}")))?;

            let mut embed_inputs: Vec<(Cow<'_, str>, ort::session::SessionInputValue<'_>)> =
                vec![(
                    Cow::Borrowed("input_ids"),
                    ort::session::SessionInputValue::Owned(Value::from(ids_tensor)),
                )];
            if self.embed_has_position_ids {
                let pos_tensor = Tensor::from_array(([1, cur_seq_len], current_pos))
                    .map_err(|e| TtsError::Synthesis(format!("pos tensor: {e}")))?;
                embed_inputs.push((
                    Cow::Borrowed("position_ids"),
                    ort::session::SessionInputValue::Owned(Value::from(pos_tensor)),
                ));
            }

            let embeds = extract_f32_tensor(
                &embed_session
                    .run(ort::session::SessionInputs::from(embed_inputs))
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
                let speech_seq = bos_embeds.shape[1];
                let bos_seq = bos_embeds.shape[1];
                let total = cond_seq + text_seq + speech_seq + bos_seq;
                let mut data = Vec::with_capacity(batch_size * total * hidden_dim);

                // Upstream prepare_input_embeds includes a single START_SPEECH token,
                // and inference() then appends another BOS embed before the first LM call.
                data.extend_from_slice(&self.cond.cond_emb.data);
                data.extend_from_slice(&embeds.data);
                data.extend_from_slice(&bos_embeds.data);
                data.extend_from_slice(&bos_embeds.data);

                if use_cfg {
                    // Batch 1: same conditioning, text position embeddings only,
                    // then the same duplicated START_SPEECH embeddings.
                    data.extend_from_slice(&self.cond.cond_emb.data);
                    data.extend_from_slice(self.text_position_slice(text_seq, hidden_dim)?);
                    data.extend_from_slice(&bos_embeds.data);
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
            embed_time += embed_start.elapsed();

            if step == 0 {
                attention_mask = vec![1i64; lm_seq_len];
            } else {
                attention_mask.push(1);
            }

            let dump_first_step = step == 0 && std::env::var_os("NOBODYWHO_TTS_DUMP_DIR").is_some();
            let dump_inputs_embeds = if dump_first_step {
                Some(lm_embeds_data.clone())
            } else {
                None
            };
            let embeds_tensor =
                Tensor::from_array(([batch_size, lm_seq_len, hidden_dim], lm_embeds_data))
                    .map_err(|e| TtsError::Synthesis(format!("embeds tensor: {e}")))?;
            let attn_data: Vec<i64> = (0..batch_size)
                .flat_map(|_| attention_mask.iter().copied())
                .collect();
            let dump_attention_mask = if dump_first_step {
                Some(attn_data.clone())
            } else {
                None
            };
            let attn_tensor = Tensor::from_array(([batch_size, attention_mask.len()], attn_data))
                .map_err(|e| TtsError::Synthesis(format!("attn tensor: {e}")))?;

            let mut lm_inputs: Vec<(Cow<'_, str>, ort::session::SessionInputValue<'_>)> = vec![
                (
                    Cow::Borrowed("inputs_embeds"),
                    ort::session::SessionInputValue::Owned(Value::from(embeds_tensor)),
                ),
                (
                    Cow::Borrowed("attention_mask"),
                    ort::session::SessionInputValue::Owned(Value::from(attn_tensor)),
                ),
            ];

            // KV cache
            for layer in 0..self.num_layers {
                for (kv_idx, kv_name) in ["key", "value"].iter().enumerate() {
                    let cache_idx = layer * 2 + kv_idx;
                    let name = format!("past_key_values.{layer}.{kv_name}");
                    let tensor = if kv_seq_len == 0 {
                        Tensor::<f32>::new(
                            &Allocator::default(),
                            Shape::new([
                                batch_size as i64,
                                self.num_kv_heads as i64,
                                0,
                                self.head_dim as i64,
                            ]),
                        )
                        .map_err(|e| TtsError::Synthesis(format!("empty kv {name}: {e}")))?
                    } else {
                        let cache_data = std::mem::take(&mut kv_cache[cache_idx]);
                        Tensor::from_array((
                            [batch_size, self.num_kv_heads, kv_seq_len, self.head_dim],
                            cache_data,
                        ))
                        .map_err(|e| TtsError::Synthesis(format!("kv {name}: {e}")))?
                    };
                    lm_inputs.push((
                        Cow::Owned(name),
                        ort::session::SessionInputValue::Owned(Value::from(tensor)),
                    ));
                }
            }

            let lm_start = Instant::now();
            let lm_outputs = lm_session
                .run(ort::session::SessionInputs::from(lm_inputs))
                .map_err(|e| TtsError::Synthesis(format!("language model step {step}: {e}")))?;
            lm_time += lm_start.elapsed();

            let sample_start = Instant::now();
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

            if dump_first_step {
                maybe_dump_first_step(&FirstStepDump {
                    inputs_embeds: dump_inputs_embeds.as_deref().unwrap_or(&[]),
                    inputs_embeds_shape: [batch_size, lm_seq_len, hidden_dim],
                    attention_mask: dump_attention_mask.as_deref().unwrap_or(&[]),
                    attention_mask_shape: [batch_size, attention_mask.len()],
                    logits: &logits.data,
                    logits_shape: [batch_size, out_seq, vocab_size],
                    final_logits: &final_logits,
                })?;
            }

            apply_repetition_penalty(&mut final_logits, &generated, REPETITION_PENALTY);
            let mut processed_logits = final_logits.clone();
            if sampling.temperature > 1e-6 {
                if sampling.temperature != 1.0 {
                    for score in processed_logits.iter_mut() {
                        *score /= sampling.temperature;
                    }
                }
                apply_min_p_warper(&mut processed_logits, sampling.min_p);
                apply_top_p_warper(&mut processed_logits, sampling.top_p);
            }
            maybe_dump_step(&StepDump {
                step,
                logits: &logits.data,
                logits_shape: [batch_size, out_seq, vocab_size],
                processed_logits: &processed_logits,
                generated: &generated,
            })?;
            let next_token = if let Some(token) = debug_sampler.forced_token(step) {
                token
            } else {
                sample_token(&mut final_logits, sampling, &mut debug_sampler)? as i64
            };
            generated.push(next_token);

            if next_token == STOP_SPEECH_TOKEN {
                break;
            }

            let new_kv_seq_len = kv_seq_len + lm_seq_len;
            for i in 0..(self.num_layers * 2) {
                kv_cache[i] = extract_f32_tensor(&lm_outputs[1 + i], &format!("kv_{i}"))?.data;
            }
            kv_seq_len = new_kv_seq_len;
            sample_time += sample_start.elapsed();

            if step % 50 == 0 && step > 0 {
                info!(
                    step,
                    generated = generated.len().saturating_sub(1),
                    embed_elapsed = ?embed_time,
                    lm_elapsed = ?lm_time,
                    sample_elapsed = ?sample_time,
                    "Røst: generation progress"
                );
            }
        }

        let generated_count = generated.len().saturating_sub(1);
        let total_loop = embed_time + lm_time + sample_time;
        info!(
            generated = generated_count,
            bos_elapsed = ?bos_elapsed,
            embed_elapsed = ?embed_time,
            lm_elapsed = ?lm_time,
            sample_elapsed = ?sample_time,
            loop_elapsed = ?total_loop,
            tokens_per_sec = if total_loop.is_zero() {
                0.0
            } else {
                generated_count as f64 / total_loop.as_secs_f64()
            },
            "Røst: generation timings"
        );

        Ok(generated
            .into_iter()
            .filter(|&t| t != START_SPEECH_TOKEN && t != STOP_SPEECH_TOKEN)
            .collect())
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
        let input_prep_elapsed = decode_start.elapsed();

        let mut session = self
            .conditional_decoder
            .lock()
            .map_err(|e| TtsError::Synthesis(format!("lock: {e}")))?;
        let run_start = Instant::now();
        let outputs = session
            .run(ort::session::SessionInputs::from(inputs))
            .map_err(|e| TtsError::Synthesis(format!("conditional decoder: {e}")))?;
        let run_elapsed = run_start.elapsed();

        let extract_start = Instant::now();
        let wav = extract_f32_tensor(&outputs[0], "wav")?.data;
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
}

// ── Helpers ──────────────────────────────────────────────────────────────────

#[derive(Clone)]
struct TensorData<T> {
    data: Vec<T>,
    shape: Vec<usize>,
}

fn maybe_dump_first_step(dump: &FirstStepDump<'_>) -> Result<(), TtsError> {
    let Some(dir) = std::env::var_os("NOBODYWHO_TTS_DUMP_DIR") else {
        return Ok(());
    };
    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir)
        .map_err(|e| TtsError::Synthesis(format!("create dump dir {}: {e}", dir.display())))?;

    write_f32_bin(&dir.join("rust_inputs_embeds.bin"), dump.inputs_embeds)?;
    write_i64_bin(&dir.join("rust_attention_mask.bin"), dump.attention_mask)?;
    write_f32_bin(&dir.join("rust_logits.bin"), dump.logits)?;
    write_f32_bin(
        &dir.join("rust_final_logits_pre_penalty.bin"),
        dump.final_logits,
    )?;

    let manifest = serde_json::json!({
        "inputs_embeds_shape": dump.inputs_embeds_shape,
        "attention_mask_shape": dump.attention_mask_shape,
        "logits_shape": dump.logits_shape,
        "final_logits_shape": [dump.final_logits.len()],
    });
    std::fs::write(
        dir.join("rust_manifest.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|e| TtsError::Synthesis(format!("serialize dump manifest: {e}")))?,
    )
    .map_err(|e| TtsError::Synthesis(format!("write dump manifest: {e}")))?;
    Ok(())
}

fn maybe_dump_step(dump: &StepDump<'_>) -> Result<(), TtsError> {
    let Some(dir) = std::env::var_os("NOBODYWHO_TTS_DUMP_DIR") else {
        return Ok(());
    };
    let target_step = std::env::var("NOBODYWHO_TTS_DUMP_STEP")
        .ok()
        .and_then(|s| s.parse::<usize>().ok());
    if target_step != Some(dump.step) {
        return Ok(());
    }

    let dir = Path::new(&dir);
    std::fs::create_dir_all(dir)
        .map_err(|e| TtsError::Synthesis(format!("create dump dir {}: {e}", dir.display())))?;

    write_f32_bin(&dir.join("rust_step_logits.bin"), dump.logits)?;
    write_f32_bin(
        &dir.join("rust_step_final_logits.bin"),
        dump.processed_logits,
    )?;
    write_i64_bin(&dir.join("rust_step_generated.bin"), dump.generated)?;

    let manifest = serde_json::json!({
        "step": dump.step,
        "logits_shape": dump.logits_shape,
        "final_logits_shape": [dump.processed_logits.len()],
        "generated_len": dump.generated.len(),
    });
    std::fs::write(
        dir.join("rust_step_manifest.json"),
        serde_json::to_vec_pretty(&manifest)
            .map_err(|e| TtsError::Synthesis(format!("serialize step manifest: {e}")))?,
    )
    .map_err(|e| TtsError::Synthesis(format!("write step manifest: {e}")))?;
    Ok(())
}

fn write_f32_bin(path: &Path, data: &[f32]) -> Result<(), TtsError> {
    let mut bytes = Vec::with_capacity(data.len() * 4);
    for &value in data {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes)
        .map_err(|e| TtsError::Synthesis(format!("write {}: {e}", path.display())))
}

fn write_i64_bin(path: &Path, data: &[i64]) -> Result<(), TtsError> {
    let mut bytes = Vec::with_capacity(data.len() * 8);
    for &value in data {
        bytes.extend_from_slice(&value.to_le_bytes());
    }
    std::fs::write(path, bytes)
        .map_err(|e| TtsError::Synthesis(format!("write {}: {e}", path.display())))
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

#[allow(dead_code)]
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

fn load_precomputed_cond(dir: &Path) -> Option<PrecomputedCond> {
    let manifest_str = std::fs::read_to_string(dir.join("manifest.json")).ok()?;
    let manifest: serde_json::Value = serde_json::from_str(&manifest_str).ok()?;

    let load_f32 = |name: &str| -> Option<TensorData<f32>> {
        let shape: Vec<usize> = manifest[name]["shape"]
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
        let shape: Vec<usize> = manifest[name]["shape"]
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

fn load_session(path: &Path, device: TtsDevice) -> Result<Session, TtsError> {
    SessionBuilder::new()
        .map_err(|e| TtsError::Init(format!("ort session builder: {e}")))?
        .with_log_level(ort::logging::LogLevel::Warning)
        .map_err(|e| TtsError::Init(format!("ort log level: {e}")))?
        .with_optimization_level(GraphOptimizationLevel::Disable)
        .map_err(|e| TtsError::Init(format!("ort optimization level: {e}")))?
        .with_execution_providers(ort_execution_providers(device))
        .map_err(|e| TtsError::Init(format!("ort execution providers: {e}")))?
        .commit_from_file(path)
        .map_err(|e| TtsError::Init(format!("ort load model {}: {e}", path.display())))
}

fn find_and_load_language_model(onnx_dir: &Path, device: TtsDevice) -> Result<Session, TtsError> {
    for name in [
        "language_model.onnx",
        "language_model_q4.onnx",
        "language_model_fp16.onnx",
    ] {
        let path = onnx_dir.join(name);
        if path.exists() {
            info!(model = name, "Loading language model");
            return load_session(&path, device);
        }
    }
    Err(TtsError::Init("no language model ONNX file found".into()))
}

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

fn prepare_text_for_mtl_tokenizer(text: &str, language: &str) -> String {
    let punctuated = punc_norm(text);
    let normalized: String = punctuated.to_lowercase().nfkd().collect();
    let language = if language.is_empty() { "da" } else { language }.to_lowercase();
    format!("[{}]{}", language, normalized).replace(' ', "[SPACE]")
}

fn apply_repetition_penalty(logits: &mut [f32], generated: &[i64], penalty: f32) {
    let mut seen = vec![false; logits.len()];
    for &token_id in generated {
        let idx = token_id as usize;
        if idx >= logits.len() || seen[idx] {
            continue;
        }
        seen[idx] = true;
        if let Some(score) = logits.get_mut(idx) {
            if *score < 0.0 {
                *score *= penalty;
            } else {
                *score /= penalty;
            }
        }
    }
}

impl DebugSampler {
    fn from_env() -> Result<Self, TtsError> {
        let uniforms = std::env::var("NOBODYWHO_TTS_SAMPLE_UNIFORMS")
            .ok()
            .map(|s| {
                s.split(',')
                    .filter(|v| !v.trim().is_empty())
                    .map(|v| {
                        v.trim().parse::<f32>().map_err(|e| {
                            TtsError::Synthesis(format!(
                                "invalid NOBODYWHO_TTS_SAMPLE_UNIFORMS entry `{v}`: {e}"
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?;
        let forced_tokens = std::env::var("NOBODYWHO_TTS_FORCE_TOKENS")
            .ok()
            .map(|s| {
                s.split(',')
                    .filter(|v| !v.trim().is_empty())
                    .map(|v| {
                        v.trim().parse::<i64>().map_err(|e| {
                            TtsError::Synthesis(format!(
                                "invalid NOBODYWHO_TTS_FORCE_TOKENS entry `{v}`: {e}"
                            ))
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()
            })
            .transpose()?
            .unwrap_or_default();
        Ok(Self {
            uniforms,
            next_idx: 0,
            forced_tokens,
        })
    }

    fn next_uniform(&mut self) -> f32 {
        match &self.uniforms {
            Some(values) if !values.is_empty() => {
                let idx = self.next_idx.min(values.len() - 1);
                self.next_idx += 1;
                values[idx]
            }
            _ => rand::random(),
        }
    }

    fn forced_token(&self, step: usize) -> Option<i64> {
        self.forced_tokens.get(step).copied()
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

fn sample_token(
    logits: &mut [f32],
    params: &SamplingParams,
    debug_sampler: &mut DebugSampler,
) -> Result<usize, TtsError> {
    if params.temperature <= 1e-6 {
        return Ok(argmax(logits));
    }

    if params.temperature != 1.0 {
        for score in logits.iter_mut() {
            *score /= params.temperature;
        }
    }

    apply_min_p_warper(logits, params.min_p);
    apply_top_p_warper(logits, params.top_p);

    Ok(sample_from_masked_logits(logits, debug_sampler))
}

fn apply_min_p_warper(logits: &mut [f32], min_p: f32) {
    if min_p <= 0.0 {
        return;
    }

    let probs = softmax(logits);
    let (_, top_prob) = probs
        .iter()
        .copied()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .unwrap_or((0, 0.0));
    let threshold = min_p * top_prob;
    let mut tokens_to_remove: Vec<bool> = probs.iter().map(|&p| p < threshold).collect();

    let mut sorted_indices: Vec<usize> = (0..logits.len()).collect();
    sorted_indices.sort_unstable_by(|&a, &b| {
        logits[b]
            .partial_cmp(&logits[a])
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    // Match HF MinPLogitsWarper(min_tokens_to_keep=1).
    if let Some(&idx) = sorted_indices.first() {
        tokens_to_remove[idx] = false;
    }
    for (idx, score) in logits.iter_mut().enumerate() {
        if tokens_to_remove[idx] {
            *score = f32::NEG_INFINITY;
        }
    }
}

fn apply_top_p_warper(logits: &mut [f32], top_p: f32) {
    if top_p >= 1.0 {
        return;
    }

    let mut sorted: Vec<(usize, f32)> = logits.iter().copied().enumerate().collect();
    sorted.sort_unstable_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));

    let sorted_logits: Vec<f32> = sorted.iter().map(|(_, score)| *score).collect();
    let sorted_probs = softmax(&sorted_logits);
    let mut cumulative = 0.0f32;
    let cutoff = 1.0 - top_p;

    for (i, (orig_idx, _)) in sorted.iter().enumerate() {
        cumulative += sorted_probs[i];
        if cumulative <= cutoff && i + 1 < sorted.len() {
            logits[*orig_idx] = f32::NEG_INFINITY;
        }
    }
}

fn sample_from_masked_logits(logits: &[f32], debug_sampler: &mut DebugSampler) -> usize {
    let probs = softmax(logits);
    let mut rng = debug_sampler.next_uniform() as f64;
    for (idx, prob) in probs.iter().copied().enumerate() {
        rng -= prob as f64;
        if rng <= 0.0 {
            return idx;
        }
    }
    probs
        .iter()
        .enumerate()
        .rfind(|(_, p)| **p > 0.0)
        .map(|(idx, _)| idx)
        .unwrap_or(0)
}

fn softmax(logits: &[f32]) -> Vec<f32> {
    let max_logit = logits
        .iter()
        .copied()
        .filter(|v| v.is_finite())
        .fold(f32::NEG_INFINITY, f32::max);

    if !max_logit.is_finite() {
        let mut probs = vec![0.0; logits.len()];
        if let Some(first) = probs.first_mut() {
            *first = 1.0;
        }
        return probs;
    }

    let mut probs: Vec<f32> = logits
        .iter()
        .map(|&v| {
            if v.is_finite() {
                (v - max_logit).exp()
            } else {
                0.0
            }
        })
        .collect();
    let sum: f32 = probs.iter().sum();
    if sum > 0.0 {
        for prob in &mut probs {
            *prob /= sum;
        }
    } else if let Some(first) = probs.first_mut() {
        *first = 1.0;
    }
    probs
}
