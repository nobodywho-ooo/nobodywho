//! Voice Activity Detection (speech start/end) from live, streaming audio.
//!
//! Backed by [Silero VAD](https://github.com/snakers4/silero-vad) (MIT
//! licensed) by default (`hf://onnx-community/silero-vad`),
//! downloaded and cached the same way `Stt`'s Whisper architecture resolves
//! its models — first use requires network access, subsequent uses are
//! offline. The model source is configurable via [`VadConfig::source`] for
//! forks/mirrors that keep the same `onnx/model.onnx` layout.
//!
//! Feed each newest chunk to [`Vad::push`] as it arrives — `Vad` buffers the
//! current turn internally, seeded with a small pre-roll so the confirmed
//! speech isn't clipped at the start. Once a `SpeechEnded` comes back, call
//! [`Vad::finish`] to get that turn's audio and reset for the next one.

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
    /// `hf://owner/repo` HuggingFace source or local directory path for the
    /// VAD ONNX model. Expected to contain `onnx/model.onnx` at the
    /// standard Silero VAD layout — a fork or mirror of the reference
    /// model works as long as it matches that layout. Defaults to
    /// `hf://onnx-community/silero-vad`, the canonical Silero VAD mirror;
    /// most users should leave this as-is.
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
    /// How long should we keep before the true speech start event comes.
    /// Avoids filtering out start of the speech because the VAD is "unsure" yet.
    pub preroll_duration_ms: u32,
}

impl Default for VadConfig {
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
            backend: VadBackend::new(
                &config.source,
                config.sample_rate,
                config.preroll_duration_ms,
                debounce_config,
                device,
            )?,
        })
    }

    /// Feed the newest chunk of audio (not the whole accumulated buffer —
    /// `Vad` tracks the current turn internally). Returns `Some(VadEvent)`
    /// if this call crossed a confirmed speech/silence boundary. Errors on
    /// ONNX inference or resampling failures — typically a corrupt or
    /// incompatible downloaded model.
    pub fn push(&mut self, chunk: &[i16]) -> Result<Option<VadEvent>, VadError> {
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

    /// Run whatever complete Silero frames `chunk` completes through the
    /// model and return their raw speech probabilities, in order — no
    /// debouncing, no audio buffering. For callers who want to do their own
    /// thresholding/smoothing instead of using `push`'s built-in debounce
    /// logic, or who want zero memory overhead beyond fixed model state.
    /// Safe to call with any chunk size, from a live mic buffer up to an
    /// entire recording at once. If you reuse one `Vad` across unrelated
    /// audio sessions, call `finish` in between to clear state so it doesn't
    /// leak across sessions.
    pub fn predict(&mut self, chunk: &[i16]) -> Result<Vec<f32>, VadError> {
        self.backend.predict(chunk)
    }

    /// Detect every speech segment in a complete audio buffer at once,
    /// returning each segment's audio (with a small pre-roll lead-in) in
    /// order. Unlike `push`, this is guaranteed not to drop a transition
    /// regardless of buffer size — the right tool for offline/batch
    /// processing of a full recording rather than live streaming.
    pub fn segment(&mut self, samples: &[i16]) -> Result<Vec<Vec<i16>>, VadError> {
        self.backend.segment(samples)
    }
}
