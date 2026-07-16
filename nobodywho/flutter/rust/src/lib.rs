use flutter_rust_bridge::DartFnFuture;
use nobodywho::chat::Asset;
use std::collections::HashMap;
use std::sync::Arc;
// ^ in general I've only done fully-qualified imports, but these things need to be imported to
// satisfy some frb macros

mod frb_generated;
mod parse;

pub use nobodywho::tool_calling::ToolCall;

#[flutter_rust_bridge::frb]
pub enum Message {
    User {
        content: String,
        #[frb(default = "const []")]
        assets: Vec<nobodywho::chat::Asset>,
    },
    Assistant {
        content: String,
        tool_calls: Option<Vec<ToolCall>>,
    },
    System {
        content: String,
    },
    Tool {
        name: String,
        content: String,
    },
}

impl From<nobodywho::chat::Message> for Message {
    fn from(msg: nobodywho::chat::Message) -> Self {
        match msg {
            nobodywho::chat::Message::User { content, assets } => Message::User {
                content: content.to_string(),
                assets,
            },
            nobodywho::chat::Message::Assistant {
                content,
                tool_calls,
            } => Message::Assistant {
                content,
                tool_calls,
            },
            nobodywho::chat::Message::System { content } => Message::System { content },
            nobodywho::chat::Message::Tool { name, content } => Message::Tool { name, content },
        }
    }
}

impl From<Message> for nobodywho::chat::Message {
    fn from(msg: Message) -> Self {
        match msg {
            Message::User { content, assets } => nobodywho::chat::Message::User {
                content: nobodywho::chat::MessageContent::Text(content),
                assets,
            },
            Message::Assistant {
                content,
                tool_calls,
            } => nobodywho::chat::Message::Assistant {
                content,
                tool_calls,
            },
            Message::System { content } => nobodywho::chat::Message::System { content },
            Message::Tool { name, content } => nobodywho::chat::Message::Tool { name, content },
        }
    }
}

/// A part of a multimodal prompt. Use [`PromptPart::Text`] for text,
/// [`PromptPart::Image`] for images, and [`PromptPart::Audio`] for audio clips.
pub enum PromptPart {
    Text { content: String },
    Image { path: String },
    Audio { path: String },
}

/// No-op default for `onDownloadProgress` callbacks. Not meant to be called by
/// users — it exists so we can reference it as a const tear-off in the Dart
/// `#[frb(default = "noopOnDownloadProgress")]` attribute (closure literals
/// aren't const in Dart, but top-level function tear-offs are).
#[flutter_rust_bridge::frb(sync, positional)]
pub fn noop_on_download_progress(_downloaded: i64, _total: i64) {}

/// Bridge a Dart async progress callback into the synchronous closure core
/// expects, with ~10 Hz throttling provided by `throttled_progress_callback`.
///
/// The Dart callback takes `(i64, i64)` rather than `(u64, u64)` so that frb
/// generates a plain `int` Dart parameter; i64::MAX is ~9.2 EB which is far
/// beyond any practical model file size.
fn wrap_progress<F>(callback: F) -> nobodywho::llm::DownloadProgressCallback
where
    F: Fn(i64, i64) -> flutter_rust_bridge::DartFnFuture<()> + Send + Sync + 'static,
{
    nobodywho::llm::throttled_progress_callback(move |downloaded, total| {
        futures::executor::block_on(callback(downloaded as i64, total as i64));
    })
}

fn parse_tts_architecture(
    architecture: Option<String>,
) -> Result<Option<nobodywho::tts::TtsArchitecture>, String> {
    architecture
        .as_deref()
        .map(str::parse)
        .transpose()
        .map_err(|()| "architecture must be one of 'kokoro' or 'supertonic'".to_string())
}

fn tts_device_from_use_gpu(use_gpu: bool) -> nobodywho::tts::TtsDevice {
    if use_gpu {
        nobodywho::tts::TtsDevice::Auto
    } else {
        nobodywho::tts::TtsDevice::Cpu
    }
}

fn build_tts_config(
    source: String,
    architecture: Option<String>,
    voice: Option<String>,
    language: Option<String>,
    speed: Option<f32>,
    steps: Option<u32>,
    silence_duration: Option<f32>,
) -> Result<nobodywho::tts::TtsConfig, String> {
    let architecture = parse_tts_architecture(architecture)?;
    let mut config = nobodywho::tts::TtsConfig::from_source(&source, architecture).ok_or_else(|| {
        "architecture is required for unknown TTS sources; pass architecture='kokoro' or architecture='supertonic'"
            .to_string()
    })?;

    match &mut config {
        nobodywho::tts::TtsConfig::Kokoro(config) => {
            if let Some(voice) = voice {
                config.voice = voice;
            }
            if let Some(language) = language {
                config.language = language;
            }
            if let Some(speed) = speed {
                config.speed = speed;
            }
        }
        nobodywho::tts::TtsConfig::Supertonic(config) => {
            if let Some(voice) = voice {
                config.voice = voice;
            }
            if let Some(language) = language {
                config.language = language;
            }
            if let Some(speed) = speed {
                config.speed = speed;
            }
            if let Some(steps) = steps {
                config.steps = steps as usize;
            }
            if let Some(silence_duration) = silence_duration {
                config.silence_duration = silence_duration;
            }
        }
    }
    Ok(config)
}

#[flutter_rust_bridge::frb(mirror(ToolCall))]
pub struct _ToolCall {
    pub name: String,
    pub arguments: serde_json::Value, // Flexible structure for arbitrary arguments
}

/// Helper function to convert ToolCall arguments to a JSON string.
/// This is needed because serde_json::Value becomes an opaque type in Dart.
#[flutter_rust_bridge::frb(sync)]
pub fn tool_call_arguments_json(tool_call: &ToolCall) -> Result<String, String> {
    serde_json::to_string(&tool_call.arguments).map_err(|e| e.to_string())
}

#[flutter_rust_bridge::frb(opaque)]
pub struct Model {
    model: Arc<nobodywho::llm::Model>,
}

