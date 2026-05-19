use crate::errors::TtsError;
use crate::tts::{backends, TtsConfig, TtsDevice};
use std::time::Instant;
use tracing::info;

pub(super) trait TtsBackendImpl: Send {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError>;
}

pub(super) fn load_backend(
    config: TtsConfig,
    device: TtsDevice,
) -> Result<Box<dyn TtsBackendImpl>, TtsError> {
    match config {
        TtsConfig::Kokoro(config) => {
            let init_start = Instant::now();
            let backend = backends::KokoroBackend::new(
                &config.model_dir,
                config.voice,
                config.language,
                config.speed,
                device,
            )?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Kokoro TTS");
            Ok(Box::new(backend))
        }
    }
}

pub(super) fn synthesize_sync(
    backend: &mut dyn TtsBackendImpl,
    text: &str,
) -> Result<Vec<u8>, TtsError> {
    let synth_start = Instant::now();
    let (samples, sample_rate) = backend.synthesize_raw(text)?;

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
            let s = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
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
