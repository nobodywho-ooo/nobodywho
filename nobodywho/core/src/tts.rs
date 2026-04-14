/// Text-to-speech synthesis using OuteTTS + WavTokenizer.
///
/// The default public API accepts plain text and formats the required OuteTTS
/// prompt internally. [`Tts::synthesize_prompt`] is available as an escape hatch
/// for callers that need full control over the prompt.
use crate::errors::{DecodingError, InitWorkerError, TtsWorkerError};
use crate::llm::{self, Worker, WorkerGuard, GLOBAL_INFERENCE_LOCK};
use crate::memory;
use crate::sampler_config::SamplerPresets;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::AddBos;
use llama_cpp_2::token::LlamaToken;
use serde::{Deserialize, Serialize};
use stft_rs::{BatchIstft, ReconstructionMode, Spectrum, StftConfigBuilder, WindowType};
use std::sync::Arc;
use std::collections::VecDeque;
use std::time::Instant;
use tracing::{debug, error, info};

/// Output sample rate for WAV encoding.
const SAMPLE_RATE: u32 = 24000;
const LOW_OUTPUT_PEAK_THRESHOLD: f32 = 0.02;

/// Synchronous TTS handle. Wraps [`TtsAsync`] and blocks the calling thread.
#[derive(Clone)]
pub struct Tts {
    async_handle: TtsAsync,
}

/// Async TTS handle.
#[derive(Clone)]
pub struct TtsAsync {
    guard: Arc<WorkerGuard<TtsMsg>>,
    backend: TtsBackendConfig,
}

#[derive(Clone, Debug)]
pub struct TtsBackendConfig {
    text_model: TextModelBackend,
    vocoder: VocoderBackend,
}

impl Default for TtsBackendConfig {
    fn default() -> Self {
        Self {
            text_model: TextModelBackend::OuteTtsV02,
            vocoder: VocoderBackend::WavTokenizer75,
        }
    }
}

#[derive(Clone, Debug)]
pub enum TextModelBackend {
    OuteTtsV02,
    OuteTtsV03,
}

#[derive(Clone, Debug)]
pub enum VocoderBackend {
    WavTokenizer75,
}

impl TtsBackendConfig {
    pub fn new(text_model: TextModelBackend, vocoder: VocoderBackend) -> Self {
        Self { text_model, vocoder }
    }

    pub fn text_model(&self) -> &TextModelBackend {
        &self.text_model
    }

    pub fn vocoder(&self) -> &VocoderBackend {
        &self.vocoder
    }
}

impl TextModelBackend {
    fn normalize_text(&self, text: &str) -> String {
        match self {
            TextModelBackend::OuteTtsV02 => process_text_v02(text),
            TextModelBackend::OuteTtsV03 => process_text_v03(text),
        }
    }

    fn audio_code_range(&self) -> Option<(i32, i32)> {
        match self {
            TextModelBackend::OuteTtsV02 => Some((151672, 155772)),
            TextModelBackend::OuteTtsV03 => Some((50307, 54402)),
        }
    }

    fn stop_token(&self) -> Option<&'static str> {
        match self {
            TextModelBackend::OuteTtsV02 => Some("<|audio_end|>"),
            TextModelBackend::OuteTtsV03 => Some("<|audio_end|>"),
        }
    }

    fn cached_prompt_prefix(&self, speaker: &TtsSpeaker) -> Option<String> {
        match (self, speaker) {
            (TextModelBackend::OuteTtsV02, TtsSpeaker::Preset(TtsSpeakerPreset::DefaultEnglishMale)) => {
                Some(default_english_male_prefix())
            }
            (TextModelBackend::OuteTtsV03, _) => None,
            (TextModelBackend::OuteTtsV02, TtsSpeaker::Profile(_)) => None,
        }
    }

    fn prompt(&self, request: &PreparedTtsRequest) -> Result<String, TtsWorkerError> {
        match (self, &request.speaker) {
            (TextModelBackend::OuteTtsV02, TtsSpeaker::Preset(TtsSpeakerPreset::DefaultEnglishMale)) => {
                Ok(build_default_english_male_prompt(&request.processed_text))
            }
            (TextModelBackend::OuteTtsV02, TtsSpeaker::Profile(_)) => Err(TtsWorkerError::InvalidRequest(
                "speaker profiles are not supported by the OuteTTS 0.2 backend".into(),
            )),
            (TextModelBackend::OuteTtsV03, TtsSpeaker::Preset(_)) => {
                Ok(build_outetts_v03_prompt_no_speaker(&request.processed_text))
            }
            (TextModelBackend::OuteTtsV03, TtsSpeaker::Profile(profile)) => {
                Ok(build_outetts_v03_prompt(&request.processed_text, profile)?)
            }
        }
    }

    fn sampler_config(&self) -> Result<crate::sampler_config::SamplerConfig, TtsWorkerError> {
        match self {
            TextModelBackend::OuteTtsV02 => Ok(SamplerPresets::top_k(4)),
            TextModelBackend::OuteTtsV03 => Ok(SamplerPresets::top_k(4)),
        }
    }

    fn max_audio_codes(&self, spoken_char_count: usize) -> Option<usize> {
        match self {
            TextModelBackend::OuteTtsV02 => None,
            TextModelBackend::OuteTtsV03 => Some(std::cmp::max(256, spoken_char_count.saturating_mul(20))),
        }
    }
}

impl VocoderBackend {
    fn hop_size(&self) -> usize {
        match self {
            VocoderBackend::WavTokenizer75 => 320,
        }
    }

    fn n_ubatch(&self, n_ctx: u32) -> u32 {
        match self {
            VocoderBackend::WavTokenizer75 => n_ctx,
        }
    }
}

#[derive(Clone, Debug)]
struct PreparedTtsRequest {
    processed_text: String,
    spoken_char_count: usize,
    speaker: TtsSpeaker,
}