impl Model {
    /// Load a model from a local path, HuggingFace path (`huggingface:owner/repo/file.gguf`),
    /// HTTPS URL, or `auto` for memory-based selection. Remote models are downloaded
    /// and cached automatically.
    ///
    /// Args:
    ///     model_path: Path, URL, or `auto`.
    ///     on_download_progress: Invoked with `(downloadedBytes, totalBytes)` while a
    ///         remote model is being downloaded. Throttled to ~10 Hz with a guaranteed
    ///         final emit on completion. Not invoked for cached/local files.
    ///     use_gpu: Whether to use GPU acceleration. Defaults to true.
    ///     projection_model_path: Optional path to a `.mmproj` file for vision/multimodal models.
    ///     draft_model_path: Optional path to an MTP draft-heads gguf. Loading it lets
    ///         chats built from this model opt into MTP speculative decoding.
    pub fn max_ctx(&self) -> u32 {
        self.model.max_ctx()
    }

    #[flutter_rust_bridge::frb]
    pub fn load(
        model_path: &str,
        #[frb(default = "noopOnDownloadProgress")] on_download_progress: impl Fn(i64, i64) -> DartFnFuture<()>
            + Send
            + Sync
            + 'static,
        #[frb(default = true)] use_gpu: bool,
        #[frb(default = "null")] projection_model_path: Option<String>,
        #[frb(default = "null")] draft_model_path: Option<String>,
    ) -> Result<Self, String> {
        let model = nobodywho::llm::get_model(
            model_path,
            use_gpu,
            projection_model_path.as_deref(),
            draft_model_path.as_deref(),
            Some(wrap_progress(on_download_progress)),
        )
        .map_err(|e| nobodywho::render_miette(&e))?;
        Ok(Self {
            model: Arc::new(model),
        })
    }
}

/// Download a model from a remote URL or HuggingFace path and return the local file path.
///
/// Use this when you need custom headers, e.g. for gated models that require authentication.
/// For unauthenticated downloads, pass the URL directly to `Model.load`.
///
/// Args:
///     model_path: Path or URL to a GGUF model file.
///     headers: Optional HTTP headers (e.g. `{"Authorization": "Bearer hf_..."}`).
///     on_download_progress: Invoked with `(downloadedBytes, totalBytes)` while downloading.
#[flutter_rust_bridge::frb]
pub fn download_model(
    model_path: String,
    #[frb(default = "const {}")] headers: HashMap<String, String>,
    #[frb(default = "noopOnDownloadProgress")] on_download_progress: impl Fn(i64, i64) -> DartFnFuture<()>
        + Send
        + Sync
        + 'static,
) -> Result<String, String> {
    let headers_vec: Vec<(String, String)> = headers.into_iter().collect();
    nobodywho::llm::download_model(
        &model_path,
        headers_vec,
        Some(wrap_progress(on_download_progress)),
    )
    .map(|p| p.to_string_lossy().into_owned())
    .map_err(|e| nobodywho::render_miette(&e))
}

#[flutter_rust_bridge::frb(opaque)]
pub struct Tts {
    handle: nobodywho::tts::Tts,
}

impl Tts {
    /// Create a TTS synthesizer.
    ///
    /// Args:
    ///     source: Local model directory or HuggingFace repo (`hf://owner/repo`).
    ///     architecture: "kokoro" or "supertonic". Required for local or unknown sources.
    ///     voice: Voice name. Architecture default is used when omitted.
    ///     language: Language code. Architecture default is used when omitted.
    ///     speed: Speaking speed. Architecture default is used when omitted.
    ///     steps: Supertonic denoising steps. Ignored by Kokoro.
    ///     silence_duration: Supertonic silence between chunks in seconds.
    ///     use_gpu: Whether to use GPU acceleration. Defaults to true.
    #[flutter_rust_bridge::frb]
    pub fn load(
        source: String,
        #[frb(default = "null")] architecture: Option<String>,
        #[frb(default = "null")] voice: Option<String>,
        #[frb(default = "null")] language: Option<String>,
        #[frb(default = "null")] speed: Option<f32>,
        #[frb(default = "null")] steps: Option<u32>,
        #[frb(default = "null")] silence_duration: Option<f32>,
        #[frb(default = true)] use_gpu: bool,
    ) -> Result<Self, String> {
        let config = build_tts_config(
            source,
            architecture,
            voice,
            language,
            speed,
            steps,
            silence_duration,
        )?;
        let device = tts_device_from_use_gpu(use_gpu);
        let handle = nobodywho::tts::Tts::with_device(config, device)
            .map_err(|e| nobodywho::render_miette(&e))?;
        Ok(Self { handle })
    }

    /// Synthesize text and return WAV bytes.
    pub async fn synthesize(&self, text: String) -> Result<Vec<u8>, String> {
        self.handle
            .synthesize_async(text)
            .await
            .map_err(|e| nobodywho::render_miette(&e))
    }
}

pub struct ChatStats {
    pub context_size: u32,
    pub context_used: u32,
}

#[flutter_rust_bridge::frb(opaque)]
pub struct RustChat {
    chat: nobodywho::chat::ChatHandleAsync,
}

impl RustChat {
    /// Create chat from existing model.
    ///
    /// For vision/multimodal models, load the model with image ingestion enabled first:
    /// ```dart
    /// final model = Model.load("model.gguf", projectionModelPath: "mmproj.gguf");
    /// final chat = Chat(model: model);
    /// ```
    ///
    /// Args:
    ///     model: A Model instance (may include a projection model for vision)
    ///     system_prompt: System message to guide the model's behavior
    ///     context_size: Context size (maximum conversation length in tokens)
    ///     tools: List of Tool instances the model can call
    ///     sampler: SamplerConfig for token selection. Pass null to use default sampler.
    #[flutter_rust_bridge::frb(sync)]
    pub fn new(
        model: &Model,
        #[frb(default = "null")] system_prompt: Option<String>,
        #[frb(default = 4096)] context_size: u32,
        #[frb(default = "null")] allow_thinking: Option<bool>,
        #[frb(default = "const {}")] template_variables: HashMap<String, bool>,
        #[frb(default = "const []")] tools: Vec<RustTool>,
        #[frb(default = "null")] sampler: Option<SamplerConfig>,
    ) -> Result<Self, String> {
        let sampler_config = sampler.map(|s| s.sampler_config).unwrap_or_default();

        // Handle deprecated allow_thinking parameter
        let mut template_vars = template_variables;
        if let Some(allow) = allow_thinking {
            tracing::warn!(
                "allow_thinking parameter is deprecated. Use template_variables={{\"enable_thinking\": {}}} instead.",
                allow
            );
            template_vars.insert("enable_thinking".to_string(), allow);
        }

        let chat = nobodywho::chat::ChatBuilder::new(Arc::clone(&model.model))
            .with_context_size(context_size)
            .with_template_variables(template_vars)
            .with_tools(tools.into_iter().map(|t| t.tool).collect())
            .with_system_prompt(system_prompt)
            .with_sampler(sampler_config)
            .build_async()
            .map_err(|e| nobodywho::render_miette(&e))?;

        Ok(Self { chat })
    }

