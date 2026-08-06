use std::sync::Arc;

use godot::builtin::PackedByteArray;
use godot::prelude::*;

use nobodywho::errors::SpeechToTextError;
use nobodywho::onnx::Device;
use nobodywho::speech_to_text::{SpeechToText as CoreStt, SpeechToTextConfig, WhisperConfig};

use crate::chat::NobodyWhoTokenStream;
use crate::convert::{dict_get, resolve_godot_path};
use crate::task::{on_blocking_thread, task};

/// A speech-to-text transcriber (Whisper ONNX). Build it with the async
/// factory, then transcribe files or raw PCM:
///
/// ```gdscript
/// var stt = await NobodyWhoSpeechToText.create("hf://onnx-community/whisper-base", {
///     "language": "en", "device": "auto",
/// })
/// var text = await stt.transcribe_file("res://recording.wav")
/// var stream = stt.transcribe_file_stream("res://recording.wav")
/// while true:
///     var piece = await stream.next_token()
///     if piece == null: break
///     $Label.text += piece
/// ```
///
/// `source` is a HuggingFace repo (`hf://owner/repo`) or a local directory.
/// Resolves to the `NobodyWhoSpeechToText`, or null on failure.
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoSpeechToText {
    stt: Arc<CoreStt>,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoSpeechToText {
    /// Create a transcriber asynchronously (the model downloads/loads off the
    /// main thread). `await create(...)` resolves to the transcriber, or null.
    ///
    /// `config` is a Dictionary with optional keys:
    /// - `"language"` (String): ISO 639-1 code (e.g. `"en"`); `""`/omit =
    ///   auto-detect.
    /// - `"quantization"` (String): ONNX precision variant — `"default"`,
    ///   `"fp16"`, `"int8"`, `"uint8"`, `"bnb4"`, `"q4"`, `"q4f16"`;
    ///   `""`/omit = core default (`"q4"`, falling back to `"default"`).
    /// - `"device"` (String): `"auto"` (default), `"cpu"`, or `"cuda"`.
    #[func]
    fn create(source: GString, config: VarDictionary) -> Variant {
        let source = resolve_godot_path(&source);
        let cfg = match parse_stt_config(&source, &config) {
            Ok(c) => c,
            Err(e) => {
                godot_error!("NobodyWhoSpeechToText.create: {e}");
                return Variant::nil();
            }
        };
        task(async move {
            let result = on_blocking_thread(move || CoreStt::with_device(cfg.0, cfg.1)).await;
            match result {
                Some(Ok(stt)) => Gd::from_init_fn(|base| Self {
                    stt: Arc::new(stt),
                    base,
                })
                .to_variant(),
                Some(Err(e)) => {
                    godot_error!("Failed to create SpeechToText: {}", render_stt_error(&e));
                    Variant::nil()
                }
                None => {
                    godot_error!("SpeechToText worker init panicked");
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Transcribe an audio file (WAV / MP3 / …). `await transcribe_file(...)`
    /// resolves to the full transcript String, or null on failure.
    #[func]
    fn transcribe_file(&self, path: GString) -> Variant {
        let stt = self.stt.clone();
        let path = resolve_godot_path(&path);
        task(async move {
            match stt.transcribe_file_async(path).await {
                Ok(text) => GString::from(&text).to_variant(),
                Err(e) => {
                    godot_error!("transcribe_file failed: {}", render_stt_error(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Transcribe raw i16 PCM samples (e.g. from a microphone).
    /// `samples` is a `PackedByteArray` of interleaved little-endian i16
    /// samples. `sample_rate` is the capture rate in Hz (e.g. 44100).
    /// `await transcribe_pcm(...)` resolves to the full transcript, or null.
    #[func]
    fn transcribe_pcm(&self, samples: PackedByteArray, sample_rate: i64) -> Variant {
        let stt = self.stt.clone();
        let samples = pcm_bytes_to_i16(samples.as_slice());
        let sample_rate = sample_rate.max(0) as u32;
        task(async move {
            match stt.transcribe_pcm_async(samples, sample_rate).await {
                Ok(text) => GString::from(&text).to_variant(),
                Err(e) => {
                    godot_error!("transcribe_pcm failed: {}", render_stt_error(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Transcribe an audio file, streaming tokens as they're decoded. Returns
    /// a `NobodyWhoTokenStream` immediately; pull tokens via `next_token()`,
    /// or await the full text via `completed()`. Returns null on failure to
    /// start (e.g. the file can't be read).
    #[func]
    fn transcribe_file_stream(&self, path: GString) -> Variant {
        let stt = self.stt.clone();
        let path = resolve_godot_path(&path);
        match stt.transcribe_file_stream_async(path) {
            Ok(stream) => NobodyWhoTokenStream::wrap_stt(stream).to_variant(),
            Err(e) => {
                godot_error!("transcribe_file_stream failed: {}", render_stt_error(&e));
                Variant::nil()
            }
        }
    }

    /// Transcribe raw i16 PCM samples, streaming tokens. `samples` is a
    /// `PackedByteArray` of LE i16 samples. Returns a `NobodyWhoTokenStream`
    /// immediately, or null on failure to start.
    #[func]
    fn transcribe_pcm_stream(&self, samples: PackedByteArray, sample_rate: i64) -> Variant {
        let stt = self.stt.clone();
        let samples = pcm_bytes_to_i16(samples.as_slice());
        let sample_rate = sample_rate.max(0) as u32;
        match stt.transcribe_pcm_stream_async(samples, sample_rate) {
            Ok(stream) => NobodyWhoTokenStream::wrap_stt(stream).to_variant(),
            Err(e) => {
                godot_error!("transcribe_pcm_stream failed: {}", render_stt_error(&e));
                Variant::nil()
            }
        }
    }
}

/// Parse the GDScript config Dictionary into a core `SpeechToTextConfig` +
/// `Device`.
fn parse_stt_config(
    source: &str,
    config: &VarDictionary,
) -> Result<(SpeechToTextConfig, Device), String> {
    let mut cfg = WhisperConfig::new(source);
    cfg.language = dict_get::<GString>(config, "language")?
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    if let Some(q) = dict_get::<GString>(config, "quantization")?.filter(|s| !s.is_empty()) {
        cfg.quantization = q.to_string();
    }
    let device = dict_get::<GString>(config, "device")?
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .as_deref()
        .map(parse_device)
        .transpose()?
        .unwrap_or(Device::Auto);
    Ok((SpeechToTextConfig::Whisper(cfg), device))
}

fn parse_device(s: &str) -> Result<Device, String> {
    match s.to_ascii_lowercase().as_str() {
        "auto" => Ok(Device::Auto),
        "cpu" => Ok(Device::Cpu),
        "cuda" => Ok(Device::Cuda),
        _ => Err(format!(
            "device must be 'auto', 'cpu', or 'cuda', got '{s}'"
        )),
    }
}

/// Reinterpret a byte slice as little-endian i16 samples.
fn pcm_bytes_to_i16(bytes: &[u8]) -> Vec<i16> {
    bytes
        .chunks_exact(2)
        .map(|b| i16::from_le_bytes([b[0], b[1]]))
        .collect()
}

/// `SpeechToTextError` is `thiserror::Error` but not `miette::Diagnostic`, so
/// plain `to_string()` (mirrors the Python binding).
fn render_stt_error(e: &SpeechToTextError) -> String {
    e.to_string()
}
