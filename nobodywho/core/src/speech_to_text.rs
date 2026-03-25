use crate::errors::SpeechToTextError;
use crate::llm::WorkerGuard;
use rubato::{FftFixedIn, Resampler};
use std::sync::Arc;
use symphonia::core::audio::SampleBuffer;
use symphonia::core::codecs::{CodecParameters, Decoder, DecoderOptions, CODEC_TYPE_NULL};
use symphonia::core::errors::Error::{DecodeError, IoError};
use symphonia::core::formats::{FormatOptions, FormatReader};
use symphonia::core::io::MediaSourceStream;
use symphonia::core::meta::MetadataOptions;
use symphonia::core::probe::Hint;
use tokio::sync::mpsc::UnboundedReceiver;
use tracing::error;
use whisper_rs::{
    FullParams, SamplingStrategy, SegmentCallbackData, WhisperContext, WhisperContextParameters,
};

const RESAMPLE_CHUNK_SIZE: usize = 1024;

/// Configuration for speech-to-text transcription.
#[derive(Clone, Debug)]
pub struct SpeechToTextConfig {
    /// Target language code (e.g. "en", "de"). None for auto-detect.
    pub language: Option<String>,
    /// Translate output to English instead of transcribing. Default: false.
    pub translate: bool,
    /// Text to prime the decoder with domain-specific vocabulary. Default: None.
    pub initial_prompt: Option<String>,
}

impl Default for SpeechToTextConfig {
    fn default() -> Self {
        Self {
            language: Some("en".to_string()),
            translate: false,
            initial_prompt: None,
        }
    }
}

/// Output from a streaming transcription.
pub enum SttOutput {
    /// A single whisper segment as it is produced.
    Segment(String),
    /// Signals completion; carries the full trimmed transcript.
    Done(String),
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
    Convert(
        String,
        tokio::sync::mpsc::Sender<Result<String, SpeechToTextError>>,
    ),
    Stream(String, tokio::sync::mpsc::UnboundedSender<SttOutput>),
}

/// A stream of transcript segments, sync version.
pub struct SegmentStream {
    rx: UnboundedReceiver<SttOutput>,
    completed_transcript: Option<String>,
}

/// A stream of transcript segments, async version.
pub struct SegmentStreamAsync {
    rx: UnboundedReceiver<SttOutput>,
    completed_transcript: Option<String>,
}

// -- Public API --

impl SpeechToText {
    pub fn new(model_path: String, config: SpeechToTextConfig) -> Result<Self, SpeechToTextError> {
        let async_handle = SpeechToTextAsync::new(model_path, config)?;
        Ok(Self { async_handle })
    }

    pub fn convert(&self, audio_path: String) -> Result<String, SpeechToTextError> {
        futures::executor::block_on(async { self.async_handle.convert(audio_path).await })
    }

    pub fn stream(&self, audio_path: String) -> SegmentStream {
        SegmentStream::new(self.async_handle.stream_channel(audio_path))
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
                    SttMsg::Convert(audio_path, respond) => {
                        let result = transcribe(&mut state, &config, &audio_path);
                        let _ = respond.blocking_send(result);
                    }
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

    pub async fn convert(&self, audio_path: String) -> Result<String, SpeechToTextError> {
        let (tx, mut rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(SttMsg::Convert(audio_path, tx));
        rx.recv().await.ok_or(SpeechToTextError::NoResponse)?
    }

    pub fn stream(&self, audio_path: String) -> SegmentStreamAsync {
        SegmentStreamAsync::new(self.stream_channel(audio_path))
    }

    fn stream_channel(&self, audio_path: String) -> UnboundedReceiver<SttOutput> {
        let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
        self.guard.send(SttMsg::Stream(audio_path, tx));
        rx
    }
}

impl SegmentStream {
    fn new(rx: UnboundedReceiver<SttOutput>) -> Self {
        Self {
            rx,
            completed_transcript: None,
        }
    }

    /// Get the next segment. Returns `None` when transcription is complete.
    pub fn next_segment(&mut self) -> Option<String> {
        if self.completed_transcript.is_some() {
            return None;
        }
        match self.rx.blocking_recv()? {
            SttOutput::Segment(text) => Some(text),
            SttOutput::Done(full) => {
                self.completed_transcript = Some(full);
                None
            }
        }
    }

    /// Drain the stream and return the full transcript. Idempotent.
    pub fn completed(&mut self) -> Result<String, SpeechToTextError> {
        while self.next_segment().is_some() {}
        self.completed_transcript
            .clone()
            .ok_or(SpeechToTextError::NoResponse)
    }
}

impl SegmentStreamAsync {
    fn new(rx: UnboundedReceiver<SttOutput>) -> Self {
        Self {
            rx,
            completed_transcript: None,
        }
    }

    /// Wait for the next segment. Returns `None` when transcription is complete.
    pub async fn next_segment(&mut self) -> Option<String> {
        if self.completed_transcript.is_some() {
            return None;
        }
        match self.rx.recv().await? {
            SttOutput::Segment(text) => Some(text),
            SttOutput::Done(full) => {
                self.completed_transcript = Some(full);
                None
            }
        }
    }

    /// Drain the stream and return the full transcript. Idempotent.
    pub async fn completed(&mut self) -> Result<String, SpeechToTextError> {
        while self.next_segment().await.is_some() {}
        self.completed_transcript
            .clone()
            .ok_or(SpeechToTextError::NoResponse)
    }
}

// -- Transcription --

fn transcribe(
    state: &mut whisper_rs::WhisperState,
    config: &SpeechToTextConfig,
    audio_path: &str,
) -> Result<String, SpeechToTextError> {
    let samples = load_audio(audio_path, 16000)?;
    let params = build_whisper_params(config);

    state
        .full(params, &samples)
        .map_err(|e| SpeechToTextError::Transcribe(e.to_string()))?;

    Ok(collect_transcript(state))
}

fn transcribe_streaming(
    state: &mut whisper_rs::WhisperState,
    config: &SpeechToTextConfig,
    audio_path: &str,
    output_tx: &tokio::sync::mpsc::UnboundedSender<SttOutput>,
) {
    let samples = match load_audio(audio_path, 16000) {
        Ok(s) => s,
        Err(_) => return,
    };

    let mut params = build_whisper_params(config);
    let tx = output_tx.clone();
    params.set_segment_callback_safe_lossy(move |data: SegmentCallbackData| {
        let _ = tx.send(SttOutput::Segment(data.text));
    });

    if state.full(params, &samples).is_err() {
        return;
    }

    let _ = output_tx.send(SttOutput::Done(collect_transcript(state)));
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
    let (track_id, sample_rate, n_channels, codec_params) = read_track_info(&format_reader, path)?;
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
    format_reader: &Box<dyn FormatReader>,
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