    /// Create chat directly from a model path. This is async as it loads a model
    ///
    /// Args:
    ///     model_path: Path to GGUF model file
    ///     on_download_progress: Invoked with `(downloadedBytes, totalBytes)` while a
    ///         remote model is being downloaded. Throttled to ~10 Hz with a guaranteed
    ///         final emit on completion. Not invoked for cached/local files.
    ///     projection_model_path: Path to a .mmproj file for vision/multimodal models
    ///     system_prompt: System message to guide the model's behavior
    ///     context_size: Context size (maximum conversation length in tokens)
    ///     tools: List of Tool instances the model can call
    ///     sampler: SamplerConfig for token selection. Pass null to use default sampler.
    ///     use_gpu: Whether to use GPU acceleration. Defaults to true.
    #[flutter_rust_bridge::frb]
    #[allow(clippy::too_many_arguments)]
    pub fn from_path(
        model_path: &str,
        #[frb(default = "noopOnDownloadProgress")] on_download_progress: impl Fn(i64, i64) -> DartFnFuture<()>
            + Send
            + Sync
            + 'static,
        #[frb(default = "null")] projection_model_path: Option<String>,
        #[frb(default = "null")] system_prompt: Option<String>,
        #[frb(default = 4096)] context_size: u32,
        #[frb(default = "null")] allow_thinking: Option<bool>,
        #[frb(default = "const {}")] template_variables: HashMap<String, bool>,
        #[frb(default = "const []")] tools: Vec<RustTool>,
        #[frb(default = "null")] sampler: Option<SamplerConfig>,
        #[frb(default = true)] use_gpu: bool,
    ) -> Result<Self, String> {
        let model = nobodywho::llm::get_model(
            model_path,
            use_gpu,
            projection_model_path.as_deref(),
            None,
            false,
            Some(wrap_progress(on_download_progress)),
        )
        .map_err(|e| nobodywho::render_miette(&e))?;
        let sampler_config = sampler.map(|s| s.sampler_config).unwrap_or_default();

        // Handle deprecated allow_thinking parameter
        let mut template_vars = template_variables;
        if let Some(allow) = allow_thinking {
            tracing::warn!(
                "allow_thinking parameter is deprecated. Use template_variables={{\"enable_thinking\": {}}} instead.",
                allow
            );
            template_vars.insert("enable_thinking".to_string(), allow);
        }

        let chat = nobodywho::chat::ChatBuilder::new(Arc::new(model))
            .with_context_size(context_size)
            .with_template_variables(template_vars)
            .with_tools(tools.into_iter().map(|t| t.tool).collect())
            .with_system_prompt(system_prompt)
            .with_sampler(sampler_config)
            .build_async()
            .map_err(|e| nobodywho::render_miette(&e))?;
        Ok(Self { chat })
    }

    #[flutter_rust_bridge::frb(sync, positional)]
    pub fn ask(&self, message: String) -> RustTokenStream {
        RustTokenStream {
            stream: self.chat.ask(message),
        }
    }

    /// Send a multimodal prompt (text + images) and get a stream of response tokens.
    ///
    /// Args:
    ///     parts: List of PromptPart (text or image) making up the prompt
    #[flutter_rust_bridge::frb(sync)]
    pub fn ask_with_prompt(&self, parts: Vec<PromptPart>) -> RustTokenStream {
        let prompt = nobodywho::tokenizer::Prompt::new(parts.into_iter().map(|part| match part {
            PromptPart::Text { content } => nobodywho::tokenizer::PromptPart::Text(content),
            PromptPart::Image { path } => nobodywho::tokenizer::PromptPart::Image(path.into()),
            PromptPart::Audio { path } => nobodywho::tokenizer::PromptPart::Audio(path.into()),
        }));

        RustTokenStream {
            stream: self.chat.ask(prompt),
        }
    }

    /// Send a raw JSON prompt and get a stream of response tokens.
    /// The JSON string is parsed and passed as a structured content field.
    /// Called by the Dart SDK layer — the json argument is always valid JSON.
    #[flutter_rust_bridge::frb(sync)]
    pub fn ask_with_json_prompt(&self, json: String) -> RustTokenStream {
        let value: serde_json::Value = serde_json::from_str(&json)
            .expect("ask_with_json_prompt: invalid JSON (this is a bug in the SDK)");
        RustTokenStream {
            stream: self
                .chat
                .ask(nobodywho::tokenizer::Prompt::from_json(value)),
        }
    }

    pub async fn get_chat_history(&self) -> Result<Vec<Message>, nobodywho::errors::GetterError> {
        self.chat
            .get_chat_history()
            .await
            .map(|msgs| msgs.into_iter().map(Message::from).collect())
    }

    pub async fn set_chat_history(
        &self,
        messages: Vec<Message>,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat
            .set_chat_history(
                messages
                    .into_iter()
                    .map(nobodywho::chat::Message::from)
                    .collect(),
            )
            .await
    }

    pub async fn set_sampler_config(
        &self,
        sampler_config: SamplerConfig,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat
            .set_sampler_config(sampler_config.sampler_config)
            .await
    }

    pub async fn get_sampler_config(
        &self,
    ) -> Result<SamplerConfig, nobodywho::errors::GetterError> {
        self.chat
            .get_sampler_config()
            .await
            .map(|sampler_config| SamplerConfig { sampler_config })
    }

    pub async fn reset_context(
        &self,
        system_prompt: Option<String>,
        tools: Vec<RustTool>,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat
            .reset_chat(system_prompt, tools.into_iter().map(|t| t.tool).collect())
            .await
    }

    pub async fn reset_history(&self) -> Result<(), nobodywho::errors::SetterError> {
        self.chat.reset_history().await
    }

    #[deprecated(note = "Use setTemplateVariable(\"enable_thinking\", value) instead")]
    pub async fn set_allow_thinking(
        &self,
        allow_thinking: bool,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat
            .set_template_variable("enable_thinking".to_string(), allow_thinking)
            .await
    }

    pub async fn set_system_prompt(
        &self,
        system_prompt: Option<String>,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat.set_system_prompt(system_prompt).await
    }

    pub async fn get_system_prompt(
        &self,
    ) -> Result<Option<String>, nobodywho::errors::GetterError> {
        self.chat.get_system_prompt().await
    }

