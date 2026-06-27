use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::{ort_util, TtsDevice};
use ndarray::{Array, Array3};
use ort::session::Session;
use ort::value::Tensor;
use rand_distr::{Distribution, Normal};
use serde::Deserialize;
use std::fs::File;
use std::io::BufReader;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

mod text;

use text::{chunk_text, normalize_tts_text};

const DEFAULT_SILENCE_DURATION: f32 = 0.3;
const MAX_CHUNK_LENGTH: usize = 300;
const MAX_CJK_CHUNK_LENGTH: usize = 120;
const REQUIRED_ONNX_ASSETS: &[&str] = &[
    "tts.json",
    "unicode_indexer.json",
    "duration_predictor.onnx",
    "text_encoder.onnx",
    "vector_estimator.onnx",
    "vocoder.onnx",
];
const SUPPORTED_LANGS: &[&str] = &[
    "en", "ko", "ja", "ar", "bg", "cs", "da", "de", "el", "es", "et", "fi", "fr", "hi", "hr", "hu",
    "id", "it", "lt", "lv", "nl", "pl", "pt", "ro", "ru", "sk", "sl", "sv", "tr", "uk", "vi", "na",
];

pub(in crate::tts) struct SupertonicBackend {
    text_to_speech: TextToSpeech,
    style: Style,
    language: SupertonicLanguage,
    settings: SupertonicSettings,
}

struct SupertonicLanguage(String);

impl SupertonicLanguage {
    fn new(language: &str) -> Result<Self, TtsError> {
        if SUPPORTED_LANGS.contains(&language) {
            Ok(Self(language.to_string()))
        } else {
            Err(TtsError::UnsupportedLanguage {
                language: language.into(),
                supported: SUPPORTED_LANGS.join(", "),
            })
        }
    }

    fn as_str(&self) -> &str {
        &self.0
    }

    fn uses_short_chunks(&self) -> bool {
        self.0 == "ko" || self.0 == "ja"
    }

    fn preprocess_text(&self, text: &str) -> String {
        let text = normalize_tts_text(text);
        format!("<{}>{text}</{}>", self.0, self.0)
    }
}

struct SupertonicSettings {
    steps: usize,
    speed: f32,
    silence_duration: f32,
}

impl SupertonicSettings {
    fn new(steps: usize, speed: f32, silence_duration: f32) -> Result<Self, TtsError> {
        if steps == 0 {
            return Err(TtsError::InvalidConfig {
                message: "Supertonic steps must be greater than 0".into(),
            });
        }
        if !speed.is_finite() || speed <= 0.0 {
            return Err(TtsError::InvalidConfig {
                message: "Supertonic speed must be finite and greater than 0".into(),
            });
        }
        if !silence_duration.is_finite() || silence_duration < 0.0 {
            return Err(TtsError::InvalidConfig {
                message: "Supertonic silence duration must be finite and non-negative".into(),
            });
        }
        Ok(Self {
            steps,
            speed,
            silence_duration,
        })
    }
}

struct SupertonicAssets {
    onnx_dir: PathBuf,
    voice_style_path: PathBuf,
}

impl SupertonicAssets {
    fn new(model_dir: &Path, voice: &str) -> Result<Self, TtsError> {
        let onnx_dir = model_dir.join("onnx");
        for asset in REQUIRED_ONNX_ASSETS {
            ensure_asset_exists(&onnx_dir.join(asset))?;
        }

        let voice_style_path = model_dir.join("voice_styles").join(format!("{voice}.json"));
        if !voice_style_path.exists() {
            return Err(TtsError::MissingVoice {
                voice: voice.to_string(),
                available: list_voices(model_dir),
            });
        }

        Ok(Self {
            onnx_dir,
            voice_style_path,
        })
    }
}