/// Supported built-in speaker presets for prompt construction.
#[derive(Clone, Copy, Debug, Default)]
pub enum TtsSpeakerPreset {
    #[default]
    DefaultEnglishMale,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TtsSpeakerProfile {
    pub text: String,
    pub words: Vec<TtsSpeakerWord>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TtsSpeakerWord {
    pub word: String,
    pub duration: f32,
    pub codes: Vec<u16>,
}

#[derive(Clone, Debug)]
pub enum TtsSpeaker {
    Preset(TtsSpeakerPreset),
    Profile(TtsSpeakerProfile),
}

impl Default for TtsSpeaker {
    fn default() -> Self {
        Self::Preset(TtsSpeakerPreset::default())
    }
}

impl TtsSpeakerProfile {
    pub fn from_json_str(json: &str) -> Result<Self, TtsWorkerError> {
        let profile: Self = serde_json::from_str(json).map_err(|err| {
            TtsWorkerError::InvalidRequest(format!("failed to parse speaker profile JSON: {err}"))
        })?;
        profile.validate()?;
        Ok(profile)
    }

    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<Self, TtsWorkerError> {
        let path = path.as_ref();
        let json = std::fs::read_to_string(path).map_err(|err| {
            TtsWorkerError::InvalidRequest(format!(
                "failed to read speaker profile {}: {err}",
                path.display()
            ))
        })?;
        Self::from_json_str(&json)
    }

    fn validate(&self) -> Result<(), TtsWorkerError> {
        if self.words.is_empty() {
            return Err(TtsWorkerError::InvalidRequest(
                "speaker profile must contain at least one word".into(),
            ));
        }

        for word in &self.words {
            if !word.duration.is_finite() || word.duration < 0.0 {
                return Err(TtsWorkerError::InvalidRequest(format!(
                    "speaker profile word {:?} has invalid duration {}",
                    word.word, word.duration
                )));
            }
            if word.codes.is_empty() {
                return Err(TtsWorkerError::InvalidRequest(format!(
                    "speaker profile word {:?} has no audio codes",
                    word.word
                )));
            }
            if word.codes.iter().any(|&code| code > 4095) {
                return Err(TtsWorkerError::InvalidRequest(format!(
                    "speaker profile word {:?} contains audio code outside 0..=4095",
                    word.word
                )));
            }
        }

        Ok(())
    }
}

/// High-level TTS request. Plain text requests are preferred over raw prompts.
#[derive(Clone, Debug)]
pub struct TtsRequest {
    text: String,
    speaker: TtsSpeaker,
}

impl TtsRequest {
    pub fn new(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            speaker: TtsSpeaker::default(),
        }
    }

    pub fn with_speaker(mut self, speaker: TtsSpeakerPreset) -> Self {
        self.speaker = TtsSpeaker::Preset(speaker);
        self
    }

    pub fn with_speaker_profile(mut self, speaker: TtsSpeakerProfile) -> Self {
        self.speaker = TtsSpeaker::Profile(speaker);
        self
    }

    fn prepare_with_backend(
        self,
        backend: &TextModelBackend,
    ) -> Result<PreparedTtsRequest, TtsWorkerError> {
        let spoken_char_count = self.text.chars().filter(|c| c.is_alphabetic()).count();
        let processed = backend.normalize_text(&self.text);
        if processed.is_empty() {
            return Err(TtsWorkerError::InvalidRequest(
                "text must contain at least one ASCII letter after normalization".into(),
            ));
        }

        Ok(PreparedTtsRequest {
            processed_text: processed,
            spoken_char_count,
            speaker: self.speaker,
        })
    }
}

impl Tts {
    pub fn new_with_backend(
        tts_model: Arc<llm::Model>,
        vocoder_model: Arc<llm::Model>,
        n_ctx: u32,
        backend: TtsBackendConfig,
    ) -> Self {
        Self::try_new_with_backend(tts_model, vocoder_model, n_ctx, backend)
            .expect("failed to initialize TTS workers")
    }

    pub fn new(tts_model: Arc<llm::Model>, vocoder_model: Arc<llm::Model>, n_ctx: u32) -> Self {
        Self::try_new(tts_model, vocoder_model, n_ctx)
            .expect("failed to initialize TTS workers")
    }

    pub fn try_new(
        tts_model: Arc<llm::Model>,
        vocoder_model: Arc<llm::Model>,
        n_ctx: u32,
    ) -> Result<Self, TtsWorkerError> {
        Self::try_new_with_backend(tts_model, vocoder_model, n_ctx, TtsBackendConfig::default())
    }

    pub fn try_new_with_backend(
        tts_model: Arc<llm::Model>,
        vocoder_model: Arc<llm::Model>,
        n_ctx: u32,
        backend: TtsBackendConfig,
    ) -> Result<Self, TtsWorkerError> {
        Ok(Self {
            async_handle: TtsAsync::try_new_with_backend(
                tts_model,
                vocoder_model,
                n_ctx,
                backend,
            )?,
        })
    }

    /// Synthesize speech from plain text using the default prompt builder.
    pub fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsWorkerError> {
        futures::executor::block_on(self.async_handle.synthesize(text))
    }

    /// Synthesize speech from a structured text request.
    pub fn synthesize_request(&self, request: TtsRequest) -> Result<Vec<u8>, TtsWorkerError> {
        futures::executor::block_on(self.async_handle.synthesize_request(request))
    }

    /// Synthesize speech from a fully preformatted model prompt.
    pub fn synthesize_prompt(&self, prompt: impl Into<String>) -> Result<Vec<u8>, TtsWorkerError> {
        futures::executor::block_on(self.async_handle.synthesize_prompt(prompt))
    }
}

impl TtsAsync {
    pub fn new_with_backend(
        tts_model: Arc<llm::Model>,
        vocoder_model: Arc<llm::Model>,
        n_ctx: u32,
        backend: TtsBackendConfig,
    ) -> Self {
        Self::try_new_with_backend(tts_model, vocoder_model, n_ctx, backend)
            .expect("failed to initialize TTS workers")
    }

    pub fn new(tts_model: Arc<llm::Model>, vocoder_model: Arc<llm::Model>, n_ctx: u32) -> Self {
        Self::try_new(tts_model, vocoder_model, n_ctx)
            .expect("failed to initialize TTS workers")
    }

    pub fn try_new(
        tts_model: Arc<llm::Model>,
        vocoder_model: Arc<llm::Model>,
        n_ctx: u32,
    ) -> Result<Self, TtsWorkerError> {
        Self::try_new_with_backend(tts_model, vocoder_model, n_ctx, TtsBackendConfig::default())
    }

    pub fn try_new_with_backend(
        tts_model: Arc<llm::Model>,
        vocoder_model: Arc<llm::Model>,
        n_ctx: u32,
        backend: TtsBackendConfig,
    ) -> Result<Self, TtsWorkerError> {
        let (msg_tx, msg_rx) = std::sync::mpsc::channel();
        let (ready_tx, ready_rx) = std::sync::mpsc::sync_channel(1);
        let worker_backend = backend.clone();

        let join_handle = std::thread::spawn(move || {
            let init_start = Instant::now();
            let mut tts_worker = match Worker::new_tts_worker(
                &tts_model,
                n_ctx,
                worker_backend.text_model.clone(),
            ) {
                Ok(w) => w,
                Err(e) => {
                    let err = TtsWorkerError::InitWorker(e);
                    let _ = ready_tx.send(Err(err));
                    return error!("Failed to initialize TTS worker");
                }
            };
            let mut audio_decoder = match create_audio_decoder(
                &vocoder_model,
                n_ctx,
                worker_backend.vocoder.clone(),
            ) {
                Ok(w) => w,
                Err(err) => {
                    let _ = ready_tx.send(Err(err));
                    return error!("Failed to initialize audio decoder");
                }
            };
            if let Err(err) = initialize_tts_caches(&mut tts_worker) {
                let _ = ready_tx.send(Err(err));
                return error!("Failed to initialize TTS prompt caches");
            }
            info!(elapsed = ?init_start.elapsed(), "Initialized TTS workers");
            let _ = ready_tx.send(Ok(()));

            while let Ok(msg) = msg_rx.recv() {
                if let Err(e) = process_worker_msg(&mut tts_worker, &mut audio_decoder, msg) {
                    return error!(error = %e, "TTS worker crashed");
                }
            }
        });

        let this = Self {
            guard: Arc::new(WorkerGuard::new(msg_tx, join_handle, None)),
            backend,
        };

        match ready_rx.recv() {
            Ok(Ok(())) => Ok(this),
            Ok(Err(err)) => Err(err),
            Err(_) => Err(TtsWorkerError::NoResponse),
        }
    }

    /// Synthesize speech from plain text using the default prompt builder.
    pub async fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsWorkerError> {
        self.synthesize_request(TtsRequest::new(text)).await
    }

    /// Synthesize speech from a structured text request.
    pub async fn synthesize_request(&self, request: TtsRequest) -> Result<Vec<u8>, TtsWorkerError> {
        let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(1);
        self.guard
            .send(TtsMsg::SynthesizePrepared(
                request.prepare_with_backend(&self.backend.text_model)?,
                result_tx,
            ));
        result_rx.recv().await.ok_or(TtsWorkerError::NoResponse)?
    }

