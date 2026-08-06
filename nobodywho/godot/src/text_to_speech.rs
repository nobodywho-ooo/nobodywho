use std::sync::Arc;

use godot::builtin::PackedByteArray;
use godot::prelude::*;

use nobodywho::errors::TextToSpeechError;
use nobodywho::text_to_speech::{
    TextToSpeech as CoreTts, TextToSpeechArchitecture, TextToSpeechConfig, TextToSpeechDevice,
};

use crate::convert::{dict_get, resolve_godot_path};
use crate::task::{on_blocking_thread, task};

/// A text-to-speech synthesizer. Build it with the async factory:
///
/// ```gdscript
/// var tts = await NobodyWhoTextToSpeech.create("hf://hexgrad/Kokoro-82M", {
///     "voice": "af_heart", "language": "en", "device": "auto",
/// })
/// var wav: PackedByteArray = await tts.synthesize("Hello, world!")
/// ```
///
/// `source` is a local model directory, a Godot path (`res://` / `user://`),
/// or a HuggingFace repo (`hf://owner/repo`). `architecture` is inferred
/// from the source when omitted; set it to `"kokoro"`, `"pocket-tts"`, or
/// `"supertonic"` for unrecognizable sources. Resolves to the
/// `NobodyWhoTextToSpeech`, or null on failure (with a `godot_error!`).
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoTextToSpeech {
    tts: Arc<CoreTts>,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoTextToSpeech {
    /// Create a synthesizer asynchronously (the model loads off the main
    /// thread). `await create(...)` resolves to the synthesizer, or null.
    ///
    /// `config` is a Dictionary with optional keys (all default to the
    /// architecture's defaults when omitted/empty):
    /// - `"architecture"` (String): `""` (infer), `"kokoro"`, `"pocket-tts"`,
    ///   `"supertonic"`.
    /// - `"voice"` (String), `"language"` (String), `"speed"` (float, 0 =
    ///   default).
    /// - `"steps"` (int, 0 = default): Supertonic denoising steps or Pocket
    ///   TTS LSD steps.
    /// - `"silence_duration"` (float, <0 = default): Supertonic silence
    ///   between chunks.
    /// - `"precision"` (String): Pocket TTS `"int8"` or `"fp32"`.
    /// - `"temperature"` (float, <0 = default): Pocket TTS generation
    ///   temperature.
    /// - `"huggingface_token"` (String): Pocket TTS gated-voice token; `""`
    ///   uses the `HF_TOKEN` env var.
    /// - `"device"` (String): `"auto"` (default), `"cpu"`, or `"cuda"`.
    #[func]
    fn create(source: GString, config: VarDictionary) -> Variant {
        let source = resolve_godot_path(&source);
        let cfg = match parse_tts_config(&source, &config) {
            Ok(c) => c,
            Err(e) => {
                godot_error!("NobodyWhoTextToSpeech.create: {e}");
                return Variant::nil();
            }
        };
        task(async move {
            let result = on_blocking_thread(move || CoreTts::with_device(cfg.0, cfg.1)).await;
            match result {
                Some(Ok(tts)) => Gd::from_init_fn(|base| Self {
                    tts: Arc::new(tts),
                    base,
                })
                .to_variant(),
                Some(Err(e)) => {
                    godot_error!("Failed to create TextToSpeech: {}", render_tts_error(&e));
                    Variant::nil()
                }
                None => {
                    godot_error!("TextToSpeech worker init panicked");
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Synthesize `text` to WAV bytes. `await synthesize(...)` resolves to a
    /// `PackedByteArray` (a complete WAV container), or null on failure.
    /// Wrap the bytes in an `AudioStreamWAV` on the GDScript side to play
    /// them.
    #[func]
    fn synthesize(&self, text: GString) -> Variant {
        let tts = self.tts.clone();
        let text = text.to_string();
        task(async move {
            match tts.synthesize_async(text).await {
                Ok(wav) => PackedByteArray::from(wav).to_variant(),
                Err(e) => {
                    godot_error!("synthesize failed: {}", render_tts_error(&e));
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }
}

/// Parse the GDScript config Dictionary into a core `TextToSpeechConfig` +
/// `Device`. Empty strings / 0 / -1 mean "use the architecture default".
fn parse_tts_config(
    source: &str,
    config: &VarDictionary,
) -> Result<(TextToSpeechConfig, TextToSpeechDevice), String> {
    let architecture = dict_get::<GString>(config, "architecture")?
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .as_deref()
        .map(parse_architecture)
        .transpose()?;
    let voice = str_opt(config, "voice")?;
    let language = str_opt(config, "language")?;
    let speed = dict_get::<f32>(config, "speed")?.filter(|&s| s > 0.0);
    let steps = dict_get::<i64>(config, "steps")?
        .filter(|&s| s > 0)
        .map(|s| s as usize);
    let silence_duration = dict_get::<f32>(config, "silence_duration")?.filter(|&s| s >= 0.0);
    let precision = dict_get::<GString>(config, "precision")?
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let temperature = dict_get::<f32>(config, "temperature")?.filter(|&t| t >= 0.0);
    let huggingface_token = str_opt(config, "huggingface_token")?;
    let device = dict_get::<GString>(config, "device")?
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .as_deref()
        .map(parse_device)
        .transpose()?
        .unwrap_or(TextToSpeechDevice::Auto);

    let mut cfg = TextToSpeechConfig::from_source(source, architecture)
        .ok_or_else(|| {
            "architecture is required for unknown sources; set it to 'kokoro', 'pocket-tts', or 'supertonic'".to_string()
        })?;
    match &mut cfg {
        TextToSpeechConfig::Kokoro(c) => {
            if let Some(v) = voice {
                c.voice = v;
            }
            if let Some(l) = language {
                c.language = l;
            }
            if let Some(s) = speed {
                c.speed = s;
            }
        }
        TextToSpeechConfig::PocketTts(c) => {
            if let Some(v) = voice {
                c.voice = v;
            }
            if let Some(l) = language {
                c.language = l;
            }
            if let Some(s) = steps {
                c.lsd_steps = s;
            }
            if let Some(p) = precision {
                c.precision = match p.to_ascii_lowercase().as_str() {
                    "int8" => nobodywho::text_to_speech::PocketTtsPrecision::Int8,
                    "fp32" => nobodywho::text_to_speech::PocketTtsPrecision::Fp32,
                    _ => return Err("precision must be 'int8' or 'fp32'".into()),
                };
            }
            if let Some(t) = temperature {
                c.temperature = t;
            }
            if let Some(tok) = huggingface_token {
                c.huggingface_token = Some(tok);
            }
        }
        TextToSpeechConfig::Supertonic(c) => {
            if let Some(v) = voice {
                c.voice = v;
            }
            if let Some(l) = language {
                c.language = l;
            }
            if let Some(s) = speed {
                c.speed = s;
            }
            if let Some(s) = steps {
                c.steps = s;
            }
            if let Some(sd) = silence_duration {
                c.silence_duration = sd;
            }
        }
    }
    Ok((cfg, device))
}

fn parse_architecture(s: &str) -> Result<TextToSpeechArchitecture, String> {
    match s.to_ascii_lowercase().as_str() {
        "kokoro" => Ok(TextToSpeechArchitecture::Kokoro),
        "pocket-tts" | "pockettts" => Ok(TextToSpeechArchitecture::PocketTts),
        "supertonic" => Ok(TextToSpeechArchitecture::Supertonic),
        _ => Err(format!(
            "architecture must be 'kokoro', 'pocket-tts', or 'supertonic', got '{s}'"
        )),
    }
}

fn parse_device(s: &str) -> Result<TextToSpeechDevice, String> {
    match s.to_ascii_lowercase().as_str() {
        "auto" => Ok(TextToSpeechDevice::Auto),
        "cpu" => Ok(TextToSpeechDevice::Cpu),
        "cuda" => Ok(TextToSpeechDevice::Cuda),
        _ => Err(format!(
            "device must be 'auto', 'cpu', or 'cuda', got '{s}'"
        )),
    }
}

/// A config string key that's `None` when empty.
fn str_opt(config: &VarDictionary, key: &str) -> Result<Option<String>, String> {
    Ok(dict_get::<GString>(config, key)?
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string()))
}

/// `TextToSpeechError` is `thiserror::Error` but not `miette::Diagnostic`, so
/// plain `to_string()` (mirrors the Python binding's STT/TTS error handling).
fn render_tts_error(e: &TextToSpeechError) -> String {
    e.to_string()
}
