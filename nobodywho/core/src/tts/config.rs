use crate::errors::TtsError;
use crate::tts::{Tts, TtsDevice, TtsSampling};
use std::path::PathBuf;

/// Backend selection and model directory for a [`Tts`] handle.
///
/// Every backend takes a single directory containing all the files it needs.
/// Each backend hardcodes the filenames it expects inside that directory; see
/// the per-config docs for the layout. This lets users either:
///
/// - Point at a downloaded HuggingFace snapshot (one repo id → one local dir).
/// - Point at a directory of files they assembled themselves on disk.
///
/// Both paths use the exact same loader code.
#[derive(Clone, Debug)]
pub enum TtsConfig {
    Kokoro(KokoroConfig),
    Piper(PiperConfig),
    Chatterbox(ChatterboxConfig),
    Roest(RoestConfig),
}

impl TtsConfig {
    pub fn kokoro(model_dir: impl Into<PathBuf>) -> Self {
        Self::Kokoro(KokoroConfig::new(model_dir))
    }

    pub fn piper(model_dir: impl Into<PathBuf>) -> Self {
        Self::Piper(PiperConfig::new(model_dir))
    }

    pub fn chatterbox(model_dir: impl Into<PathBuf>) -> Self {
        Self::Chatterbox(ChatterboxConfig::new(model_dir))
    }

    pub fn roest(model_dir: impl Into<PathBuf>) -> Self {
        Self::Roest(RoestConfig::new(model_dir))
    }
}

/// Expected layout under `model_dir`:
/// - `model.onnx` — Kokoro inference model
/// - `voices/<voice>.bin` — one raw little-endian f32 file per voice,
///   each `510 * 256 * 4 = 522240` bytes (matches the
///   `onnx-community/Kokoro-82M-v1.0-ONNX` voice export).
#[derive(Clone, Debug)]
pub struct KokoroConfig {
    pub model_dir: PathBuf,
    pub voice: String,
    pub language: String,
    pub speed: f32,
}

impl KokoroConfig {
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        Self {
            model_dir: model_dir.into(),
            voice: "af_heart".into(),
            language: "en-us".into(),
            speed: 1.0,
        }
    }
}

/// Expected layout under `model_dir`:
/// - `model.onnx` — Piper VITS model
/// - `model.onnx.json` — Piper config (phoneme map, audio params, espeak voice)
#[derive(Clone, Debug)]
pub struct PiperConfig {
    pub model_dir: PathBuf,
    /// Speaker index for multi-speaker voices. Ignored for single-speaker
    /// voices. Validated against `num_speakers` at load time.
    pub speaker_id: u32,
}

impl PiperConfig {
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        Self {
            model_dir: model_dir.into(),
            speaker_id: 0,
        }
    }
}

/// Expected layout under `model_dir`: the Chatterbox export tree
/// (T3, S3Gen, vocoder, voice encoder, tokenizer, conditioning blobs).
/// `default_voice.wav` next to it is used as the reference voice when
/// `reference_wav` is `None`.
#[derive(Clone, Debug)]
pub struct ChatterboxConfig {
    pub model_dir: PathBuf,
    pub reference_wav: Option<PathBuf>,
    pub language: String,
    pub sampling: TtsSampling,
}

impl ChatterboxConfig {
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        Self {
            model_dir: model_dir.into(),
            reference_wav: None,
            language: "en-us".into(),
            sampling: TtsSampling::default(),
        }
    }
}

/// Expected layout under `model_dir`: the Røst500M export tree
/// (`onnx/{text_embed,speech_embed,speech_encoder,language_model,conditional_decoder}.onnx`
/// plus matching `.onnx_data` sidecars, tokenizer files, `text_pos_emb.bin`,
/// and the voice-conditioning `default_cond/` directory and presets).
#[derive(Clone, Debug)]
pub struct RoestConfig {
    pub model_dir: PathBuf,
    pub language: String,
    pub sampling: TtsSampling,
}

impl RoestConfig {
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        Self {
            model_dir: model_dir.into(),
            language: "da".into(),
            sampling: TtsSampling::default(),
        }
    }
}

/// Builder for creating a [`Tts`] handle with an explicit backend config.
pub struct TtsBuilder {
    pub(crate) config: TtsConfig,
    pub(crate) device: TtsDevice,
}

impl TtsBuilder {
    pub fn new(config: TtsConfig) -> Self {
        Self {
            config,
            device: TtsDevice::Auto,
        }
    }

    pub fn with_device(mut self, device: TtsDevice) -> Self {
        self.device = device;
        self
    }

    pub fn build(self) -> Result<Tts, TtsError> {
        Tts::from_config(self.config, self.device)
    }
}
