//! Text-to-speech synthesis supporting multiple backends.
//!
//! | Backend      | Quality       | Voice cloning | Languages                 | File cue             |
//! |--------------|---------------|---------------|---------------------------|----------------------|
//! | Kokoro       | High          | No            | English, Chinese          | `voices-*.bin`       |
//! | Piper        | Medium        | No            | 80+ (espeak-ng backed)    | `*.onnx.json` config |
//! | Chatterbox   | High          | Yes (WAV)     | 23 (incl. Danish)         | model directory      |
//! | Røst         | High (Danish) | Preset        | Danish (finetune)         | model directory      |
//!
//! [`Tts::new`] auto-detects Kokoro vs Piper from the second file path —
//! a `.bin` file selects Kokoro, a `.json` file selects Piper. Chatterbox
//! and Røst are loaded explicitly via [`Tts::new_chatterbox`] and
//! [`Tts::new_roest`].

mod chatterbox;
mod chatterbox_roest;
mod instrumentation;
mod ort_util;
mod piper;
mod sampling;

use crate::errors::TtsError;
use crate::tts::sampling::SamplingParams;
use kokoros::tts::koko::TTSKoko;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

/// Hardware target for ONNX Runtime execution.
///
/// Kokoro is always CPU (its `kokoros` dependency manages its own runtime
/// internally); Piper, Chatterbox, and Røst honor this setting.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TtsDevice {
    /// Prefer CUDA, silently fall back to CPU if unavailable.
    Auto,
    /// Force CPU execution.
    Cpu,
    /// Require CUDA; fail loudly if it isn't available.
    Cuda,
}

pub(crate) fn ort_execution_providers(
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

/// Audio sample rate shared by Kokoro, Chatterbox, and Røst. Piper reports
/// its own rate from its config.
const DEFAULT_SAMPLE_RATE: u32 = 24000;

/// Synchronous TTS handle. Wraps [`TtsAsync`] and blocks the calling thread.
#[derive(Clone)]
pub struct Tts {
    inner: TtsAsync,
}

/// Async TTS handle. Backend work is dispatched via `tokio::task::spawn_blocking`
/// so the calling async runtime stays responsive during long syntheses.
#[derive(Clone)]
pub struct TtsAsync {
    backend: TtsBackend,
}

#[derive(Clone)]
enum TtsBackend {
    Kokoro {
        koko: Arc<TTSKoko>,
    },
    Piper {
        model: Arc<piper::PiperModel>,
        sample_rate: u32,
    },
    Chatterbox {
        model: Arc<chatterbox::ChatterboxModel>,
        /// Pre-loaded reference audio samples (24kHz mono f32) for voice
        /// cloning. `None` means Chatterbox will fall back to the model's
        /// `default_cond/` precomputed conditioning and error if that's
        /// also absent.
        reference_audio: Option<Arc<Vec<f32>>>,
    },
    Roest {
        model: Arc<chatterbox_roest::RoestModel>,
    },
}

/// A TTS synthesis request: the text to synthesize plus every knob supported
/// by any backend. Fields that don't apply to the chosen backend are ignored.
#[derive(Clone, Debug)]
pub struct TtsRequest {
    pub text: String,
    /// Kokoro voice name (e.g. `"af_heart"`). Other backends ignore this.
    pub voice: String,
    /// Kokoro speech rate multiplier. Other backends ignore this.
    pub speed: f32,
    /// BCP-47 language tag. Used by Kokoro (e.g. `"en-us"`) and
    /// Chatterbox/Røst (prefixed as `"[da]"` inside the tokenizer).
    pub language: String,
    /// Chatterbox: emotion exaggeration (0.0–1.0+, default 0.5).
    pub exaggeration: f32,
    /// Chatterbox/Røst: sampling temperature (default 0.8). 0.0 = greedy.
    pub temperature: f32,
    /// Chatterbox/Røst: top-k sampling. 0 = disabled.
    pub top_k: usize,
    /// Chatterbox/Røst: top-p / nucleus sampling (default 1.0). 1.0 = disabled.
    pub top_p: f32,
    /// Chatterbox/Røst: min-p filtering (default 0.05). 0.0 = disabled.
    pub min_p: f32,
    /// Chatterbox/Røst: classifier-free guidance weight (default 0.5).
    /// 0.0 = CFG disabled (single-batch inference, ~2x faster).
    pub cfg_weight: f32,
}

impl TtsRequest {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            voice: "af_heart".into(),
            speed: 1.0,
            language: "en-us".into(),
            exaggeration: 0.5,
            temperature: 0.8,
            top_k: 0,
            top_p: 1.0,
            min_p: 0.05,
            cfg_weight: 0.5,
        }
    }

    pub fn with_voice(mut self, voice: impl Into<String>) -> Self {
        self.voice = voice.into();
        self
    }

    pub fn with_speed(mut self, speed: f32) -> Self {
        self.speed = speed;
        self
    }

    pub fn with_language(mut self, language: impl Into<String>) -> Self {
        self.language = language.into();
        self
    }

    /// Set emotion exaggeration (Chatterbox only, default 0.5).
    pub fn with_exaggeration(mut self, exaggeration: f32) -> Self {
        self.exaggeration = exaggeration;
        self
    }

    /// Set sampling temperature (Chatterbox/Røst, default 0.8). 0.0 = greedy.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Set top-k sampling (Chatterbox/Røst). 0 disables the filter.
    pub fn with_top_k(mut self, top_k: usize) -> Self {
        self.top_k = top_k;
        self
    }

    /// Set top-p / nucleus sampling (Chatterbox/Røst). 1.0 disables the filter.
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = top_p;
        self
    }

    /// Set min-p filtering (Chatterbox/Røst). 0.0 disables the filter.
    pub fn with_min_p(mut self, min_p: f32) -> Self {
        self.min_p = min_p;
        self
    }

    /// Set classifier-free guidance weight (Chatterbox/Røst). 0.0 disables CFG.
    pub fn with_cfg_weight(mut self, cfg_weight: f32) -> Self {
        self.cfg_weight = cfg_weight;
        self
    }

    fn sampling_params(&self) -> SamplingParams {
        SamplingParams {
            temperature: self.temperature,
            top_k: self.top_k,
            top_p: self.top_p,
            min_p: self.min_p,
            cfg_weight: self.cfg_weight,
        }
    }
}

