use crate::errors::VadError;
use crate::huggingface;
use crate::onnx::{load_session, Device};
use crate::vad::events::{DebounceConfig, Debouncer, VadEvent};
use ort::session::Session;
use ort::value::Tensor;
use rubato::{
    Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType, WindowFunction,
};
use std::collections::VecDeque;

/// Silero operates on exactly this many samples per frame at 16kHz.
const FRAME_SAMPLES: usize = 512;
const SILERO_SAMPLE_RATE: i64 = 16_000;
const RESAMPLE_CHUNK_SIZE: usize = 1024;

pub(super) struct VadBackend {
    session: Session,
    resampler: Option<StreamResampler>,
    sample_rate: u32,
    preroll_duration_ms: u32,
    /// LSTM hidden state carried between frames: shape (2, 1, 128) flattened.
    model_state: Vec<f32>,
    frames: FrameAccumulator,
    debouncer: Debouncer,
    preroll: Preroll,
    capture: TurnCapture,
}

impl VadBackend {
    pub(super) fn new(
        source: &str,
        sample_rate: u32,
        preroll_duration_ms: u32,
        debounce_config: DebounceConfig,
        device: Device,
    ) -> Result<Self, VadError> {
        let model_dir = huggingface::download_onnx(source, &["onnx/model.onnx".to_string()], None)?;
        let session = load_session(&model_dir.join("onnx").join("model.onnx"), device)?;
        let resampler = if sample_rate == SILERO_SAMPLE_RATE as u32 {
            None
        } else {
            Some(StreamResampler::new(
                sample_rate,
                SILERO_SAMPLE_RATE as u32,
            )?)
        };
        Ok(Self {
            session,
            resampler,
            sample_rate,
            preroll_duration_ms,
            model_state: vec![0.0; 2 * 128],
            frames: FrameAccumulator::new(),
            debouncer: Debouncer::new(debounce_config),
            preroll: Preroll::new(sample_rate, preroll_duration_ms),
            capture: TurnCapture::new(),
        })
    }

    pub(super) fn reset(&mut self) {
        self.model_state = vec![0.0; 2 * 128];
        self.frames.clear();
        self.debouncer.reset();
    }

    pub(super) fn predict(&mut self, chunk: &[i16]) -> Result<Vec<f32>, VadError> {
        let raw_f32: Vec<f32> = chunk.iter().map(|&s| s as f32 / 32768.0).collect();
        let samples_16k = match &mut self.resampler {
            Some(resampler) => resampler.push(&raw_f32)?,
            None => raw_f32,
        };
        self.frames.extend(samples_16k);

        let mut probs = Vec::new();
        while let Some(frame) = self.frames.next_frame() {
            probs.push(self.run_frame(&frame)?);
        }
        Ok(probs)
    }

    pub(super) fn push(&mut self, chunk: &[i16]) -> Result<Option<VadEvent>, VadError> {
        self.preroll.push(chunk);
        self.capture.push(chunk);

        let mut event = None;
        for prob in self.predict(chunk)? {
            if let Some(e) = self.debouncer.step(prob) {
                event = Some(e);
            }
        }

        match event {
            // preroll already includes this call's chunk, so it alone covers
            // the not-yet-confirmed lead-in plus the confirming chunk.
            Some(VadEvent::SpeechStarted) => self.capture.start(self.preroll.snapshot()),
            Some(VadEvent::SpeechEnded) => {
                self.capture.stop();
                self.reset();
            }
            None => {}
        }

        Ok(event)
    }

    pub(super) fn finish(&mut self) -> Vec<i16> {
        self.preroll.clear();
        self.reset();
        self.capture.take()
    }

    pub(super) fn segment(&mut self, samples: &[i16]) -> Result<Vec<Vec<i16>>, VadError> {
        self.reset();

        let probs = self.predict(samples)?;
        let preroll_native =
            (self.sample_rate as u64 * self.preroll_duration_ms as u64 / 1000) as usize;

        let mut segments = Vec::new();
        let mut start: Option<usize> = None;
        for (i, &prob) in probs.iter().enumerate() {
            // Native-domain sample count consumed by the end of this frame.
            let end_native = ((i + 1) as u64 * FRAME_SAMPLES as u64 * self.sample_rate as u64
                / SILERO_SAMPLE_RATE as u64) as usize;
            match self.debouncer.step(prob) {
                Some(VadEvent::SpeechStarted) => {
                    start = Some(end_native.saturating_sub(preroll_native));
                }
                Some(VadEvent::SpeechEnded) => {
                    if let Some(s) = start.take() {
                        segments.push(
                            samples[s.min(samples.len())..end_native.min(samples.len())].to_vec(),
                        );
                    }
                }
                None => {}
            }
        }
        // Flush trailing speech that never got a confirmed SpeechEnded —
        // the recording just stopped mid-utterance.
        if let Some(s) = start {
            segments.push(samples[s.min(samples.len())..].to_vec());
        }

        self.reset();
        Ok(segments)
    }