    pub async fn tokenize(
        &self,
        message: String,
    ) -> Result<Vec<Option<i32>>, nobodywho::errors::TokenizeError> {
        self.chat.tokenize(message).await
    }

    pub async fn tokenize_with_prompt(
        &self,
        parts: Vec<PromptPart>,
    ) -> Result<Vec<Option<i32>>, nobodywho::errors::TokenizeError> {
        let prompt = nobodywho::tokenizer::Prompt::new(parts.into_iter().map(|part| match part {
            PromptPart::Text { content } => nobodywho::tokenizer::PromptPart::Text(content),
            PromptPart::Image { path } => nobodywho::tokenizer::PromptPart::Image(path.into()),
            PromptPart::Audio { path } => nobodywho::tokenizer::PromptPart::Audio(path.into()),
        }));
        self.chat.tokenize(prompt).await
    }

    pub async fn get_stats(&self) -> Result<ChatStats, nobodywho::errors::GetterError> {
        self.chat.get_stats().await.map(|s| ChatStats {
            context_size: s.context_size,
            context_used: s.context_used,
        })
    }

    pub async fn set_tools(
        &self,
        tools: Vec<RustTool>,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat
            .set_tools(tools.into_iter().map(|t| t.tool).collect())
            .await
    }

    pub async fn set_template_variable(
        &self,
        name: String,
        value: bool,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat.set_template_variable(name, value).await
    }

    pub async fn set_template_variables(
        &self,
        variables: HashMap<String, bool>,
    ) -> Result<(), nobodywho::errors::SetterError> {
        self.chat.set_template_variables(variables).await
    }

    pub async fn get_template_variables(
        &self,
    ) -> Result<HashMap<String, bool>, nobodywho::errors::GetterError> {
        self.chat.get_template_variables().await
    }

