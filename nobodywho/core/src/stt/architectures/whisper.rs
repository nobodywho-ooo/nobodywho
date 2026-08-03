//! Whisper STT architecture via ONNX Runtime.
//!
//! Pipeline: 16 kHz mono f32 audio → log-mel spectrogram → encoder → greedy
//! token decode with KV cache → text.
//!
//! Uses `hf://onnx-community/whisper-*` model repos which ship:
//!   `onnx/encoder_model.onnx`, `onnx/decoder_model_merged.onnx`,
//!   `tokenizer.json`, `generation_config.json`, `config.json`.

use crate::errors::{HuggingFaceError, SttError};
use crate::onnx::Device;
use crate::stt::architecture::SttArchitectureImpl;
use mel_spec::prelude::*;
use ort::session::Session;
use ort::value::{DynValue, Tensor};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tokenizers::Tokenizer;
use tracing::{debug, info};

/// Mel frames per 30-second Whisper window (hop_length=160 → 480000/160).
const N_MEL_FRAMES: usize = 3_000;
/// Whisper encoder output length (N_MEL_FRAMES / 2 due to 2× downsampling).
const ENC_SEQ_LEN: usize = N_MEL_FRAMES / 2;
const MAX_NEW_TOKENS: usize = 448;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// Default value of [`WhisperConfig::quantization`] — Q4, the smallest and
/// fastest commonly-shipped ONNX variant. Not every `onnx-community/whisper-*`
/// repo ships a Q4 variant, so [`resolve_model_dir`] transparently falls back
/// to [`FALLBACK_QUANTIZATION`] when it's missing.
pub const DEFAULT_QUANTIZATION: &str = "q4";

/// Fallback used by [`resolve_model_dir`] when [`DEFAULT_QUANTIZATION`] isn't
/// available for a given source — the unsuffixed fp32 ONNX variant, which
/// every `onnx-community/whisper-*` repo ships.
const FALLBACK_QUANTIZATION: &str = "default";

/// Map a user-supplied quantization name to the ONNX filename suffix used by
/// `onnx-community/whisper-*` repos, e.g. `"fp16"` -> `"_fp16"`.
///
/// `onnx-community/whisper-*` repos ship the same encoder and decoder graph
/// exported at several precisions (see [`WhisperBackend`] docs for why only
/// the *merged* decoder variant is used, never the `_with_past` ones).
/// Picking one avoids downloading every variant just to use one of them.
fn quantization_suffix(quantization: &str) -> Result<&'static str, SttError> {
    match quantization.to_ascii_lowercase().as_str() {
        "default" | "fp32" => Ok(""),
        "fp16" => Ok("_fp16"),
        "int8" => Ok("_int8"),
        "uint8" => Ok("_uint8"),
        "bnb4" => Ok("_bnb4"),
        "q4" => Ok("_q4"),
        "q4f16" => Ok("_q4f16"),
        "quantized" => Ok("_quantized"),
        other => Err(SttError::Init(format!(
            "unknown Whisper quantization {other:?}; expected one of: \
             default, fp32, fp16, int8, uint8, bnb4, q4, q4f16, quantized"
        ))),
    }
}

/// Configuration for the Whisper STT architecture.
#[derive(Clone, Debug)]
pub struct WhisperConfig {
    /// HuggingFace Hub repo (`"hf://onnx-community/whisper-base"`) or local directory path.
    pub source: String,
    /// ISO 639-1 language code (e.g. `"en"`, `"fr"`). `None` → auto-detect.
    pub language: Option<String>,
    /// ONNX precision variant to download and load: one of `"default"`
    /// (fp32, no suffix), `"fp16"`, `"int8"`, `"uint8"`, `"bnb4"`, `"q4"`.
    /// Defaults to `"q4"`, falling back to `"default"` (fp32) if the source
    /// doesn't have a `"q4"` variant. Most users never need to set this.
    pub quantization: String,
}

impl WhisperConfig {
    pub fn new(source: impl AsRef<str>) -> Self {
        Self {
            source: source.as_ref().to_string(),
            language: None,
            quantization: DEFAULT_QUANTIZATION.to_string(),
        }
    }
}

