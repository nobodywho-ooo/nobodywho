use crate::errors::TtsError;
use crate::tts::{Tts, TtsDevice, TtsSampling};
use std::path::PathBuf;

/// Backend selection and model paths for a [`Tts`] handle.
#[derive(Clone, Debug)]
pub enum TtsConfig {
    Kokoro(KokoroConfig),
    Piper(PiperConfig),
    Chatterbox(ChatterboxConfig),
    Roest(RoestConfig),
}

impl TtsConfig {
    pub fn kokoro(model_path: impl Into<PathBuf>, voices_path: impl Into<PathBuf>) -> Self {
        Self::Kokoro(KokoroConfig::new(model_path, voices_path))
    }

    pub fn piper(model_path: impl Into<PathBuf>, config_path: impl Into<PathBuf>) -> Self {
        Self::Piper(PiperConfig::new(model_path, config_path))
    }

    pub fn chatterbox(model_dir: impl Into<PathBuf>) -> Self {
        Self::Chatterbox(ChatterboxConfig::new(model_dir))
    }

    pub fn roest(model_dir: impl Into<PathBuf>) -> Self {
        Self::Roest(RoestConfig::new(model_dir))
    }
}

#[derive(Clone, Debug)]
pub struct KokoroConfig {
    pub model_path: PathBuf,
    pub voices_path: PathBuf,
    pub voice: String,
    pub language: String,
    pub speed: f32,
}

impl KokoroConfig {
    pub fn new(model_path: impl Into<PathBuf>, voices_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
            voices_path: voices_path.into(),
            voice: "af_heart".into(),
            language: "en-us".into(),
            speed: 1.0,
        }
    }
}

#[derive(Clone, Debug)]
pub struct PiperConfig {
    pub model_path: PathBuf,
    pub config_path: PathBuf,
}

impl PiperConfig {
    pub fn new(model_path: impl Into<PathBuf>, config_path: impl Into<PathBuf>) -> Self {
        Self {
            model_path: model_path.into(),
            config_path: config_path.into(),
        }
    }
}

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
