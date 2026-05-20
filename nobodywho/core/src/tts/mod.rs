//! Text-to-speech synthesis using the Kokoro model family.
//!
//! Pass a model directory to [`TtsConfig::kokoro`] and build a [`Tts`] handle
//! with [`Tts::new`]. Download weights from our HuggingFace collection:
//! <https://huggingface.co/NobodyWho/collections>
//!
//! ```no_run
//! # use nobodywho::tts::{KokoroConfig, Tts, TtsConfig};
//! # fn main() -> Result<(), Box<dyn std::error::Error>> {
//! let tts = Tts::new(TtsConfig::Kokoro(KokoroConfig::new("kokoro-v1")))?;
//! let wav = tts.synthesize("Hello from NobodyWho")?;
//! # let _ = wav;
//! # Ok(())
//! # }
//! ```
//!
//! From an async context use `synthesize_async`:
//!
//! ```no_run
//! # use nobodywho::tts::{KokoroConfig, Tts, TtsConfig};
//! # async fn example() -> Result<(), Box<dyn std::error::Error>> {
//! let tts = Tts::new(TtsConfig::Kokoro(KokoroConfig::new("kokoro-v1")))?;
//! let wav = tts.synthesize_async("Hello from NobodyWho").await?;
//! # let _ = wav;
//! # Ok(())
//! # }
//! ```

mod backend;
mod backends;
mod ort_util;
mod source;

use crate::errors::TtsError;
pub use backends::KokoroConfig;
use std::sync::mpsc;

/// Backend selection and model directory for a [`Tts`] handle.
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
    /// Force CPU execution.
    Cpu,
    /// Require CUDA; fail loudly if it isn't available.
    Cuda,
}

pub(super) fn ort_execution_providers(
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

pub(super) const DEFAULT_SAMPLE_RATE: u32 = 24000;

type SynthRequest = (String, mpsc::Sender<Result<Vec<u8>, TtsError>>);

/// TTS handle. Synthesis runs on a background worker thread; both sync and
/// async entry points are provided. `Clone` is cheap (channel sender clone)
/// and all clones drive the same worker.
#[derive(Clone)]
pub struct Tts {
    msg_tx: mpsc::Sender<SynthRequest>,
}

impl Tts {
    /// Build a `Tts` handle with [`TtsDevice::Auto`].
    pub fn new(config: TtsConfig) -> Result<Self, TtsError> {
        Self::with_device(config, TtsDevice::Auto)
    }

    /// Build a `Tts` handle on the specified device.
    pub fn with_device(config: TtsConfig, device: TtsDevice) -> Result<Self, TtsError> {
        let backend = backend::load_backend(config, device)?;
        Ok(Self::from_backend(backend))
    }

    fn from_backend(mut backend: Box<dyn backend::TtsBackendImpl>) -> Self {
        let (msg_tx, msg_rx) = mpsc::channel::<SynthRequest>();
        std::thread::spawn(move || {
            while let Ok((text, response_tx)) = msg_rx.recv() {
                let result = backend::synthesize_sync(backend.as_mut(), &text);
                if response_tx.send(result).is_err() {
                    tracing::warn!("TTS caller dropped before result could be delivered");
                }
            }
        });
        Self { msg_tx }
    }

    pub fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        let (response_tx, response_rx) = mpsc::channel();
        self.msg_tx
            .send((text.into(), response_tx))
            .map_err(|e| TtsError::Synthesis(format!("tts worker stopped: {e}")))?;
        response_rx
            .recv()
            .map_err(|e| TtsError::Synthesis(format!("tts worker stopped: {e}")))?
    }

    pub async fn synthesize_async(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        let this = self.clone();
        let text = text.into();
        tokio::task::spawn_blocking(move || this.synthesize(text))
            .await
            .map_err(|e| TtsError::Synthesis(format!("task join error: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::tts::backend::TtsBackendImpl;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::thread;

    struct MockBackend {
        synth_calls: Arc<AtomicUsize>,
        next_pcm: Vec<f32>,
        sample_rate: u32,
    }

    impl TtsBackendImpl for MockBackend {
        fn synthesize_raw(&mut self, _text: &str) -> Result<(Vec<f32>, u32), TtsError> {
            self.synth_calls.fetch_add(1, Ordering::SeqCst);
            Ok((self.next_pcm.clone(), self.sample_rate))
        }
    }

    #[test]
    fn multiple_concurrent_callers_all_complete() {
        let synth_calls = Arc::new(AtomicUsize::new(0));
        let backend: Box<dyn TtsBackendImpl> = Box::new(MockBackend {
            synth_calls: Arc::clone(&synth_calls),
            next_pcm: vec![0.1; 8],
            sample_rate: 16_000,
        });
        let tts = Tts::from_backend(backend);

        let threads: Vec<_> = (0..4)
            .map(|i| {
                let t = tts.clone();
                thread::spawn(move || t.synthesize(format!("t{i}")).unwrap())
            })
            .collect();

        for t in threads {
            let wav = t.join().unwrap();
            assert!(wav.starts_with(b"RIFF"));
        }
        assert_eq!(synth_calls.load(Ordering::SeqCst), 4);
    }

    #[test]
    #[ignore = "requires TEST_KOKORO_DIR with model.onnx and voices/<voice>.safetensors"]
    fn kokoro_synthesize_smoke() -> Result<(), Box<dyn std::error::Error>> {
        let dir = std::env::var("TEST_KOKORO_DIR").unwrap_or_else(|_| "kokoro".to_string());
        let tts = Tts::new(TtsConfig::kokoro(dir))?;
        let wav_bytes = tts.synthesize("Hello world")?;
        assert!(!wav_bytes.is_empty());

        let cursor = std::io::Cursor::new(&wav_bytes);
        let reader = hound::WavReader::new(cursor)?;
        assert_eq!(reader.spec().sample_rate, DEFAULT_SAMPLE_RATE);
        Ok(())
    }
}
