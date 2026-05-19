//! Piper TTS via ONNX Runtime + espeak-ng phonemization.
//!
//! Pipeline: text → espeak IPA → phoneme IDs → ONNX VITS → PCM waveform.
//! Voice parameters (sample rate, espeak voice, inference scales, phoneme map)
//! are read from `model.onnx.json` in the model directory;

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::{ort_util, TtsDevice};
use ort::session::{Session, SessionInputValue, SessionInputs};
use ort::value::{Tensor, Value};
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

const MODEL_FILE: &str = "model.onnx";
const CONFIG_FILE: &str = "model.onnx.json";

pub(in crate::tts) struct PiperBackend {
    session: Session,
    model_config: PiperModelConfig,
    speaker_id: i64,
    has_sid: bool,
    pad_id: i64,
    bos_id: i64,
    eos_id: i64,
}

impl PiperBackend {
    pub fn new(model_dir: &Path, speaker_id: u32, device: TtsDevice) -> Result<Self, TtsError> {
        let model_path = model_dir.join(MODEL_FILE);
        let config_path = model_dir.join(CONFIG_FILE);

        let raw = std::fs::read_to_string(&config_path)
            .map_err(|e| TtsError::Init(format!("piper: read {}: {e}", config_path.display())))?;
        let model_config: PiperModelConfig = serde_json::from_str(&raw)
            .map_err(|e| TtsError::Init(format!("piper: parse {}: {e}", config_path.display())))?;

        let pad_id = phoneme_token_id(&model_config.phoneme_id_map, "_", "PAD")?;
        let bos_id = phoneme_token_id(&model_config.phoneme_id_map, "^", "BOS")?;
        let eos_id = phoneme_token_id(&model_config.phoneme_id_map, "$", "EOS")?;

        let session = ort_util::load_session(&model_path, device, false)?;
        let has_sid = session.inputs().iter().any(|i| i.name() == "sid");

        info!(
            sample_rate = model_config.audio.sample_rate,
            voice = model_config.espeak.voice,
            has_sid,
            speaker_id,
            pad_id,
            bos_id,
            eos_id,
            "Loaded Piper model"
        );

        Ok(Self {
            session,
            model_config,
            speaker_id: speaker_id as i64,
            has_sid,
            pad_id,
            bos_id,
            eos_id,
        })
    }
}

impl TtsBackendImpl for PiperBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        info!("Piper: synthesising");
        let phoneme_sentences =
            espeak_rs::text_to_phonemes(text, &self.model_config.espeak.voice, None, true, false)
                .map_err(|e| TtsError::Synthesis(format!("espeak phonemization failed: {e}")))?;

        let phonemes = phoneme_sentences.join(" ");
        if phonemes.is_empty() {
            return Err(TtsError::Synthesis(
                "piper: text produced no phonemes".into(),
            ));
        }

        let phoneme_ids = self.phonemes_to_ids(&phonemes);
        let waveform = self.infer(&phoneme_ids)?;
        let sample_rate = self.model_config.audio.sample_rate;
        debug!(
            phoneme_ids = phoneme_ids.len(),
            pcm_samples = waveform.len(),
            pcm_duration_s = waveform.len() as f32 / sample_rate as f32,
            "Piper: done"
        );
        Ok((waveform, sample_rate))
    }
}

impl PiperBackend {
    fn phonemes_to_ids(&self, phonemes: &str) -> Vec<i64> {
        let mut ids = vec![self.bos_id, self.pad_id];
        for ch in phonemes.chars() {
            // Unmapped IPA characters are dropped silently — same as upstream.
            if let Some(mapped) = self.model_config.phoneme_id_map.get(&ch.to_string()) {
                ids.extend_from_slice(mapped);
                ids.push(self.pad_id);
            }
        }
        ids.push(self.eos_id);
        ids
    }

    fn infer(&mut self, phoneme_ids: &[i64]) -> Result<Vec<f32>, TtsError> {
        let seq_len = phoneme_ids.len();
        let inf = &self.model_config.inference;

        let input_tensor = Tensor::from_array(([1, seq_len], phoneme_ids.to_vec()))
            .map_err(|e| TtsError::Synthesis(format!("input tensor: {e}")))?;
        let lengths_tensor = Tensor::from_array(([1], vec![seq_len as i64]))
            .map_err(|e| TtsError::Synthesis(format!("lengths tensor: {e}")))?;
        let scales_tensor =
            Tensor::from_array(([3], vec![inf.noise_scale, inf.length_scale, inf.noise_w]))
                .map_err(|e| TtsError::Synthesis(format!("scales tensor: {e}")))?;

        let mut inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> = vec![
            (
                Cow::Borrowed("input"),
                SessionInputValue::Owned(Value::from(input_tensor)),
            ),
            (
                Cow::Borrowed("input_lengths"),
                SessionInputValue::Owned(Value::from(lengths_tensor)),
            ),
            (
                Cow::Borrowed("scales"),
                SessionInputValue::Owned(Value::from(scales_tensor)),
            ),
        ];

        if self.has_sid {
            let sid_tensor = Tensor::from_array(([1], vec![self.speaker_id]))
                .map_err(|e| TtsError::Synthesis(format!("sid tensor: {e}")))?;
            inputs.push((
                Cow::Borrowed("sid"),
                SessionInputValue::Owned(Value::from(sid_tensor)),
            ));
        }

        let outputs = self
            .session
            .run(SessionInputs::from(inputs))
            .map_err(|e| TtsError::Synthesis(format!("ort inference failed: {e}")))?;

        let output_tensor = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| TtsError::Synthesis(format!("extract waveform: {e}")))?;

        Ok(output_tensor.1.to_vec())
    }
}

fn phoneme_token_id(
    map: &HashMap<String, Vec<i64>>,
    symbol: &str,
    name: &str,
) -> Result<i64, TtsError> {
    map.get(symbol)
        .and_then(|v| v.first())
        .copied()
        .ok_or_else(|| {
            TtsError::Init(format!(
                "piper: {name} token ({symbol:?}) missing from phoneme_id_map"
            ))
        })
}

#[derive(Deserialize)]
struct PiperModelConfig {
    audio: PiperAudioConfig,
    espeak: PiperEspeakConfig,
    inference: PiperInferenceConfig,
    phoneme_id_map: HashMap<String, Vec<i64>>,
}

#[derive(Deserialize)]
struct PiperAudioConfig {
    sample_rate: u32,
}

#[derive(Deserialize)]
struct PiperEspeakConfig {
    voice: String,
}

#[derive(Deserialize)]
struct PiperInferenceConfig {
    noise_scale: f32,
    length_scale: f32,
    noise_w: f32,
}

#[derive(Clone, Debug)]
pub struct PiperConfig {
    pub model_dir: PathBuf,
    /// Speaker index for multi-speaker voices. Ignored for single-speaker
    /// voices (detected from the model's ONNX inputs at load time).
    pub speaker_id: u32,
}

impl PiperConfig {
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        Self {
            model_dir: model_dir.into(),
            speaker_id: 0,
        }
    }
}