impl SupertonicBackend {
    pub fn new(
        model_dir: &Path,
        voice: &str,
        language: &str,
        steps: usize,
        speed: f32,
        silence_duration: f32,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let language = SupertonicLanguage::new(language)?;
        let settings = SupertonicSettings::new(steps, speed, silence_duration)?;
        let assets = SupertonicAssets::new(model_dir, voice)?;
        let text_to_speech = TextToSpeech::load(&assets.onnx_dir, device)?;
        let style = Style::load(&assets.voice_style_path)?;

        info!(
            voice,
            language = language.as_str(),
            steps = settings.steps,
            speed = settings.speed,
            sample_rate = text_to_speech.sample_rate,
            "Loaded Supertonic model"
        );

        Ok(Self {
            text_to_speech,
            style,
            language,
            settings,
        })
    }
}

impl TtsBackendImpl for SupertonicBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<Vec<f32>, TtsError> {
        let text = text.trim();
        if text.is_empty() {
            return Err(TtsError::EmptyText);
        }
        self.text_to_speech.call(
            text,
            &self.language,
            &self.style,
            self.settings.steps,
            self.settings.speed,
            self.settings.silence_duration,
        )
    }

    fn sample_rate(&self) -> u32 {
        self.text_to_speech.sample_rate as u32
    }
}

/// Configuration for a Supertonic TTS model.
///
/// Build one with [`SupertonicConfig::new`] and then override fields as needed.
/// All fields have sensible defaults that match the upstream model.
#[derive(Clone, Debug)]
pub struct SupertonicConfig {
    /// HuggingFace repo id (`owner/repo`) or path to a local model directory.
    /// The directory must contain an `onnx/` folder and a `voice_styles/` folder.
    pub source: String,

    /// Voice style name, matching a `voice_styles/{voice}.json` file in the model dir.
    /// Upstream `Supertone/supertonic-3` ships `M1`–`M5` (male) and `F1`–`F5` (female).
    /// An unknown voice yields a [`MissingVoice`](crate::errors::TtsError::MissingVoice) error
    /// listing the voices present in the model dir. Defaults to `M1`.
    pub voice: String,

    /// Language code for the input text. Must be one of the supported languages
    /// (see [`TtsError::UnsupportedLanguage`](crate::errors::TtsError::UnsupportedLanguage)).
    /// Defaults to `en`.
    pub language: String,

    /// Number of diffusion steps used by the latent estimator. More steps generally
    /// improve audio quality at the cost of slower synthesis. Must be greater than 0.
    /// Defaults to `8`.
    pub steps: usize,

    /// Speech speed multiplier. Values greater than `1.0` speed the audio up,
    /// less than `1.0` slow it down. Must be finite and greater than `0.0`.
    /// Defaults to `1.05`.
    pub speed: f32,

    /// Duration (in seconds) of silence inserted between adjacent text chunks.
    /// Must be finite and non-negative. Defaults to `0.3`.
    pub silence_duration: f32,
}

impl SupertonicConfig {
    /// Create a config with upstream defaults for the given `source`.
    ///
    /// `source` is a HuggingFace repo id (`owner/repo`) or a local model directory.
    pub fn new(source: impl AsRef<str>) -> Self {
        Self {
            source: source.as_ref().to_string(),
            voice: "M1".into(),
            language: "en".into(),
            steps: 8,
            speed: 1.05,
            silence_duration: DEFAULT_SILENCE_DURATION,
        }
    }
}

// Matches the nested sections of tts.json, deserializing only fields needed at runtime.
#[derive(Debug, Clone, Deserialize)]
struct SupertonicTtsJsonConfig {
    ae: SpeechAutoencoderConfig,
    ttl: TextToLatentConfig,
}

