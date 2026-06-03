//! Whisper STT backend via ONNX Runtime.
//!
//! Pipeline: 16 kHz mono f32 audio → log-mel spectrogram → encoder → greedy
//! token decode → text.
//!
//! Uses `onnx-community/whisper-*` model repos which ship:
//!   `onnx/encoder_model.onnx`, `onnx/decoder_model.onnx`,
//!   `tokenizer.json`, `generation_config.json`, `config.json`.

use crate::errors::SttError;
use crate::onnx::Device;
use crate::stt::backend::SttBackendImpl;
use mel_spec::prelude::*;
use ort::session::Session;
use ort::value::Tensor;
use std::collections::HashMap;
use std::path::Path;
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

/// Configuration for the Whisper STT backend.
#[derive(Clone, Debug)]
pub struct WhisperConfig {
    /// HuggingFace repo ID (`"onnx-community/whisper-base"`) or local directory path.
    pub source: String,
    /// ISO 639-1 language code (e.g. `"en"`, `"fr"`). `None` → auto-detect.
    pub language: Option<String>,
}

impl WhisperConfig {
    pub fn new(source: impl AsRef<str>) -> Self {
        Self {
            source: source.as_ref().to_string(),
            language: None,
        }
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
}

impl WhisperBackend {
    pub fn new(model_dir: &Path, language: Option<&str>, device: Device) -> Result<Self, SttError> {
        let (encoder, decoder) = load_sessions(&model_dir.join("onnx"), device)?;
        let tokenizer = Tokenizer::from_file(model_dir.join("tokenizer.json"))
            .map_err(|e| SttError::Init(format!("load tokenizer: {e}")))?;
        let sot_id = token_id(&tokenizer, "<|startoftranscript|>")?;
        let eot_id = token_id(&tokenizer, "<|endoftext|>")?;
        let transcribe_id = token_id(&tokenizer, "<|transcribe|>")?;
        let notimestamps_id = token_id(&tokenizer, "<|notimestamps|>")?;
        let lang_to_id = load_lang_to_id(model_dir)?;
        let n_mels = load_n_mels(model_dir)?;

        info!(
            n_mels,
            lang_count = lang_to_id.len(),
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
            lang_to_id,
            language: language.map(String::from),
            n_mels,
        })
    }

    fn encode(&mut self, window: &[f32]) -> Result<(Vec<f32>, usize), SttError> {
        let mel_flat = compute_mel(window, self.n_mels);
        let features = Tensor::from_array(([1usize, self.n_mels, N_MEL_FRAMES], mel_flat))?;
        let enc_hidden = {
            let enc_out = self
                .encoder
                .run(ort::inputs!["input_features" => features])?;
            enc_out[0].try_extract_tensor::<f32>()?.1.to_vec()
        };
        let hidden_dim = enc_hidden.len() / ENC_SEQ_LEN;
        Ok((enc_hidden, hidden_dim))
    }

    fn resolve_language(&mut self, enc_hidden: &[f32], hidden_dim: usize) -> Result<u32, SttError> {
        match &self.language.clone() {
            Some(lang) => {
                let tok = format!("<|{lang}|>");
                self.lang_to_id.get(&tok).copied().ok_or_else(|| {
                    SttError::Transcription(format!("unknown language code: {lang:?}"))
                })
            }
            None => self.detect_language(enc_hidden, hidden_dim),
        }
    }

    fn detect_language(&mut self, enc_hidden: &[f32], hidden_dim: usize) -> Result<u32, SttError> {
        let input_ids = Tensor::from_array(([1usize, 1], vec![self.sot_id as i64]))?;
        let enc_tensor =
            Tensor::from_array(([1usize, ENC_SEQ_LEN, hidden_dim], enc_hidden.to_vec()))?;
        let dec_out = self
            .decoder
            .run(ort::inputs!["input_ids" => input_ids, "encoder_hidden_states" => enc_tensor])?;
        let logits_flat = dec_out[0].try_extract_tensor::<f32>()?.1.to_vec();

        let (best_token_str, _) = self
            .lang_to_id
            .iter()
            .map(|(tok, &id)| {
                let score = if (id as usize) < logits_flat.len() {
                    logits_flat[id as usize]
                } else {
                    f32::NEG_INFINITY
                };
                (tok, score)
            })
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .ok_or_else(|| SttError::Transcription("no language tokens in vocabulary".into()))?;

        let lang_id = self.lang_to_id[best_token_str];
        info!(detected = %best_token_str, "Language detected");
        Ok(lang_id)
    }

