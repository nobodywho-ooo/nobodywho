/// Text-to-speech synthesis using Kokoro (via ONNX Runtime) or Piper (VITS + espeak-ng).
///
/// Wraps the `kokoros` crate for Kokoro and implements Piper inference directly
/// using `ort` + `espeak-rs`. Both backends return WAV bytes from plain text input.
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
}

/// A TTS synthesis request with text, voice, speed, and language.
#[derive(Clone, Debug)]
pub struct TtsRequest {
    pub text: String,
    pub voice: String,
    pub speed: f32,
    pub language: String,
}

impl TtsRequest {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            voice: "af_heart".into(),
            speed: 1.0,
            language: "en-us".into(),
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
}

impl Tts {
    /// Create a new Kokoro TTS handle from model and voice file paths.
    pub fn new(
        model_path: impl Into<String>,
        voices_path: impl Into<String>,
    ) -> Result<Self, TtsError> {
        Ok(Self {
            inner: TtsAsync::new(model_path, voices_path)?,
        })
    }

    /// Create a new Piper TTS handle from ONNX model and config JSON paths.
    pub fn new_piper(
        model_path: impl AsRef<Path>,
        config_path: impl AsRef<Path>,
    ) -> Result<Self, TtsError> {
        Ok(Self {
            inner: TtsAsync::new_piper(model_path, config_path)?,
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

    /// List available voice names (Kokoro only; returns empty for Piper).
    pub fn available_voices(&self) -> Vec<String> {
        self.inner.available_voices()
    }
}

impl TtsAsync {
    /// Create a new async Kokoro TTS handle.
    pub fn new(
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

    /// Create a new async Piper TTS handle.
    pub fn new_piper(
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

    /// List available voice names (Kokoro only; returns empty for Piper).
    pub fn available_voices(&self) -> Vec<String> {
        match &self.backend {
            TtsBackend::Kokoro { koko, .. } => koko.get_available_voices(),
            TtsBackend::Piper { .. } => Vec::new(),
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
        let tts = Tts::new_piper(&model, &config)?;
        let wav_bytes = tts.synthesize("Hej verden")?;
        assert!(!wav_bytes.is_empty());

        let cursor = std::io::Cursor::new(&wav_bytes);
        let reader = hound::WavReader::new(cursor)?;
        assert_eq!(reader.spec().sample_rate, 22050);
        Ok(())
    }
}
