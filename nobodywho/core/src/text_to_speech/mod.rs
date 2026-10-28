//! Text-to-speech synthesis using the Kokoro model family.
//!
//! [`TextToSpeech::new`] takes a [`TextToSpeechConfig`] pointing at either a local directory
//! or a HuggingFace Hub repo (`hf://owner/repo`). HF repos are downloaded
//! into the user's cache on first use, then reused.
//!
//! ```no_run
//! # use nobodywho::text_to_speech::{TextToSpeech, TextToSpeechConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let tts = TextToSpeech::new(TextToSpeechConfig::kokoro("hf://hexgrad/Kokoro-82M"))?;
//! let wav = tts.synthesize("Hello from NobodyWho!")?;
//! # let _ = wav;
//! # Ok(())
//! # }
//! ```
//!
//! Override voice, speed, and language (espeak-ng language code). The full
//! list of voices lives on the model's HuggingFace page:
//!
//! ```no_run
//! # use nobodywho::text_to_speech::{KokoroConfig, TextToSpeech, TextToSpeechConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut cfg = KokoroConfig::new("hf://hexgrad/Kokoro-82M");
//! cfg.voice = "am_michael".into();
//! cfg.speed = 1.1;
//! cfg.language = "en-us".into();
//! let tts = TextToSpeech::new(TextToSpeechConfig::Kokoro(cfg))?;
//! # Ok(())
//! # }
//! ```
//!
//! From an async context use [`TextToSpeech::synthesize_async`]:
//!
//! ```no_run
//! # use nobodywho::text_to_speech::{TextToSpeech, TextToSpeechConfig};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let tts = TextToSpeech::new(TextToSpeechConfig::kokoro("hf://hexgrad/Kokoro-82M"))?;
//! let wav = tts.synthesize_async("Hello from NobodyWho!").await?;
//! # let _ = wav;
//! # Ok(())
//! # }
//! ```

mod architecture;
mod kokoro;
mod pocket;
mod supertonic;

pub use crate::onnx::Device as TextToSpeechDevice;
use crate::{errors::TextToSpeechError, worker::AsyncWorker};
pub use kokoro::KokoroConfig;
pub use pocket::{PocketTtsConfig, PocketTtsPrecision};
use std::{str::FromStr, sync::Arc};
pub use supertonic::SupertonicConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TextToSpeechArchitecture {
    Kokoro,
    PocketTts,
    Supertonic,
}

impl TextToSpeechArchitecture {
    /// Infer the architecture from the source string by case-insensitive
    /// substring matching. A source containing `"kokoro"` resolves to
    /// [`TextToSpeechArchitecture::Kokoro`], one containing `"supertonic"` to
    /// [`TextToSpeechArchitecture::Supertonic`], otherwise `None`. Forks and renamed
    /// repos are detected as long as the architecture name appears in the path.
    pub fn infer_from_source(source: &str) -> Option<Self> {
        let lower = source.to_ascii_lowercase();
        if lower.contains("kokoro") {
            Some(Self::Kokoro)
        } else if lower.contains("pocket-tts") || lower.contains("pocket_tts") {
            Some(Self::PocketTts)
        } else if lower.contains("supertonic") {
            Some(Self::Supertonic)
        } else {
            None
        }
    }
}

impl FromStr for TextToSpeechArchitecture {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "kokoro" => Ok(Self::Kokoro),
            "pocket-tts" | "pocket_tts" | "pockettts" => Ok(Self::PocketTts),
            "supertonic" => Ok(Self::Supertonic),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
pub enum TextToSpeechConfig {
    Kokoro(KokoroConfig),
    PocketTts(PocketTtsConfig),
    Supertonic(SupertonicConfig),
}

impl TextToSpeechConfig {
    pub fn from_source(
        source: impl AsRef<str>,
        architecture: Option<TextToSpeechArchitecture>,
    ) -> Option<Self> {
        let source = source.as_ref();
        match architecture.or_else(|| TextToSpeechArchitecture::infer_from_source(source))? {
            TextToSpeechArchitecture::Kokoro => Some(Self::kokoro(source)),
            TextToSpeechArchitecture::PocketTts => Some(Self::pocket_tts(source)),
            TextToSpeechArchitecture::Supertonic => Some(Self::supertonic(source)),
        }
    }