    /// Synthesize speech from a fully preformatted model prompt.
    pub async fn synthesize_prompt(
        &self,
        prompt: impl Into<String>,
    ) -> Result<Vec<u8>, TtsWorkerError> {
        let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(1);
        self.guard.send(TtsMsg::Synthesize(prompt.into(), result_tx));
        result_rx.recv().await.ok_or(TtsWorkerError::NoResponse)?
    }
}

enum TtsMsg {
    SynthesizePrepared(
        PreparedTtsRequest,
        tokio::sync::mpsc::Sender<Result<Vec<u8>, TtsWorkerError>>,
    ),
    Synthesize(
        String,
        tokio::sync::mpsc::Sender<Result<Vec<u8>, TtsWorkerError>>,
    ),
}

struct TtsWorker {
    backend: TextModelBackend,
    default_english_male: Option<CachedPromptState>,
}
struct VocoderWorker {
    backend: VocoderBackend,
}

enum AudioDecoder<'a> {
    WavTokenizer(Worker<'a, VocoderWorker>),
}

#[derive(Clone)]
struct CachedPromptState {
    state: Vec<u8>,
    n_past: i32,
}

impl llm::PoolingType for TtsWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::None
    }
}

impl llm::PoolingType for VocoderWorker {
    fn pooling_type(&self) -> LlamaPoolingType {
        LlamaPoolingType::None
    }
}

impl<'a> Worker<'a, TtsWorker> {
    fn new_tts_worker(
        model: &'a llm::Model,
        n_ctx: u32,
        backend: TextModelBackend,
    ) -> Result<Worker<'a, TtsWorker>, InitWorkerError> {
        Worker::new_with_type(
            model,
            n_ctx,
            false,
            TtsWorker {
                backend,
                default_english_male: None,
            },
        )
    }
}

impl<'a> Worker<'a, VocoderWorker> {
    fn new_vocoder_worker(
        model: &'a llm::Model,
        n_ctx: u32,
        backend: VocoderBackend,
    ) -> Result<Worker<'a, VocoderWorker>, InitWorkerError> {
        // The vocoder encodes the full audio-code sequence in one pass and then
        // reads per-token embeddings back out, so n_ubatch must cover the active
        // sequence length for longer utterances.
        let ctx = new_vocoder_context(model, n_ctx, &backend)?;
        let big_batch = LlamaBatch::new(ctx.n_ctx() as usize, 1);
        let small_batch = LlamaBatch::new(1, 1);
        let add_bos = llm::read_add_bos_metadata(&model.language_model)?;
        let tokenizer = crate::tokenizer::Tokenizer::new(
            &model.language_model,
            model.projection_model.as_ref(),
            add_bos,
        );

        Ok(Worker {
            n_past: 0,
            ctx,
            big_batch,
            small_batch,
            projection_model: model.projection_model.as_ref(),
            tokenizer,
            extra: VocoderWorker { backend },
        })
    }
}

fn new_vocoder_context<'a>(
    model: &'a llm::Model,
    n_ctx: u32,
    backend: &VocoderBackend,
) -> Result<LlamaContext<'a>, InitWorkerError> {
    let projection_model = model.projection_model.as_ref();
    let n_threads = std::thread::available_parallelism()?.get() as i32;
    let ctx_plan = memory::plan_context(
        std::cmp::min(n_ctx, model.language_model.n_ctx_train()),
        projection_model.is_some(),
        memory::ModelArchitecture {
            n_layers: model.language_model.n_layer(),
            n_embd: model.language_model.n_embd() as u32,
            n_head: model.language_model.n_head(),
            n_head_kv: model.language_model.n_head_kv(),
        },
    )?;
    let n_ctx = ctx_plan.n_ctx;
    for w in &ctx_plan.warnings {
        tracing::warn!("{}", w);
    }

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(std::num::NonZero::new(n_ctx))
        .with_n_batch(n_ctx)
        .with_n_ubatch(backend.n_ubatch(n_ctx))
        .with_n_threads(n_threads)
        .with_n_threads_batch(n_threads)
        .with_embeddings(true)
        .with_pooling_type(LlamaPoolingType::None);

    model
        .language_model
        .new_context(&llm::LLAMA_BACKEND, ctx_params)
        .map_err(InitWorkerError::from)
}

fn process_worker_msg(
    tts_worker: &mut Worker<'_, TtsWorker>,
    audio_decoder: &mut AudioDecoder<'_>,
    msg: TtsMsg,
) -> Result<(), TtsWorkerError> {
    match msg {
        TtsMsg::SynthesizePrepared(request, respond) => {
            let result = synthesize_prepared_inner(tts_worker, audio_decoder, request);
            let _ = respond.blocking_send(result);
        }
        TtsMsg::Synthesize(prompt, respond) => {
            let result = synthesize_inner(tts_worker, audio_decoder, prompt);
            let _ = respond.blocking_send(result);
        }
    }
    Ok(())
}

fn create_audio_decoder<'a>(
    vocoder_model: &'a llm::Model,
    n_ctx: u32,
    backend: VocoderBackend,
) -> Result<AudioDecoder<'a>, TtsWorkerError> {
    match backend {
        VocoderBackend::WavTokenizer75 => Worker::new_vocoder_worker(
            vocoder_model,
            n_ctx,
            VocoderBackend::WavTokenizer75,
        )
        .map(AudioDecoder::WavTokenizer)
        .map_err(TtsWorkerError::InitWorker),
    }
}

fn reset_audio_decoder(audio_decoder: &mut AudioDecoder<'_>) {
    let AudioDecoder::WavTokenizer(worker) = audio_decoder;
    worker.reset_context();
}

fn decode_audio(
    audio_decoder: &mut AudioDecoder<'_>,
    audio_codes: &[LlamaToken],
) -> Result<Vec<f32>, TtsWorkerError> {
    match audio_decoder {
        AudioDecoder::WavTokenizer(worker) => {
            let vocoder_start = Instant::now();
            let frames = run_vocoder(worker, audio_codes)?;
            info!(
                n_frames = frames.len(),
                elapsed = ?vocoder_start.elapsed(),
                "Got vocoder spectral frames"
            );
            reconstruct_audio_with_backend(&frames, &worker.extra.backend)
        }
    }
}

fn synthesize_inner(
    tts_worker: &mut Worker<'_, TtsWorker>,
    audio_decoder: &mut AudioDecoder<'_>,
    prompt: String,
) -> Result<Vec<u8>, TtsWorkerError> {
    synthesize_inner_with_limit(tts_worker, audio_decoder, prompt, None)
}

fn synthesize_inner_with_limit(
    tts_worker: &mut Worker<'_, TtsWorker>,
    audio_decoder: &mut AudioDecoder<'_>,
    prompt: String,
    max_audio_codes: Option<usize>,
) -> Result<Vec<u8>, TtsWorkerError> {
    let total_start = Instant::now();
    tts_worker.reset_context();
    reset_audio_decoder(audio_decoder);

    let codes_start = Instant::now();
    let audio_codes = generate_audio_codes(tts_worker, prompt, max_audio_codes)?;
    info!(
        n_codes = audio_codes.len(),
        elapsed = ?codes_start.elapsed(),
        "Generated audio codes"
    );

    if audio_codes.is_empty() {
        return Err(TtsWorkerError::Vocoder(
            "no audio codes generated from the prompt".into(),
        ));
    }

    let reconstruct_start = Instant::now();
    let pcm = decode_audio(audio_decoder, &audio_codes)?;
    let pcm_peak = pcm.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    if pcm_peak < LOW_OUTPUT_PEAK_THRESHOLD {
        tracing::warn!(
            peak_amplitude = pcm_peak,
            threshold = LOW_OUTPUT_PEAK_THRESHOLD,
            "Reconstructed PCM audio is very quiet"
        );
    }
    info!(
        n_samples = pcm.len(),
        peak_amplitude = pcm_peak,
        elapsed = ?reconstruct_start.elapsed(),
        "Reconstructed PCM audio"
    );

    let wav_start = Instant::now();
    let wav = encode_wav(&pcm)?;
    info!(elapsed = ?wav_start.elapsed(), total = ?total_start.elapsed(), "Encoded WAV output");
    Ok(wav)
}