impl Tts {
    /// Create a Kokoro- or Piper-backed handle.
    ///
    /// The backend is auto-detected from `second_path`'s extension — `.json`
    /// → Piper (treated as a Piper config), anything else → Kokoro (treated
    /// as a voices `.bin` file).
    pub fn new(
        model_path: impl AsRef<Path>,
        second_path: impl AsRef<Path>,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        Ok(Self {
            inner: TtsAsync::new(model_path, second_path, device)?,
        })
    }

    /// Create a Chatterbox-backed handle.
    ///
    /// The directory must contain `tokenizer.json` and an `onnx/` subdirectory
    /// with the four ONNX model files. Optionally provide a reference WAV for
    /// voice cloning; otherwise a `default_voice.wav` sibling file is picked
    /// up, and finally any `default_cond/` precomputed conditioning takes over.
    pub fn new_chatterbox(
        model_dir: impl AsRef<Path>,
        reference_wav: Option<impl AsRef<Path>>,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        Ok(Self {
            inner: TtsAsync::new_chatterbox(model_dir, reference_wav, device)?,
        })
    }

    /// Create a Røst-backed handle.
    ///
    /// The directory must contain `tokenizer.json`, `model_config.json`,
    /// `default_cond/` with pre-computed conditioning, and `onnx/` with the
    /// ONNX models.
    pub fn new_roest(model_dir: impl AsRef<Path>, device: TtsDevice) -> Result<Self, TtsError> {
        Ok(Self {
            inner: TtsAsync::new_roest(model_dir, device)?,
        })
    }

    /// Synthesize text with default settings. Returns WAV bytes.
    pub fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        self.synthesize_request(TtsRequest::new(text))
    }

    /// Synthesize with full control over voice, speed, language, and sampler
    /// parameters. Returns WAV bytes.
    pub fn synthesize_request(&self, request: TtsRequest) -> Result<Vec<u8>, TtsError> {
        synthesize_sync(&self.inner.backend, request)
    }

    /// List available voice names (Kokoro only; returns empty for other backends).
    pub fn available_voices(&self) -> Vec<String> {
        self.inner.available_voices()
    }
}