    /// Run one 512-sample 16kHz frame through the Silero ONNX model.
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

/// Accumulates resampled 16kHz samples and hands out fixed-size frames as
/// they become available.
struct FrameAccumulator {
    samples: Vec<f32>,
}

impl FrameAccumulator {
    fn new() -> Self {
        Self {
            samples: Vec::with_capacity(FRAME_SAMPLES * 2),
        }
    }

    fn extend(&mut self, samples: Vec<f32>) {
        self.samples.extend(samples);
    }

    fn next_frame(&mut self) -> Option<Vec<f32>> {
        (self.samples.len() >= FRAME_SAMPLES).then(|| self.samples.drain(..FRAME_SAMPLES).collect())
    }

    fn clear(&mut self) {
        self.samples.clear();
    }
}

/// Ring buffer of the most recent raw samples, capped to `PREROLL_DURATION_MS`
/// worth of audio — the not-yet-confirmed lead-in a `SpeechStarted` seeds from.
struct Preroll {
    samples: VecDeque<i16>,
    capacity: usize,
}

impl Preroll {
    fn new(sample_rate: u32, duration_ms: u32) -> Self {
        let capacity = (sample_rate as u64 * duration_ms as u64 / 1000) as usize;
        Self {
            samples: VecDeque::with_capacity(capacity),
            capacity,
        }
    }

    fn push(&mut self, chunk: &[i16]) {
        for &sample in chunk {
            if self.samples.len() == self.capacity {
                self.samples.pop_front();
            }
            self.samples.push_back(sample);
        }
    }

    fn snapshot(&self) -> Vec<i16> {
        self.samples.iter().copied().collect()
    }

    fn clear(&mut self) {
        self.samples.clear();
    }
}

/// Raw audio for the in-progress/just-finished turn: idle until `start()`,
/// accumulates pushed chunks until `stop()` or `take()`.
struct TurnCapture {
    samples: Vec<i16>,
    active: bool,
}

impl TurnCapture {
    fn new() -> Self {
        Self {
            samples: Vec::new(),
            active: false,
        }
    }

    fn start(&mut self, preroll: Vec<i16>) {
        self.samples = preroll;
        self.active = true;
    }

    fn push(&mut self, chunk: &[i16]) {
        if self.active {
            self.samples.extend_from_slice(chunk);
        }
    }

    fn stop(&mut self) {
        self.active = false;
    }

    fn take(&mut self) -> Vec<i16> {
        self.active = false;
        std::mem::take(&mut self.samples)
    }
}

/// Streaming wrapper around `rubato`'s chunked sinc resampler: filter state
/// persists across `push()` calls instead of resampling from scratch each time.
struct StreamResampler {
    resampler: SincFixedIn<f32>,
    pending: Vec<f32>,
}

impl StreamResampler {
    fn new(from_rate: u32, to_rate: u32) -> Result<Self, VadError> {
        let ratio = to_rate as f64 / from_rate as f64;
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            interpolation: SincInterpolationType::Linear,
            oversampling_factor: 256,
            window: WindowFunction::BlackmanHarris2,
        };
        let resampler = SincFixedIn::<f32>::new(ratio, 2.0, params, RESAMPLE_CHUNK_SIZE, 1)
            .map_err(|e| VadError::Audio(format!("resampler init: {e}")))?;
        Ok(Self {
            resampler,
            pending: Vec::new(),
        })
    }

    fn push(&mut self, samples: &[f32]) -> Result<Vec<f32>, VadError> {
        self.pending.extend_from_slice(samples);
        let mut output = Vec::new();
        while self.pending.len() >= RESAMPLE_CHUNK_SIZE {
            let chunk: Vec<f32> = self.pending.drain(..RESAMPLE_CHUNK_SIZE).collect();
            let mut waves = self
                .resampler
                .process(&[chunk], None)
                .map_err(|e| VadError::Audio(format!("resample: {e}")))?;
            output.extend(waves.remove(0));
        }
        Ok(output)
    }
}