fn synthesize_prepared_inner(
    tts_worker: &mut Worker<'_, TtsWorker>,
    audio_decoder: &mut AudioDecoder<'_>,
    request: PreparedTtsRequest,
) -> Result<Vec<u8>, TtsWorkerError> {
    let max_audio_codes = tts_worker
        .extra
        .backend
        .max_audio_codes(request.spoken_char_count);

    match &request.speaker {
        TtsSpeaker::Preset(TtsSpeakerPreset::DefaultEnglishMale) => {
            if let Some(cache) = tts_worker.extra.default_english_male.clone() {
                let suffix = build_default_english_male_suffix(&request.processed_text);
                return synthesize_with_cached_prefix(
                    tts_worker,
                    audio_decoder,
                    &cache,
                    suffix,
                    max_audio_codes,
                );
            }
        }
        TtsSpeaker::Profile(_) => {}
    }

    synthesize_inner_with_limit(
        tts_worker,
        audio_decoder,
        tts_worker.extra.backend.prompt(&request)?,
        max_audio_codes,
    )
}

fn synthesize_with_cached_prefix(
    tts_worker: &mut Worker<'_, TtsWorker>,
    audio_decoder: &mut AudioDecoder<'_>,
    cache: &CachedPromptState,
    prompt_suffix: String,
    max_audio_codes: Option<usize>,
) -> Result<Vec<u8>, TtsWorkerError> {
    let total_start = Instant::now();
    tts_worker.reset_context();
    reset_audio_decoder(audio_decoder);

    let restore_start = Instant::now();
    let restored = unsafe { tts_worker.ctx.set_state_data(&cache.state) };
    if restored == 0 {
        return Err(TtsWorkerError::InvalidRequest(
            "failed to restore cached TTS prompt prefix state".into(),
        ));
    }
    tts_worker.n_past = cache.n_past;
    info!(elapsed = ?restore_start.elapsed(), "Restored cached TTS prefix");

    let suffix_start = Instant::now();
    tts_worker.read_string(prompt_suffix)?;
    info!(
        elapsed = ?suffix_start.elapsed(),
        n_past = tts_worker.n_past,
        "Read dynamic TTS suffix"
    );

    let codes_start = Instant::now();
    let audio_codes = generate_audio_codes_from_current_state(tts_worker, max_audio_codes)?;
    info!(
        n_codes = audio_codes.len(),
        elapsed = ?codes_start.elapsed(),
        "Generated audio codes"
    );

    if audio_codes.is_empty() {
        return Err(TtsWorkerError::Vocoder(
            "no audio codes generated from the prompt".into(),
        ));
    }

    let reconstruct_start = Instant::now();
    let pcm = decode_audio(audio_decoder, &audio_codes)?;
    let pcm_peak = pcm.iter().map(|x| x.abs()).fold(0.0f32, f32::max);
    if pcm_peak < LOW_OUTPUT_PEAK_THRESHOLD {
        tracing::warn!(
            peak_amplitude = pcm_peak,
            threshold = LOW_OUTPUT_PEAK_THRESHOLD,
            "Reconstructed PCM audio is very quiet"
        );
    }
    info!(
        n_samples = pcm.len(),
        peak_amplitude = pcm_peak,
        elapsed = ?reconstruct_start.elapsed(),
        "Reconstructed PCM audio"
    );

    let wav_start = Instant::now();
    let wav = encode_wav(&pcm)?;
    info!(elapsed = ?wav_start.elapsed(), total = ?total_start.elapsed(), "Encoded WAV output");
    Ok(wav)
}

/// Run the OuteTTS language model autoregressively, collecting audio code tokens.
fn generate_audio_codes(
    worker: &mut Worker<'_, TtsWorker>,
    prompt: String,
    max_audio_codes: Option<usize>,
) -> Result<Vec<LlamaToken>, TtsWorkerError> {
    worker.read_string(prompt)?;
    generate_audio_codes_from_current_state(worker, max_audio_codes)
}

fn generate_audio_codes_from_current_state(
    worker: &mut Worker<'_, TtsWorker>,
    max_audio_codes: Option<usize>,
) -> Result<Vec<LlamaToken>, TtsWorkerError> {
    const SHORT_AUDIO_GRACE_THRESHOLD: usize = 128;
    const SHORT_AUDIO_GRACE_AUDIO_CODES: usize = 12;
    const TOKEN_TRACE_LEN: usize = 24;

    let audio_end_token = worker
        .extra
        .backend
        .stop_token()
        .map(|piece| resolve_special_token(worker, piece))
        .transpose()?;
    let (audio_token_min, audio_token_max) = worker.extra.backend.audio_code_range().ok_or_else(|| {
        TtsWorkerError::InvalidRequest(
            "audio-code extraction for this TTS backend is not implemented yet".into(),
        )
    })?;
    let remaining_ctx = worker.ctx.n_ctx() as i32 - worker.n_past;
    if remaining_ctx <= 0 {
        return Err(TtsWorkerError::InvalidRequest(
            "prompt exhausted the entire context window; increase n_ctx or shorten the prompt"
                .into(),
        ));
    }

    // Hold the global inference lock for the entire sampling loop.
    let _lock = GLOBAL_INFERENCE_LOCK.lock().unwrap();

    let mut sampler = worker
        .extra
        .backend
        .sampler_config()?
        .to_stateful(worker.ctx.model)?;
    let mut audio_codes = Vec::new();
    let mut recent_tokens = VecDeque::with_capacity(TOKEN_TRACE_LEN);
    let max_tokens = remaining_ctx as usize;
    let mut stop_reason = "context_exhausted";
    let mut pending_terminal_reason: Option<&'static str> = None;
    let mut grace_audio_codes_remaining = 0usize;

    for _ in 0..max_tokens {
        let token = sampler.sample(&worker.ctx, -1);
        push_recent_token(worker, &mut recent_tokens, token);

        worker.small_batch.clear();
        worker
            .small_batch
            .add(token, worker.n_past, &[0], true)
            .map_err(|e| TtsWorkerError::Decoding(DecodingError::from(e)))?;
        worker
            .ctx
            .decode(&mut worker.small_batch)
            .map_err(|e| TtsWorkerError::Decoding(DecodingError::from(e)))?;
        worker.n_past += 1;

        let hit_audio_end = audio_end_token.is_some_and(|stop_token| token == stop_token);
        let hit_eog = worker.ctx.model.is_eog_token(token);

        if (audio_token_min..=audio_token_max).contains(&token.0) {
            audio_codes.push(LlamaToken(token.0 - audio_token_min));
            if grace_audio_codes_remaining > 0 {
                grace_audio_codes_remaining -= 1;
                if grace_audio_codes_remaining == 0 {
                    stop_reason = pending_terminal_reason.unwrap_or("grace_exhausted");
                    debug!(stop_reason, "grace audio-code window exhausted, stopping generation");
                    break;
                }
            }
            if max_audio_codes.is_some_and(|limit| audio_codes.len() >= limit) {
                stop_reason = "max_audio_codes";
                debug!(
                    limit = max_audio_codes.unwrap_or(audio_codes.len()),
                    "max audio-code limit reached, stopping generation"
                );
                break;
            }
        }

        if hit_audio_end || hit_eog {
            let terminal_reason = if hit_audio_end { "audio_end" } else { "eog" };
            if grace_audio_codes_remaining == 0
                && audio_codes.len() < SHORT_AUDIO_GRACE_THRESHOLD
                && pending_terminal_reason.is_none()
            {
                pending_terminal_reason = Some(terminal_reason);
                grace_audio_codes_remaining = SHORT_AUDIO_GRACE_AUDIO_CODES;
                debug!(
                    audio_codes = audio_codes.len(),
                    grace_audio_codes_remaining,
                    terminal_reason,
                    "terminal token reached on short utterance, entering grace window"
                );
                continue;
            }

            stop_reason = pending_terminal_reason.unwrap_or(terminal_reason);
            debug!(terminal_reason = stop_reason, "terminal token reached, stopping generation");
            break;
        }

    }

    let recent_tokens = recent_tokens.into_iter().collect::<Vec<_>>().join(" ");
    info!(
        n_codes = audio_codes.len(),
        stop_reason,
        recent_tokens,
        "Stopped TTS token generation"
    );

    Ok(audio_codes)
}

