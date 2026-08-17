use crate::errors::SpeechToTextError;
use crate::onnx::Device;
use crate::speech_to_text::{architectures, audio, AudioInput, SpeechToTextConfig};
use crate::stream::StreamOutput;
use std::time::Instant;
use tokio::sync::mpsc::UnboundedSender;
use tracing::info;

pub(super) trait SpeechToTextArchitectureImpl: Send {
    /// Transcribe a single 30-second window of 16 kHz mono f32 samples.
    /// `on_token` is called with each decoded token piece as it is generated.
    fn transcribe_window(
        &mut self,
        window: &[f32],
        on_token: &mut dyn FnMut(String),
    ) -> Result<String, SpeechToTextError>;
}

pub(super) fn load_architecture(
    config: SpeechToTextConfig,
    device: Device,
) -> Result<Box<dyn SpeechToTextArchitectureImpl>, SpeechToTextError> {
    match config {
        SpeechToTextConfig::Whisper(config) => {
            let init_start = Instant::now();
            let architecture = architectures::WhisperBackend::new(
                &config.source,
                config.language.as_deref(),
                &config.quantization,
                device,
            )?;
            info!(elapsed = ?init_start.elapsed(), "Initialized Whisper SpeechToText");
            Ok(Box::new(architecture))
        }
    }
}

fn decode_input(input: AudioInput) -> Result<Vec<Vec<f32>>, SpeechToTextError> {
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
    architecture: &mut dyn SpeechToTextArchitectureImpl,
    input: AudioInput,
) -> Result<String, SpeechToTextError> {
    let start = Instant::now();
    let windows = decode_input(input)?;
    let n_windows = windows.len();

    let mut parts: Vec<String> = Vec::with_capacity(n_windows);
    for (i, window) in windows.into_iter().enumerate() {
        let text = architecture.transcribe_window(&window, &mut |_| {})?;
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
    architecture: &mut dyn SpeechToTextArchitectureImpl,
    input: AudioInput,
    tx: UnboundedSender<StreamOutput<SpeechToTextError>>,
) {
    if let Err(e) = do_transcribe_streaming(architecture, input, &tx) {
        let _ = tx.send(StreamOutput::Error(e));
    }
}

fn do_transcribe_streaming(
    architecture: &mut dyn SpeechToTextArchitectureImpl,
    input: AudioInput,
    tx: &UnboundedSender<StreamOutput<SpeechToTextError>>,
) -> Result<(), SpeechToTextError> {
    let windows = decode_input(input)?;
    let mut full_transcript = String::new();

    for window in windows {
        let text = architecture.transcribe_window(&window, &mut |piece| {
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