/// Files actually read by [`WhisperBackend`] for a given `quantization`, out
/// of the many ONNX precision variants a `onnx-community/whisper-*` repo
/// ships. Downloading only these avoids pulling down gigabytes of unused
/// variants and the never-used `_with_past` graphs.
pub(in crate::stt) fn required_files(quantization: &str) -> Result<Vec<String>, SttError> {
    let suffix = quantization_suffix(quantization)?;
    Ok(vec![
        "config.json".into(),
        "generation_config.json".into(),
        "tokenizer.json".into(),
        format!("onnx/encoder_model{suffix}.onnx"),
        format!("onnx/decoder_model_merged{suffix}.onnx"),
    ])
}

/// Resolve `source` to a local model directory for `quantization`. Called by
/// [`WhisperBackend::new`] before it loads anything.
fn resolve_model_dir(source: &str, quantization: &str) -> Result<(PathBuf, String), SttError> {
    let is_default = quantization == DEFAULT_QUANTIZATION;
    let files = required_files(quantization)?;

    match crate::huggingface::download_onnx(source, &files, None) {
        Ok(dir) => Ok((dir, quantization.to_string())),
        Err(HuggingFaceError::MissingRequiredFiles { .. }) if is_default => {
            info!(
                source,
                quantization = FALLBACK_QUANTIZATION,
                "q4 Whisper quantization unavailable, falling back"
            );
            let files = required_files(FALLBACK_QUANTIZATION)?;
            let dir = crate::huggingface::download_onnx(source, &files, None)?;
            Ok((dir, FALLBACK_QUANTIZATION.to_string()))
        }
        Err(e) => Err(e.into()),
    }
}

// ---------------------------------------------------------------------------
// KV cache
// ---------------------------------------------------------------------------

/// KV cache for the Whisper merged decoder.
///
/// Holds one `(past_kv_input_name, flat_f32_data)` entry per layer per
/// attention type (decoder self-attn key/value + encoder cross-attn key/value).
/// The tensor shape is `[1, num_heads, seq_len, head_dim]`; `seq_len` is
/// derived from `data.len()` at the point of use.
struct KVCache(Vec<(String, Vec<f32>)>);

impl KVCache {
    fn new() -> Self {
        Self(Vec::new())
    }

    /// Extract `present.*` tensors from decoder outputs, returning a new cache
    /// keyed as `past_key_values.*` for the next step.
    ///
    /// When `use_cache_branch=true` the model emits empty `present.*.encoder.*`
    /// tensors (encoder cross-attention KV is unchanged). `prev` carries those
    /// entries forward so they are not lost.
    fn collect(
        outputs: &ort::session::SessionOutputs<'_>,
        num_layers: usize,
        prev: Option<&Self>,
    ) -> Result<Self, SttError> {
        let mut entries = Vec::new();
        for i in 0..num_layers {
            for kind in &["decoder", "encoder"] {
                for field in &["key", "value"] {
                    let present = format!("present.{i}.{kind}.{field}");
                    let past = format!("past_key_values.{i}.{kind}.{field}");
                    let raw = outputs[present.as_str()].try_extract_tensor::<f32>()?.1;
                    let data = if raw.is_empty() {
                        prev.and_then(|kv| kv.iter().find(|(n, _)| n == &past))
                            .map(|(_, d)| d.clone())
                            .unwrap_or_default()
                    } else {
                        raw.to_vec()
                    };
                    entries.push((past, data));
                }
            }
        }
        Ok(Self(entries))
    }

    /// Convert each KV entry to an ONNX tensor, using `num_heads` and `head_dim`
    /// to reconstruct the `[1, num_heads, seq_len, head_dim]` shape from the
    /// flat data length.
    fn as_tensors(
        &self,
        num_heads: usize,
        head_dim: usize,
    ) -> Result<Vec<(Cow<'static, str>, DynValue)>, SttError> {
        self.iter()
            .map(|(name, data)| {
                let seq_len = data.len() / (num_heads * head_dim);
                let tensor = Tensor::from_array((
                    [1i64, num_heads as i64, seq_len as i64, head_dim as i64],
                    data.clone(),
                ))?;
                Ok((name.clone().into(), tensor.into_dyn()))
            })
            .collect()
    }
}