fn push_recent_token(
    worker: &Worker<'_, TtsWorker>,
    recent_tokens: &mut VecDeque<String>,
    token: LlamaToken,
) {
    if recent_tokens.len() == recent_tokens.capacity() {
        recent_tokens.pop_front();
    }
    recent_tokens.push_back(format!("{}:{}", token.0, debug_token_piece(worker, token)));
}

fn debug_token_piece(worker: &Worker<'_, TtsWorker>, token: LlamaToken) -> String {
    let token_bytes = match worker.ctx.model.token_to_piece_bytes(token, 8, true, None) {
        Err(llama_cpp_2::TokenToStringError::InsufficientBufferSpace(i)) => worker
            .ctx
            .model
            .token_to_piece_bytes(token, (-i).try_into().unwrap_or(8), true, None),
        x => x,
    };

    match token_bytes {
        Ok(bytes) => String::from_utf8_lossy(&bytes).replace('\n', "\\n"),
        Err(_) => "<detok_err>".into(),
    }
}

fn resolve_special_token(
    worker: &Worker<'_, TtsWorker>,
    piece: &str,
) -> Result<LlamaToken, TtsWorkerError> {
    let tokens = worker
        .ctx
        .model
        .str_to_token(piece, AddBos::Never)
        .map_err(|e| TtsWorkerError::InvalidRequest(format!("failed to tokenize {piece}: {e}")))?;

    match tokens.as_slice() {
        [token] => Ok(*token),
        _ => Err(TtsWorkerError::InvalidRequest(format!(
            "{piece} did not resolve to a single token"
        ))),
    }
}

fn initialize_tts_caches(worker: &mut Worker<'_, TtsWorker>) -> Result<(), TtsWorkerError> {
    let cache_start = Instant::now();
    worker.reset_context();
    let Some(prefix) = worker
        .extra
        .backend
        .cached_prompt_prefix(&TtsSpeaker::Preset(TtsSpeakerPreset::DefaultEnglishMale))
    else {
        worker.reset_context();
        return Ok(());
    };
    worker.read_string(prefix)?;

    let mut state = vec![0u8; worker.ctx.get_state_size()];
    let copied = unsafe { worker.ctx.copy_state_data(state.as_mut_ptr()) };
    state.truncate(copied);

    worker.extra.default_english_male = Some(CachedPromptState {
        state,
        n_past: worker.n_past,
    });
    info!(
        elapsed = ?cache_start.elapsed(),
        n_past = worker.n_past,
        "Cached default TTS prompt prefix"
    );

    worker.reset_context();
    Ok(())
}

/// Run the WavTokenizer vocoder and return per-token spectral embeddings.
fn run_vocoder(
    worker: &mut Worker<'_, VocoderWorker>,
    audio_codes: &[LlamaToken],
) -> Result<Vec<Vec<f32>>, TtsWorkerError> {
    let n_codes = audio_codes.len();
    if n_codes > worker.ctx.n_ctx() as usize {
        return Err(TtsWorkerError::Vocoder(format!(
            "too many audio codes ({n_codes}) for vocoder context size ({})",
            worker.ctx.n_ctx()
        )));
    }

    let _lock = GLOBAL_INFERENCE_LOCK.lock().unwrap();

    worker.big_batch.clear();
    for (i, &token) in audio_codes.iter().enumerate() {
        worker
            .big_batch
            .add(token, i as i32, &[0], true)
            .map_err(|e| TtsWorkerError::Vocoder(e.to_string()))?;
    }

    worker
        .ctx
        .encode(&mut worker.big_batch)
        .map_err(|e| TtsWorkerError::Vocoder(e.to_string()))?;

    // llama-cpp-2's embeddings_ith() uses model.n_embd() (=features_length=512) to size the
    // returned slice, but for WavTokenizer the actual per-token output is n_embd_out
    // (=embedding_length=1282). Read the correct size from GGUF metadata and use the raw
    // llama_get_embeddings_ith pointer directly with the right length.
    let n_embd_out = worker
        .ctx
        .model
        .meta_val_str("wavtokenizer-dec.embedding_length")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or_else(|| worker.ctx.model.n_embd() as usize);
    debug!(
        n_embd_features = worker.ctx.model.n_embd(),
        n_embd_out,
        "Vocoder embedding dimensions"
    );

    let mut frames = Vec::with_capacity(n_codes);
    for i in 0..n_codes {
        let emb = worker
            .ctx
            .embeddings_ith(i as i32)
            .map_err(|e| TtsWorkerError::Vocoder(e.to_string()))?;
        // llama-cpp-2's embeddings_ith() sizes the slice to model.n_embd() (=features_length=512),
        // but for WavTokenizer the actual per-token output is n_embd_out (=embedding_length=1282).
        // We extend the slice to read all n_embd_out values from the same underlying buffer.
        let extended = unsafe { std::slice::from_raw_parts(emb.as_ptr(), n_embd_out) };
        frames.push(extended.to_vec());
    }

    Ok(frames)
}

/// Reconstruct a PCM waveform from WavTokenizer spectral frames using stft-rs WOLA.
///
/// Each frame is a flat vector of [log_magnitudes | phases] (n_embd/2 values each).
/// WOLA with a Hann window matches the energy-normalized OLA used by tts.cpp.
#[cfg(test)]
fn reconstruct_audio(frames: &[Vec<f32>]) -> Result<Vec<f32>, TtsWorkerError> {
    reconstruct_audio_with_backend(frames, &VocoderBackend::WavTokenizer75)
}