    #[flutter_rust_bridge::frb(sync)]
    pub fn stop_generation(&self) {
        self.chat.stop_generation()
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct RustTokenStream {
    stream: nobodywho::chat::TokenStreamAsync,
}

impl RustTokenStream {
    pub async fn iter(
        &mut self,
        sink: crate::frb_generated::StreamSink<String>,
    ) -> Result<(), String> {
        loop {
            match self.stream.next_token().await {
                Ok(Some(token)) => sink.add(token).map_err(|e| e.to_string())?,
                Ok(None) => break,
                Err(e) => return Err(nobodywho::render_miette(&e)),
            }
        }
        Ok(())
    }

    pub async fn next_token(&mut self) -> Result<Option<String>, String> {
        self.stream
            .next_token()
            .await
            .map_err(|e| nobodywho::render_miette(&e))
    }

    pub async fn completed(&mut self) -> Result<String, nobodywho::errors::CompletionError> {
        self.stream.completed().await
    }
}

// ---------------------------------------------------------------------------
// STT
// ---------------------------------------------------------------------------

/// Speech-to-text handle. Create with `RustSTT.new_()`, then call
/// `transcribeFile` or `transcribePcm` to get a `RustSTTStream`.
#[flutter_rust_bridge::frb(opaque)]
pub struct RustSTT {
    stt: nobodywho::stt::Stt,
}

impl RustSTT {
    /// Create an STT handle.
    /// `source` — HuggingFace repo (`hf://owner/repo`, e.g. `"hf://onnx-community/whisper-base"`) or local dir.
    /// `language` — ISO 639-1 code (e.g. `"en"`); pass `None` for auto-detect.
    /// `quantization` — ONNX precision variant to download and load: one of
    /// `"default"`, `"fp16"`, `"int8"`, `"uint8"`, `"bnb4"`, `"q4"`, `"q4f16"`, `"quantized"`; pass `None`
    /// to use `"default"`.
    #[flutter_rust_bridge::frb(sync)]
    pub fn new_(
        source: String,
        #[frb(default = "null")] language: Option<String>,
        #[frb(default = "null")] quantization: Option<String>,
    ) -> Result<Self, String> {
        let mut cfg = nobodywho::stt::WhisperConfig::new(&source);
        cfg.language = language;
        if let Some(quantization) = quantization {
            cfg.quantization = quantization;
        }
        let stt = nobodywho::stt::Stt::new(nobodywho::stt::SttConfig::Whisper(cfg))
            .map_err(|e| e.to_string())?;
        Ok(Self { stt })
    }

    /// Transcribe an audio file (WAV / MP3).
    #[flutter_rust_bridge::frb(sync)]
    pub fn transcribe_file(&self, path: String) -> Result<RustSTTStream, String> {
        let stream = self
            .stt
            .transcribe_file_stream_async(path)
            .map_err(|e| e.to_string())?;
        Ok(RustSTTStream { stream })
    }

    /// Transcribe raw i16 PCM samples (e.g. from `mic_stream`).
    /// `sample_rate` is the capture rate in Hz; resampled to 16 kHz internally.
    #[flutter_rust_bridge::frb(sync)]
    pub fn transcribe_pcm(
        &self,
        samples: Vec<i16>,
        sample_rate: u32,
    ) -> Result<RustSTTStream, String> {
        let stream = self
            .stt
            .transcribe_pcm_stream_async(samples, sample_rate)
            .map_err(|e| e.to_string())?;
        Ok(RustSTTStream { stream })
    }
}

/// A stream of transcript tokens. Consume via `iter(sink)`, `nextToken()`, or `completed()`.
#[flutter_rust_bridge::frb(opaque)]
pub struct RustSTTStream {
    stream: nobodywho::stt::TokenStreamAsync<nobodywho::errors::SttError>,
}

impl RustSTTStream {
    /// Stream all tokens into `sink`. Resolves when transcription is complete.
    pub async fn iter(
        &mut self,
        sink: crate::frb_generated::StreamSink<String>,
    ) -> Result<(), String> {
        loop {
            match self.stream.next_token().await {
                Ok(Some(piece)) => sink.add(piece).map_err(|e| e.to_string())?,
                Ok(None) => break,
                Err(e) => return Err(e.to_string()),
            }
        }
        Ok(())
    }

    pub async fn next_token(&mut self) -> Result<Option<String>, String> {
        self.stream.next_token().await.map_err(|e| e.to_string())
    }

    pub async fn completed(&mut self) -> Result<String, String> {
        self.stream.completed().await.map_err(|e| e.to_string())
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct Encoder {
    handle: nobodywho::encoder::EncoderAsync,
}

impl Encoder {
    #[flutter_rust_bridge::frb(sync)]
    pub fn new(model: &Model, #[frb(default = 4096)] n_ctx: u32) -> Self {
        let handle = nobodywho::encoder::EncoderAsync::new(Arc::clone(&model.model), n_ctx);
        Self { handle }
    }

    /// Load an embedding model from a local path, HuggingFace path, or HTTPS URL.
    ///
    /// Args:
    ///     model_path: Path or URL to a GGUF embedding model file.
    ///     on_download_progress: Invoked with `(downloadedBytes, totalBytes)` while a
    ///         remote model is being downloaded. Throttled to ~10 Hz with a guaranteed
    ///         final emit on completion. Not invoked for cached/local files.
    ///     n_ctx: Context size for the encoder. Defaults to 4096.
    ///     use_gpu: Whether to use GPU acceleration. Defaults to true.
    #[flutter_rust_bridge::frb]
    pub fn from_path(
        model_path: &str,
        #[frb(default = "noopOnDownloadProgress")] on_download_progress: impl Fn(i64, i64) -> DartFnFuture<()>
            + Send
            + Sync
            + 'static,
        #[frb(default = 4096)] n_ctx: u32,
        #[frb(default = true)] use_gpu: bool,
    ) -> Result<Self, String> {
        let model = nobodywho::llm::get_model(
            model_path,
            use_gpu,
            None,
            None,
            false,
            Some(wrap_progress(on_download_progress)),
        )
        .map_err(|e| nobodywho::render_miette(&e))?;
        let handle = nobodywho::encoder::EncoderAsync::new(Arc::new(model), n_ctx);

        Ok(Self { handle })
    }

    pub async fn encode(
        &self,
        text: String,
    ) -> Result<Vec<f32>, nobodywho::errors::EncoderWorkerError> {
        self.handle.encode(text).await
    }
}

#[flutter_rust_bridge::frb(opaque)]
pub struct CrossEncoder {
    handle: nobodywho::crossencoder::CrossEncoderAsync,
}

impl CrossEncoder {
    #[flutter_rust_bridge::frb(sync)]
    pub fn new(model: &Model, #[frb(default = 4096)] n_ctx: u32) -> Self {
        let handle =
            nobodywho::crossencoder::CrossEncoderAsync::new(Arc::clone(&model.model), n_ctx);
        Self { handle }
    }

    /// Load a cross-encoder model from a local path, HuggingFace path, or HTTPS URL.
    ///
    /// Args:
    ///     model_path: Path or URL to a GGUF cross-encoder model file.
    ///     on_download_progress: Invoked with `(downloadedBytes, totalBytes)` while a
    ///         remote model is being downloaded. Throttled to ~10 Hz with a guaranteed
    ///         final emit on completion. Not invoked for cached/local files.
    ///     n_ctx: Context size for the cross-encoder. Defaults to 4096.
    ///     use_gpu: Whether to use GPU acceleration. Defaults to true.
    #[flutter_rust_bridge::frb]
    pub fn from_path(
        model_path: &str,
        #[frb(default = "noopOnDownloadProgress")] on_download_progress: impl Fn(i64, i64) -> DartFnFuture<()>
            + Send
            + Sync
            + 'static,
        #[frb(default = 4096)] n_ctx: u32,
        #[frb(default = true)] use_gpu: bool,
    ) -> Result<Self, String> {
        let model = nobodywho::llm::get_model(
            model_path,
            use_gpu,
            None,
            None,
            false,
            Some(wrap_progress(on_download_progress)),
        )
        .map_err(|e| nobodywho::render_miette(&e))?;
        let handle = nobodywho::crossencoder::CrossEncoderAsync::new(Arc::new(model), n_ctx);
        Ok(Self { handle })
    }
    pub async fn rank(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<f32>, nobodywho::errors::CrossEncoderWorkerError> {
        self.handle.rank(query, documents).await
    }

    pub async fn rank_and_sort(
        &self,
        query: String,
        documents: Vec<String>,
    ) -> Result<Vec<(String, f32)>, nobodywho::errors::CrossEncoderWorkerError> {
        self.handle.rank_and_sort(query, documents).await
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn cosine_similarity(a: Vec<f32>, b: Vec<f32>) -> f32 {
    nobodywho::encoder::cosine_similarity(&a, &b)
}

/// Returns every cached .gguf model paired with its byte size.
///
/// Each entry is (absolute path, size in bytes).
#[flutter_rust_bridge::frb(sync)]
pub fn get_cached_models() -> Result<Vec<(String, usize)>, String> {
    nobodywho::llm::get_cached_models()
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|(path, size)| Ok((path.to_string_lossy().into_owned(), size)))
        .collect()
}

#[flutter_rust_bridge::frb(opaque)]
pub struct RustTool {
    tool: nobodywho::tool_calling::Tool,
    schema: serde_json::Value,
}

impl RustTool {
    /// Get the JSON schema for this tool's parameters as a string
    #[flutter_rust_bridge::frb(sync)]
    pub fn get_schema_json(&self) -> String {
        self.schema.to_string()
    }
}

#[flutter_rust_bridge::frb(sync)]
pub fn new_tool_impl(
    function: impl Fn(String) -> DartFnFuture<String> + Send + Sync + 'static,
    name: String,
    description: String,
    runtime_type: String,
    parameter_descriptions: &std::collections::HashMap<String, String>,
) -> Result<RustTool, String> {
    let json_schema = dart_function_type_to_json_schema(&runtime_type, parameter_descriptions)?;

    // TODO: this seems to silently block forever if we get a type error on the dart side.
    //       it'd be *much* better to fail hard and throw a dart exception if that happens
    //       we might have to fix it on the dart side...
    let sync_callback = move |json: serde_json::Value| {
        futures::executor::block_on(async { function(json.to_string()).await })
    };

    let tool = nobodywho::tool_calling::Tool::new(
        name,
        description,
        json_schema.clone(),
        std::sync::Arc::new(sync_callback),
    );

    Ok(RustTool {
        tool,
        schema: json_schema,
    })
}

#[flutter_rust_bridge::frb(sync)]
pub fn new_bash_tool(max_commands: Option<usize>) -> RustTool {
    let tool = nobodywho::tool_calling::Tool::bash(max_commands);
    let schema = tool.json_schema.clone();
    RustTool { tool, schema }
}

#[flutter_rust_bridge::frb(sync)]
pub fn new_python_tool(
    max_duration_secs: Option<u64>,
    max_memory_bytes: Option<usize>,
    max_recursion_depth: Option<usize>,
) -> RustTool {
    let tool = nobodywho::tool_calling::Tool::python(
        max_duration_secs.map(std::time::Duration::from_secs),
        max_memory_bytes,
        max_recursion_depth,
    );
    let schema = tool.json_schema.clone();
    RustTool { tool, schema }
}

/// Converts a Dart function runtimeType string directly to a JSON schema
/// Example input: "({required String a, required int b}) => String" or "() => String"
/// Returns a JSON schema for the function parameters
/// XXX: this whole function is vibe-coded, and hence the implementation is pretty messy...
#[tracing::instrument(ret, level = "debug")]
fn dart_function_type_to_json_schema(
    runtime_type: &str,
    parameter_descriptions: &std::collections::HashMap<String, String>,
) -> Result<serde_json::Value, String> {
    let (parsed_parameters, return_type) = match parse::runtime_type_parser(runtime_type) {
        Ok((_, (pp, rt))) => (pp, rt),
        Err(nom::Err::Error(e)) => {
            if runtime_type.starts_with('(')
                && !runtime_type.starts_with("({")
                && !runtime_type.starts_with("()")
            {
                return Err(format!(
                    "Tool function `{runtime_type}` uses positional parameters, which are not supported. \
                     All parameters must be named and marked `required`. \
                     Example: `({{required String text}}) => String`."
                ));
            }
            if parse::type_parser(e.input).is_ok() && parse::parameter_parser(e.input).is_err() {
                return Err(format!(
                    "Tool function `{runtime_type}` has parameters without the `required` keyword, which is not supported. \
                     All parameters must be marked `required`. \
                     Example: `({{required String text}}) => String`."
                ));
            }
            return Err(format!(
                "Error while parsing tool function. Parsing failed at: {} . \
                 Supported types: String, int, double, num, bool, DateTime, \
                 List<T>, Set<T>, Map<String, T>.",
                e.input
            ));
        }
        Err(nom::Err::Failure(e)) => {
            return Err(format!(
                "Error while parsing runtime_type. Input: {}",
                e.input
            ))
        }
        Err(_) => return Err("Something has gone horribly wrong while parsing!".into()),
    };

    if parse::return_type_parser(return_type).is_err() {
        tracing::warn!("Return type of this tool should be `String or Future<String>`. Anything else will be cast to string, which might lead to unexpected results.")
    }

    let mut properties = serde_json::Map::new();
    let mut required = Vec::new();

    for (parameter_name, mut parameter_type) in parsed_parameters {
        required.push(parameter_name);

        if let Some(description) = parameter_descriptions.get(parameter_name) {
            if let Some(obj) = parameter_type.as_object_mut() {
                obj.insert(
                    "description".to_string(),
                    serde_json::Value::String(description.to_string()),
                );
            }
        }
        properties.insert(parameter_name.into(), parameter_type);
    }

    tracing::debug!(
        "\n\n{}\n\n",
        serde_json::json!({
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false
        })
    );

    Ok(serde_json::json!({
        "type": "object",
        "properties": properties,
        "required": required,
        "additionalProperties": false
    }))
}

// TODO:
// - blocking ask
// - embeddings
// - cross encoder

/// `SamplerConfig` contains the configuration for a token sampler. The mechanism by which
/// NobodyWho will sample a token from the probability distribution, to include in the
/// generation result.
/// A `SamplerConfig` can be constructed either using a preset function from the `SamplerPresets`
/// class, or by manually constructing a sampler chain using the `SamplerBuilder` class.
/// `SamplerConfig` supports serialization to/from JSON via `toJson()` and `fromJson()`.
#[flutter_rust_bridge::frb(
    opaque,
    dart_code = "
  @override
  String toString() => toJson();
"
)]
#[derive(Clone, Default)]
pub struct SamplerConfig {
    sampler_config: nobodywho::sampler::SamplerConfig,
}

impl SamplerConfig {
    /// Serialize the sampler configuration to a JSON string.
    #[flutter_rust_bridge::frb(sync)]
    pub fn to_json(&self) -> Result<String, String> {
        serde_json::to_string(&self.sampler_config).map_err(|e| e.to_string())
    }

    /// Deserialize a sampler configuration from a JSON string.
    #[flutter_rust_bridge::frb(sync)]
    pub fn from_json(json_str: &str) -> Result<Self, String> {
        let sampler_config: nobodywho::sampler::SamplerConfig =
            serde_json::from_str(json_str).map_err(|e| e.to_string())?;
        Ok(Self { sampler_config })
    }
}

fn shift_step(builder: SamplerBuilder, step: nobodywho::sampler::ShiftStep) -> SamplerBuilder {
    SamplerBuilder {
        inner: builder.inner.shift(step),
    }
}

fn sample_step(builder: SamplerBuilder, step: nobodywho::sampler::SampleStep) -> SamplerConfig {
    SamplerConfig {
        sampler_config: builder.inner.sample(step),
    }
}

/// `SamplerBuilder` is used to manually construct a sampler chain.
/// A sampler chain consists of any number of probability-shifting steps, and a single sampling step.
/// Probability-shifting steps are operations that transform the probability distribution of next
/// tokens, as generated by the model. E.g. the top_k step will zero the probability of all tokens
/// that aren't among the top K most probable (where K is some integer).
/// A sampling step is a final step that selects a single token from the probability distribution
/// that results from applying all of the probability-shifting steps in order.
/// E.g. the `dist` sampling step selects a token with weighted randomness, and the
/// `greedy` sampling step always selects the most probable.
#[flutter_rust_bridge::frb(opaque)]
#[derive(Clone)]
pub struct SamplerBuilder {
    inner: nobodywho::sampler::SamplerBuilder,
}

impl SamplerBuilder {
    /// Create a new SamplerBuilder to construct a custom sampler chain.
    #[flutter_rust_bridge::frb(sync)]
    pub fn new() -> Self {
        Self {
            inner: nobodywho::sampler::SamplerBuilder::new(),
        }
    }

    /// Keep only the top K most probable tokens. Typical values: 40-50.
    ///
    /// Args:
    ///     top_k: Number of top tokens to keep
    #[flutter_rust_bridge::frb(sync)]
    pub fn top_k(&self, top_k: i32) -> Self {
        shift_step(self.clone(), nobodywho::sampler::ShiftStep::TopK { top_k })
    }

    /// Keep tokens whose cumulative probability is below top_p. Typical values: 0.9-0.95.
    ///
    /// Args:
    ///     top_p: Cumulative probability threshold (0.0 to 1.0)
    ///     min_keep: Minimum number of tokens to always keep
    #[flutter_rust_bridge::frb(sync)]
    pub fn top_p(&self, top_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::TopP { top_p, min_keep },
        )
    }

    /// Keep tokens with probability above min_p * (probability of most likely token).
    ///
    /// Args:
    ///     min_p: Minimum relative probability threshold (0.0 to 1.0). Typical: 0.05-0.1.
    ///     min_keep: Minimum number of tokens to always keep
    #[flutter_rust_bridge::frb(sync)]
    pub fn min_p(&self, min_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::MinP { min_p, min_keep },
        )
    }

    /// XTC (eXclude Top Choices) sampler that probabilistically excludes high-probability tokens.
    /// This can increase output diversity by sometimes forcing the model to pick less obvious tokens.
    ///
    /// Args:
    ///     xtc_probability: Probability of applying XTC on each token (0.0 to 1.0)
    ///     xtc_threshold: Tokens with probability above this threshold may be excluded (0.0 to 1.0)
    ///     min_keep: Minimum number of tokens to always keep (prevents excluding all tokens)
    #[flutter_rust_bridge::frb(sync)]
    pub fn xtc(&self, xtc_probability: f32, xtc_threshold: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::XTC {
                xtc_probability,
                xtc_threshold,
                min_keep,
            },
        )
    }

    /// Set the RNG seed used by random samplers (`dist`, `mirostat_v1`, `mirostat_v2`, `xtc`).
    /// `greedy` ignores it. If unset, a default seed is used.
    #[flutter_rust_bridge::frb(sync)]
    pub fn seed(&self, seed: u32) -> Self {
        SamplerBuilder {
            inner: self.inner.clone().seed(seed),
        }
    }

    /// Typical sampling: keeps tokens close to expected information content.
    ///
    /// Args:
    ///     typ_p: Typical probability mass (0.0 to 1.0). Typical: 0.9.
    ///     min_keep: Minimum number of tokens to always keep
    #[flutter_rust_bridge::frb(sync)]
    pub fn typical_p(&self, typ_p: f32, min_keep: u32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::TypicalP { typ_p, min_keep },
        )
    }

