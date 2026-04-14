/// Text-to-speech synthesis using OuteTTS + WavTokenizer.
///
/// The default public API accepts plain text and formats the required OuteTTS
/// prompt internally. [`Tts::synthesize_prompt`] is available as an escape hatch
/// for callers that need full control over the prompt.
mod v02;
mod v03;

use crate::errors::{DecodingError, InitWorkerError, TtsWorkerError};
use crate::llm::{self, Worker, WorkerGuard, GLOBAL_INFERENCE_LOCK};
use crate::memory;
use crate::sampler_config::SamplerPresets;
use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::context::params::LlamaPoolingType;
use llama_cpp_2::context::LlamaContext;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::AddBos;
use llama_cpp_2::token::LlamaToken;
use serde::{Deserialize, Serialize};
use stft_rs::{BatchIstft, ReconstructionMode, Spectrum, StftConfigBuilder, WindowType};
use std::collections::VecDeque;
use std::sync::Arc;
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
            TextModelBackend::OuteTtsV02 => v02::process_text(text),
            TextModelBackend::OuteTtsV03 => v03::process_text(text),
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
        match self {
            TextModelBackend::OuteTtsV02 => v02::cached_prompt_prefix(speaker),
            TextModelBackend::OuteTtsV03 => None,
        }
    }

    fn prompt(&self, request: &PreparedTtsRequest) -> Result<String, TtsWorkerError> {
        match self {
            TextModelBackend::OuteTtsV02 => v02::prompt(request),
            TextModelBackend::OuteTtsV03 => v03::prompt(request),
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
pub(crate) struct PreparedTtsRequest {
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

    pub(crate) fn validate(&self) -> Result<(), TtsWorkerError> {
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

    pub fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsWorkerError> {
        futures::executor::block_on(self.async_handle.synthesize(text))
    }

    pub fn synthesize_request(&self, request: TtsRequest) -> Result<Vec<u8>, TtsWorkerError> {
        futures::executor::block_on(self.async_handle.synthesize_request(request))
    }

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

    pub async fn synthesize(&self, text: impl Into<String>) -> Result<Vec<u8>, TtsWorkerError> {
        self.synthesize_request(TtsRequest::new(text)).await
    }

    pub async fn synthesize_request(&self, request: TtsRequest) -> Result<Vec<u8>, TtsWorkerError> {
        let (result_tx, mut result_rx) = tokio::sync::mpsc::channel(1);
        self.guard
            .send(TtsMsg::SynthesizePrepared(
                request.prepare_with_backend(&self.backend.text_model)?,
                result_tx,
            ));
        result_rx.recv().await.ok_or(TtsWorkerError::NoResponse)?
    }

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
                let suffix = v02::build_default_english_male_suffix(&request.processed_text);
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
        let extended = unsafe { std::slice::from_raw_parts(emb.as_ptr(), n_embd_out) };
        frames.push(extended.to_vec());
    }

    Ok(frames)
}

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
    let n_bins = n_embd / 2;
    let n_fft = (n_bins - 1) * 2;

    let config = StftConfigBuilder::<f32>::new()
        .fft_size(n_fft)
        .hop_size(backend.hop_size())
        .window(WindowType::Hann)
        .reconstruction_mode(ReconstructionMode::Wola)
        .build()
        .map_err(|e| TtsWorkerError::Vocoder(format!("stft-rs config error: {e}")))?;

    let istft = BatchIstft::new(config);
    let n_codes = frames.len();

    let n_flush = n_fft / backend.hop_size();
    let n_frames = n_codes + n_flush;

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
        let processed = v02::process_text("Hello, Rust-world 123!");
        assert_eq!(processed, "hello<|text_sep|>rust<|text_sep|>world");
    }

    #[test]
    fn test_process_text_v03() {
        let processed = v03::process_text("Hello, world! Really...");
        assert_eq!(
            processed,
            "hello<|comma|><|space|>world<|exclamation_mark|><|space|>really<|ellipsis|>"
        );
    }

    #[test]
    fn test_request_builds_prompt() -> Result<(), Box<dyn std::error::Error>> {
        let prepared = TtsRequest::new("Hello world")
            .prepare_with_backend(&TextModelBackend::OuteTtsV02)?;
        let prompt = v02::prompt(&prepared)?;
        assert!(prompt.contains("<|audio_start|>"));
        assert!(prompt.contains("<|text_start|>hello<|text_sep|>world<|text_end|>"));
        Ok(())
    }

    #[test]
    fn test_outetts_v03_no_speaker_builds_prompt() -> Result<(), Box<dyn std::error::Error>> {
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
        assert!(prompt.contains("<|text_start|>hello<|space|>world<|space|>hello<|space|>world<|text_end|>"));
        assert!(prompt.contains("<|audio_start|>"));
        assert!(prompt.contains("hello<|t_0.52|><|551|><|552|>"));
        assert!(prompt.contains("world<|t_0.25|><|3|>"));
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
