//! Speech-to-text transcription using Whisper via ONNX Runtime.
//!
//! [`Stt::new`] takes an [`SttConfig`] pointing at either a local directory
//! or a HuggingFace Hub repo ID (`owner/repo`). HF repos are downloaded into
//! the user's cache on first use, then reused.
//!
//! ```no_run
//! # use nobodywho::stt::{Stt, SttConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let stt = Stt::new(SttConfig::whisper("onnx-community/whisper-base"))?;
//! let text = stt.transcribe_file("recording.wav")?;
//! # let _ = text;
//! # Ok(())
//! # }
//! ```
//!
//! Override language (ISO 639-1 code; default is auto-detect):
//!
//! ```no_run
//! # use nobodywho::stt::{Stt, SttConfig, WhisperConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut cfg = WhisperConfig::new("onnx-community/whisper-base");
//! cfg.language = Some("en".into());
//! let stt = Stt::new(SttConfig::Whisper(cfg))?;
//! # Ok(())
//! # }
//! ```
//!
//! Pass raw i16 PCM from a microphone stream (e.g. Flutter `mic_stream` or
//! React Native `voice-processor`):
//!
//! ```no_run
//! # use nobodywho::stt::{Stt, SttConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! # let stt = Stt::new(SttConfig::whisper("onnx-community/whisper-base"))?;
//! # let mic_chunks: Vec<i16> = vec![];
//! let text = stt.transcribe_pcm(mic_chunks, 16_000)?;
//! # Ok(())
//! # }
//! ```
//!
//! From an async context use the `_async` variants:
//!
//! ```no_run
//! # use nobodywho::stt::{Stt, SttConfig};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let stt = Stt::new(SttConfig::whisper("onnx-community/whisper-base"))?;
//! let text = stt.transcribe_file_async("recording.mp3").await?;
//! # Ok(())
//! # }
//! ```

pub(crate) mod audio;
mod backend;
mod backends;

use crate::errors::SttError;
pub use backends::WhisperConfig;
pub use crate::onnx::Device;
use std::path::PathBuf;
use std::sync::mpsc;

/// Backend selection and model source for an [`Stt`] handle.
#[derive(Clone, Debug)]
pub enum SttConfig {
    Whisper(WhisperConfig),
}

impl SttConfig {
    pub fn whisper(source: impl AsRef<str>) -> Self {
        Self::Whisper(WhisperConfig::new(source))
    }
}

/// Internal audio input discriminant — not part of the public API.
enum AudioInput {
    File(PathBuf),
    Pcm { samples: Vec<i16>, sample_rate: u32 },
}

type SttRequest = (AudioInput, tokio::sync::mpsc::Sender<Result<String, SttError>>);

/// STT handle. Transcription runs on a background worker thread.
///
/// Cheap to clone — cloning only copies the channel sender.
#[derive(Clone)]
pub struct Stt {
    msg_tx: mpsc::Sender<SttRequest>,
}

impl Stt {
    /// Build an `Stt` handle using [`Device::Auto`] (prefer CUDA, fall back to CPU).
    pub fn new(config: SttConfig) -> Result<Self, SttError> {
        Self::with_device(config, Device::Auto)
    }

    /// Build an `Stt` handle targeting a specific [`Device`].
    pub fn with_device(config: SttConfig, device: Device) -> Result<Self, SttError> {
        let mut backend = backend::load_backend(config, device)?;
        let (msg_tx, msg_rx) = mpsc::channel::<SttRequest>();
        std::thread::spawn(move || {
            while let Ok((input, response_tx)) = msg_rx.recv() {
                let result = backend::transcribe_sync(backend.as_mut(), input);
                if response_tx.blocking_send(result).is_err() {
                    tracing::warn!("STT caller dropped before result could be delivered");
                }
            }
        });
        Ok(Self { msg_tx })
    }

    fn enqueue(
        &self,
        input: AudioInput,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<String, SttError>>, SttError> {
        let (response_tx, response_rx) = tokio::sync::mpsc::channel(1);
        self.msg_tx
            .send((input, response_tx))
            .map_err(|e| SttError::Transcription(format!("stt worker stopped: {e}")))?;
        Ok(response_rx)
    }

    /// Transcribe an audio file (WAV / MP3 / FLAC). Blocks until complete.
    pub fn transcribe_file(&self, path: impl Into<PathBuf>) -> Result<String, SttError> {
        self.enqueue(AudioInput::File(path.into()))?
            .blocking_recv()
            .ok_or_else(|| SttError::Transcription("stt worker dropped response sender".into()))?
    }

    /// Transcribe an audio file asynchronously.
    pub async fn transcribe_file_async(
        &self,
        path: impl Into<PathBuf>,
    ) -> Result<String, SttError> {
        self.enqueue(AudioInput::File(path.into()))?
            .recv()
            .await
            .ok_or_else(|| SttError::Transcription("stt worker dropped response sender".into()))?
    }

    /// Transcribe raw i16 PCM samples captured from a microphone.
    ///
    /// `sample_rate` is the capture rate (e.g. 16000, 44100). The backend
    /// resamples to 16 kHz internally if needed. Blocks until complete.
    pub fn transcribe_pcm(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
    ) -> Result<String, SttError> {
        self.enqueue(AudioInput::Pcm { samples, sample_rate })?
            .blocking_recv()
            .ok_or_else(|| SttError::Transcription("stt worker dropped response sender".into()))?
    }

    /// Transcribe raw i16 PCM samples asynchronously.
    pub async fn transcribe_pcm_async(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
    ) -> Result<String, SttError> {
        self.enqueue(AudioInput::Pcm { samples, sample_rate })?
            .recv()
            .await
            .ok_or_else(|| SttError::Transcription("stt worker dropped response sender".into()))?
    }
}
