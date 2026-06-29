//! Text-to-speech synthesis using the Kokoro model family.
//!
//! [`Tts::new`] takes a [`TtsConfig`] pointing at either a local directory
//! or a HuggingFace Hub repo ID (`owner/repo`). HF repos are downloaded
//! into the user's cache on first use, then reused.
//!
//! ```no_run
//! # use nobodywho::tts::{Tts, TtsConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let tts = Tts::new(TtsConfig::kokoro("NobodyWho/Kokoro-82M"))?;
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
//! // American English using [`Tts::synthesize_async`]:
//!
//! # use nobodywho::tts::{KokoroConfig, Tts, TtsConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut cfg = KokoroConfig::new("NobodyWho/Kokoro-82M");
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
//! let tts = Tts::new(TtsConfig::kokoro("NobodyWho/Kokoro-82M"))?;
//! let wav = tts.synthesize_async("Hello from NobodyWho!").await?;
//! # let _ = wav;
//! # Ok(())
//! # }
//! ```

mod backend;
mod kokoro;

use crate::errors::TtsError;
pub use crate::onnx::Device as TtsDevice;
pub use kokoro::KokoroConfig;
use std::sync::mpsc;

#[derive(Clone, Debug)]
pub enum TtsConfig {
    Kokoro(KokoroConfig),
}

impl TtsConfig {
    pub fn kokoro(source: impl AsRef<str>) -> Self {
        Self::Kokoro(KokoroConfig::new(source))
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
