//! Piper TTS inference via ONNX Runtime + espeak-ng phonemization.
//!
//! Implements the VITS-based Piper pipeline: text → espeak phonemes →
//! phoneme IDs → ONNX → PCM audio.

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::{ort_util, TtsDevice};
use ort::session::Session;
use ort::value::Tensor;
use ort::value::Value;
use serde::Deserialize;
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use tracing::info;

pub(super) struct PiperBackend {
    model: PiperModel,
    sample_rate: u32,
}

impl PiperBackend {
    pub fn new(model: PiperModel) -> Self {
        let sample_rate = model.sample_rate();
        Self { model, sample_rate }
    }
}

impl TtsBackendImpl for PiperBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        Ok((self.model.synthesize(text)?, self.sample_rate))
    }
}

#[derive(Deserialize)]
pub(crate) struct PiperConfig {
    pub audio: AudioConfig,
    pub espeak: ESpeakConfig,
    pub inference: InferenceConfig,
    #[serde(default)]
    pub num_speakers: u32,
    pub phoneme_id_map: HashMap<String, Vec<i64>>,
}

#[derive(Deserialize)]
pub(crate) struct AudioConfig {
    pub sample_rate: u32,
}

#[derive(Deserialize)]
pub(crate) struct ESpeakConfig {
    pub voice: String,
}

#[derive(Deserialize, Clone)]
pub(crate) struct InferenceConfig {
    pub noise_scale: f32,
    pub length_scale: f32,
    pub noise_w: f32,
}

pub(crate) struct PiperModel {
    session: Session,
    config: PiperConfig,
}

const PAD_ID: i64 = 0; // "_"
const BOS_ID: i64 = 1; // "^"
const EOS_ID: i64 = 2; // "$"

impl PiperModel {
    pub fn new(model_path: &Path, config_path: &Path, device: TtsDevice) -> Result<Self, TtsError> {
        let config_str = std::fs::read_to_string(config_path)
            .map_err(|e| TtsError::Init(format!("failed to read piper config: {e}")))?;
        let config: PiperConfig = serde_json::from_str(&config_str)
            .map_err(|e| TtsError::Init(format!("failed to parse piper config: {e}")))?;

        let session = ort_util::load_session(model_path, device, false)?;

        info!(
            sample_rate = config.audio.sample_rate,
            voice = config.espeak.voice,
            num_speakers = config.num_speakers,
            "Loaded Piper model"
        );

        Ok(Self { session, config })
    }

    pub fn sample_rate(&self) -> u32 {
        self.config.audio.sample_rate
    }

    pub fn synthesize(&mut self, text: &str) -> Result<Vec<f32>, TtsError> {
        let phoneme_sentences =
            espeak_rs::text_to_phonemes(text, &self.config.espeak.voice, None, true, false)
                .map_err(|e| TtsError::Synthesis(format!("espeak phonemization failed: {e}")))?;

        let phonemes = phoneme_sentences.join(" ");
        if phonemes.is_empty() {
            return Err(TtsError::Synthesis(
                "text produced no phonemes after espeak processing".into(),
            ));
        }

        let phoneme_ids = self.phonemes_to_ids(&phonemes);
        self.infer(
            &phoneme_ids,
            self.config.inference.noise_scale,
            self.config.inference.length_scale,
            self.config.inference.noise_w,
        )
    }

    fn phonemes_to_ids(&self, phonemes: &str) -> Vec<i64> {
        let mut ids = vec![BOS_ID];
        ids.push(PAD_ID);

        for ch in phonemes.chars() {
            let key = ch.to_string();
            if let Some(mapped) = self.config.phoneme_id_map.get(&key) {
                ids.extend_from_slice(mapped);
                ids.push(PAD_ID);
            }
        }

        ids.push(EOS_ID);
        ids
    }

    fn infer(
        &mut self,
        phoneme_ids: &[i64],
        noise_scale: f32,
        length_scale: f32,
        noise_w: f32,
    ) -> Result<Vec<f32>, TtsError> {
        let seq_len = phoneme_ids.len();

        let input_tensor = Tensor::from_array(([1, seq_len], phoneme_ids.to_vec()))
            .map_err(|e| TtsError::Synthesis(format!("input tensor: {e}")))?;

        let lengths_tensor = Tensor::from_array(([1], vec![seq_len as i64]))
            .map_err(|e| TtsError::Synthesis(format!("lengths tensor: {e}")))?;

        let scales_tensor = Tensor::from_array(([3], vec![noise_scale, length_scale, noise_w]))
            .map_err(|e| TtsError::Synthesis(format!("scales tensor: {e}")))?;

        let mut inputs: Vec<(Cow<'_, str>, ort::session::SessionInputValue<'_>)> = vec![
            (
                Cow::Borrowed("input"),
                ort::session::SessionInputValue::Owned(Value::from(input_tensor)),
            ),
            (
                Cow::Borrowed("input_lengths"),
                ort::session::SessionInputValue::Owned(Value::from(lengths_tensor)),
            ),
            (
                Cow::Borrowed("scales"),
                ort::session::SessionInputValue::Owned(Value::from(scales_tensor)),
            ),
        ];

        if self.config.num_speakers > 1 {
            let sid_tensor = Tensor::from_array(([1], vec![0i64]))
                .map_err(|e| TtsError::Synthesis(format!("sid tensor: {e}")))?;
            inputs.push((
                Cow::Borrowed("sid"),
                ort::session::SessionInputValue::Owned(Value::from(sid_tensor)),
            ));
        }

        let outputs = self
            .session
            .run(ort::session::SessionInputs::from(inputs))
            .map_err(|e| TtsError::Synthesis(format!("ort inference failed: {e}")))?;

        let output_tensor = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| TtsError::Synthesis(format!("extract output: {e}")))?;

        Ok(output_tensor.1.to_vec())
    }
}
