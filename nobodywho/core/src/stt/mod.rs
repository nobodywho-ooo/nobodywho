pub(crate) mod audio;
mod backend;
mod backends;

use crate::errors::SttError;
pub use crate::onnx::Device;
pub use crate::stream::{StreamOutput, TokenStream, TokenStreamAsync};
pub use backends::WhisperConfig;
use std::path::PathBuf;
use std::sync::mpsc;
use tokio::sync::mpsc::UnboundedReceiver;

// ---------------------------------------------------------------------------
// Internal request types
// ---------------------------------------------------------------------------

enum AudioInput {
    File(PathBuf),
    Pcm { samples: Vec<i16>, sample_rate: u32 },
}

enum SttResponseChannel {
    Full(tokio::sync::mpsc::Sender<Result<String, SttError>>),
    Stream(tokio::sync::mpsc::UnboundedSender<StreamOutput<SttError>>),
}

type SttRequest = (AudioInput, SttResponseChannel);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

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

/// STT handle. Transcription runs on a background worker thread.
///
/// Cheap to clone — cloning only copies the channel sender.
#[derive(Clone)]
pub struct Stt {
    msg_tx: mpsc::Sender<SttRequest>,
}

impl Stt {
    pub fn new(config: SttConfig) -> Result<Self, SttError> {
        Self::with_device(config, Device::Auto)
    }

    pub fn with_device(config: SttConfig, device: Device) -> Result<Self, SttError> {
        let mut backend = backend::load_backend(config, device)?;
        let (msg_tx, msg_rx) = mpsc::channel::<SttRequest>();
        std::thread::spawn(move || {
            while let Ok((input, response)) = msg_rx.recv() {
                match response {
                    SttResponseChannel::Full(tx) => {
                        let result = backend::transcribe_sync(backend.as_mut(), input);
                        if tx.blocking_send(result).is_err() {
                            tracing::warn!("STT caller dropped before result could be delivered");
                        }
                    }
                    SttResponseChannel::Stream(tx) => {
                        backend::transcribe_streaming(backend.as_mut(), input, tx);
                    }
                }
            }
        });
        Ok(Self { msg_tx })
    }

    fn enqueue(&self, input: AudioInput) -> Result<tokio::sync::mpsc::Receiver<Result<String, SttError>>, SttError> {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        self.msg_tx
            .send((input, SttResponseChannel::Full(tx)))
            .map_err(|e| SttError::Transcription(format!("stt worker stopped: {e}")))?;
        Ok(rx)
    }

    fn enqueue_stream(&self, input: AudioInput) -> Result<UnboundedReceiver<StreamOutput<SttError>>, SttError> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.msg_tx
            .send((input, SttResponseChannel::Stream(tx)))
            .map_err(|e| SttError::Transcription(format!("stt worker stopped: {e}")))?;
        Ok(rx)
    }

    // --- Full transcription ---

    pub fn transcribe_file(&self, path: impl Into<PathBuf>) -> Result<String, SttError> {
        self.enqueue(AudioInput::File(path.into()))?
            .blocking_recv()
            .ok_or_else(|| SttError::Transcription("stt worker dropped response sender".into()))?
    }

    pub async fn transcribe_file_async(&self, path: impl Into<PathBuf>) -> Result<String, SttError> {
        self.enqueue(AudioInput::File(path.into()))?
            .recv().await
            .ok_or_else(|| SttError::Transcription("stt worker dropped response sender".into()))?
    }

    pub fn transcribe_pcm(&self, samples: Vec<i16>, sample_rate: u32) -> Result<String, SttError> {
        self.enqueue(AudioInput::Pcm { samples, sample_rate })?
            .blocking_recv()
            .ok_or_else(|| SttError::Transcription("stt worker dropped response sender".into()))?
    }

    pub async fn transcribe_pcm_async(&self, samples: Vec<i16>, sample_rate: u32) -> Result<String, SttError> {
        self.enqueue(AudioInput::Pcm { samples, sample_rate })?
            .recv().await
            .ok_or_else(|| SttError::Transcription("stt worker dropped response sender".into()))?
    }

    // --- Streaming ---

    pub fn transcribe_file_stream(&self, path: impl Into<PathBuf>) -> Result<TokenStream<SttError>, SttError> {
        Ok(TokenStream::new(self.enqueue_stream(AudioInput::File(path.into()))?))
    }

    pub fn transcribe_file_stream_async(&self, path: impl Into<PathBuf>) -> Result<TokenStreamAsync<SttError>, SttError> {
        Ok(TokenStreamAsync::new(self.enqueue_stream(AudioInput::File(path.into()))?))
    }

    pub fn transcribe_pcm_stream(&self, samples: Vec<i16>, sample_rate: u32) -> Result<TokenStream<SttError>, SttError> {
        Ok(TokenStream::new(self.enqueue_stream(AudioInput::Pcm { samples, sample_rate })?))
    }

    pub fn transcribe_pcm_stream_async(&self, samples: Vec<i16>, sample_rate: u32) -> Result<TokenStreamAsync<SttError>, SttError> {
        Ok(TokenStreamAsync::new(self.enqueue_stream(AudioInput::Pcm { samples, sample_rate })?))
    }
}
