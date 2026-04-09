use crate::errors::SpeechToTextError;
use crate::llm::{TokenStream, TokenStreamAsync, WorkerGuard, WriteOutput};
use rubato::{FftFixedIn, Resampler};
use std::sync::Arc;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error::{DecodeError, IoError};
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tracing::error;
use whisper_rs::{
    FullParams, SamplingStrategy, SegmentCallbackData, WhisperContext, WhisperContextParameters,
};

const RESAMPLE_CHUNK_SIZE: usize = 1024;

/// Configuration for speech-to-text transcription.
#[derive(Clone, Debug, Default)]
pub struct SpeechToTextConfig {
    /// Target language code (e.g. "en", "de"). None for auto-detect.
    pub language: Option<String>,
    /// Translate output to English instead of transcribing. Default: false.
    pub translate: bool,
    /// Text to prime the decoder with domain-specific vocabulary. Default: None.
    pub initial_prompt: Option<String>,
}

/// Synchronous speech-to-text handle. Wraps [`SpeechToTextAsync`].
#[derive(Clone)]
pub struct SpeechToText {
    async_handle: SpeechToTextAsync,
}

/// Asynchronous speech-to-text handle backed by a dedicated worker thread.
#[derive(Clone)]
pub struct SpeechToTextAsync {
    guard: Arc<WorkerGuard<SttMsg>>,
}

enum SttMsg {
    Stream(String, tokio::sync::mpsc::UnboundedSender<WriteOutput>),
}

// -- Public API --

impl SpeechToText {
    pub fn new(model_path: String, config: SpeechToTextConfig) -> Result<Self, SpeechToTextError> {
        let async_handle = SpeechToTextAsync::new(model_path, config)?;
        Ok(Self { async_handle })
    }

    pub fn transcribe(&self, audio_path: String) -> TokenStream {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.async_handle.guard.send(SttMsg::Stream(audio_path, tx));
        TokenStream::new(rx)
    }
}

impl SpeechToTextAsync {
    pub fn new(model_path: String, config: SpeechToTextConfig) -> Result<Self, SpeechToTextError> {
        let ctx = WhisperContext::new_with_params(&model_path, WhisperContextParameters::default())
            .map_err(|e| SpeechToTextError::LoadModel(e.to_string()))?;

        let (msg_tx, msg_rx) = std::sync::mpsc::channel::<SttMsg>();

        let join_handle = std::thread::spawn(move || {
            let mut state = match ctx.create_state() {
                Ok(s) => s,
                Err(e) => return error!(error=%e, "Could not create whisper state"),
            };

            while let Ok(msg) = msg_rx.recv() {
                match msg {
                    SttMsg::Stream(audio_path, output_tx) => {
                        transcribe_streaming(&mut state, &config, &audio_path, &output_tx);
                    }
                }
            }
        });

        Ok(Self {
            guard: Arc::new(WorkerGuard::new(msg_tx, join_handle, None)),
        })
    }

    pub fn transcribe(&self, audio_path: String) -> TokenStreamAsync {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.guard.send(SttMsg::Stream(audio_path, tx));
        TokenStreamAsync::new(rx)
    }
}

// -- Transcription --

