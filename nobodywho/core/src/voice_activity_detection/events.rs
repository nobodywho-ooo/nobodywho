//! Pure speech/silence edge-detection from a stream of per-frame speech
//! probabilities. No model, no I/O. See `backend.rs` for what feeds this.

/// One 32ms Silero frame is this long at 16kHz.
const FRAME_MS: u32 = 32;

/// Affects the size of the debouncing gap.
const DEBOUNCING_FRACTION: f32 = 0.3;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VoiceActivityDetectionEvent {
    /// Confirmed speech, unchanged since the last step.
    Speech,
    /// This step confirmed the transition into speech.
    SpeechStarted,
    /// This step confirmed the transition into silence.
    SpeechEnded,
    /// Confirmed silence (or not-yet-confirmed speech), unchanged since the last step.
    Silence,
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

    /// Current confirmed state, without stepping — `Silence` for `State::Silence`
    /// and `State::PendingSpeech` (not yet confirmed as speech), `Speech` for
    /// `State::Speech` and `State::PendingSilence` (not yet confirmed as silence).
    pub fn current(&self) -> VoiceActivityDetectionEvent {
        match self.state {
            State::Silence | State::PendingSpeech { .. } => VoiceActivityDetectionEvent::Silence,
            State::Speech | State::PendingSilence { .. } => VoiceActivityDetectionEvent::Speech,
        }
    }

    pub fn step(&mut self, speech_prob: f32) -> VoiceActivityDetectionEvent {
        let min_speech_frames = (self.config.min_speech_duration_ms / FRAME_MS).max(1);
        let min_silence_frames = (self.config.min_silence_duration_ms / FRAME_MS).max(1);

        // Gap between speech and silence bounds absorbs flicker around the
        // threshold. Scaled by threshold (not absolute) so it stays valid
        // for any threshold value.
        let silence_bound = self.config.threshold * (1.0 - DEBOUNCING_FRACTION);

        let is_speech = speech_prob >= self.config.threshold;
        let is_silence = speech_prob < silence_bound;

        match self.state {
            State::Silence => {
                if is_speech {
                    if min_speech_frames <= 1 {
                        self.state = State::Speech;
                        return VoiceActivityDetectionEvent::SpeechStarted;
                    }
                    self.state = State::PendingSpeech { frames: 1 };
                }
                VoiceActivityDetectionEvent::Silence
            }
            State::PendingSpeech { frames } => {
                if is_speech {
                    let frames = frames + 1;
                    if frames >= min_speech_frames {
                        self.state = State::Speech;
                        VoiceActivityDetectionEvent::SpeechStarted
                    } else {
                        self.state = State::PendingSpeech { frames };
                        VoiceActivityDetectionEvent::Silence
                    }
                } else {
                    self.state = State::Silence;
                    VoiceActivityDetectionEvent::Silence
                }
            }
            State::Speech => {
                if is_silence {
                    if min_silence_frames <= 1 {
                        self.state = State::Silence;
                        return VoiceActivityDetectionEvent::SpeechEnded;
                    }
                    self.state = State::PendingSilence { frames: 1 };
                }
                VoiceActivityDetectionEvent::Speech
            }
            State::PendingSilence { frames } => {
                if is_silence {
                    let frames = frames + 1;
                    if frames >= min_silence_frames {
                        self.state = State::Silence;
                        VoiceActivityDetectionEvent::SpeechEnded
                    } else {
                        self.state = State::PendingSilence { frames };
                        VoiceActivityDetectionEvent::Speech
                    }
                } else {
                    // Back above threshold before hangover elapsed — same utterance.
                    self.state = State::Speech;
                    VoiceActivityDetectionEvent::Speech
                }
            }
        }
    }
}
