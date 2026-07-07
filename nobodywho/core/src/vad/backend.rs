//! Silero VAD backend: owns the ONNX session, converts an arbitrary-rate,
//! arbitrary-chunked, ever-growing `&[i16]` buffer into 512-sample 16kHz
//! frames, and drives the pure debounce state machine in `events.rs`.
//!
//! Model: https://huggingface.co/onnx-community/silero-vad (MIT licensed,
//! mirrors https://github.com/snakers4/silero-vad), resolved and cached the
//! same way `Stt`'s Whisper backend resolves its models.

use crate::errors::VadError;
use crate::huggingface;
use crate::onnx::{load_session, Device};
use crate::stt::audio::{AudioResampler, DecodedAudio};
use crate::vad::events::{DebounceConfig, Debouncer, VadEvent};
use ort::session::Session;
use ort::value::Tensor;

/// Silero operates on exactly this many samples per frame at 16kHz.
const FRAME_SAMPLES: usize = 512;
const SILERO_SAMPLE_RATE: i64 = 16_000;

pub(super) struct VadBackend {
    session: Session,
    sample_rate: u32,
    /// LSTM hidden state carried between frames: shape (2, 1, 128) flattened.
    model_state: Vec<f32>,
    /// 16kHz f32 samples accumulated but not yet consumed as a full frame.
    frame_buffer: Vec<f32>,
    /// How many raw i16 samples of the caller's buffer we've already turned
    /// into frame_buffer input. Used both to only process new tail data and
    /// to detect "buffer got shorter -> new turn" (see push()). Always in
    /// units of the caller's raw buffer, regardless of sample rate.
    raw_processed_len: usize,
    /// How many samples of the *resampled* 16kHz stream have already been
    /// fed into frame_buffer. Only meaningful on the non-16kHz path, where
    /// the whole accumulated buffer is re-resampled from scratch each call
    /// (see push()) and we must avoid re-feeding samples we already consumed.
    /// Stays 0 always on the native-16kHz path.
    resampled_processed_len: usize,
    debouncer: Debouncer,
}

impl VadBackend {
    pub(super) fn new(
        source: &str,
        sample_rate: u32,
        debounce_config: DebounceConfig,
        device: Device,
    ) -> Result<Self, VadError> {
        let model_dir = huggingface::download_onnx(source, &["onnx/model.onnx".to_string()])?;
        let session = load_session(&model_dir.join("onnx").join("model.onnx"), device)?;
        Ok(Self {
            session,
            sample_rate,
            model_state: vec![0.0; 2 * 128],
            frame_buffer: Vec::with_capacity(FRAME_SAMPLES * 2),
            raw_processed_len: 0,
            resampled_processed_len: 0,
            debouncer: Debouncer::new(debounce_config),
        })
    }

    /// Clears everything needed to start detecting a fresh utterance: the
    /// model's LSTM state, any buffered-but-not-yet-processed frame samples,
    /// and the debounce state machine. Does NOT touch the read cursors
    /// (`raw_processed_len`/`resampled_processed_len`) — those must be left
    /// alone here so a caller who keeps extending one buffer across multiple
    /// turns doesn't have already-consumed audio re-fed on the next push().
    fn reset_detection_state(&mut self) {
        self.model_state = vec![0.0; 2 * 128];
        self.frame_buffer.clear();
        self.debouncer.reset();
    }

    /// Full reset for a genuinely new buffer (the caller started over with a
    /// shorter buffer than previously seen, e.g. after cancelling). Includes
    /// everything reset_detection_state() does, plus zeroing both read
    /// cursors, since position 0 in the new buffer really is unprocessed.
    fn reset_for_new_buffer(&mut self) {
        self.reset_detection_state();
        self.raw_processed_len = 0;
        self.resampled_processed_len = 0;
    }