    /// Apply temperature scaling to the probability distribution.
    ///
    /// Args:
    ///     temperature: Temperature value (0.0 = deterministic, 1.0 = unchanged, >1.0 = more random)
    #[flutter_rust_bridge::frb(sync)]
    pub fn temperature(&self, temperature: f32) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::Temperature { temperature },
        )
    }

    /// Deprecated: Use `SamplerPresets.constrain_with_grammar()` instead. It accepts both Lark and GBNF strings.
    #[flutter_rust_bridge::frb(sync)]
    #[deprecated(
        note = "Use SamplerPresets.constrainWithGrammar() instead. It accepts both Lark and GBNF strings."
    )]
    #[allow(deprecated)]
    pub fn grammar(&self, grammar: String, trigger_on: Option<String>, root: String) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::Grammar {
                grammar,
                trigger_on,
                root,
            },
        )
    }

    /// DRY (Don't Repeat Yourself) sampler to reduce repetition.
    ///
    /// Args:
    ///     multiplier: Penalty strength multiplier
    ///     base: Base penalty value
    ///     allowed_length: Maximum allowed repetition length
    ///     penalty_last_n: Number of recent tokens to consider
    ///     seq_breakers: List of strings that break repetition sequences
    #[flutter_rust_bridge::frb(sync)]
    pub fn dry(
        &self,
        multiplier: f32,
        base: f32,
        allowed_length: i32,
        penalty_last_n: i32,
        seq_breakers: Vec<String>,
    ) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::DRY {
                multiplier,
                base,
                allowed_length,
                penalty_last_n,
                seq_breakers,
            },
        )
    }

    /// Apply repetition penalties to discourage repeated tokens.
    ///
    /// Args:
    ///     penalty_last_n: Number of recent tokens to penalize (0 = disable)
    ///     penalty_repeat: Base repetition penalty (1.0 = no penalty, >1.0 = penalize)
    ///     penalty_freq: Frequency penalty based on token occurrence count
    ///     penalty_present: Presence penalty for any token that appeared before
    #[flutter_rust_bridge::frb(sync)]
    pub fn penalties(
        &self,
        penalty_last_n: i32,
        penalty_repeat: f32,
        penalty_freq: f32,
        penalty_present: f32,
    ) -> Self {
        shift_step(
            self.clone(),
            nobodywho::sampler::ShiftStep::Penalties {
                penalty_last_n,
                penalty_repeat,
                penalty_freq,
                penalty_present,
            },
        )
    }

    /// Sample from the probability distribution (weighted random selection).
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    #[flutter_rust_bridge::frb(sync)]
    pub fn dist(&self) -> SamplerConfig {
        sample_step(self.clone(), nobodywho::sampler::SampleStep::Dist)
    }

    /// Always select the most probable token (deterministic).
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    #[flutter_rust_bridge::frb(sync)]
    pub fn greedy(&self) -> SamplerConfig {
        sample_step(self.clone(), nobodywho::sampler::SampleStep::Greedy)
    }

    /// Use Mirostat v1 algorithm for perplexity-controlled sampling.
    /// Mirostat dynamically adjusts sampling to maintain a target "surprise" level,
    /// producing more coherent output than fixed temperature. Good for long-form generation.
    ///
    /// Args:
    ///     tau: Target perplexity/surprise value (typically 3.0-5.0; lower = more focused)
    ///     eta: Learning rate for perplexity adjustment (typically 0.1)
    ///     m: Number of candidates to consider (typically 100)
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    #[flutter_rust_bridge::frb(sync)]
    pub fn mirostat_v1(&self, tau: f32, eta: f32, m: i32) -> SamplerConfig {
        sample_step(
            self.clone(),
            nobodywho::sampler::SampleStep::MirostatV1 { tau, eta, m },
        )
    }

    /// Use Mirostat v2 algorithm for perplexity-controlled sampling.
    /// Mirostat v2 is a simplified version of Mirostat that's often preferred.
    /// It dynamically adjusts sampling to maintain a target "surprise" level.
    ///
    /// Args:
    ///     tau: Target perplexity/surprise value (typically 3.0-5.0; lower = more focused)
    ///     eta: Learning rate for perplexity adjustment (typically 0.1)
    ///
    /// Returns:
    ///     A complete SamplerConfig ready to use
    #[flutter_rust_bridge::frb(sync)]
    pub fn mirostat_v2(&self, tau: f32, eta: f32) -> SamplerConfig {
        sample_step(
            self.clone(),
            nobodywho::sampler::SampleStep::MirostatV2 { tau, eta },
        )
    }
}

