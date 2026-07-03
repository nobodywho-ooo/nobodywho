use crate::errors::SttError;
use crate::huggingface;
use crate::onnx::Device;
use crate::stream::StreamOutput;
use crate::stt::{audio, backends, AudioInput, SttConfig};
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

pub(super) trait SttBackendImpl: Send {
    /// Transcribe a single 30-second window of 16 kHz mono f32 samples.
    /// `on_token` is called with each decoded token piece as it is generated.
    fn transcribe_window(
        &mut self,
        window: &[f32],
        on_token: &mut dyn FnMut(String),
    ) -> Result<String, SttError>;
}

pub(super) fn load_backend(
    config: SttConfig,
    device: Device,
) -> Result<Box<dyn SttBackendImpl>, SttError> {
    match config {
        SttConfig::Whisper(config) => {
            let init_start = Instant::now();
            let required_files = backends::whisper_required_files(&config.quantization)?;
            let model_dir =
                huggingface::resolve(huggingface::parse(&config.source)?, &required_files)?;
            let backend = backends::WhisperBackend::new(
                &model_dir,
                config.language.as_deref(),
                &config.quantization,
                device,
            )?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Whisper STT");
            Ok(Box::new(backend))
        }
    }
}

fn decode_input(input: AudioInput) -> Result<Vec<Vec<f32>>, SttError> {
    Ok(audio::AudioResampler::default()
        .resample(match input {
            AudioInput::File(path) => audio::DecodedAudio::from_file(&path)?,
            AudioInput::Pcm {
                samples,
                sample_rate,
            } => audio::DecodedAudio::from_pcm_i16(&samples, sample_rate),
        })?
        .into_windows())
}

pub(super) fn transcribe_sync(
    backend: &mut dyn SttBackendImpl,
    input: AudioInput,
) -> Result<String, SttError> {
    let start = Instant::now();
    let windows = decode_input(input)?;
    let n_windows = windows.len();

    let mut parts: Vec<String> = Vec::with_capacity(n_windows);
    for (i, window) in windows.into_iter().enumerate() {
        let text = backend.transcribe_window(&window, &mut |_| {})?;
        info!(window = i + 1, total = n_windows, text = %text, "Transcribed window");
        if !text.trim().is_empty() {
            parts.push(text.trim().to_string());
        }
    }

    let transcript = parts.join(" ");
    info!(n_windows, chars = transcript.len(), elapsed = ?start.elapsed(), "Transcription complete");
    Ok(transcript)
}

pub(super) fn transcribe_streaming(
    backend: &mut dyn SttBackendImpl,
    input: AudioInput,
    tx: UnboundedSender<StreamOutput<SttError>>,
) {
    if let Err(e) = do_transcribe_streaming(backend, input, &tx) {
        let _ = tx.send(StreamOutput::Error(e));
    }
}

fn do_transcribe_streaming(
    backend: &mut dyn SttBackendImpl,
    input: AudioInput,
    tx: &UnboundedSender<StreamOutput<SttError>>,
) -> Result<(), SttError> {
    let windows = decode_input(input)?;
    let mut full_transcript = String::new();

    for window in windows {
        let text = backend.transcribe_window(&window, &mut |piece| {
            let _ = tx.send(StreamOutput::Token(piece));
        })?;
        if !text.trim().is_empty() {
            if !full_transcript.is_empty() {
                full_transcript.push(' ');
            }
            full_transcript.push_str(text.trim());
        }
    }

    let _ = tx.send(StreamOutput::Done(full_transcript));
    Ok(())
}