    fn decode_step(
        &mut self,
        tokens: &[i64],
        enc_hidden: &[f32],
        hidden_dim: usize,
    ) -> Result<i64, SttError> {
        let seq_len = tokens.len();
        let input_ids = Tensor::from_array(([1usize, seq_len], tokens.to_vec()))?;
        let enc_tensor =
            Tensor::from_array(([1usize, ENC_SEQ_LEN, hidden_dim], enc_hidden.to_vec()))?;
        let dec_out = self
            .decoder
            .run(ort::inputs!["input_ids" => input_ids, "encoder_hidden_states" => enc_tensor])?;
        let logits_flat = dec_out[0].try_extract_tensor::<f32>()?.1.to_vec();
        let vocab_size = logits_flat.len() / seq_len;
        let last_logits = &logits_flat[(seq_len - 1) * vocab_size..];
        Ok(last_logits
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal))
            .map(|(i, _)| i as i64)
            .unwrap_or(self.eot_id as i64))
    }

    fn greedy_decode(
        &mut self,
        enc_hidden: &[f32],
        hidden_dim: usize,
        lang_id: u32,
    ) -> Result<Vec<u32>, SttError> {
        let mut tokens: Vec<i64> = vec![
            self.sot_id as i64,
            lang_id as i64,
            self.transcribe_id as i64,
            self.notimestamps_id as i64,
        ];
        let mut generated: Vec<u32> = Vec::new();

        for step in 0..MAX_NEW_TOKENS {
            let next_token = self.decode_step(&tokens, enc_hidden, hidden_dim)?;
            debug!(step, next_token, "Decode step");
            if next_token == self.eot_id as i64 {
                break;
            }
            tokens.push(next_token);
            generated.push(next_token as u32);
        }

        Ok(generated)
    }
}

impl SttBackendImpl for WhisperBackend {
    fn transcribe_window(&mut self, window: &[f32]) -> Result<String, SttError> {
        let (enc_hidden, hidden_dim) = self.encode(window)?;
        let lang_id = self.resolve_language(&enc_hidden, hidden_dim)?;
        let generated = self.greedy_decode(&enc_hidden, hidden_dim, lang_id)?;
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

fn load_sessions(onnx_dir: &Path, device: Device) -> Result<(Session, Session), SttError> {
    let encoder = crate::onnx::load_session(&onnx_dir.join("encoder_model.onnx"), device)?;
    let decoder = crate::onnx::load_session(&onnx_dir.join("decoder_model.onnx"), device)?;
    Ok((encoder, decoder))
}

fn load_lang_to_id(model_dir: &Path) -> Result<HashMap<String, u32>, SttError> {
    let f = std::fs::File::open(model_dir.join("generation_config.json"))
        .map_err(|e| SttError::Init(format!("open generation_config.json: {e}")))?;
    let cfg: serde_json::Value = serde_json::from_reader(f)
        .map_err(|e| SttError::Init(format!("parse generation_config.json: {e}")))?;
    let map = cfg["lang_to_id"]
        .as_object()
        .ok_or_else(|| SttError::Init("generation_config.json missing `lang_to_id`".into()))?
        .iter()
        .filter_map(|(k, v)| v.as_u64().map(|id| (k.clone(), id as u32)))
        .collect();
    Ok(map)
}

fn load_n_mels(model_dir: &Path) -> Result<usize, SttError> {
    let f = std::fs::File::open(model_dir.join("config.json"))
        .map_err(|e| SttError::Init(format!("open config.json: {e}")))?;
    let cfg: serde_json::Value = serde_json::from_reader(f)
        .map_err(|e| SttError::Init(format!("parse config.json: {e}")))?;
    Ok(cfg["num_mel_bins"].as_u64().unwrap_or(80) as usize)
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
        for mel_bin in 0..n_mels {
            out[mel_bin * N_MEL_FRAMES + t] = mel[t][mel_bin];
        }
    }
    out
}
