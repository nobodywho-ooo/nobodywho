use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::DEFAULT_SAMPLE_RATE;
use kokoros::tts::koko::TTSKoko;
use std::sync::Arc;

pub(super) struct KokoroBackend {
    koko: Arc<TTSKoko>,
    voice: String,
    language: String,
    speed: f32,
}

impl KokoroBackend {
    pub fn new(koko: TTSKoko, voice: String, language: String, speed: f32) -> Self {
        Self {
            koko: Arc::new(koko),
            voice,
            language,
            speed,
        }
    }
}

impl TtsBackendImpl for KokoroBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        let samples = self
            .koko
            .tts_raw_audio(
                text,
                &self.language,
                &self.voice,
                self.speed,
                None,
                None,
                None,
                None,
            )
            .map_err(|e| TtsError::Synthesis(e.to_string()))?;
        Ok((samples, DEFAULT_SAMPLE_RATE))
    }

    fn available_voices(&self) -> Vec<String> {
        self.koko.get_available_voices()
    }
}