#[derive(Debug, Clone, Deserialize)]
struct SpeechAutoencoderConfig {
    sample_rate: i32,
    base_chunk_size: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct TextToLatentConfig {
    chunk_compress_factor: i32,
    latent_dim: i32,
}

#[derive(Debug, Clone, Deserialize)]
struct VoiceStyleData {
    style_ttl: StyleComponent,
    style_dp: StyleComponent,
}

#[derive(Debug, Clone, Deserialize)]
struct StyleComponent {
    data: Vec<Vec<Vec<f32>>>,
    dims: Vec<usize>,
}

struct Style {
    ttl: Array3<f32>,
    dp: Array3<f32>,
}

impl Style {
    fn load(path: &Path) -> Result<Self, TtsError> {
        let data: VoiceStyleData = read_json(path)?;
        let ttl = component_to_array(&data.style_ttl, path, "style_ttl")?;
        let dp = component_to_array(&data.style_dp, path, "style_dp")?;
        Ok(Self { ttl, dp })
    }

    fn ttl_tensor(&self) -> Result<Tensor<f32>, TtsError> {
        Ok(Tensor::from_array(self.ttl.clone())?)
    }

    fn dp_tensor(&self) -> Result<Tensor<f32>, TtsError> {
        Ok(Tensor::from_array(self.dp.clone())?)
    }
}

struct UnicodeProcessor {
    indexer: Vec<i64>,
}

struct EncodedTexts {
    text_ids: Tensor<i64>,
    text_mask: Tensor<f32>,
}

impl UnicodeProcessor {
    fn new(path: &Path) -> Result<Self, TtsError> {
        Ok(Self {
            indexer: read_json(path)?,
        })
    }

    fn encode_texts(
        &self,
        text_list: &[String],
        language: &SupertonicLanguage,
    ) -> Result<EncodedTexts, TtsError> {
        let processed_texts: Vec<String> = text_list
            .iter()
            .map(|text| language.preprocess_text(text))
            .collect();
        let lengths: Vec<usize> = processed_texts
            .iter()
            .map(|text| text.chars().count())
            .collect();
        let max_len = lengths.iter().copied().max().unwrap_or(0);
        let mut text_ids = vec![0i64; processed_texts.len() * max_len];

        for (row_index, text) in processed_texts.iter().enumerate() {
            for (column_index, unicode_value) in text.chars().map(|c| c as usize).enumerate() {
                text_ids[row_index * max_len + column_index] =
                    self.indexer.get(unicode_value).copied().unwrap_or(-1);
            }
        }

        let text_ids = Array::from_shape_vec((processed_texts.len(), max_len), text_ids)?;
        Ok(EncodedTexts {
            text_ids: Tensor::from_array(text_ids)?,
            text_mask: Tensor::from_array(length_to_mask(&lengths, max_len))?,
        })
    }
}

struct TextToSpeech {
    config: SupertonicTtsJsonConfig,
    text_processor: UnicodeProcessor,
    duration_predictor: Session,
    text_encoder: Session,
    vector_estimator: Session,
    vocoder: Session,
    sample_rate: i32,
}

impl TextToSpeech {
    fn load(onnx_dir: &Path, device: TtsDevice) -> Result<Self, TtsError> {
        let config: SupertonicTtsJsonConfig = read_json(&onnx_dir.join("tts.json"))?;
        let text_processor = UnicodeProcessor::new(&onnx_dir.join("unicode_indexer.json"))?;
        let duration_predictor =
            ort_util::load_session(&onnx_dir.join("duration_predictor.onnx"), device)?;
        let text_encoder = ort_util::load_session(&onnx_dir.join("text_encoder.onnx"), device)?;
        let vector_estimator =
            ort_util::load_session(&onnx_dir.join("vector_estimator.onnx"), device)?;
        let vocoder = ort_util::load_session(&onnx_dir.join("vocoder.onnx"), device)?;
        let sample_rate = config.ae.sample_rate;

        Ok(Self {
            config,
            text_processor,
            duration_predictor,
            text_encoder,
            vector_estimator,
            vocoder,
            sample_rate,
        })
    }

