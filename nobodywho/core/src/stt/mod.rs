pub(crate) mod audio;
mod backend;
mod backends;

use crate::errors::SttError;
pub use crate::onnx::Device;
pub use backends::WhisperConfig;
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

type SttRequest = (
    AudioInput,
    tokio::sync::mpsc::Sender<Result<String, SttError>>,
);

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
    pub fn transcribe_pcm(&self, samples: Vec<i16>, sample_rate: u32) -> Result<String, SttError> {
        self.enqueue(AudioInput::Pcm {
            samples,
            sample_rate,
        })?
        .blocking_recv()
        .ok_or_else(|| SttError::Transcription("stt worker dropped response sender".into()))?
    }

    /// Transcribe raw i16 PCM samples asynchronously.
    pub async fn transcribe_pcm_async(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
    ) -> Result<String, SttError> {
        self.enqueue(AudioInput::Pcm {
            samples,
            sample_rate,
        })?
        .recv()
        .await
        .ok_or_else(|| SttError::Transcription("stt worker dropped response sender".into()))?
    }
}