fn transcribe_streaming(
    state: &mut whisper_rs::WhisperState,
    config: &SpeechToTextConfig,
    audio_path: &str,
    output_tx: &tokio::sync::mpsc::UnboundedSender<WriteOutput>,
) {
    let samples = match load_audio(audio_path, 16000) {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut params = build_whisper_params(config);
    let tx = output_tx.clone();
    params.set_segment_callback_safe_lossy(move |data: SegmentCallbackData| {
        let _ = tx.send(WriteOutput::Token(data.text));
    });

    if state.full(params, &samples).is_err() {
        return;
    }

    let _ = output_tx.send(WriteOutput::Done(collect_transcript(state)));
}

fn build_whisper_params(config: &'_ SpeechToTextConfig) -> FullParams<'_, '_> {
    let n_threads = std::thread::available_parallelism()
        .map(|p| p.get() as i32)
        .unwrap_or(4);
    let mut params = FullParams::new(SamplingStrategy::Greedy { best_of: 5 });
    params.set_n_threads(n_threads);
    params.set_translate(config.translate);
    params.set_language(config.language.as_deref());
    params.set_print_progress(false);
    params.set_print_realtime(false);
    params.set_print_special(false);
    if let Some(ref prompt) = config.initial_prompt {
        params.set_initial_prompt(prompt);
    }
    params
}

fn collect_transcript(state: &whisper_rs::WhisperState) -> String {
    (0..state.full_n_segments())
        .filter_map(|i| state.get_segment(i))
        .map(|seg| seg.to_string())
        .collect::<String>()
        .trim()
        .to_string()
}

// -- Audio loading --

fn load_audio(path: &str, target_rate: u32) -> Result<Vec<f32>, SpeechToTextError> {
    let mut format_reader = open_format_reader(path)?;
    let (track_id, sample_rate, n_channels, codec_params) =
        read_track_info(format_reader.as_ref(), path)?;
    let mut decoder = make_decoder(codec_params)?;
    let interleaved = collect_samples(&mut format_reader, &mut decoder, track_id)?;
    let mono = to_mono(interleaved, n_channels);

    if sample_rate == target_rate {
        Ok(mono)
    } else {
        resample(mono, sample_rate, target_rate)
    }
}

fn open_format_reader(path: &str) -> Result<Box<dyn FormatReader>, SpeechToTextError> {
    let file = std::fs::File::open(path)
        .map_err(|e| SpeechToTextError::AudioDecode(format!("Could not open '{}': {}", path, e)))?;

    let mss = MediaSourceStream::new(Box::new(file), Default::default());

    let mut hint = Hint::new();
    if let Some(ext) = std::path::Path::new(path)
        .extension()
        .and_then(|e| e.to_str())
    {
        hint.with_extension(ext);
    }

    symphonia::default::get_probe()
        .format(
            &hint,
            mss,
            &FormatOptions::default(),
            &MetadataOptions::default(),
        )
        .map(|p| p.format)
        .map_err(|e| SpeechToTextError::AudioDecode(format!("Could not probe format: {}", e)))
}

fn read_track_info(
    format_reader: &dyn FormatReader,
    path: &str,
) -> Result<(u32, u32, usize, CodecParameters), SpeechToTextError> {
    let track = format_reader
        .tracks()
        .iter()
        .find(|t| t.codec_params.codec != CODEC_TYPE_NULL)
        .ok_or_else(|| SpeechToTextError::AudioDecode(format!("No audio track in '{}'", path)))?;

    let track_id = track.id;
    let sample_rate = track
        .codec_params
        .sample_rate
        .ok_or_else(|| SpeechToTextError::AudioDecode("Unknown sample rate".into()))?;
    let n_channels = track.codec_params.channels.map(|c| c.count()).unwrap_or(1);
    let codec_params = track.codec_params.clone();

    Ok((track_id, sample_rate, n_channels, codec_params))
}

fn make_decoder(codec_params: CodecParameters) -> Result<Box<dyn Decoder>, SpeechToTextError> {
    symphonia::default::get_codecs()
        .make(&codec_params, &DecoderOptions::default())
        .map_err(|e| SpeechToTextError::AudioDecode(format!("Could not create decoder: {}", e)))
}

fn collect_samples(
    format_reader: &mut Box<dyn FormatReader>,
    decoder: &mut Box<dyn Decoder>,
    track_id: u32,
) -> Result<Vec<f32>, SpeechToTextError> {
    let mut interleaved: Vec<f32> = Vec::new();

    loop {
        let packet = match format_reader.next_packet() {
            Ok(p) => p,
            Err(IoError(ref e)) if e.kind() == std::io::ErrorKind::UnexpectedEof => break,
            Err(e) => return Err(SpeechToTextError::AudioDecode(e.to_string())),
        };

        if packet.track_id() != track_id {
            continue;
        }

        let decoded = match decoder.decode(&packet) {
            Ok(d) => d,
            Err(IoError(_) | DecodeError(_)) => continue,
            Err(e) => return Err(SpeechToTextError::AudioDecode(e.to_string())),
        };

        let mut sample_buf = SampleBuffer::<f32>::new(decoded.capacity() as u64, *decoded.spec());
        sample_buf.copy_interleaved_ref(decoded);
        interleaved.extend_from_slice(sample_buf.samples());
    }

    Ok(interleaved)
}

fn to_mono(interleaved: Vec<f32>, n_channels: usize) -> Vec<f32> {
    if n_channels == 1 {
        return interleaved;
    }
    interleaved
        .chunks_exact(n_channels)
        .map(|frame| frame.iter().sum::<f32>() / n_channels as f32)
        .collect()
}

// -- Resampling --

fn resample(
    samples: Vec<f32>,
    from_rate: u32,
    to_rate: u32,
) -> Result<Vec<f32>, SpeechToTextError> {
    let n_input = samples.len();
    let expected_output = (n_input as f64 * to_rate as f64 / from_rate as f64).ceil() as usize;

    let mut resampler = FftFixedIn::<f32>::new(
        from_rate as usize,
        to_rate as usize,
        RESAMPLE_CHUNK_SIZE,
        2,
        1,
    )
    .map_err(|e| SpeechToTextError::Resample(e.to_string()))?;

    let mut output = Vec::with_capacity(expected_output);

    for chunk in samples.chunks(RESAMPLE_CHUNK_SIZE) {
        let resampled = process_chunk(&mut resampler, chunk)?;
        output.extend_from_slice(&resampled);
    }

    output.truncate(expected_output);
    Ok(output)
}

fn process_chunk(
    resampler: &mut FftFixedIn<f32>,
    chunk: &[f32],
) -> Result<Vec<f32>, SpeechToTextError> {
    let mut padded = vec![0.0f32; RESAMPLE_CHUNK_SIZE];
    padded[..chunk.len()].copy_from_slice(chunk);

    resampler
        .process(&[&padded], None)
        .map(|out| out.into_iter().next().unwrap_or_default())
        .map_err(|e| SpeechToTextError::Resample(e.to_string()))
}