    pub(super) fn push(&mut self, buffer: &[i16]) -> Result<Option<VadEvent>, VadError> {
        if buffer.len() < self.raw_processed_len {
            // Buffer got shorter than what we last saw -> caller started a
            // new buffer for a new turn (e.g. after cancelling). Start over.
            self.reset_for_new_buffer();
        }

        let new_samples_16k = if self.sample_rate == SILERO_SAMPLE_RATE as u32 {
            // Native 16kHz: only the new tail needs converting, O(1)-amortized.
            let new_raw = &buffer[self.raw_processed_len..];
            new_raw.iter().map(|&s| s as f32 / 32768.0).collect()
        } else {
            // Non-16kHz: resample the *whole* accumulated buffer from scratch
            // every call (see module doc for why), then only take the new
            // tail of the *resampled* output that we haven't consumed yet.
            let resampled = self.to_16khz_f32(buffer)?;
            let start = self.resampled_processed_len.min(resampled.len());
            let new_tail = resampled[start..].to_vec();
            self.resampled_processed_len = resampled.len();
            new_tail
        };
        self.raw_processed_len = buffer.len();

        self.frame_buffer.extend(new_samples_16k);

        let mut last_event = None;
        while self.frame_buffer.len() >= FRAME_SAMPLES {
            let frame: Vec<f32> = self.frame_buffer.drain(..FRAME_SAMPLES).collect();
            let prob = self.run_frame(&frame)?;
            if let Some(event) = self.debouncer.step(prob) {
                last_event = Some(event);
            }
        }

        if last_event == Some(VadEvent::SpeechEnded) {
            // Auto-reset: caller may keep extending the same buffer across
            // multiple turns without ever shrinking it, so the shrink-based
            // reset above wouldn't catch this case. Only the detection state
            // resets here — the read cursors must stay put, since they
            // already reflect "everything up to here is consumed" and
            // zeroing them would cause the next push() to re-feed this
            // turn's already-processed audio as if it were new.
            self.reset_detection_state();
        }

        Ok(last_event)
    }

    /// Resample the *whole* accumulated raw buffer to 16kHz f32 from scratch.
    /// Only called on the non-default (non-16kHz) sample-rate path — see
    /// module doc for why this must operate on the whole buffer every call
    /// rather than just the newly-arrived tail (a sinc resampler needs
    /// continuous history; resampling independent small chunks introduces
    /// discontinuities at each chunk boundary). Callers are responsible for
    /// only consuming the new tail of the *returned* (resampled) samples.
    fn to_16khz_f32(&self, buffer: &[i16]) -> Result<Vec<f32>, VadError> {
        let decoded = DecodedAudio {
            samples: buffer.iter().map(|&s| s as f32 / 32768.0).collect(),
            sample_rate: self.sample_rate,
        };
        let resampled = AudioResampler {
            target_rate: SILERO_SAMPLE_RATE as u32,
            ..AudioResampler::default()
        }
        .resample(decoded)
        .map_err(|e| VadError::Audio(e.to_string()))?;
        Ok(resampled.samples)
    }

    /// Run one 512-sample 16kHz frame through the Silero ONNX model.
    ///
    /// Output tensors are named `output` (speech probability, shape [1,1])
    /// and `stateN` (new LSTM state, shape [2,1,128]) — confirmed against
    /// the real `onnx-community/silero-vad` `onnx/model.onnx` model.
    fn run_frame(&mut self, frame: &[f32]) -> Result<f32, VadError> {
        let input = Tensor::from_array(([1usize, FRAME_SAMPLES], frame.to_vec()))?;
        let state = Tensor::from_array(([2usize, 1usize, 128usize], self.model_state.clone()))?;
        let sr = Tensor::from_array(([1usize], vec![SILERO_SAMPLE_RATE]))?;

        let outputs = self
            .session
            .run(ort::inputs!["input" => input, "state" => state, "sr" => sr])?;

        let (_, prob_data) = outputs["output"].try_extract_tensor::<f32>()?;
        let (_, new_state) = outputs["stateN"].try_extract_tensor::<f32>()?;
        self.model_state = new_state.to_vec();
        Ok(prob_data[0])
    }
}