    pub fn kokoro(source: impl AsRef<str>) -> Self {
        Self::Kokoro(KokoroConfig::new(source))
    }

    pub fn pocket_tts(source: impl AsRef<str>) -> Self {
        Self::PocketTts(PocketTtsConfig::new(source))
    }

    pub fn supertonic(source: impl AsRef<str>) -> Self {
        Self::Supertonic(SupertonicConfig::new(source))
    }
}

pub(super) const DEFAULT_SAMPLE_RATE: u32 = 24000;

pub struct TextToSpeech {
    architecture: Arc<tokio::sync::Mutex<Box<dyn architecture::TextToSpeechArchitectureImpl>>>,
    worker: AsyncWorker,
}

impl TextToSpeech {
    pub fn new(config: TextToSpeechConfig) -> Result<Self, TextToSpeechError> {
        Self::with_device(config, TextToSpeechDevice::Auto)
    }

    pub fn with_device(
        config: TextToSpeechConfig,
        device: TextToSpeechDevice,
    ) -> Result<Self, TextToSpeechError> {
        // FIXME(madsmtm): ONNX Runtime is not yet available on Emscripten.
        if cfg!(target_os = "emscripten") {
            return Err(TextToSpeechError::WasmUnsupported);
        }
        let architecture = architecture::load_architecture(config, device)?;
        Ok(Self {
            architecture: Arc::new(tokio::sync::Mutex::new(architecture)),
            worker: AsyncWorker::new(),
        })
    }

    pub fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TextToSpeechError> {
        self.architecture.blocking_lock().synthesize(&text.into())
    }

    pub async fn synthesize_async(
        &self,
        text: impl Into<String>,
    ) -> Result<Vec<u8>, TextToSpeechError> {
        let text = text.into();
        let mut arch = Arc::clone(&self.architecture).lock_owned().await;
        self.worker.run(move || arch.synthesize(&text)).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_kokoro_from_substring() {
        assert_eq!(
            TextToSpeechArchitecture::infer_from_source("hf://hexgrad/Kokoro-82M"),
            Some(TextToSpeechArchitecture::Kokoro)
        );
        assert_eq!(
            TextToSpeechArchitecture::infer_from_source("hf://my-org/my-kokoro-fork"),
            Some(TextToSpeechArchitecture::Kokoro)
        );
    }

    #[test]
    fn infers_pocket_tts_from_substring() {
        assert_eq!(
            TextToSpeechArchitecture::infer_from_source("hf://KevinAHM/pocket-tts-onnx"),
            Some(TextToSpeechArchitecture::PocketTts)
        );
    }

    #[test]
    fn infers_supertonic_from_substring() {
        assert_eq!(
            TextToSpeechArchitecture::infer_from_source("hf://Supertone/supertonic-3"),
            Some(TextToSpeechArchitecture::Supertonic)
        );
    }

    #[test]
    fn inference_is_case_insensitive() {
        assert_eq!(
            TextToSpeechArchitecture::infer_from_source("hf://org/KOKORO-big"),
            Some(TextToSpeechArchitecture::Kokoro)
        );
    }

    #[test]
    fn infers_none_for_unknown_source() {
        assert_eq!(
            TextToSpeechArchitecture::infer_from_source("hf://random/repo"),
            None
        );
    }

    #[test]
    fn explicit_architecture_overrides_inference() {
        // If the user passes an explicit architecture, it wins even when the
        // source string would infer a different one.
        let config = TextToSpeechConfig::from_source(
            "hf://org/supertonic-style",
            Some(TextToSpeechArchitecture::Kokoro),
        );
        assert!(matches!(config, Some(TextToSpeechConfig::Kokoro(_))));
    }

    #[test]
    fn from_source_returns_none_when_architecture_unknown() {
        assert!(TextToSpeechConfig::from_source("hf://random/repo", None).is_none());
    }
}