/// `SamplerPresets` is a static class which contains a bunch of functions to easily create a
/// `SamplerConfig` from some pre-defined sampler chain.
/// E.g. `SamplerPresets.temperature(0.8)` will return a `SamplerConfig` with temperature=0.8.
#[flutter_rust_bridge::frb(opaque)]
pub struct SamplerPresets {
    _private: (),
}

impl SamplerPresets {
    /// Get the default sampler configuration.
    #[flutter_rust_bridge::frb(sync)]
    pub fn default_sampler() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerConfig::default(),
        }
    }

    /// Create a sampler with top-k filtering only.
    ///
    /// Args:
    ///     top_k: Number of top tokens to keep
    #[flutter_rust_bridge::frb(sync)]
    pub fn top_k(top_k: i32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::top_k(top_k),
        }
    }

    /// Create a sampler with nucleus (top-p) sampling.
    ///
    /// Args:
    ///     top_p: Cumulative probability threshold (0.0 to 1.0)
    #[flutter_rust_bridge::frb(sync)]
    pub fn top_p(top_p: f32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::top_p(top_p),
        }
    }

    /// Create a greedy sampler (always picks most probable token).
    #[flutter_rust_bridge::frb(sync)]
    pub fn greedy() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::greedy(),
        }
    }

    /// Create a sampler with temperature scaling.
    ///
    /// Args:
    ///     temperature: Temperature value (lower = more focused, higher = more random)
    #[flutter_rust_bridge::frb(sync)]
    pub fn temperature(temperature: f32) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::temperature(temperature),
        }
    }

    /// Create a DRY sampler preset to reduce repetition.
    #[flutter_rust_bridge::frb(sync)]
    pub fn dry() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::dry(),
        }
    }

    /// Create a sampler that constrains output to a JSON schema via llguidance.
    #[flutter_rust_bridge::frb(sync)]
    pub fn constrain_with_json_schema(schema: String) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::constrain_with_json_schema(schema),
        }
    }

    /// Create a sampler that constrains output to a regular expression via llguidance.
    #[flutter_rust_bridge::frb(sync)]
    pub fn constrain_with_regex(pattern: String) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::constrain_with_regex(pattern),
        }
    }

    /// Create a sampler that constrains output using a Lark grammar via llguidance.
    #[flutter_rust_bridge::frb(sync)]
    pub fn constrain_with_grammar(grammar: String) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::constrain_with_grammar(grammar),
        }
    }

    /// Deprecated: Use `SamplerPresets.constrain_with_json_schema()` instead.
    #[flutter_rust_bridge::frb(sync)]
    #[allow(deprecated)]
    pub fn json() -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::json(),
        }
    }

    /// Deprecated: Use `SamplerPresets.constrain_with_grammar()` instead.
    #[flutter_rust_bridge::frb(sync)]
    #[deprecated(note = "Use SamplerPresets.constrain_with_grammar() instead")]
    #[allow(deprecated)]
    pub fn grammar(grammar: String) -> SamplerConfig {
        SamplerConfig {
            sampler_config: nobodywho::sampler::SamplerPresets::grammar(grammar),
        }
    }
}

