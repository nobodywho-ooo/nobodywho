use godot::builtin::PackedByteArray;
use godot::prelude::*;

use nobodywho::onnx::Device;
use nobodywho::voice_activity_detection::{
    VoiceActivityDetection as CoreVad, VoiceActivityDetectionConfig, VoiceActivityDetectionEvent,
};

use crate::convert::{dict_get, resolve_godot_path};
use crate::task::{on_blocking_thread, task};

/// A streaming voice activity detector (Silero VAD) for live microphone
/// input. Build it with the async factory, then feed audio chunks and watch
/// for the `"speech_started"` / `"speech_ended"` events:
///
/// ```gdscript
/// var vad = await NobodyWhoVoiceActivityDetection.create("", {
///     "sample_rate": 16000,
/// })
/// var event: String = vad.push(pcm_bytes)
/// if event == "speech_started":
///     print("listening...")
/// elif event == "speech_ended":
///     # The complete utterance, i16 LE PCM — hand it to
///     # NobodyWhoSpeechToText.transcribe_pcm() or a file.
///     var turn: PackedByteArray = vad.finish()
/// ```
///
/// `source` is a HuggingFace repo (`hf://owner/repo`) or a local directory
/// containing `onnx/model.onnx` (the standard Silero layout). Empty uses the
/// canonical default, `hf://onnx-community/silero-vad`.
#[derive(GodotClass)]
#[class(no_init, base=RefCounted)]
pub struct NobodyWhoVoiceActivityDetection {
    vad: CoreVad,
    base: Base<RefCounted>,
}

#[godot_api]
impl NobodyWhoVoiceActivityDetection {
    /// Create a detector asynchronously (the model downloads/loads off the
    /// main thread). `await create(...)` resolves to the detector, or null
    /// on failure (with a `godot_error!`).
    ///
    /// `config` is a Dictionary with optional keys (all default to the
    /// Silero-tuned defaults when omitted/empty):
    /// - `"sample_rate"` (int): rate of the audio fed to `push()`. Silero
    ///   runs at 16 kHz internally — anything else is resampled. Must be
    ///   non-zero.
    /// - `"threshold"` (float, 0.0-1.0): speech-probability cutoff.
    /// - `"min_silence_duration_ms"` (int): how long silence must persist
    ///   before `"speech_ended"` fires.
    /// - `"min_speech_duration_ms"` (int): how long speech must persist
    ///   before `"speech_started"` fires.
    /// - `"preroll_duration_ms"` (int): how much audio to buffer before the
    ///   confirmed start, so the turn isn't clipped while deciding.
    /// - `"device"` (String): `"auto"` (default), `"cpu"`, or `"cuda"`.
    #[func]
    fn create(source: GString, config: VarDictionary) -> Variant {
        let source = resolve_godot_path(&source);
        let cfg = match parse_vad_config(&source, &config) {
            Ok(c) => c,
            Err(e) => {
                godot_error!("NobodyWhoVoiceActivityDetection.create: {e}");
                return Variant::nil();
            }
        };
        task(async move {
            let result = on_blocking_thread(move || CoreVad::with_device(cfg.0, cfg.1)).await;
            match result {
                Some(Ok(vad)) => Gd::from_init_fn(|base| Self { vad, base }).to_variant(),
                Some(Err(e)) => {
                    godot_error!("Failed to create VoiceActivityDetection: {e}");
                    Variant::nil()
                }
                None => {
                    godot_error!("VoiceActivityDetection worker init panicked");
                    Variant::nil()
                }
            }
        })
        .bind()
        .wait()
    }

    /// Feed the newest chunk of audio (not the whole accumulated buffer —
    /// the detector tracks the current turn internally). `samples` is a
    /// `PackedByteArray` of interleaved little-endian i16 samples at the
    /// `sample_rate` given to `create()`. Returns the current event as a
    /// String: `"speech"`, `"silence"`, or the `"speech_started"` /
    /// `"speech_ended"` transition this call confirmed, or null on error.
    #[func]
    fn push(&mut self, samples: PackedByteArray) -> Variant {
        let samples = pcm_bytes_to_i16(samples.as_slice());
        match self.vad.push(&samples) {
            Ok(event) => GString::from(event_str(event)).to_variant(),
            Err(e) => {
                godot_error!("VAD push failed: {e}");
                Variant::nil()
            }
        }
    }