impl Default for KVCache {
    fn default() -> Self {
        Self::new()
    }
}

impl std::ops::Deref for KVCache {
    type Target = Vec<(String, Vec<f32>)>;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<'a> IntoIterator for &'a KVCache {
    type Item = &'a (String, Vec<f32>);
    type IntoIter = std::slice::Iter<'a, (String, Vec<f32>)>;
    fn into_iter(self) -> Self::IntoIter {
        self.0.iter()
    }
}

// ---------------------------------------------------------------------------
// Backend
// ---------------------------------------------------------------------------

pub(in crate::stt) struct WhisperBackend {
    encoder: Session,
    decoder: Session,
    tokenizer: Tokenizer,
    sot_id: u32,
    eot_id: u32,
    transcribe_id: u32,
    notimestamps_id: u32,
    lang_to_id: HashMap<String, u32>,
    language: Option<String>,
    n_mels: usize,
    num_layers: usize,
    num_heads: usize,
    head_dim: usize,
    /// Encoder output width (= num_heads × head_dim).
    hidden_dim: usize,
    /// KV cache accumulated during a single window's decode loop.
    kv: KVCache,
}

impl WhisperBackend {
    pub fn new(
        source: &str,
        language: Option<&str>,
        quantization: &str,
        device: Device,
    ) -> Result<Self, SttError> {
        let (model_dir, quantization) = resolve_model_dir(source, quantization)?;
        let (encoder, decoder) = load_sessions(&model_dir.join("onnx"), &quantization, device)?;
        let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|e| SttError::Init(format!("load tokenizer: {e}")))?;
        let sot_id = token_id(&tokenizer, "<|startoftranscript|>")?;
        let eot_id = token_id(&tokenizer, "<|endoftext|>")?;
        let transcribe_id = token_id(&tokenizer, "<|transcribe|>")?;
        let notimestamps_id = token_id(&tokenizer, "<|notimestamps|>")?;
        let model_cfg = ModelConfig::from_dir(&model_dir)?;
        let gen_cfg = GenerationConfig::from_dir(&model_dir)?;

        info!(
            n_mels = model_cfg.n_mels,
            num_layers = model_cfg.num_layers,
            num_heads = model_cfg.num_heads,
            head_dim = model_cfg.head_dim,
            lang_count = gen_cfg.lang_to_id.len(),
            language = language.unwrap_or("auto"),
            "Loaded Whisper STT"
        );

        Ok(Self {
            encoder,
            decoder,
            tokenizer,
            sot_id,
            eot_id,
            transcribe_id,
            notimestamps_id,
            lang_to_id: gen_cfg.lang_to_id,
            language: language.map(String::from),
            n_mels: model_cfg.n_mels,
            num_layers: model_cfg.num_layers,
            num_heads: model_cfg.num_heads,
            head_dim: model_cfg.head_dim,
            hidden_dim: model_cfg.num_heads * model_cfg.head_dim,
            kv: KVCache::new(),
        })
    }

    // -----------------------------------------------------------------------
    // Encode
    // -----------------------------------------------------------------------

    fn encode(&mut self, window: &[f32]) -> Result<Vec<f32>, SttError> {
        let mel = compute_mel(window, self.n_mels);
        let features = Tensor::from_array(([1usize, self.n_mels, N_MEL_FRAMES], mel))?;
        let enc_out = self
            .encoder
            .run(ort::inputs!["input_features" => features])?;
        Ok(enc_out[0].try_extract_tensor::<f32>()?.1.to_vec())
    }

    // -----------------------------------------------------------------------
    // Language detection
    // -----------------------------------------------------------------------

    fn resolve_language(&mut self, enc_hidden: &[f32]) -> Result<u32, SttError> {
        match self.language.as_deref() {
            Some(lang) => {
                let tok = format!("<|{lang}|>");
                self.lang_to_id.get(&tok).copied().ok_or_else(|| {
                    SttError::Transcription(format!("unknown language code: {lang:?}"))
                })
            }
            None => self.detect_language(enc_hidden),
        }
    }

