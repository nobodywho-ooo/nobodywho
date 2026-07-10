//! Text-to-speech synthesis using the Kokoro model family.
//!
//! [`Tts::new`] takes a [`TtsConfig`] pointing at either a local directory
//! or a HuggingFace Hub repo (`hf://owner/repo`). HF repos are downloaded
//! into the user's cache on first use, then reused.
//!
//! ```no_run
//! # use nobodywho::tts::{Tts, TtsConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let tts = Tts::new(TtsConfig::kokoro("hf://hexgrad/Kokoro-82M"))?;
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
//! # use nobodywho::tts::{KokoroConfig, Tts, TtsConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut cfg = KokoroConfig::new("hf://hexgrad/Kokoro-82M");
//! cfg.voice = "am_michael".into();
//! cfg.speed = 1.1;
//! cfg.language = "en-us".into();
//! let tts = Tts::new(TtsConfig::Kokoro(cfg))?;
//! # Ok(())
//! # }
//! ```
//!
//! From an async context use [`Tts::synthesize_async`]:
//!
//! ```no_run
//! # use nobodywho::tts::{Tts, TtsConfig};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let tts = Tts::new(TtsConfig::kokoro("hf://hexgrad/Kokoro-82M"))?;
//! let wav = tts.synthesize_async("Hello from NobodyWho!").await?;
//! # let _ = wav;
//! # Ok(())
//! # }
//! ```

mod backend;
mod kokoro;
mod supertonic;

use crate::errors::TtsError;
pub use crate::onnx::Device as TtsDevice;
pub use kokoro::KokoroConfig;
use std::{str::FromStr, sync::mpsc};
pub use supertonic::SupertonicConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtsArchitecture {
    Kokoro,
    Supertonic,
}

impl TtsArchitecture {
    /// Infer the architecture from the source string by case-insensitive
    /// substring matching. A source containing `"kokoro"` resolves to
    /// [`TtsArchitecture::Kokoro`], one containing `"supertonic"` to
    /// [`TtsArchitecture::Supertonic`], otherwise `None`. Forks and renamed
    /// repos are detected as long as the architecture name appears in the path.
    pub fn infer_from_source(source: &str) -> Option<Self> {
        let lower = source.to_ascii_lowercase();
        if lower.contains("kokoro") {
            Some(Self::Kokoro)
        } else if lower.contains("supertonic") {
            Some(Self::Supertonic)
        } else {
            None
        }
    }
}

impl FromStr for TtsArchitecture {
    type Err = ();

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value.to_ascii_lowercase().as_str() {
            "kokoro" => Ok(Self::Kokoro),
            "supertonic" => Ok(Self::Supertonic),
            _ => Err(()),
        }
    }
}

#[derive(Clone, Debug)]
pub enum TtsConfig {
    Kokoro(KokoroConfig),
    Supertonic(SupertonicConfig),
}

impl TtsConfig {
    pub fn from_source(
        source: impl AsRef<str>,
        architecture: Option<TtsArchitecture>,
    ) -> Option<Self> {
        let source = source.as_ref();
        match architecture.or_else(|| TtsArchitecture::infer_from_source(source))? {
            TtsArchitecture::Kokoro => Some(Self::kokoro(source)),
            TtsArchitecture::Supertonic => Some(Self::supertonic(source)),
        }
    }

    pub fn kokoro(source: impl AsRef<str>) -> Self {
        Self::Kokoro(KokoroConfig::new(source))
    }

    pub fn supertonic(source: impl AsRef<str>) -> Self {
        Self::Supertonic(SupertonicConfig::new(source))
    }
}

pub(super) const DEFAULT_SAMPLE_RATE: u32 = 24000;

type SynthRequest = (String, tokio::sync::mpsc::Sender<Result<Vec<u8>, TtsError>>);

#[derive(Clone)]
pub struct Tts {
    msg_tx: mpsc::Sender<SynthRequest>,
}

impl Tts {
    pub fn new(config: TtsConfig) -> Result<Self, TtsError> {
        Self::with_device(config, TtsDevice::Auto)
    }

    pub fn with_device(config: TtsConfig, device: TtsDevice) -> Result<Self, TtsError> {
        let mut backend = backend::load_backend(config, device)?;
        let (msg_tx, msg_rx) = mpsc::channel::<SynthRequest>();
        std::thread::spawn(move || {
            while let Ok((text, response_tx)) = msg_rx.recv() {
                let result = backend.synthesize(&text);
                if response_tx.blocking_send(result).is_err() {
                    tracing::warn!("TTS caller dropped before result could be delivered");
                }
            }
        });
        Ok(Self { msg_tx })
    }

    fn enqueue(
        &self,
        text: String,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<Vec<u8>, TtsError>>, TtsError> {
        let (response_tx, response_rx) = tokio::sync::mpsc::channel(1);
        self.msg_tx
            .send((text, response_tx))
            .map_err(|_| TtsError::WorkerDead)?;
        Ok(response_rx)
    }

    pub fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        self.enqueue(text.into())?
            .blocking_recv()
            .ok_or(TtsError::WorkerDead)?
    }

    pub async fn synthesize_async(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        self.enqueue(text.into())?
            .recv()
            .await
            .ok_or(TtsError::WorkerDead)?
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn infers_kokoro_from_substring() {
        assert_eq!(
            TtsArchitecture::infer_from_source("hf://hexgrad/Kokoro-82M"),
            Some(TtsArchitecture::Kokoro)
        );
        assert_eq!(
            TtsArchitecture::infer_from_source("hf://my-org/my-kokoro-fork"),
            Some(TtsArchitecture::Kokoro)
        );
    }

    #[test]
    fn infers_supertonic_from_substring() {
        assert_eq!(
            TtsArchitecture::infer_from_source("hf://Supertone/supertonic-3"),
            Some(TtsArchitecture::Supertonic)
        );
    }

    #[test]
    fn inference_is_case_insensitive() {
        assert_eq!(
            TtsArchitecture::infer_from_source("hf://org/KOKORO-big"),
            Some(TtsArchitecture::Kokoro)
        );
    }

    #[test]
    fn infers_none_for_unknown_source() {
        assert_eq!(TtsArchitecture::infer_from_source("hf://random/repo"), None);
    }

    #[test]
    fn explicit_architecture_overrides_inference() {
        // If the user passes an explicit architecture, it wins even when the
        // source string would infer a different one.
        let config =
            TtsConfig::from_source("hf://org/supertonic-style", Some(TtsArchitecture::Kokoro));
        assert!(matches!(config, Some(TtsConfig::Kokoro(_))));
    }

    #[test]
    fn from_source_returns_none_when_architecture_unknown() {
        assert!(TtsConfig::from_source("hf://random/repo", None).is_none());
    }
}