    fn call(
        &mut self,
        text: &str,
        language: &SupertonicLanguage,
        style: &Style,
        steps: usize,
        speed: f32,
        silence_duration: f32,
    ) -> Result<Vec<f32>, TtsError> {
        let max_len = if language.uses_short_chunks() {
            MAX_CJK_CHUNK_LENGTH
        } else {
            MAX_CHUNK_LENGTH
        };
        let chunks = chunk_text(text, max_len);
        let mut wav = Vec::new();

        for (index, chunk) in chunks.iter().enumerate() {
            let (chunk_wav, duration) =
                self.infer(std::slice::from_ref(chunk), language, style, steps, speed)?;
            append_chunk_audio(
                &mut wav,
                &chunk_wav,
                duration[0],
                self.sample_rate,
                silence_duration,
                index > 0,
            );
        }

        Ok(wav)
    }

    fn infer(
        &mut self,
        text_list: &[String],
        language: &SupertonicLanguage,
        style: &Style,
        steps: usize,
        speed: f32,
    ) -> Result<(Vec<f32>, Vec<f32>), TtsError> {
        let batch_size = text_list.len();
        let encoded_texts = self.text_processor.encode_texts(text_list, language)?;
        let duration = self.predict_duration(&encoded_texts, style, speed)?;
        let style_ttl_tensor = style.ttl_tensor()?;
        let text_emb = self.encode_text_embeddings(&encoded_texts, &style_ttl_tensor)?;
        let latent = self.denoise_latent(
            &duration,
            &encoded_texts,
            &text_emb,
            &style_ttl_tensor,
            steps,
            batch_size,
        )?;
        let wav_data = self.vocode(latent)?;

        debug!(
            samples = wav_data.len(),
            duration = duration[0],
            "Supertonic: done"
        );
        Ok((wav_data, duration))
    }

    fn predict_duration(
        &mut self,
        encoded_texts: &EncodedTexts,
        style: &Style,
        speed: f32,
    ) -> Result<Vec<f32>, TtsError> {
        let style_dp_tensor = style.dp_tensor()?;
        let duration_outputs = self.duration_predictor.run(ort::inputs! {
            "text_ids" => &encoded_texts.text_ids,
            "style_dp" => &style_dp_tensor,
            "text_mask" => &encoded_texts.text_mask,
        })?;
        let (_, duration_data) = duration_outputs["duration"].try_extract_tensor::<f32>()?;
        let mut duration = duration_data.to_vec();
        for value in &mut duration {
            *value /= speed;
        }
        Ok(duration)
    }

    fn encode_text_embeddings(
        &mut self,
        encoded_texts: &EncodedTexts,
        style_ttl_tensor: &Tensor<f32>,
    ) -> Result<Array3<f32>, TtsError> {
        let text_outputs = self.text_encoder.run(ort::inputs! {
            "text_ids" => &encoded_texts.text_ids,
            "style_ttl" => style_ttl_tensor,
            "text_mask" => &encoded_texts.text_mask,
        })?;
        let (text_emb_shape, text_emb_data) =
            text_outputs["text_emb"].try_extract_tensor::<f32>()?;
        Ok(Array3::from_shape_vec(
            (
                text_emb_shape[0] as usize,
                text_emb_shape[1] as usize,
                text_emb_shape[2] as usize,
            ),
            text_emb_data.to_vec(),
        )?)
    }