#[flutter_rust_bridge::frb(init)]
pub fn init_app() {
    // send llamacpp logs into tracing
    nobodywho::send_llamacpp_logs_to_tracing();

    // send logs to the appropriate places for android, ios and wasm
    flutter_rust_bridge::setup_default_user_utils();

    let log_level = if cfg!(debug_assertions) {
        tracing::Level::DEBUG
    } else {
        tracing::Level::INFO
    };

    tracing_subscriber::fmt()
        .with_max_level(log_level)
        .with_timer(tracing_subscriber::fmt::time::uptime())
        .with_span_events(tracing_subscriber::fmt::format::FmtSpan::CLOSE)
        .try_init()
        .ok();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dart_function_to_schema() {
        let schema = dart_function_type_to_json_schema(
            "({required String name, required int age, required List<String> tags}) => String",
            &std::collections::HashMap::new(),
        )
        .unwrap();
        let expected = serde_json::json!({
            "type": "object",
            "properties": {
                "name": { "type": "string" },
                "age": { "type": "integer" },
                "tags": {
                    "type": "array",
                    "items": { "type": "string" }
                }
            },
            "required": ["name", "age", "tags"],
            "additionalProperties": false
        });
        assert_eq!(schema, expected);
    }

    #[test]
    fn test_single_string_parameter() {
        let dart_type = "({required String text}) => Future<String>";
        let json_schema =
            dart_function_type_to_json_schema(dart_type, &std::collections::HashMap::new())
                .unwrap();
        let expected = serde_json::json!({
            "type": "object",
            "properties": { "text": { "type": "string" } },
            "required": [ "text" ],
            "additionalProperties": false,
        });
        assert_eq!(json_schema, expected);
    }

    #[test]
    fn test_no_parameters() {
        let dart_type = "() => String";
        let json_schema =
            dart_function_type_to_json_schema(dart_type, &std::collections::HashMap::new())
                .unwrap();
        let expected = serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        });
        assert_eq!(json_schema, expected);
    }

    #[test]
    fn test_no_parameters_async() {
        let dart_type = "() => Future<String>";
        let json_schema =
            dart_function_type_to_json_schema(dart_type, &std::collections::HashMap::new())
                .unwrap();
        let expected = serde_json::json!({
            "type": "object",
            "properties": {},
            "required": [],
            "additionalProperties": false
        });
        assert_eq!(json_schema, expected);
    }

    #[test]
    fn test_positional_params_error() {
        let err = dart_function_type_to_json_schema(
            "(String text) => String",
            &std::collections::HashMap::new(),
        )
        .unwrap_err();
        assert!(
            err.contains("positional parameters"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn test_missing_required_keyword_error() {
        let err = dart_function_type_to_json_schema(
            "({String text}) => String",
            &std::collections::HashMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("required"), "unexpected error: {err}");
    }

    #[test]
    fn test_unsupported_type_error() {
        let err = dart_function_type_to_json_schema(
            "({required Foo x}) => String",
            &std::collections::HashMap::new(),
        )
        .unwrap_err();
        assert!(err.contains("Supported types"), "unexpected error: {err}");
    }
}