    /// Return the current turn's captured audio (from the confirmed
    /// `"speech_started"`, including the pre-roll, through to
    /// `"speech_ended"`) and reset state for the next turn. Call after
    /// handling a `"speech_ended"`, or at any point to abandon the turn.
    /// Empty if speech was never confirmed. Interleaved little-endian i16.
    #[func]
    fn finish(&mut self) -> PackedByteArray {
        pcm_i16_to_bytes(&self.vad.finish())
    }

    /// Detect every speech segment in a complete recording, offline. Unlike
    /// `push`, finds all segments regardless of buffer size. `samples` is a
    /// `PackedByteArray` of LE i16 samples. Returns an Array of
    /// `PackedByteArray`s (each a segment, with a short pre-roll), or null
    /// on error.
    #[func]
    fn segment(&mut self, samples: PackedByteArray) -> Variant {
        let samples = pcm_bytes_to_i16(samples.as_slice());
        match self.vad.segment(&samples) {
            Ok(segments) => {
                let mut arr: Array<Variant> = Array::new();
                for segment in segments {
                    arr.push(&pcm_i16_to_bytes(&segment).to_variant());
                }
                arr.to_variant()
            }
            Err(e) => {
                godot_error!("VAD segment failed: {e}");
                Variant::nil()
            }
        }
    }
}

fn event_str(event: VoiceActivityDetectionEvent) -> &'static str {
    match event {
        VoiceActivityDetectionEvent::Speech => "speech",
        VoiceActivityDetectionEvent::Silence => "silence",
        VoiceActivityDetectionEvent::SpeechStarted => "speech_started",
        VoiceActivityDetectionEvent::SpeechEnded => "speech_ended",
    }
}

/// Parse the GDScript config Dictionary into a core `VoiceActivityDetectionConfig` +
/// `Device`. Empty string / 0 mean "use the default".
fn parse_vad_config(
    source: &str,
    config: &VarDictionary,
) -> Result<(VoiceActivityDetectionConfig, Device), String> {
    let mut cfg = VoiceActivityDetectionConfig::default();
    if !source.is_empty() {
        cfg.source = source.to_string();
    }
    if let Some(rate) = dict_get::<i64>(config, "sample_rate")?.filter(|&r| r > 0) {
        cfg.sample_rate = rate as u32;
    }
    if let Some(threshold) = dict_get::<f32>(config, "threshold")?.filter(|&t| t > 0.0) {
        cfg.threshold = threshold;
    }
    if let Some(ms) = dict_get::<i64>(config, "min_silence_duration_ms")?.filter(|&v| v > 0) {
        cfg.min_silence_duration_ms = ms as u32;
    }
    if let Some(ms) = dict_get::<i64>(config, "min_speech_duration_ms")?.filter(|&v| v > 0) {
        cfg.min_speech_duration_ms = ms as u32;
    }
    if let Some(ms) = dict_get::<i64>(config, "preroll_duration_ms")?.filter(|&v| v > 0) {
        cfg.preroll_duration_ms = ms as u32;
    }
    let device = dict_get::<GString>(config, "device")?
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string())
        .as_deref()
        .map(parse_device)
        .transpose()?
        .unwrap_or(Device::Auto);
    Ok((cfg, device))
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

/// Encode i16 samples as a `PackedByteArray` of little-endian bytes.
fn pcm_i16_to_bytes(samples: &[i16]) -> PackedByteArray {
    PackedByteArray::from(
        samples
            .iter()
            .flat_map(|s| s.to_le_bytes())
            .collect::<Vec<u8>>(),
    )
}