    fn denoise_latent(
        &mut self,
        duration: &[f32],
        encoded_texts: &EncodedTexts,
        text_emb: &Array3<f32>,
        style_ttl_tensor: &Tensor<f32>,
        steps: usize,
        batch_size: usize,
    ) -> Result<Array3<f32>, TtsError> {
        let (mut noisy_latent, latent_mask) = sample_noisy_latent(
            duration,
            self.sample_rate,
            self.config.ae.base_chunk_size,
            self.config.ttl.chunk_compress_factor,
            self.config.ttl.latent_dim,
        );
        let total_step_tensor = Tensor::from_array(Array::from_elem(batch_size, steps as f32))?;

        for step in 0..steps {
            let noisy_latent_tensor = Tensor::from_array(noisy_latent)?;
            let text_emb_tensor = Tensor::from_array(text_emb.clone())?;
            let latent_mask_tensor = Tensor::from_array(latent_mask.clone())?;
            let current_step_tensor =
                Tensor::from_array(Array::from_elem(batch_size, step as f32))?;

            let outputs = self.vector_estimator.run(ort::inputs! {
                "noisy_latent" => &noisy_latent_tensor,
                "text_emb" => &text_emb_tensor,
                "style_ttl" => style_ttl_tensor,
                "latent_mask" => &latent_mask_tensor,
                "text_mask" => &encoded_texts.text_mask,
                "current_step" => &current_step_tensor,
                "total_step" => &total_step_tensor,
            })?;
            let (denoised_shape, denoised_data) =
                outputs["denoised_latent"].try_extract_tensor::<f32>()?;
            noisy_latent = Array3::from_shape_vec(
                (
                    denoised_shape[0] as usize,
                    denoised_shape[1] as usize,
                    denoised_shape[2] as usize,
                ),
                denoised_data.to_vec(),
            )?;
        }

        Ok(noisy_latent)
    }