fn reconstruct_audio_with_backend(
    frames: &[Vec<f32>],
    backend: &VocoderBackend,
) -> Result<Vec<f32>, TtsWorkerError> {
    if frames.is_empty() {
        return Ok(Vec::new());
    }

    let n_embd = frames[0].len();
    if n_embd == 0 || n_embd % 2 != 0 {
        return Err(TtsWorkerError::Vocoder(format!(
            "unsupported vocoder embedding width {n_embd}"
        )));
    }
    if frames.iter().any(|frame| frame.len() != n_embd) {
        return Err(TtsWorkerError::Vocoder(
            "inconsistent vocoder embedding widths across frames".into(),
        ));
    }
    let n_bins = n_embd / 2; // 641 for n_embd=1282
    let n_fft = (n_bins - 1) * 2; // 1280

    let config = StftConfigBuilder::<f32>::new()
        .fft_size(n_fft)
        .hop_size(backend.hop_size())
        .window(WindowType::Hann)
        .reconstruction_mode(ReconstructionMode::Wola)
        .build()
        .map_err(|e| TtsWorkerError::Vocoder(format!("stft-rs config error: {e}")))?;

    let istft = BatchIstft::new(config);
    let n_codes = frames.len();

    // The WOLA ISTFT only outputs n_codes * hop_size samples, but the last frame's
    // window extends (n_fft - hop_size) samples beyond that, cutting off the audio
    // tail. Append silent flush frames so the reconstruction emits the full tail.
    let n_flush = n_fft / backend.hop_size(); // 4 for WavTokenizer75 (1280/320)
    let n_frames = n_codes + n_flush;

    // stft-rs layout: data[0..n_frames*n_bins] = all real parts (row-major),
    //                 data[n_frames*n_bins..]  = all imaginary parts.
    // Flush frames remain zero (already zero-initialised).
    let mut data = vec![0.0f32; 2 * n_frames * n_bins];
    for (i, frame) in frames.iter().enumerate() {
        let log_mags = &frame[..n_bins];
        let phases = &frame[n_bins..];
        for j in 0..n_bins {
            let mag = log_mags[j].exp().min(1e2);
            let phi = phases[j];
            data[i * n_bins + j] = mag * phi.cos();
            data[n_frames * n_bins + i * n_bins + j] = mag * phi.sin();
        }
    }

    let spectrum = Spectrum { num_frames: n_frames, freq_bins: n_bins, data };
    let pcm = istft.process(&spectrum);

    Ok(pcm)
}

fn encode_wav(pcm: &[f32]) -> Result<Vec<u8>, TtsWorkerError> {
    let spec = hound::WavSpec {
        channels: 1,
        sample_rate: SAMPLE_RATE,
        bits_per_sample: 16,
        sample_format: hound::SampleFormat::Int,
    };

    let mut buffer = Vec::new();
    {
        let cursor = std::io::Cursor::new(&mut buffer);
        let mut writer = hound::WavWriter::new(cursor, spec)
            .map_err(|e| TtsWorkerError::WavEncoding(e.to_string()))?;

        for &sample in pcm {
            let s = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer
                .write_sample(s)
                .map_err(|e| TtsWorkerError::WavEncoding(e.to_string()))?;
        }

        writer
            .finalize()
            .map_err(|e| TtsWorkerError::WavEncoding(e.to_string()))?;
    }

    Ok(buffer)
}

