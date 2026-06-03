use crate::errors::SttError;
use crate::huggingface;
use crate::onnx::Device;
use crate::stt::{audio, backends, AudioInput, SttConfig};
use std::time::Instant;
use tracing::info;

pub(super) trait SttBackendImpl: Send {
    /// Transcribe a single 30-second window of 16 kHz mono f32 samples.
    fn transcribe_window(&mut self, window: &[f32]) -> Result<String, SttError>;
}

pub(super) fn load_backend(
    config: SttConfig,
    device: Device,
) -> Result<Box<dyn SttBackendImpl>, SttError> {
    match config {
        SttConfig::Whisper(config) => {
            let init_start = Instant::now();
            let model_dir = huggingface::resolve(huggingface::parse(&config.source)?)?;
            let backend = backends::WhisperBackend::new(
                &model_dir,
                config.language.as_deref(),
                device,
            )?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Whisper STT");
            Ok(Box::new(backend))
        }
    }
}

pub(super) fn transcribe_sync(
    backend: &mut dyn SttBackendImpl,
    input: AudioInput,
) -> Result<String, SttError> {
    let start = Instant::now();

    // Decode and normalize to 16 kHz mono f32
    let windows = audio::AudioResampler::default()
        .resample(match input {
            AudioInput::File(path) => audio::DecodedAudio::from_file(&path)?,
            AudioInput::Pcm { samples, sample_rate } => {
                audio::DecodedAudio::from_pcm_i16(&samples, sample_rate)
            }
        })?
        .into_windows();
    let n_windows = windows.len();

    let mut parts: Vec<String> = Vec::with_capacity(n_windows);
    for (i, window) in windows.into_iter().enumerate() {
        let text = backend.transcribe_window(&window)?;
        info!(
            window = i + 1,
            total = n_windows,
            text = %text,
            "Transcribed window"
        );
        if !text.trim().is_empty() {
            parts.push(text.trim().to_string());
        }
    }

    let transcript = parts.join(" ");
    info!(
        n_windows,
        chars = transcript.len(),
        elapsed = ?start.elapsed(),
        "Transcription complete"
    );
    Ok(transcript)
}
