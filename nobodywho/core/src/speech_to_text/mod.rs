mod architecture;
mod architectures;
pub(crate) mod audio;

use crate::errors::SpeechToTextError;
pub use crate::onnx::Device;
pub use crate::stream::{StreamOutput, TokenStream, TokenStreamAsync};
pub use architectures::WhisperConfig;
use std::path::PathBuf;
use std::sync::mpsc;
use std::sync::Arc;
use tokio::sync::mpsc::UnboundedReceiver;

// ---------------------------------------------------------------------------
// Internal request types
// ---------------------------------------------------------------------------

enum AudioInput {
    File(PathBuf),
    Pcm { samples: Vec<i16>, sample_rate: u32 },
}

enum SpeechToTextResponseChannel {
    Full(tokio::sync::mpsc::Sender<Result<String, SpeechToTextError>>),
    Stream(tokio::sync::mpsc::UnboundedSender<StreamOutput<SpeechToTextError>>),
}

type SpeechToTextRequest = (AudioInput, SpeechToTextResponseChannel);

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Architecture selection and model source for an [`SpeechToText`] handle.
#[derive(Clone, Debug)]
pub enum SpeechToTextConfig {
    Whisper(WhisperConfig),
}

impl SpeechToTextConfig {
    pub fn whisper(source: impl AsRef<str>) -> Self {
        Self::Whisper(WhisperConfig::new(source))
    }
}

/// SpeechToText handle. Transcription runs on a background worker thread.
///
/// Cheap to clone — cloning only copies the channel sender.
#[derive(Clone)]
pub struct SpeechToText {
    worker: Arc<Worker>,
}

// Dropped when the last handle drops. Joining matters: otherwise the worker
// thread may still be dropping the model while the host process (e.g. Godot)
// unloads this library at exit, which segfaults. Mirrors the TextToSpeech fix.
struct Worker {
    // Field order is load-bearing: msg_tx drops first, closing the channel
    // so the worker loop exits; then the join below can complete.
    msg_tx: mpsc::Sender<SpeechToTextRequest>,
    _join: crate::join_on_drop::JoinOnDrop,
}

impl SpeechToText {
    pub fn new(config: SpeechToTextConfig) -> Result<Self, SpeechToTextError> {
        Self::with_device(config, Device::Auto)
    }

    pub fn with_device(
        config: SpeechToTextConfig,
        device: Device,
    ) -> Result<Self, SpeechToTextError> {
        let mut architecture = architecture::load_architecture(config, device)?;
        let (msg_tx, msg_rx) = mpsc::channel::<SpeechToTextRequest>();
        let join = std::thread::spawn(move || {
            while let Ok((input, response)) = msg_rx.recv() {
                match response {
                    SpeechToTextResponseChannel::Full(tx) => {
                        let result = architecture::transcribe_sync(architecture.as_mut(), input);
                        if tx.blocking_send(result).is_err() {
                            tracing::warn!(
                                "SpeechToText caller dropped before result could be delivered"
                            );
                        }
                    }
                    SpeechToTextResponseChannel::Stream(tx) => {
                        architecture::transcribe_streaming(architecture.as_mut(), input, tx);
                    }
                }
            }
        });
        Ok(Self {
            worker: Arc::new(Worker {
                msg_tx,
                _join: crate::join_on_drop::JoinOnDrop::new(join),
            }),
        })
    }

    fn enqueue(
        &self,
        input: AudioInput,
    ) -> Result<tokio::sync::mpsc::Receiver<Result<String, SpeechToTextError>>, SpeechToTextError>
    {
        let (tx, rx) = tokio::sync::mpsc::channel(1);
        self.worker
            .msg_tx
            .send((input, SpeechToTextResponseChannel::Full(tx)))
            .map_err(|e| SpeechToTextError::Transcription(format!("stt worker stopped: {e}")))?;
        Ok(rx)
    }

    fn enqueue_stream(
        &self,
        input: AudioInput,
    ) -> Result<UnboundedReceiver<StreamOutput<SpeechToTextError>>, SpeechToTextError> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.worker
            .msg_tx
            .send((input, SpeechToTextResponseChannel::Stream(tx)))
            .map_err(|e| SpeechToTextError::Transcription(format!("stt worker stopped: {e}")))?;
        Ok(rx)
    }

    // --- Full transcription ---

    pub fn transcribe_file(&self, path: impl Into<PathBuf>) -> Result<String, SpeechToTextError> {
        self.enqueue(AudioInput::File(path.into()))?
            .blocking_recv()
            .ok_or_else(|| {
                SpeechToTextError::Transcription("stt worker dropped response sender".into())
            })?
    }

    pub async fn transcribe_file_async(
        &self,
        path: impl Into<PathBuf>,
    ) -> Result<String, SpeechToTextError> {
        self.enqueue(AudioInput::File(path.into()))?
            .recv()
            .await
            .ok_or_else(|| {
                SpeechToTextError::Transcription("stt worker dropped response sender".into())
            })?
    }

    pub fn transcribe_pcm(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
    ) -> Result<String, SpeechToTextError> {
        self.enqueue(AudioInput::Pcm {
            samples,
            sample_rate,
        })?
        .blocking_recv()
        .ok_or_else(|| {
            SpeechToTextError::Transcription("stt worker dropped response sender".into())
        })?
    }

    pub async fn transcribe_pcm_async(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
    ) -> Result<String, SpeechToTextError> {
        self.enqueue(AudioInput::Pcm {
            samples,
            sample_rate,
        })?
        .recv()
        .await
        .ok_or_else(|| {
            SpeechToTextError::Transcription("stt worker dropped response sender".into())
        })?
    }

    // --- Streaming ---

    pub fn transcribe_file_stream(
        &self,
        path: impl Into<PathBuf>,
    ) -> Result<TokenStream<SpeechToTextError>, SpeechToTextError> {
        Ok(TokenStream::new(
            self.enqueue_stream(AudioInput::File(path.into()))?,
        ))
    }

    pub fn transcribe_file_stream_async(
        &self,
        path: impl Into<PathBuf>,
    ) -> Result<TokenStreamAsync<SpeechToTextError>, SpeechToTextError> {
        Ok(TokenStreamAsync::new(
            self.enqueue_stream(AudioInput::File(path.into()))?,
        ))
    }

    pub fn transcribe_pcm_stream(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
    ) -> Result<TokenStream<SpeechToTextError>, SpeechToTextError> {
        Ok(TokenStream::new(self.enqueue_stream(AudioInput::Pcm {
            samples,
            sample_rate,
        })?))
    }

    pub fn transcribe_pcm_stream_async(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
    ) -> Result<TokenStreamAsync<SpeechToTextError>, SpeechToTextError> {
        Ok(TokenStreamAsync::new(self.enqueue_stream(
            AudioInput::Pcm {
                samples,
                sample_rate,
            },
        )?))
    }
}