impl TtsAsync {
    /// Create a Kokoro- or Piper-backed handle (see [`Tts::new`]).
    pub fn new(
        model_path: impl AsRef<Path>,
        second_path: impl AsRef<Path>,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let second = second_path.as_ref();
        let is_piper = second
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));

        if is_piper {
            Self::new_piper(model_path, second, device)
        } else {
            Self::new_kokoro(
                model_path.as_ref().to_string_lossy(),
                second.to_string_lossy(),
            )
        }
    }

    fn new_kokoro(
        model_path: impl Into<String>,
        voices_path: impl Into<String>,
    ) -> Result<Self, TtsError> {
        let model_path = model_path.into();
        let voices_path = voices_path.into();

        let init_start = Instant::now();
        let koko = load_kokoro(&model_path, &voices_path)?;
        info!(elapsed = ?init_start.elapsed(), "Initialized Kokoro TTS");

        Ok(Self {
            backend: TtsBackend::Kokoro {
                koko: Arc::new(koko),
            },
        })
    }

    fn new_piper(
        model_path: impl AsRef<Path>,
        config_path: impl AsRef<Path>,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let init_start = Instant::now();
        let model = piper::PiperModel::new(model_path.as_ref(), config_path.as_ref(), device)?;
        let sample_rate = model.sample_rate();
        info!(elapsed = ?init_start.elapsed(), "Initialized Piper TTS");

        Ok(Self {
            backend: TtsBackend::Piper {
                model: Arc::new(model),
                sample_rate,
            },
        })
    }

    /// Create a Chatterbox-backed handle (see [`Tts::new_chatterbox`]).
    pub fn new_chatterbox(
        model_dir: impl AsRef<Path>,
        reference_wav: Option<impl AsRef<Path>>,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let init_start = Instant::now();
        let model = chatterbox::ChatterboxModel::new(model_dir.as_ref(), device)?;
        let reference_audio = load_chatterbox_reference(model_dir.as_ref(), reference_wav)?;

        info!(elapsed = ?init_start.elapsed(), "Initialized Chatterbox TTS");

        Ok(Self {
            backend: TtsBackend::Chatterbox {
                model: Arc::new(model),
                reference_audio,
            },
        })
    }

    /// Create a Røst-backed handle (see [`Tts::new_roest`]).
    pub fn new_roest(model_dir: impl AsRef<Path>, device: TtsDevice) -> Result<Self, TtsError> {
        let init_start = Instant::now();
        let model = chatterbox_roest::RoestModel::new(model_dir.as_ref(), device)?;
        info!(elapsed = ?init_start.elapsed(), "Initialized Røst TTS");

        Ok(Self {
            backend: TtsBackend::Roest {
                model: Arc::new(model),
            },
        })
    }

    /// Synthesize text with default settings. Returns WAV bytes.
    pub async fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        self.synthesize_request(TtsRequest::new(text)).await
    }

    /// Synthesize with full control. Returns WAV bytes.
    pub async fn synthesize_request(&self, request: TtsRequest) -> Result<Vec<u8>, TtsError> {
        let backend = self.backend.clone();
        tokio::task::spawn_blocking(move || synthesize_sync(&backend, request))
            .await
            .map_err(|e| TtsError::Synthesis(format!("task join error: {e}")))?
    }

    pub fn available_voices(&self) -> Vec<String> {
        match &self.backend {
            TtsBackend::Kokoro { koko, .. } => koko.get_available_voices(),
            TtsBackend::Piper { .. } | TtsBackend::Chatterbox { .. } | TtsBackend::Roest { .. } => {
                Vec::new()
            }
        }
    }
}

/// Kokoro's initializer is async. We have two callers — a sync context (where
/// we spin up our own runtime) and a sync-inside-async context (where we
/// borrow the ambient runtime via `block_in_place`). Hide both behind one
/// function so `new_kokoro` reads top-down.
fn load_kokoro(model_path: &str, voices_path: &str) -> Result<TTSKoko, TtsError> {
    if let Ok(handle) = tokio::runtime::Handle::try_current() {
        Ok(tokio::task::block_in_place(|| {
            handle.block_on(TTSKoko::new(model_path, voices_path))
        }))
    } else {
        let rt = tokio::runtime::Runtime::new()
            .map_err(|e| TtsError::Init(format!("failed to create tokio runtime: {e}")))?;
        Ok(rt.block_on(TTSKoko::new(model_path, voices_path)))
    }
}

/// Resolve the reference audio for Chatterbox: explicit WAV path wins, then
/// a `default_voice.wav` next to the model files, then fall through to
/// whatever pre-computed conditioning the model directory carries.
fn load_chatterbox_reference(
    model_dir: &Path,
    reference_wav: Option<impl AsRef<Path>>,
) -> Result<Option<Arc<Vec<f32>>>, TtsError> {
    if let Some(path) = reference_wav {
        let samples = chatterbox::load_reference_audio(path.as_ref())?;
        info!(
            samples = samples.len(),
            "Loaded reference audio for voice cloning"
        );
        return Ok(Some(Arc::new(samples)));
    }

    let default_path = model_dir.join("default_voice.wav");
    if default_path.exists() {
        let samples = chatterbox::load_reference_audio(&default_path)?;
        info!(samples = samples.len(), "Loaded default reference voice");
        return Ok(Some(Arc::new(samples)));
    }

    Ok(None)
}

fn synthesize_sync(backend: &TtsBackend, request: TtsRequest) -> Result<Vec<u8>, TtsError> {
    let synth_start = Instant::now();
    let (samples, sample_rate) = backend.run_synthesis(&request)?;

    info!(
        n_samples = samples.len(),
        duration_secs = samples.len() as f32 / sample_rate as f32,
        elapsed = ?synth_start.elapsed(),
        "Synthesized audio"
    );

    encode_wav(&samples, sample_rate)
}