    fn detect_language(&mut self, enc_hidden: &[f32]) -> Result<u32, SttError> {
        let logits = self.run_decoder(&[self.sot_id as i64], enc_hidden)?;

        let (best, _) = self
            .lang_to_id
            .iter()
            .map(|(tok, &id)| {
                let score = if (id as usize) < logits.len() {
                    logits[id as usize]
                } else {
                    f32::NEG_INFINITY
                };
                (tok, score)
            })
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or_else(|| SttError::Transcription("no language tokens in vocabulary".into()))?;

        info!(detected = %best, "Language detected");
        Ok(self.lang_to_id[best])
    }

    // -----------------------------------------------------------------------
    // Decode
    // -----------------------------------------------------------------------

    /// Assemble the full ONNX input list for one decoder step.
    fn build_inputs(
        &self,
        input_ids: Tensor<i64>,
        enc_tensor: Tensor<f32>,
        kv: &KVCache,
    ) -> Result<Vec<(Cow<'static, str>, DynValue)>, SttError> {
        let mut inputs: Vec<(Cow<'static, str>, DynValue)> = vec![
            ("input_ids".into(), input_ids.into_dyn()),
            ("encoder_hidden_states".into(), enc_tensor.into_dyn()),
            (
                "use_cache_branch".into(),
                Tensor::from_array(([1usize], vec![!kv.is_empty()]))?.into_dyn(),
            ),
        ];
        inputs.extend(kv.as_tensors(self.num_heads, self.head_dim)?);
        Ok(inputs)
    }

    /// Run one decoder step for `tokens`, returning last-position logits.
    ///
    /// Moves `self.kv` out via `mem::take` to avoid a borrow conflict while
    /// the decoder session holds a reference to `self.decoder`, then writes
    /// the updated cache back to `self.kv` before returning.
    fn run_decoder(&mut self, tokens: &[i64], enc_hidden: &[f32]) -> Result<Vec<f32>, SttError> {
        let prev_kv = std::mem::take(&mut self.kv);
        let seq_len = tokens.len();
        let input_ids = Tensor::from_array(([1usize, seq_len], tokens.to_vec()))?;
        let enc_tensor =
            Tensor::from_array(([1usize, ENC_SEQ_LEN, self.hidden_dim], enc_hidden.to_vec()))?;
        let num_layers = self.num_layers;
        let inputs = self.build_inputs(input_ids, enc_tensor, &prev_kv)?;
        let (last_logits, new_kv) = {
            let outputs = self.decoder.run(inputs)?;
            let logits_flat = outputs[0].try_extract_tensor::<f32>()?.1.to_vec();
            let vocab_size = logits_flat.len() / seq_len;
            let last_logits = logits_flat[(seq_len - 1) * vocab_size..].to_vec();
            let new_kv = KVCache::collect(
                &outputs,
                num_layers,
                (!prev_kv.is_empty()).then_some(&prev_kv),
            )?;
            (last_logits, new_kv)
        };
        self.kv = new_kv;
        Ok(last_logits)
    }

    fn greedy_decode(
        &mut self,
        enc_hidden: &[f32],
        lang_id: u32,
        on_token: &mut dyn FnMut(String),
    ) -> Result<Vec<u32>, SttError> {
        self.kv = KVCache::new();

        let prompt: Vec<i64> = vec![
            self.sot_id as i64,
            lang_id as i64,
            self.transcribe_id as i64,
            self.notimestamps_id as i64,
        ];

        let mut next_token = argmax(&self.run_decoder(&prompt, enc_hidden)?);
        let mut generated = Vec::new();

        for step in 0..MAX_NEW_TOKENS {
            if next_token == self.eot_id as i64 {
                break;
            }
            generated.push(next_token as u32);
            if let Ok(piece) = self.tokenizer.decode(&[next_token as u32], false) {
                on_token(piece);
            }
            debug!(step, next_token, "Decode step");
            next_token = argmax(&self.run_decoder(&[next_token], enc_hidden)?);
        }

        Ok(generated)
    }
}

