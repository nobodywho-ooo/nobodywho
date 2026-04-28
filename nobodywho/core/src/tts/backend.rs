use crate::errors::TtsError;
use crate::tts::{chatterbox, chatterbox_roest, kokoro, piper, TtsConfig, TtsDevice};
use kokoros::tts::koko::TTSKoko;
use std::path::Path;
use std::time::Instant;
use tracing::info;

pub(super) trait TtsBackendImpl: Send {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError>;

    fn available_voices(&self) -> Vec<String> {
        Vec::new()
    }
}

pub(super) fn load_backend(
    config: TtsConfig,
    device: TtsDevice,
) -> Result<Box<dyn TtsBackendImpl>, TtsError> {
    match config {
        TtsConfig::Kokoro(config) => {
            let init_start = Instant::now();
            let koko = load_kokoro(
                &config.model_path.to_string_lossy(),
                &config.voices_path.to_string_lossy(),
            )?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Kokoro TTS");
            Ok(Box::new(kokoro::KokoroBackend::new(
                koko,
                config.voice,
                config.language,
                config.speed,
            )))
        }
        TtsConfig::Piper(config) => {
            let init_start = Instant::now();
            let model = piper::PiperModel::new(&config.model_path, &config.config_path, device)?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Piper TTS");
            Ok(Box::new(piper::PiperBackend::new(model)))
        }
        TtsConfig::Chatterbox(config) => {
            let init_start = Instant::now();
            let model = chatterbox::ChatterboxModel::new(&config.model_dir, device)?;
            let reference_audio =
                load_chatterbox_reference(&config.model_dir, config.reference_wav.as_deref())?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Chatterbox TTS");
            Ok(Box::new(chatterbox::ChatterboxBackend::new(
                model,
                reference_audio,
                config.language,
                config.sampling,
            )))
        }
        TtsConfig::Roest(config) => {
            let init_start = Instant::now();
            let model = chatterbox_roest::RoestModel::new(&config.model_dir, device)?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Røst TTS");
            Ok(Box::new(chatterbox_roest::RoestBackend::new(
                model,
                config.language,
                config.sampling,
            )))
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

/// Kokoro's initializer is async. We have two callers: sync code and code
/// already inside a tokio runtime.
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

fn load_chatterbox_reference(
    model_dir: &Path,
    reference_wav: Option<&Path>,
) -> Result<Option<Vec<f32>>, TtsError> {
    if let Some(path) = reference_wav {
        let samples = chatterbox::load_reference_audio(path)?;
        info!(
            samples = samples.len(),
            "Loaded reference audio for voice cloning"
        );
        return Ok(Some(samples));
    }

    let default_path = model_dir.join("default_voice.wav");
    if default_path.exists() {
        let samples = chatterbox::load_reference_audio(&default_path)?;
        info!(samples = samples.len(), "Loaded default reference voice");
        return Ok(Some(samples));
    }

    Ok(None)
}
