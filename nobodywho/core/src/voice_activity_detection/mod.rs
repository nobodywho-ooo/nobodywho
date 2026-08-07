//! Voice Activity Detection (speech start/end) from live, streaming audio.
//!
//! Backed by [Silero VAD](https://github.com/snakers4/silero-vad) (MIT
//! licensed) by default (`hf://onnx-community/silero-vad`), downloaded and
//! cached the same way `SpeechToText`'s Whisper models are — first use
//! requires network access, subsequent uses are offline. Point
//! [`VoiceActivityDetectionConfig::source`] at a fork/mirror that keeps the
//! same `onnx/model.onnx` layout to use a different one.
//!
//! Two ways to use it, depending on what you have:
//! - **Streaming audio**: feed each newest chunk to [`VoiceActivityDetection::push`]
//!   as it arrives. Once it returns `SpeechEnded`, call [`VoiceActivityDetection::finish`]
//!   to get that turn's audio (with a pre-roll so the start isn't clipped) and reset for the next one.
//! - **A complete recording**: call [`VoiceActivityDetection::segment`] once to get every
//!   speech segment's audio back at once.

mod backend;
mod events;

use crate::errors::VoiceActivityDetectionError;
pub use crate::onnx::Device;
use backend::VoiceActivityDetectionBackend;
use events::DebounceConfig;
pub use events::VoiceActivityDetectionEvent;

/// Configuration for [`VoiceActivityDetection`].
#[derive(Clone, Debug)]
pub struct VoiceActivityDetectionConfig {
    /// `hf://owner/repo` HuggingFace source or local directory path for the
    /// VAD ONNX model. Expected to contain `onnx/model.onnx` at the
    /// standard Silero VAD layout — a fork or mirror of the reference
    /// model works as long as it matches that layout. Defaults to
    /// `hf://onnx-community/silero-vad`, the canonical Silero VAD mirror;
    /// most users should leave this as-is.
    pub source: String,
    /// Sample rate of the buffers you'll pass to [`VoiceActivityDetection::push`]. Silero
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
    /// How much audio to buffer before a confirmed speech start, so the
    /// beginning of speech isn't clipped while the debouncer is still deciding.
    pub preroll_duration_ms: u32,
}

impl Default for VoiceActivityDetectionConfig {
    fn default() -> Self {
        let debounce = DebounceConfig::default();
        Self {
            source: "hf://onnx-community/silero-vad".to_string(),
            sample_rate: 16_000,
            threshold: debounce.threshold,
            min_silence_duration_ms: debounce.min_silence_duration_ms,
            min_speech_duration_ms: debounce.min_speech_duration_ms,
            preroll_duration_ms: 500,
        }
    }
}

/// Voice activity detector. See the module docs for usage.
pub struct VoiceActivityDetection {
    backend: VoiceActivityDetectionBackend,
}

impl VoiceActivityDetection {
    pub fn new(config: VoiceActivityDetectionConfig) -> Result<Self, VoiceActivityDetectionError> {
        Self::with_device(config, Device::Auto)
    }

    pub fn with_device(
        config: VoiceActivityDetectionConfig,
        device: Device,
    ) -> Result<Self, VoiceActivityDetectionError> {
        if config.sample_rate == 0 {
            return Err(VoiceActivityDetectionError::Init(
                "sample_rate must be non-zero".into(),
            ));
        }
        let debounce_config = DebounceConfig {
            threshold: config.threshold,
            min_silence_duration_ms: config.min_silence_duration_ms,
            min_speech_duration_ms: config.min_speech_duration_ms,
        };
        Ok(Self {
            backend: VoiceActivityDetectionBackend::new(
                &config.source,
                config.sample_rate,
                config.preroll_duration_ms,
                debounce_config,
                device,
            )?,
        })
    }

    /// Feed the newest chunk of audio (not the whole accumulated buffer —
    /// `VoiceActivityDetection` tracks the current turn internally). Always
    /// returns the current confirmed state: `Speech`/`Silence` if unchanged
    /// since the last call, or `SpeechStarted`/`SpeechEnded` on the call that
    /// confirmed the transition. Errors on ONNX inference or resampling
    /// failures — typically a corrupt or incompatible downloaded model.
    pub fn push(
        &mut self,
        chunk: &[i16],
    ) -> Result<VoiceActivityDetectionEvent, VoiceActivityDetectionError> {
        self.backend.push(chunk)
    }

    /// Return the current turn's captured audio (from the confirmed
    /// `SpeechStarted`, including a small pre-roll, through to
    /// `SpeechEnded`) and reset internal state for the next turn. Call this
    /// once you've handled a `SpeechEnded`, or at any point to abandon the
    /// current turn early. Empty if speech was never confirmed.
    pub fn finish(&mut self) -> Vec<i16> {
        self.backend.finish()
    }

    /// Detect every speech segment in a complete audio buffer, returning
    /// each segment's audio (with a short pre-roll) in order. Unlike `push`,
    /// correctly finds every segment regardless of buffer size — use this
    /// for offline/batch processing instead of live streaming.
    pub fn segment(
        &mut self,
        samples: &[i16],
    ) -> Result<Vec<Vec<i16>>, VoiceActivityDetectionError> {
        self.backend.segment(samples)
    }
}
