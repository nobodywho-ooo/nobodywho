//! [`Tts::new`] takes a [`TtsConfig`] pointing at either a local directory
//! or a HuggingFace Hub repo ID (`owner/repo`).
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
//! ```no_run
//! // American English using [`Tts::synthesize_async`]:
//!
//! # use nobodywho::tts::{KokoroConfig, Tts, TtsConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let mut cfg = KokoroConfig::new("NobodyWho/Kokoro-82M");
//! cfg.voice = "am_michael".into();
//! cfg.language = "en-us".into();
//! cfg.speed = 1.1;
//! let tts = Tts::new(TtsConfig::Kokoro(cfg))?;
//! ```

mod backend;
mod kokoro;
mod ort_util;

use crate::errors::TtsError;
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

/// Hardware target for ONNX Runtime execution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtsDevice {
    /// Prefer CUDA, silently fall back to CPU if unavailable.
    Auto,
    Cpu,
    Cuda,
}

pub(super) fn ort_execution_providers(
    device: TtsDevice,
) -> Vec<ort::ep::ExecutionProviderDispatch> {
    match device {
        // CPU is listed alongside CUDA as a per-op fallback,
        // as some ops may not have ONNX CUDA kernel.
        // CUDA still takes whichever ops it supports.
        TtsDevice::Cuda => vec![
            ort::ep::CUDA::default().build().error_on_failure(),
            ort::ep::CPU::default().build(),
        ],
        TtsDevice::Cpu => vec![ort::ep::CPU::default().build()],
        TtsDevice::Auto => vec![
            ort::ep::CUDA::default().build().fail_silently(),
            ort::ep::CPU::default().build(),
        ],
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
                let result = backend::synthesize_sync(backend.as_mut(), &text);
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
            .map_err(|e| TtsError::Synthesis(format!("tts worker stopped: {e}")))?;
        Ok(response_rx)
    }

    pub fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        self.enqueue(text.into())?
            .blocking_recv()
            .ok_or_else(|| TtsError::Synthesis("tts worker dropped response sender".into()))?
    }

    pub async fn synthesize_async(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        self.enqueue(text.into())?
            .recv()
            .await
            .ok_or_else(|| TtsError::Synthesis("tts worker dropped response sender".into()))?
    }
}