fn default_english_male_prefix() -> String {
    let reference_text = "the<|text_sep|>overall<|text_sep|>package<|text_sep|>from<|text_sep|>\
        just<|text_sep|>two<|text_sep|>people<|text_sep|>is<|text_sep|>pretty<|text_sep|>\
        remarkable<|text_sep|>sure<|text_sep|>i<|text_sep|>have<|text_sep|>some<|text_sep|>\
        critiques<|text_sep|>about<|text_sep|>some<|text_sep|>of<|text_sep|>the<|text_sep|>\
        gameplay<|text_sep|>aspects<|text_sep|>but<|text_sep|>its<|text_sep|>still<|text_sep|>\
        really<|text_sep|>enjoyable<|text_sep|>and<|text_sep|>it<|text_sep|>looks<|text_sep|>\
        lovely";

    let reference_audio = r#"<|audio_start|>
the<|t_0.08|><|code_start|><|257|><|740|><|636|><|913|><|788|><|1703|><|code_end|>
overall<|t_0.36|><|code_start|><|127|><|201|><|191|><|774|><|700|><|532|><|1056|><|557|><|798|><|298|><|1741|><|747|><|1662|><|1617|><|1702|><|1527|><|368|><|1588|><|1049|><|1008|><|1625|><|747|><|1576|><|728|><|1019|><|1696|><|1765|><|code_end|>
package<|t_0.56|><|code_start|><|935|><|584|><|1319|><|627|><|1016|><|1491|><|1344|><|1117|><|1526|><|1040|><|239|><|1435|><|951|><|498|><|723|><|1180|><|535|><|789|><|1649|><|1637|><|78|><|465|><|1668|><|901|><|595|><|1675|><|117|><|1009|><|1667|><|320|><|840|><|79|><|507|><|1762|><|1508|><|1228|><|1768|><|802|><|1450|><|1457|><|232|><|639|><|code_end|>
from<|t_0.19|><|code_start|><|604|><|782|><|1682|><|872|><|1532|><|1600|><|1036|><|1761|><|647|><|1554|><|1371|><|653|><|1595|><|950|><|code_end|>
just<|t_0.25|><|code_start|><|1782|><|1670|><|317|><|786|><|1748|><|631|><|599|><|1155|><|1364|><|1524|><|36|><|1591|><|889|><|1535|><|541|><|440|><|1532|><|50|><|870|><|code_end|>
two<|t_0.24|><|code_start|><|1681|><|1510|><|673|><|799|><|805|><|1342|><|330|><|519|><|62|><|640|><|1138|><|565|><|1552|><|1497|><|1552|><|572|><|1715|><|1732|><|code_end|>
people<|t_0.39|><|code_start|><|593|><|274|><|136|><|740|><|691|><|633|><|1484|><|1061|><|1138|><|1485|><|344|><|428|><|397|><|1562|><|645|><|917|><|1035|><|1449|><|1669|><|487|><|442|><|1484|><|1329|><|1832|><|1704|><|600|><|761|><|653|><|269|><|code_end|>
is<|t_0.16|><|code_start|><|566|><|583|><|1755|><|646|><|1337|><|709|><|802|><|1008|><|485|><|1583|><|652|><|10|><|code_end|>
pretty<|t_0.32|><|code_start|><|1818|><|1747|><|692|><|733|><|1010|><|534|><|406|><|1697|><|1053|><|1521|><|1355|><|1274|><|816|><|1398|><|211|><|1218|><|817|><|1472|><|1703|><|686|><|13|><|822|><|445|><|1068|><|code_end|>
remarkable<|t_0.68|><|code_start|><|230|><|1048|><|1705|><|355|><|706|><|1149|><|1535|><|1787|><|1356|><|1396|><|835|><|1583|><|486|><|1249|><|286|><|937|><|1076|><|1150|><|614|><|42|><|1058|><|705|><|681|><|798|><|934|><|490|><|514|><|1399|><|572|><|1446|><|1703|><|1346|><|1040|><|1426|><|1304|><|664|><|171|><|1530|><|625|><|64|><|1708|><|1830|><|1030|><|443|><|1509|><|1063|><|1605|><|1785|><|721|><|1440|><|923|><|code_end|>
sure<|t_0.36|><|code_start|><|792|><|1780|><|923|><|1640|><|265|><|261|><|1525|><|567|><|1491|><|1250|><|1730|><|362|><|919|><|1766|><|543|><|1|><|333|><|113|><|970|><|252|><|1606|><|133|><|302|><|1810|><|1046|><|1190|><|1675|><|code_end|>
i<|t_0.08|><|code_start|><|123|><|439|><|1074|><|705|><|1799|><|637|><|code_end|>
have<|t_0.16|><|code_start|><|1509|><|599|><|518|><|1170|><|552|><|1029|><|1267|><|864|><|419|><|143|><|1061|><|0|><|code_end|>
some<|t_0.16|><|code_start|><|619|><|400|><|1270|><|62|><|1370|><|1832|><|917|><|1661|><|167|><|269|><|1366|><|1508|><|code_end|>
critiques<|t_0.60|><|code_start|><|559|><|584|><|1163|><|1129|><|1313|><|1728|><|721|><|1146|><|1093|><|577|><|928|><|27|><|630|><|1080|><|1346|><|1337|><|320|><|1382|><|1175|><|1682|><|1556|><|990|><|1683|><|860|><|1721|><|110|><|786|><|376|><|1085|><|756|><|1523|><|234|><|1334|><|1506|><|1578|><|659|><|612|><|1108|><|1466|><|1647|><|308|><|1470|><|746|><|556|><|1061|><|code_end|>
about<|t_0.29|><|code_start|><|26|><|1649|><|545|><|1367|><|1263|><|1728|><|450|><|859|><|1434|><|497|><|1220|><|1285|><|179|><|755|><|1154|><|779|><|179|><|1229|><|1213|><|922|><|1774|><|1408|><|code_end|>
some<|t_0.23|><|code_start|><|986|><|28|><|1649|><|778|><|858|><|1519|><|1|><|18|><|26|><|1042|><|1174|><|1309|><|1499|><|1712|><|1692|><|1516|><|1574|><|code_end|>
of<|t_0.07|><|code_start|><|197|><|716|><|1039|><|1662|><|64|><|code_end|>
the<|t_0.08|><|code_start|><|1811|><|1568|><|569|><|886|><|1025|><|1374|><|code_end|>
gameplay<|t_0.48|><|code_start|><|1269|><|1092|><|933|><|1362|><|1762|><|1700|><|1675|><|215|><|781|><|1086|><|461|><|838|><|1022|><|759|><|649|><|1416|><|1004|><|551|><|909|><|787|><|343|><|830|><|1391|><|1040|><|1622|><|1779|><|1360|><|1231|><|1187|><|1317|><|76|><|997|><|989|><|978|><|737|><|189|><|code_end|>
aspects<|t_0.56|><|code_start|><|1423|><|797|><|1316|><|1222|><|147|><|719|><|1347|><|386|><|1390|><|1558|><|154|><|440|><|634|><|592|><|1097|><|1718|><|712|><|763|><|1118|><|1721|><|1311|><|868|><|580|><|362|><|1435|><|868|><|247|><|221|><|886|><|1145|><|1274|><|1284|><|457|><|1043|><|1459|><|1818|><|62|><|599|><|1035|><|62|><|1649|><|778|><|code_end|>
but<|t_0.20|><|code_start|><|780|><|1825|><|1681|><|1007|><|861|><|710|><|702|><|939|><|1669|><|1491|><|613|><|1739|><|823|><|1469|><|648|><|code_end|>
its<|t_0.09|><|code_start|><|92|><|688|><|1623|><|962|><|1670|><|527|><|599|><|code_end|>
still<|t_0.27|><|code_start|><|636|><|10|><|1217|><|344|><|713|><|957|><|823|><|154|><|1649|><|1286|><|508|><|214|><|1760|><|1250|><|456|><|1352|><|1368|><|921|><|615|><|5|><|code_end|>
really<|t_0.36|><|code_start|><|55|><|420|><|1008|><|1659|><|27|><|644|><|1266|><|617|><|761|><|1712|><|109|><|1465|><|1587|><|503|><|1541|><|619|><|197|><|1019|><|817|><|269|><|377|><|362|><|1381|><|507|><|1488|><|4|><|1695|><|code_end|>
enjoyable<|t_0.49|><|code_start|><|678|><|501|><|864|><|319|><|288|><|1472|><|1341|><|686|><|562|><|1463|><|619|><|1563|><|471|><|911|><|730|><|1811|><|1006|><|520|><|861|><|1274|><|125|><|1431|><|638|><|621|><|153|><|876|><|1770|><|437|><|987|><|1653|><|1109|><|898|><|1285|><|80|><|593|><|1709|><|843|><|code_end|>
and<|t_0.15|><|code_start|><|1285|><|987|><|303|><|1037|><|730|><|1164|><|502|><|120|><|1737|><|1655|><|1318|><|code_end|>
it<|t_0.09|><|code_start|><|848|><|1366|><|395|><|1601|><|1513|><|593|><|1302|><|code_end|>
looks<|t_0.27|><|code_start|><|1281|><|1266|><|1755|><|572|><|248|><|1751|><|1257|><|695|><|1380|><|457|><|659|><|585|><|1315|><|1105|><|1776|><|736|><|24|><|736|><|654|><|1027|><|code_end|>
lovely<|t_0.56|><|code_start|><|634|><|596|><|1766|><|1556|><|1306|><|1285|><|1481|><|1721|><|1123|><|438|><|1246|><|1251|><|795|><|659|><|1381|><|1658|><|217|><|1772|><|562|><|952|><|107|><|1129|><|1112|><|467|><|550|><|1079|><|840|><|1615|><|1469|><|1380|><|168|><|917|><|836|><|1827|><|437|><|583|><|67|><|595|><|1087|><|1646|><|1493|><|1677|><|code_end|>
<|audio_end|>
"#;

    format!(
        "<|im_start|>\n<|text_start|>{reference_text}<|text_end|>\n{reference_audio}"
    )
}

fn build_default_english_male_suffix(processed_text: &str) -> String {
    format!("<|text_start|>{processed_text}<|text_end|>\n<|audio_start|>\n")
}

fn build_default_english_male_prompt(processed_text: &str) -> String {
    format!(
        "{}{}",
        default_english_male_prefix(),
        build_default_english_male_suffix(processed_text)
    )
}

fn build_outetts_v03_prompt_no_speaker(processed_text: &str) -> String {
    format!("<|im_start|>\n<|text_start|>{processed_text}<|text_end|>\n<|audio_start|>\n")
}

fn build_outetts_v03_prompt(
    processed_text: &str,
    profile: &TtsSpeakerProfile,
) -> Result<String, TtsWorkerError> {
    profile.validate()?;

    // Text section: speaker words + space + target words
    let speaker_text: String = profile
        .words
        .iter()
        .map(|w| process_text_v03(&w.word))
        .collect::<Vec<_>>()
        .join("<|space|>");

    let full_text = format!("{}<|space|>{}", speaker_text, processed_text);

    // Audio section: speaker audio codes as prefix (model will continue with target)
    let mut audio_prefix = String::new();
    for (i, word) in profile.words.iter().enumerate() {
        if i > 0 {
            audio_prefix.push_str("<|space|>\n");
        }
        audio_prefix.push_str(&serialize_outetts_v03_profile_word(word)?);
    }
    audio_prefix.push_str("<|space|>\n");

    Ok(format!(
        "<|im_start|>\n<|text_start|>{full_text}<|text_end|>\n<|audio_start|>\n{audio_prefix}"
    ))
}

fn serialize_outetts_v03_profile_word(word: &TtsSpeakerWord) -> Result<String, TtsWorkerError> {
    let normalized_word = process_text_v03(&word.word);
    if normalized_word.is_empty() {
        return Err(TtsWorkerError::InvalidRequest(format!(
            "speaker profile word {:?} becomes empty after normalization",
            word.word
        )));
    }

    let mut serialized = normalized_word;
    serialized.push_str(&format_duration_token(word.duration));
    for &code in &word.codes {
        serialized.push_str(&format!("<|{code}|>"));
    }
    Ok(serialized)
}