    fn vocode(&mut self, latent: Array3<f32>) -> Result<Vec<f32>, TtsError> {
        let latent_tensor = Tensor::from_array(latent)?;
        let outputs = self
            .vocoder
            .run(ort::inputs! { "latent" => &latent_tensor })?;
        let (_, wav_data) = outputs["wav_tts"].try_extract_tensor::<f32>()?;
        Ok(wav_data.to_vec())
    }
}

fn append_chunk_audio(
    wav: &mut Vec<f32>,
    chunk_wav: &[f32],
    duration: f32,
    sample_rate: i32,
    silence_duration: f32,
    include_leading_silence: bool,
) {
    let wav_len = (sample_rate as f32 * duration) as usize;
    if include_leading_silence {
        wav.extend(std::iter::repeat_n(
            0.0,
            (silence_duration * sample_rate as f32) as usize,
        ));
    }
    wav.extend_from_slice(&chunk_wav[..wav_len.min(chunk_wav.len())]);
}

fn list_voices(model_dir: &Path) -> String {
    let Some(entries) = std::fs::read_dir(model_dir.join("voice_styles")).ok() else {
        return String::new();
    };
    let mut voices: Vec<String> = entries
        .filter_map(|entry| entry.ok())
        .filter_map(|entry| {
            entry
                .path()
                .file_stem()
                .and_then(|stem| stem.to_str())
                .map(String::from)
        })
        .collect();
    voices.sort();
    voices.join(", ")
}

fn ensure_asset_exists(path: &Path) -> Result<(), TtsError> {
    if !path.exists() {
        return Err(TtsError::MissingAsset {
            path: path.display().to_string(),
        });
    }
    Ok(())
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, TtsError> {
    let file = File::open(path)?;
    Ok(serde_json::from_reader(BufReader::new(file))?)
}

fn component_to_array(
    component: &StyleComponent,
    path: &Path,
    name: &str,
) -> Result<Array3<f32>, TtsError> {
    if component.dims.len() != 3 || component.dims[0] != 1 {
        return Err(TtsError::InvalidAsset {
            path: path.display().to_string(),
            message: format!("{name} must have dims [1, rows, cols]"),
        });
    }
    let flat: Vec<f32> = component
        .data
        .iter()
        .flat_map(|batch| batch.iter())
        .flat_map(|row| row.iter().copied())
        .collect();
    Ok(Array3::from_shape_vec(
        (component.dims[0], component.dims[1], component.dims[2]),
        flat,
    )?)
}

fn length_to_mask(lengths: &[usize], max_len: usize) -> Array3<f32> {
    let mut mask = Array3::<f32>::zeros((lengths.len(), 1, max_len));
    for (batch, &len) in lengths.iter().enumerate() {
        for index in 0..len.min(max_len) {
            mask[[batch, 0, index]] = 1.0;
        }
    }
    mask
}

fn sample_noisy_latent(
    duration: &[f32],
    sample_rate: i32,
    base_chunk_size: i32,
    chunk_compress: i32,
    latent_dim: i32,
) -> (Array3<f32>, Array3<f32>) {
    let batch_size = duration.len();
    let max_duration = duration.iter().fold(0.0f32, |acc, &value| acc.max(value));
    let chunk_size = (base_chunk_size * chunk_compress) as usize;
    let wav_len_max = (max_duration * sample_rate as f32) as usize;
    let latent_len = wav_len_max.div_ceil(chunk_size);
    let latent_dim = (latent_dim * chunk_compress) as usize;
    let mut noisy_latent = Array3::<f32>::zeros((batch_size, latent_dim, latent_len));
    let normal = Normal::new(0.0, 1.0).unwrap();
    let mut rng = rand::rng();

    for value in noisy_latent.iter_mut() {
        *value = normal.sample(&mut rng);
    }

    let latent_lengths: Vec<usize> = duration
        .iter()
        .map(|&value| ((value * sample_rate as f32) as usize).div_ceil(chunk_size))
        .collect();
    let latent_mask = length_to_mask(&latent_lengths, latent_len);
    for batch in 0..batch_size {
        for dim in 0..latent_dim {
            for time in 0..latent_len {
                noisy_latent[[batch, dim, time]] *= latent_mask[[batch, 0, time]];
            }
        }
    }

    (noisy_latent, latent_mask)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preprocess_wraps_language_and_adds_period() {
        let language = SupertonicLanguage::new("en").unwrap();
        assert_eq!(language.preprocess_text("Hello"), "<en>Hello.</en>");
    }

    #[test]
    fn rejects_unknown_language() {
        assert!(matches!(
            SupertonicLanguage::new("xx"),
            Err(TtsError::UnsupportedLanguage { .. })
        ));
    }

    #[test]
    fn rejects_invalid_numeric_config() {
        assert!(matches!(
            SupertonicSettings::new(8, 0.0, DEFAULT_SILENCE_DURATION),
            Err(TtsError::InvalidConfig { .. })
        ));
        assert!(matches!(
            SupertonicSettings::new(0, 1.0, DEFAULT_SILENCE_DURATION),
            Err(TtsError::InvalidConfig { .. })
        ));
        assert!(matches!(
            SupertonicSettings::new(8, 1.0, f32::INFINITY),
            Err(TtsError::InvalidConfig { .. })
        ));
    }

    #[test]
    fn supertonic_config_defaults_match_upstream() {
        let config = SupertonicConfig::new("model-dir");
        assert_eq!(config.voice, "M1");
        assert_eq!(config.language, "en");
        assert_eq!(config.steps, 8);
        assert_eq!(config.speed, 1.05);
        assert_eq!(config.silence_duration, 0.3);
    }

    #[test]
    fn missing_voice_lists_available() {
        let dir = std::env::temp_dir().join(format!(
            "nobodywho-supertonic-test-{}",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        let styles = dir.join("voice_styles");
        std::fs::create_dir_all(&styles).unwrap();
        let onnx = dir.join("onnx");
        std::fs::create_dir_all(&onnx).unwrap();
        for asset in REQUIRED_ONNX_ASSETS {
            std::fs::write(onnx.join(asset), "{}").unwrap();
        }
        std::fs::write(styles.join("M1.json"), "{}").unwrap();
        std::fs::write(styles.join("F2.json"), "{}").unwrap();

        let Err(err) = SupertonicAssets::new(&dir, "M24") else {
            panic!("expected missing voice");
        };
        match err {
            TtsError::MissingVoice { voice, available } => {
                assert_eq!(voice, "M24");
                assert_eq!(available, "F2, M1");
            }
            other => panic!("expected MissingVoice, got {other:?}"),
        }

        std::fs::remove_dir_all(&dir).ok();
    }
}
