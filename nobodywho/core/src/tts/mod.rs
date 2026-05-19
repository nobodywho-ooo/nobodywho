//! Text-to-speech synthesis supporting multiple model families.
//!
//! Every backend takes a single model directory ([`TtsConfig::kokoro`] /
//! [`piper`][TtsConfig::piper] / [`chatterbox`][TtsConfig::chatterbox] /
//! [`roest`][TtsConfig::roest]); see each `*Config` for the expected layout.
//! To guarantee the expected layout, you can download the weights from our
//! HuggingFace: https://huggingface.co/NobodyWho/collections
//!
//! | Backend      | Quality       | Voice cloning | Languages              |
//! |--------------|---------------|---------------|------------------------|
//! | Kokoro       | High          | No            | English, Chinese       |
//! | Piper        | Medium        | No            | 80+ (espeak-ng backed) |
//! | Chatterbox   | High          | Yes (WAV)     | 23 (incl. Danish)      |
//! | Røst         | High (Danish) | Preset        | Danish (finetune)      |
//!
//! Use [`TtsBuilder`] with an explicit [`TtsConfig`] variant to load a backend.
//! Synchronous handles take text and return WAV bytes:
//!
//! ```no_run
//! # use nobodywho::tts::{RoestConfig, TtsBuilder, TtsConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let tts = TtsBuilder::new(TtsConfig::Roest(RoestConfig::new("roest500m_onnx"))).build()?;
//! let wav = tts.synthesize("Hej fra NobodyWho")?;
//! # let _ = wav;
//! # Ok(())
//! # }
//! ```
//!
//! From an async context, call `synthesize_async`:
//!
//! ```no_run
//! # use nobodywho::tts::{ChatterboxConfig, TtsBuilder, TtsConfig};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let tts = TtsBuilder::new(TtsConfig::Chatterbox(ChatterboxConfig::new("chatterbox"))).build()?;
//! let wav = tts.synthesize_async("Hello from NobodyWho").await?;
//! # let _ = wav;
//! # Ok(())
//! # }
//! ```

mod backend;
mod backends;
mod ort_util;
mod sampling;

use crate::errors::TtsError;
pub use backends::{ChatterboxConfig, KokoroConfig, PiperConfig, RoestConfig};
pub use sampling::TtsSampling;
use std::path::PathBuf;
use std::sync::Arc;

/// Backend selection and model directory for a [`Tts`] handle.
#[derive(Clone, Debug)]
pub enum TtsConfig {
    Kokoro(KokoroConfig),
    Piper(PiperConfig),
    Chatterbox(ChatterboxConfig),
    Roest(RoestConfig),
}

impl TtsConfig {
    pub fn kokoro(model_dir: impl Into<PathBuf>) -> Self {
        Self::Kokoro(KokoroConfig::new(model_dir))
    }

    pub fn piper(model_dir: impl Into<PathBuf>) -> Self {
        Self::Piper(PiperConfig::new(model_dir))
    }

    pub fn chatterbox(model_dir: impl Into<PathBuf>) -> Self {
        Self::Chatterbox(ChatterboxConfig::new(model_dir))
    }

    pub fn roest(model_dir: impl Into<PathBuf>) -> Self {
        Self::Roest(RoestConfig::new(model_dir))
    }
}

/// Builder for creating a [`Tts`] handle with an explicit backend config.
pub struct TtsBuilder {
    pub(crate) config: TtsConfig,
    pub(crate) device: TtsDevice,
}

impl TtsBuilder {
    pub fn new(config: TtsConfig) -> Self {
        Self {
            config,
            device: TtsDevice::Auto,
        }
    }

    pub fn with_device(mut self, device: TtsDevice) -> Self {
        self.device = device;
        self
    }

    pub fn build(self) -> Result<Tts, TtsError> {
        Tts::from_config(self.config, self.device)
    }
}

/// Hardware target for ONNX Runtime execution. All backends honor this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtsDevice {
    /// Prefer CUDA, silently fall back to CPU if unavailable.
    Auto,
    /// Force CPU execution.
    Cpu,
    /// Require CUDA; fail loudly if it isn't available.
    Cuda,
}

