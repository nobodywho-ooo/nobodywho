//! Voice Activity Detection (speech start/end) from live, streaming audio.
//!
//! Backed by [Silero VAD](https://github.com/snakers4/silero-vad) (MIT
//! licensed) by default (`onnx-community/silero-vad` on HuggingFace),
//! downloaded and cached the same way `Stt`'s Whisper backend resolves its
//! models — first use requires network access, subsequent uses are
//! offline. The model source is configurable via [`VadConfig::source`] for
//! forks/mirrors that keep the same `onnx/model.onnx` layout.

mod backend;
mod events;

use crate::errors::VadError;
pub use crate::onnx::Device;
use backend::VadBackend;
use events::DebounceConfig;
pub use events::VadEvent;

/// Configuration for [`Vad`].
#[derive(Clone, Debug)]
pub struct VadConfig {
    /// HuggingFace repo ID (`owner/repo`) or local directory path for the
    /// VAD ONNX model. Expected to contain `onnx/model.onnx` at the
    /// standard Silero VAD layout — a fork or mirror of the reference
    /// model works as long as it matches that layout. Defaults to
    /// `onnx-community/silero-vad`, the canonical Silero VAD mirror; most
    /// users should leave this as-is.
    pub source: String,
    /// Sample rate of the buffers you'll pass to [`Vad::push`]. Silero
    /// natively runs at 16kHz — anything else is resampled internally.
    /// Must be non-zero.
    pub sample_rate: u32,
    /// Silero speech-probability cutoff above which a frame counts as speech.
    pub threshold: f32,
    /// How long silence must persist before a confirmed `SpeechEnded` fires
    /// (avoids stopping on natural mid-sentence pauses).
    pub min_silence_duration_ms: u32,
    /// How long speech must persist before a confirmed `SpeechStarted`
    /// fires (filters out short noise blips).
    pub min_speech_duration_ms: u32,
}

impl Default for VadConfig {
    fn default() -> Self {
        let debounce = DebounceConfig::default();
        Self {
            source: "onnx-community/silero-vad".to_string(),
            sample_rate: 16_000,
            threshold: debounce.threshold,
            min_silence_duration_ms: debounce.min_silence_duration_ms,
            min_speech_duration_ms: debounce.min_speech_duration_ms,
        }
    }
}

/// Voice activity detector. See the module docs for usage.
pub struct Vad {
    backend: VadBackend,
}

impl Vad {
    pub fn new(config: VadConfig) -> Result<Self, VadError> {
        Self::with_device(config, Device::Auto)
    }

    pub fn with_device(config: VadConfig, device: Device) -> Result<Self, VadError> {
        if config.sample_rate == 0 {
            return Err(VadError::Init("sample_rate must be non-zero".into()));
        }
        let debounce_config = DebounceConfig {
            threshold: config.threshold,
            min_silence_duration_ms: config.min_silence_duration_ms,
            min_speech_duration_ms: config.min_speech_duration_ms,
        };
        Ok(Self {
            backend: VadBackend::new(&config.source, config.sample_rate, debounce_config, device)?,
        })
    }

    /// Feed your entire accumulated buffer (not just the newest chunk).
    /// Returns `Some(VadEvent)` if this call crossed a confirmed
    /// speech/silence boundary.
    pub fn push(&mut self, buffer: &[i16]) -> Option<VadEvent> {
        // push() only fails on ONNX inference errors, which would indicate
        // a corrupt/incompatible downloaded model, not a caller error —
        // surfacing that as a silently-swallowed None would hide a real
        // bug, so unwrap here and let it surface loudly.
        self.backend
            .push(buffer)
            .expect("Silero VAD inference failed — check the downloaded model is not corrupt")
    }
}
