//! Pure speech/silence edge-detection from a stream of per-frame speech
//! probabilities. No model, no I/O. See `backend.rs` for what feeds this.

/// One 32ms Silero frame is this long at 16kHz.
const FRAME_MS: u32 = 32;

/// Silero's own end-of-speech trigger uses a lower threshold than the start
/// trigger to avoid flicker right at the boundary (see Silero's `VADIterator`).
const END_HYSTERESIS: f32 = 0.15;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VadEvent {
    SpeechStarted,
    SpeechEnded,
}

#[derive(Clone, Copy, Debug)]
pub struct DebounceConfig {
    pub threshold: f32,
    pub min_silence_duration_ms: u32,
    pub min_speech_duration_ms: u32,
}

impl Default for DebounceConfig {
    fn default() -> Self {
        Self {
            threshold: 0.5,
            min_silence_duration_ms: 250,
            min_speech_duration_ms: 250,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum State {
    Silence,
    /// Speech probability is high but hasn't been sustained for
    /// `min_speech_duration_ms` yet — not confirmed as speech.
    PendingSpeech {
        frames: u32,
    },
    Speech,
    /// Probability dropped but hasn't stayed low for
    /// `min_silence_duration_ms` yet — not confirmed as silence.
    PendingSilence {
        frames: u32,
    },
}

pub struct Debouncer {
    config: DebounceConfig,
    state: State,
}

impl Debouncer {
    pub fn new(config: DebounceConfig) -> Self {
        Self {
            config,
            state: State::Silence,
        }
    }

    pub fn reset(&mut self) {
        self.state = State::Silence;
    }

    /// Feed one frame's speech probability, get back an edge event if this
    /// frame crossed a confirmed speech/silence boundary.
    pub fn step(&mut self, speech_prob: f32) -> Option<VadEvent> {
        let min_speech_frames = (self.config.min_speech_duration_ms / FRAME_MS).max(1);
        let min_silence_frames = (self.config.min_silence_duration_ms / FRAME_MS).max(1);
        let is_speech = speech_prob >= self.config.threshold;
        let is_silence = speech_prob < self.config.threshold - END_HYSTERESIS;

        match self.state {
            State::Silence => {
                if is_speech {
                    if min_speech_frames <= 1 {
                        self.state = State::Speech;
                        return Some(VadEvent::SpeechStarted);
                    }
                    self.state = State::PendingSpeech { frames: 1 };
                }
                None
            }
            State::PendingSpeech { frames } => {
                if is_speech {
                    let frames = frames + 1;
                    if frames >= min_speech_frames {
                        self.state = State::Speech;
                        Some(VadEvent::SpeechStarted)
                    } else {
                        self.state = State::PendingSpeech { frames };
                        None
                    }
                } else {
                    self.state = State::Silence;
                    None
                }
            }
            State::Speech => {
                if is_silence {
                    if min_silence_frames <= 1 {
                        self.state = State::Silence;
                        return Some(VadEvent::SpeechEnded);
                    }
                    self.state = State::PendingSilence { frames: 1 };
                }
                None
            }
            State::PendingSilence { frames } => {
                if is_silence {
                    let frames = frames + 1;
                    if frames >= min_silence_frames {
                        self.state = State::Silence;
                        Some(VadEvent::SpeechEnded)
                    } else {
                        self.state = State::PendingSilence { frames };
                        None
                    }
                } else {
                    // Back above the (higher) speech threshold before the
                    // hangover elapsed — still one continuous utterance.
                    self.state = State::Speech;
                    None
                }
            }
        }
    }
}
