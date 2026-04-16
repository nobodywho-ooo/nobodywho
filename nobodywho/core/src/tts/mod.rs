/// Text-to-speech synthesis supporting multiple backends:
///
/// - **Kokoro** — fast, high-quality English/Chinese TTS (via `kokoros` crate)
/// - **Piper** — lightweight VITS-based TTS, 80+ languages (via `ort` + `espeak-rs`)
/// - **Chatterbox** — high-quality multilingual TTS with voice cloning, 23 languages (via `ort`)
///
/// Backend auto-detection for Kokoro/Piper uses [`Tts::new`]:
/// - A `.bin` voices file selects the **Kokoro** backend.
/// - A `.json` config file selects the **Piper** backend.
///
/// For Chatterbox, use [`Tts::new_chatterbox`] with a model directory.
mod chatterbox;
mod chatterbox_roest;
mod piper;

use crate::errors::TtsError;
use kokoros::tts::koko::TTSKoko;
use std::path::Path;
use std::sync::Arc;
use std::time::Instant;
use tracing::info;

/// Synchronous TTS handle. Wraps [`TtsAsync`] and blocks the calling thread.
#[derive(Clone)]
pub struct Tts {
    inner: TtsAsync,
}

/// Async TTS handle.
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
        /// Pre-loaded reference audio samples (24kHz mono f32) for voice cloning.
        reference_audio: Option<Arc<Vec<f32>>>,
    },
    Roest {
        model: Arc<chatterbox_roest::RoestModel>,
    },
}

/// A TTS synthesis request with text, voice, speed, and language.
#[derive(Clone, Debug)]
pub struct TtsRequest {
    pub text: String,
    pub voice: String,
    pub speed: f32,
    pub language: String,
    /// Chatterbox: emotion exaggeration (0.0–1.0+, default 0.5).
    pub exaggeration: f32,
    /// Chatterbox: sampling temperature (default 0.8).
    pub temperature: f32,
    /// Chatterbox: top-k sampling (0 = disabled).
    pub top_k: usize,
    /// Chatterbox: top-p / nucleus sampling (default 1.0).
    pub top_p: f32,
    /// Chatterbox: min-p filtering (default 0.05).
    pub min_p: f32,
    /// Chatterbox: classifier-free guidance weight (default 0.5, 0.0 = disabled).
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

    /// Set sampling temperature (Chatterbox only, default 0.8). 0.0 = greedy.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// Set top-k sampling (Chatterbox only, default 1000). 0 = disabled.
    pub fn with_top_k(mut self, top_k: usize) -> Self {
        self.top_k = top_k;
        self
    }

    /// Set top-p / nucleus sampling (Chatterbox only, default 0.95). 1.0 = disabled.
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = top_p;
        self
    }
}

impl Tts {
    /// Create a new TTS handle. The backend is auto-detected from the second file path:
    /// - `voices.bin` → Kokoro (model_path is the ONNX model, second_path is the voices file)
    /// - `model.onnx.json` → Piper (model_path is the ONNX model, second_path is the config)
    pub fn new(
        model_path: impl AsRef<Path>,
        second_path: impl AsRef<Path>,
    ) -> Result<Self, TtsError> {
        Ok(Self {
            inner: TtsAsync::new(model_path, second_path)?,
        })
    }

    /// Create a Chatterbox TTS handle from a model directory.
    ///
    /// The directory must contain `tokenizer.json` and an `onnx/` subdirectory with
    /// the 4 ONNX model files. Optionally provide a reference WAV for voice cloning.
    pub fn new_chatterbox(
        model_dir: impl AsRef<Path>,
        reference_wav: Option<impl AsRef<Path>>,
    ) -> Result<Self, TtsError> {
        Ok(Self {
            inner: TtsAsync::new_chatterbox(model_dir, reference_wav)?,
        })
    }