pub(crate) fn ort_execution_providers(
    device: TtsDevice,
) -> Vec<ort::ep::ExecutionProviderDispatch> {
    match device {
        TtsDevice::Cpu => vec![ort::ep::CPU::default().build()],
        TtsDevice::Cuda => vec![
            ort::ep::CUDA::default().build().error_on_failure(),
            ort::ep::CPU::default().build(),
        ],
        TtsDevice::Auto => vec![
            ort::ep::CUDA::default().build().fail_silently(),
            ort::ep::CPU::default().build(),
        ],
    }
}

/// Default audio sample rate for Kokoro, Chatterbox, and Røst. Each backend
/// uses this unless the caller overrides `sample_rate` on the config. Piper
/// always reports its own rate from its model config file.
pub(crate) const DEFAULT_SAMPLE_RATE: u32 = 24000;

/// TTS handle. Synthesis runs on a background worker; both sync and async
/// entry points are provided.
#[derive(Clone)]
pub struct Tts {
    worker: Arc<backend::TtsWorker>,
}

impl Tts {
    pub(crate) fn from_config(config: TtsConfig, device: TtsDevice) -> Result<Self, TtsError> {
        Ok(Self {
            worker: Arc::new(backend::TtsWorker::new(backend::load_backend(
                config, device,
            )?)),
        })
    }

    pub fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        self.worker.synthesize(text.into())
    }

    pub async fn synthesize_async(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        let worker = Arc::clone(&self.worker);
        let text = text.into();
        tokio::task::spawn_blocking(move || worker.synthesize(text))
            .await
            .map_err(|e| TtsError::Synthesis(format!("task join error: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_kokoro_dir() -> String {
        std::env::var("TEST_KOKORO_DIR").unwrap_or_else(|_| "kokoro".to_string())
    }

    #[test]
    fn builder_defaults_to_auto_device() {
        let builder = TtsBuilder::new(TtsConfig::roest("model-dir"));
        assert_eq!(builder.device, TtsDevice::Auto);
    }

    #[test]
    fn typed_roest_config_sets_sampling() {
        let config = RoestConfig::new("model-dir").with_sampling(TtsSampling {
            temperature: 0.2,
            top_k: 40,
            top_p: 0.9,
            min_p: 0.02,
            cfg_weight: 0.0,
            repetition_penalty: 1.5,
        });
        let builder = TtsBuilder::new(TtsConfig::Roest(config));
        match builder.config {
            TtsConfig::Roest(config) => {
                assert_eq!(config.language, "da");
                assert_eq!(config.sampling.temperature, 0.2);
                assert_eq!(config.sampling.top_k, 40);
                assert_eq!(config.sampling.top_p, 0.9);
                assert_eq!(config.sampling.min_p, 0.02);
                assert_eq!(config.sampling.cfg_weight, 0.0);
                assert_eq!(config.sampling.repetition_penalty, 1.5);
            }
            _ => panic!("expected roest config"),
        }
    }

    #[test]
    #[ignore = "requires TEST_KOKORO_DIR with model.onnx and voices/<voice>.bin"]
    fn kokoro_synthesize_smoke() -> Result<(), Box<dyn std::error::Error>> {
        let tts = TtsBuilder::new(TtsConfig::kokoro(test_kokoro_dir())).build()?;
        let wav_bytes = tts.synthesize("Hello world")?;
        assert!(!wav_bytes.is_empty());

        let cursor = std::io::Cursor::new(&wav_bytes);
        let reader = hound::WavReader::new(cursor)?;
        assert_eq!(reader.spec().sample_rate, DEFAULT_SAMPLE_RATE);
        Ok(())
    }

    #[test]
    #[ignore = "requires TEST_PIPER_DIR with model.onnx and model.onnx.json"]
    fn piper_synthesize_smoke() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::var("TEST_PIPER_DIR").unwrap_or_else(|_| "piper".to_string());
        let tts = TtsBuilder::new(TtsConfig::piper(dir)).build()?;
        let wav_bytes = tts.synthesize("Hej verden")?;
        assert!(!wav_bytes.is_empty());

        let cursor = std::io::Cursor::new(&wav_bytes);
        let reader = hound::WavReader::new(cursor)?;
        assert_eq!(reader.spec().sample_rate, 22050);
        Ok(())
    }
}