fn format_duration_token(duration: f32) -> String {
    let centiseconds = (duration.max(0.0) * 100.0).round() as i32;
    let centiseconds = centiseconds.clamp(0, 1000);
    format!("<|t_{}.{:02}|>", centiseconds / 100, centiseconds % 100)
}

fn process_text_v02(text: &str) -> String {
    let s = text.to_lowercase();

    let s: String = s
        .chars()
        .map(|c| match c {
            '-' | '_' | '/' | ',' | '.' | '\\' => ' ',
            c => c,
        })
        .collect();

    let s: String = s
        .chars()
        .filter(|c| c.is_ascii_alphabetic() || *c == ' ')
        .collect();

    let s = s.split_whitespace().collect::<Vec<_>>().join(" ");
    s.replace(' ', "<|text_sep|>")
}

fn process_text_v03(text: &str) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if i + 2 < chars.len() && c == '.' && chars[i + 1] == '.' && chars[i + 2] == '.' {
            normalized.push_str("<|ellipsis|>");
            pending_space = false;
            i += 3;
            continue;
        }

        let mapped = match c {
            '.' => Some("<|period|>"),
            '!' => Some("<|exclamation_mark|>"),
            '?' => Some("<|question_mark|>"),
            ',' => Some("<|comma|>"),
            '"' => Some("<|double_quote|>"),
            '„' => Some("<|low_double_quote|>"),
            '¡' => Some("<|inverted_exclamation|>"),
            '¿' => Some("<|inverted_question|>"),
            '…' => Some("<|ellipsis|>"),
            '。' => Some("<|cjk_period|>"),
            '！' => Some("<|cjk_exclamation|>"),
            '？' => Some("<|cjk_question|>"),
            '，' => Some("<|cjk_comma|>"),
            '؟' => Some("<|arabic_question|>"),
            _ => None,
        };

        if let Some(token) = mapped {
            normalized.push_str(token);
            pending_space = false;
            i += 1;
            continue;
        }

        match c {
            '-' | '_' | '/' | '\\' => pending_space = !normalized.is_empty(),
            c if c.is_whitespace() => pending_space = !normalized.is_empty(),
            c if c.is_alphabetic() => {
                if pending_space && !normalized.is_empty() {
                    normalized.push(' ');
                }
                normalized.push(c);
                pending_space = false;
            }
            _ => {}
        }

        i += 1;
    }

    normalized.replace(' ', "<|space|>")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{llm, test_utils};

    fn test_tts_model_path() -> String {
        std::env::var("TEST_TTS_MODEL").unwrap_or_else(|_| "tts-model.gguf".to_string())
    }

    fn test_vocoder_model_path() -> String {
        std::env::var("TEST_VOCODER_MODEL").unwrap_or_else(|_| "vocoder.gguf".to_string())
    }

    #[test]
    fn test_process_text_v02() {
        let processed = process_text_v02("Hello, Rust-world 123!");
        assert_eq!(processed, "hello<|text_sep|>rust<|text_sep|>world");
    }

    #[test]
    fn test_process_text_v03() {
        let processed = process_text_v03("Hello, world! Really...");
        assert_eq!(
            processed,
            "hello<|comma|><|space|>world<|exclamation_mark|><|space|>really<|ellipsis|>"
        );
    }

    #[test]
    fn test_request_builds_prompt() -> Result<(), Box<dyn std::error::Error>> {
        let prepared = TtsRequest::new("Hello world")
            .prepare_with_backend(&TextModelBackend::OuteTtsV02)?;
        let prompt = build_default_english_male_prompt(&prepared.processed_text);
        assert!(prompt.contains("<|audio_start|>"));
        assert!(prompt.contains("<|text_start|>hello<|text_sep|>world<|text_end|>"));
        Ok(())
    }

    #[test]
    fn test_outetts_v03_no_speaker_builds_prompt() -> Result<(), Box<dyn std::error::Error>> {
        // v0.3 without a speaker profile should produce a valid prompt (default voice)
        let prepared = TtsRequest::new("Hello world")
            .prepare_with_backend(&TextModelBackend::OuteTtsV03)?;
        let prompt = TextModelBackend::OuteTtsV03.prompt(&prepared)?;
        assert!(prompt.contains("<|text_start|>hello<|space|>world<|text_end|>"));
        assert!(prompt.contains("<|audio_start|>"));
        assert!(!prompt.contains("<|voice_characteristic_start|>"));
        Ok(())
    }

    #[test]
    fn test_outetts_v03_profile_prompt_builds() -> Result<(), Box<dyn std::error::Error>> {
        let profile = TtsSpeakerProfile {
            text: "hello world".into(),
            words: vec![
                TtsSpeakerWord {
                    word: "hello".into(),
                    duration: 0.52,
                    codes: vec![551, 552],
                },
                TtsSpeakerWord {
                    word: "world".into(),
                    duration: 0.25,
                    codes: vec![3],
                },
            ],
        };
        let prepared = TtsRequest::new("Hello world")
            .with_speaker_profile(profile)
            .prepare_with_backend(&TextModelBackend::OuteTtsV03)?;
        let prompt = TextModelBackend::OuteTtsV03.prompt(&prepared)?;
        // Speaker words + target words appear together in the text section
        assert!(prompt.contains("<|text_start|>hello<|space|>world<|space|>hello<|space|>world<|text_end|>"));
        // Speaker audio codes appear in the audio section as a prefix
        assert!(prompt.contains("<|audio_start|>"));
        assert!(prompt.contains("hello<|t_0.52|><|551|><|552|>"));
        assert!(prompt.contains("world<|t_0.25|><|3|>"));
        // The old voice_characteristic_start format is no longer used
        assert!(!prompt.contains("<|voice_characteristic_start|>"));
        Ok(())
    }

    #[test]
    fn test_speaker_profile_json_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let json = r#"{"text":"hello","words":[{"word":"hello","duration":0.52,"codes":[551,552]}]}"#;
        let profile = TtsSpeakerProfile::from_json_str(json)?;
        assert_eq!(profile.words.len(), 1);
        assert_eq!(profile.words[0].codes, vec![551, 552]);
        Ok(())
    }

    #[test]
    fn test_reconstruct_audio_rejects_bad_dims() {
        let err = reconstruct_audio(&[vec![0.0; 7]]).unwrap_err();
        assert!(matches!(err, TtsWorkerError::Vocoder(_)));
    }

    #[test]
    fn test_reconstruct_audio_rejects_inconsistent_dims() {
        let err = reconstruct_audio(&[vec![0.0; 8], vec![0.0; 10]]).unwrap_err();
        assert!(matches!(err, TtsWorkerError::Vocoder(_)));
    }

    #[test]
    #[ignore = "requires TEST_TTS_MODEL and TEST_VOCODER_MODEL GGUF assets"]
    fn test_tts_synthesize() -> Result<(), Box<dyn std::error::Error>> {
        test_utils::init_test_tracing();

        let tts_model = Arc::new(llm::get_model(&test_tts_model_path(), true, None)?);
        let voc_model = Arc::new(llm::get_model(&test_vocoder_model_path(), true, None)?);
        let tts = Tts::new(tts_model, voc_model, 4096);

        let wav_bytes = tts.synthesize("Hello world")?;

        assert!(!wav_bytes.is_empty(), "WAV output should not be empty");

        let cursor = std::io::Cursor::new(&wav_bytes);
        let reader = hound::WavReader::new(cursor)?;
        let spec = reader.spec();
        assert_eq!(spec.sample_rate, SAMPLE_RATE);
        assert_eq!(spec.channels, 1);
        assert_eq!(spec.bits_per_sample, 16);

        Ok(())
    }
}
