use crate::errors::TtsError;
use crate::huggingface;
use crate::onnx::Device as TtsDevice;
use crate::tts::{backends, TtsConfig};
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
            let model_dir = huggingface::resolve(huggingface::parse(&config.source)?)?;
            let backend = backends::KokoroBackend::new(
                &model_dir,
                &config.voice,
                &config.language,
                config.speed,
                device,
                config.espeak_data_dir.as_deref(),
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

    let mut buffer = Vec::with_capacity(44 + pcm.len() * 2);
    {
        let cursor = std::io::Cursor::new(&mut buffer);
        let mut writer = hound::WavWriter::new(cursor, spec)?;
        for &sample in pcm {
            // https://docs.rs/hound/3.5.1/hound/#examples
            let s = (sample * i16::MAX as f32).clamp(i16::MIN as f32, i16::MAX as f32) as i16;
            writer.write_sample(s)?;
        }
        writer.finalize()?;
    }
    Ok(buffer)
}

#[cfg(test)]
mod tests {
    use super::*;
    use hound::WavReader;
    use std::io::Cursor;

    #[test]
    fn empty_pcm_produces_valid_wav_header() {
        let bytes = encode_wav(&[], 24000).unwrap();
        assert!(bytes.starts_with(b"RIFF"));
        assert_eq!(&bytes[8..12], b"WAVE");
        let reader = WavReader::new(Cursor::new(bytes)).unwrap();
        assert_eq!(reader.spec().sample_rate, 24000);
        assert_eq!(reader.spec().channels, 1);
    }

    #[test]
    fn round_trips_samples() {
        let pcm = vec![0.0, 0.5, -0.5, 1.0, -1.0];
        let bytes = encode_wav(&pcm, 16000).unwrap();
        let mut reader = WavReader::new(Cursor::new(bytes)).unwrap();
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        // 0.5 * 32767 = 16383.5 → truncate-to-zero → 16383
        // 1.0 * 32767 = 32767 = i16::MAX
        assert_eq!(samples, vec![0, 16383, -16383, i16::MAX, -i16::MAX]);
    }

    #[test]
    fn clamps_overshoot() {
        let pcm = vec![1.5, -1.5, 100.0, -100.0];
        let bytes = encode_wav(&pcm, 16000).unwrap();
        let mut reader = WavReader::new(Cursor::new(bytes)).unwrap();
        let samples: Vec<i16> = reader.samples::<i16>().map(Result::unwrap).collect();
        assert_eq!(samples, vec![i16::MAX, i16::MIN, i16::MAX, i16::MIN]);
    }
}