impl TtsBackend {
    /// Run the backend-specific synthesis step, returning `(samples, sample_rate)`.
    fn run_synthesis(&self, request: &TtsRequest) -> Result<(Vec<f32>, u32), TtsError> {
        match self {
            TtsBackend::Kokoro { koko } => {
                let samples = koko
                    .tts_raw_audio(
                        &request.text,
                        &request.language,
                        &request.voice,
                        request.speed,
                        None,
                        None,
                        None,
                        None,
                    )
                    .map_err(|e| TtsError::Synthesis(e.to_string()))?;
                Ok((samples, DEFAULT_SAMPLE_RATE))
            }
            TtsBackend::Piper { model, sample_rate } => {
                Ok((model.synthesize(&request.text)?, *sample_rate))
            }
            TtsBackend::Chatterbox {
                model,
                reference_audio,
            } => {
                let samples = model.synthesize(
                    &request.text,
                    &request.language,
                    reference_audio.as_deref().map(Vec::as_slice),
                    &request.sampling_params(),
                )?;
                Ok((samples, DEFAULT_SAMPLE_RATE))
            }
            TtsBackend::Roest { model } => {
                let samples = model.synthesize(
                    &request.text,
                    &request.language,
                    &request.sampling_params(),
                )?;
                Ok((samples, DEFAULT_SAMPLE_RATE))
            }
        }
    }
}

fn encode_wav(pcm: &[f32], sample_rate: u32) -> Result<Vec<u8>, TtsError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buffer = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buffer);
        let mut writer = hound::WavWriter::new(cursor, spec)
            .map_err(|e| TtsError::WavEncoding(e.to_string()))?;

        for &sample in pcm {
            let s = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer
                .write_sample(s)
                .map_err(|e| TtsError::WavEncoding(e.to_string()))?;
        }

        writer
            .finalize()
            .map_err(|e| TtsError::WavEncoding(e.to_string()))?;
    }

    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_kokoro_model_path() -> String {
        std::env::var("TEST_KOKORO_MODEL").unwrap_or_else(|_| "kokoro-v1.0.onnx".to_string())
    }

    fn test_kokoro_voices_path() -> String {
        std::env::var("TEST_KOKORO_VOICES").unwrap_or_else(|_| "voices-v1.0.bin".to_string())
    }

    #[test]
    fn request_builder_roundtrip() {
        let req = TtsRequest::new("hello")
            .with_voice("bm_george")
            .with_speed(1.5)
            .with_language("en-gb")
            .with_min_p(0.1)
            .with_cfg_weight(0.0);
        assert_eq!(req.text, "hello");
        assert_eq!(req.voice, "bm_george");
        assert_eq!(req.speed, 1.5);
        assert_eq!(req.language, "en-gb");
        assert_eq!(req.min_p, 0.1);
        assert_eq!(req.cfg_weight, 0.0);
    }

    #[test]
    fn request_defaults() {
        let req = TtsRequest::new("hello");
        assert_eq!(req.voice, "af_heart");
        assert_eq!(req.speed, 1.0);
        assert_eq!(req.language, "en-us");
        assert_eq!(req.temperature, 0.8);
        assert_eq!(req.top_p, 1.0);
        assert_eq!(req.min_p, 0.05);
        assert_eq!(req.cfg_weight, 0.5);
    }

    #[test]
    #[ignore = "requires TEST_KOKORO_MODEL and TEST_KOKORO_VOICES files"]
    fn kokoro_synthesize_smoke() -> Result<(), Box<dyn std::error::Error>> {
        let tts = Tts::new(
            test_kokoro_model_path(),
            test_kokoro_voices_path(),
            TtsDevice::Auto,
        )?;
        let wav_bytes = tts.synthesize("Hello world")?;
        assert!(!wav_bytes.is_empty());

        let cursor = std::io::Cursor::new(&wav_bytes);
        let reader = hound::WavReader::new(cursor)?;
        assert_eq!(reader.spec().sample_rate, DEFAULT_SAMPLE_RATE);
        Ok(())
    }

    #[test]
    #[ignore = "requires Piper ONNX model files"]
    fn piper_synthesize_smoke() -> Result<(), Box<dyn std::error::Error>> {
        let model = std::env::var("TEST_PIPER_MODEL")
            .unwrap_or_else(|_| "da_DK-talesyntese-medium.onnx".to_string());
        let config = format!("{model}.json");
        let tts = Tts::new(&model, &config, TtsDevice::Auto)?;
        let wav_bytes = tts.synthesize("Hej verden")?;
        assert!(!wav_bytes.is_empty());

        let cursor = std::io::Cursor::new(&wav_bytes);
        let reader = hound::WavReader::new(cursor)?;
        assert_eq!(reader.spec().sample_rate, 22050);
        Ok(())
    }
}