    /// Create a Røst TTS handle from a model directory.
    ///
    /// The directory must contain `tokenizer.json`, `model_config.json`,
    /// `default_cond/` with pre-computed conditioning, and `onnx/` with ONNX models.
    pub fn new_roest(model_dir: impl AsRef<Path>) -> Result<Self, TtsError> {
        Ok(Self {
            inner: TtsAsync::new_roest(model_dir)?,
        })
    }

    /// Synthesize text with default settings. Returns WAV bytes.
    pub fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsError> {
        self.synthesize_request(TtsRequest::new(text))
    }

    /// Synthesize with full control over voice, speed, and language. Returns WAV bytes.
    pub fn synthesize_request(&self, request: TtsRequest) -> Result<Vec<u8>, TtsError> {
        synthesize_sync(&self.inner.backend, request)
    }

    /// List available voice names (Kokoro only; returns empty for Piper/Chatterbox).
    pub fn available_voices(&self) -> Vec<String> {
        self.inner.available_voices()
    }
}

impl TtsAsync {
    /// Create a new async TTS handle with auto-detected backend.
    pub fn new(
        model_path: impl AsRef<Path>,
        second_path: impl AsRef<Path>,
    ) -> Result<Self, TtsError> {
        let second = second_path.as_ref();

        let is_piper = second
            .extension()
            .is_some_and(|ext| ext.eq_ignore_ascii_case("json"));

        if is_piper {
            Self::new_piper(model_path, second)
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
        let rt = tokio::runtime::Handle::try_current().ok();
        let koko = if let Some(handle) = rt {
            tokio::task::block_in_place(|| handle.block_on(TTSKoko::new(&model_path, &voices_path)))
        } else {
            let rt = tokio::runtime::Runtime::new()
                .map_err(|e| TtsError::Init(format!("failed to create tokio runtime: {e}")))?;
            rt.block_on(TTSKoko::new(&model_path, &voices_path))
        };
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
    ) -> Result<Self, TtsError> {
        let init_start = Instant::now();
        let model = piper::PiperModel::new(model_path.as_ref(), config_path.as_ref())?;
        let sample_rate = model.sample_rate();
        info!(elapsed = ?init_start.elapsed(), "Initialized Piper TTS");

        Ok(Self {
            backend: TtsBackend::Piper {
                model: Arc::new(model),
                sample_rate,
            },
        })
    }

    /// Create a Chatterbox TTS handle from a model directory.
    pub fn new_chatterbox(
        model_dir: impl AsRef<Path>,
        reference_wav: Option<impl AsRef<Path>>,
    ) -> Result<Self, TtsError> {
        let init_start = Instant::now();
        let model = chatterbox::ChatterboxModel::new(model_dir.as_ref())?;

        let reference_audio = match reference_wav {
            Some(path) => {
                let samples = chatterbox::load_reference_audio(path.as_ref())?;
                info!(samples = samples.len(), "Loaded reference audio for voice cloning");
                Some(Arc::new(samples))
            }
            None => {
                // Try default_voice.wav in the model directory
                let default_path = model_dir.as_ref().join("default_voice.wav");
                if default_path.exists() {
                    let samples = chatterbox::load_reference_audio(&default_path)?;
                    info!(samples = samples.len(), "Loaded default reference voice");
                    Some(Arc::new(samples))
                } else {
                    None
                }
            }
        };

        info!(elapsed = ?init_start.elapsed(), "Initialized Chatterbox TTS");

        Ok(Self {
            backend: TtsBackend::Chatterbox {
                model: Arc::new(model),
                reference_audio,
            },
        })
    }

    /// Create a Røst TTS handle from a model directory.
    pub fn new_roest(model_dir: impl AsRef<Path>) -> Result<Self, TtsError> {
        let init_start = Instant::now();
        let model = chatterbox_roest::RoestModel::new(model_dir.as_ref())?;
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

    /// List available voice names (Kokoro only; returns empty for Piper/Chatterbox).
    pub fn available_voices(&self) -> Vec<String> {
        match &self.backend {
            TtsBackend::Kokoro { koko, .. } => koko.get_available_voices(),
            TtsBackend::Piper { .. } | TtsBackend::Chatterbox { .. } | TtsBackend::Roest { .. } => Vec::new(),
        }
    }
}

/// Shared sync synthesis implementation.
fn synthesize_sync(backend: &TtsBackend, request: TtsRequest) -> Result<Vec<u8>, TtsError> {
    let synth_start = Instant::now();

    let (samples, sample_rate) = match backend {
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
            (samples, 24000u32)
        }
        TtsBackend::Piper { model, sample_rate } => {
            let samples = model.synthesize(&request.text)?;
            (samples, *sample_rate)
        }
        TtsBackend::Chatterbox {
            model,
            reference_audio,
        } => {
            let ref_audio = reference_audio
                .as_ref()
                .ok_or_else(|| {
                    TtsError::Synthesis(
                        "Chatterbox requires reference audio — provide a WAV file or place default_voice.wav in model dir".into(),
                    )
                })?;
            let sampling = chatterbox::SamplingParams {
                temperature: request.temperature,
                top_k: request.top_k,
                top_p: request.top_p,
                min_p: request.min_p,
                cfg_weight: request.cfg_weight,
            };
            let samples = model.synthesize(
                &request.text,
                &request.language,
                Some(ref_audio.as_slice()),
                &sampling,
            )?;
            (samples, 24000u32)
        }
        TtsBackend::Roest { model } => {
            let sampling = chatterbox_roest::SamplingParams {
                temperature: request.temperature,
                top_k: request.top_k,
                top_p: request.top_p,
                min_p: request.min_p,
                cfg_weight: request.cfg_weight,
            };
            let samples = model.synthesize(
                &request.text,
                &sampling,
            )?;
            (samples, 24000u32)
        }
    };

    info!(
        n_samples = samples.len(),
        duration_secs = samples.len() as f32 / sample_rate as f32,
        elapsed = ?synth_start.elapsed(),
        "Synthesized audio"
    );

    encode_wav(&samples, sample_rate)
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
    fn test_request_builder() {
        let req = TtsRequest::new("hello")
            .with_voice("bm_george")
            .with_speed(1.5)
            .with_language("en-gb");
        assert_eq!(req.text, "hello");
        assert_eq!(req.voice, "bm_george");
        assert_eq!(req.speed, 1.5);
        assert_eq!(req.language, "en-gb");
    }

    #[test]
    fn test_request_defaults() {
        let req = TtsRequest::new("hello");
        assert_eq!(req.voice, "af_heart");
        assert_eq!(req.speed, 1.0);
        assert_eq!(req.language, "en-us");
    }

    #[test]
    #[ignore = "requires TEST_KOKORO_MODEL and TEST_KOKORO_VOICES files"]
    fn test_kokoro_synthesize() -> Result<(), Box<dyn std::error::Error>> {
        let tts = Tts::new(test_kokoro_model_path(), test_kokoro_voices_path())?;
        let wav_bytes = tts.synthesize("Hello world")?;
        assert!(!wav_bytes.is_empty());

        let cursor = std::io::Cursor::new(&wav_bytes);
        let reader = hound::WavReader::new(cursor)?;
        assert_eq!(reader.spec().sample_rate, 24000);
        Ok(())
    }

    #[test]
    #[ignore = "requires Piper ONNX model files"]
    fn test_piper_synthesize() -> Result<(), Box<dyn std::error::Error>> {
        let model = std::env::var("TEST_PIPER_MODEL")
            .unwrap_or_else(|_| "da_DK-talesyntese-medium.onnx".to_string());
        let config = format!("{model}.json");
        let tts = Tts::new(&model, &config)?;
        let wav_bytes = tts.synthesize("Hej verden")?;
        assert!(!wav_bytes.is_empty());

        let cursor = std::io::Cursor::new(&wav_bytes);
        let reader = hound::WavReader::new(cursor)?;
        assert_eq!(reader.spec().sample_rate, 22050);
        Ok(())
    }
}