impl SttArchitectureImpl for WhisperBackend {
    fn transcribe_window(
        &mut self,
        window: &[f32],
        on_token: &mut dyn FnMut(String),
    ) -> Result<String, SttError> {
        let enc_hidden = self.encode(window)?;
        let lang_id = self.resolve_language(&enc_hidden)?;
        let generated = self.greedy_decode(&enc_hidden, lang_id, on_token)?;
        let text = self
            .tokenizer
            .decode(&generated, true)
            .map_err(|e| SttError::Transcription(format!("tokenizer decode: {e}")))?;
        debug!(tokens = generated.len(), text = %text, "Window transcribed");
        Ok(text)
    }
}

// ---------------------------------------------------------------------------
// Free helpers
// ---------------------------------------------------------------------------

fn load_sessions(
    onnx_dir: &Path,
    quantization: &str,
    device: Device,
) -> Result<(Session, Session), SttError> {
    let suffix = quantization_suffix(quantization)?;
    let encoder = crate::onnx::load_session(
        &onnx_dir.join(format!("encoder_model{suffix}.onnx")),
        device,
    )?;
    let decoder = crate::onnx::load_session(
        &onnx_dir.join(format!("decoder_model_merged{suffix}.onnx")),
        device,
    )?;
    Ok((encoder, decoder))
}

fn token_id(tokenizer: &Tokenizer, token: &str) -> Result<u32, SttError> {
    tokenizer
        .token_to_id(token)
        .ok_or_else(|| SttError::Init(format!("special token not found in vocabulary: {token:?}")))
}

fn compute_mel(window: &[f32], n_mels: usize) -> Vec<f32> {
    let mel = Spectrogram::compute_mel_spectrogram_cpu(window, 400, 160, n_mels, 16_000.0);
    let n_frames = mel.len().min(N_MEL_FRAMES);
    let mut out = vec![0.0f32; n_mels * N_MEL_FRAMES];
    for t in 0..n_frames {
        for bin in 0..n_mels {
            out[bin * N_MEL_FRAMES + t] = mel[t][bin];
        }
    }
    out
}

fn argmax(logits: &[f32]) -> i64 {
    logits
        .iter()
        .enumerate()
        .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(i, _)| i as i64)
        .unwrap_or(0)
}

struct ModelConfig {
    n_mels: usize,
    num_layers: usize,
    num_heads: usize,
    head_dim: usize,
}

impl ModelConfig {
    fn from_dir(model_dir: &Path) -> Result<Self, SttError> {
        let f = std::fs::File::open(model_dir.join("config.json"))
            .map_err(|e| SttError::Init(format!("open config.json: {e}")))?;
        let cfg: serde_json::Value = serde_json::from_reader(f)
            .map_err(|e| SttError::Init(format!("parse config.json: {e}")))?;
        let num_heads = cfg["decoder_attention_heads"].as_u64().unwrap_or(8) as usize;
        let d_model = cfg["d_model"].as_u64().unwrap_or(512) as usize;
        Ok(Self {
            n_mels: cfg["num_mel_bins"].as_u64().unwrap_or(80) as usize,
            num_layers: cfg["decoder_layers"].as_u64().unwrap_or(6) as usize,
            num_heads,
            head_dim: d_model / num_heads,
        })
    }
}

struct GenerationConfig {
    lang_to_id: HashMap<String, u32>,
}

impl GenerationConfig {
    fn from_dir(model_dir: &Path) -> Result<Self, SttError> {
        let f = std::fs::File::open(model_dir.join("generation_config.json"))
            .map_err(|e| SttError::Init(format!("open generation_config.json: {e}")))?;
        let cfg: serde_json::Value = serde_json::from_reader(f)
            .map_err(|e| SttError::Init(format!("parse generation_config.json: {e}")))?;
        let lang_to_id = cfg["lang_to_id"]
            .as_object()
            .ok_or_else(|| SttError::Init("generation_config.json missing `lang_to_id`".into()))?
            .iter()
            .filter_map(|(k, v)| v.as_u64().map(|id| (k.clone(), id as u32)))
            .collect();
        Ok(Self { lang_to_id })
    }
}
