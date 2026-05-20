//! Kokoro TTS via ONNX Runtime + espeak-ng phonemization.
//!
//! Pipeline: text → espeak IPA → phoneme IDs → ONNX → 24 kHz waveform.
//!
//! Kokoro doesn't use a single "voice embedding" per voice — it ships a
//! different style vector for each possible input length. So
//! `voices/<voice>.safetensors` holds one `"style"` tensor of shape
//! `[rows, 256]`, and at inference time we pick row `N` where
//! `N == phoneme_ids.len()`. `max_input_phonemes = rows - 1` is the largest
//! input length the voice has a style row for.

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::{ort_util, TtsDevice, DEFAULT_SAMPLE_RATE};
use ort::session::Session;
use ort::value::Tensor;
use safetensors::tensor::{Dtype, SafeTensors};
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

const STYLE_DIM: usize = 256;

pub(in crate::tts) struct KokoroBackend {
    session: Session,
    voice_style: Vec<[f32; STYLE_DIM]>,
    /// IPA character → token id
    vocab: HashMap<String, i64>,
    language: String,
    speed: f32,
    /// Kokoro supports style for finite number of phonemes
    max_input_phonemes: usize,
}

impl KokoroBackend {
    pub fn new(
        model_dir: &Path,
        voice: String,
        language: String,
        speed: f32,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let session = ort_util::load_session(&model_dir.join("model.onnx"), device)?;
        let vocab = load_vocab(&model_dir.join("config.json"))?;
        let (voice_style, max_input_phonemes) = load_voice(&model_dir.join("voices"), &voice)?;

        info!(
            voice,
            language,
            max_input_phonemes,
            vocab_len = vocab.len(),
            "Loaded Kokoro model"
        );

        Ok(Self {
            session,
            voice_style,
            vocab,
            language,
            speed,
            max_input_phonemes,
        })
    }
}

impl TtsBackendImpl for KokoroBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        let phoneme_sentences =
            espeak_rs::text_to_phonemes(text, &self.language, None, true, false)
                .map_err(|e| TtsError::Synthesis(format!("espeak phonemization failed: {e}")))?;
        let phonemes = phoneme_sentences.join(" ");
        if phonemes.is_empty() {
            return Err(TtsError::Synthesis(
                "kokoro: text produced no phonemes".into(),
            ));
        }

        let mut phoneme_ids: Vec<i64> = Vec::with_capacity(phonemes.len());
        for ch in phonemes.chars() {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            if let Some(&id) = self.vocab.get(s) {
                phoneme_ids.push(id);
            }
            // Unmapped IPA characters are dropped silently — same as upstream.
            // https://github.com/hexgrad/kokoro/blob/main/kokoro/model.py#L128
        }
        if phoneme_ids.is_empty() {
            return Err(TtsError::Synthesis(
                "kokoro: no phonemes mapped to vocab IDs".into(),
            ));
        }
        if phoneme_ids.len() > self.max_input_phonemes {
            return Err(TtsError::Synthesis(format!(
                "kokoro input is {} phonemes; max {} (chunking not yet implemented)",
                phoneme_ids.len(),
                self.max_input_phonemes
            )));
        }

        let style: Vec<f32> = self.voice_style[phoneme_ids.len()].to_vec();

        // Kokoro's KModel.forward wraps the sequence in BOS/EOS (both id 0)
        // before calling forward_with_tokens; our ONNX export captures the
        // latter, so we do the wrap here.
        // See https://github.com/hexgrad/kokoro/blob/main/kokoro/model.py#L130
        let mut tokens: Vec<i64> = Vec::with_capacity(phoneme_ids.len() + 2);
        tokens.push(0);
        tokens.extend(phoneme_ids);
        tokens.push(0);
        let token_len = tokens.len();

        let tokens = Tensor::from_array(([1usize, token_len], tokens))
            .map_err(|e| TtsError::Synthesis(format!("tokens tensor: {e}")))?;
        let style = Tensor::from_array(([1usize, STYLE_DIM], style))
            .map_err(|e| TtsError::Synthesis(format!("style tensor: {e}")))?;
        let speed = Tensor::from_array(([1usize], vec![self.speed as f64]))
            .map_err(|e| TtsError::Synthesis(format!("speed tensor: {e}")))?;

        let outputs = self
            .session
            .run(ort::inputs!["input_ids" => tokens, "style" => style, "speed" => speed])
            .map_err(|e| TtsError::Synthesis(format!("ort inference failed: {e}")))?;

        let output = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(|e| TtsError::Synthesis(format!("extract waveform: {e}")))?;

        let pcm = output.1.to_vec();
        debug!(
            phoneme_ids = token_len - 2,
            pcm_samples = pcm.len(),
            pcm_duration_s = pcm.len() as f32 / DEFAULT_SAMPLE_RATE as f32,
            "Kokoro: done"
        );
        Ok((pcm, DEFAULT_SAMPLE_RATE))
    }
}

/// Load a `<voice>.safetensors` file containing a single f32 tensor named
/// `"style"` with shape `[rows, STYLE_DIM]`. Returns one style vector per
/// row together with `max_input_phonemes` (= rows - 1).
fn load_voice(voices_dir: &Path, voice: &str) -> Result<(Vec<[f32; STYLE_DIM]>, usize), TtsError> {
    let path = voices_dir.join(format!("{voice}.safetensors"));
    let bytes = std::fs::read(&path)
        .map_err(|e| TtsError::Init(format!("kokoro: read voice {voice:?}: {e}")))?;

    let st = SafeTensors::deserialize(&bytes)
        .map_err(|e| TtsError::Init(format!("kokoro: parse voice {voice:?}: {e}")))?;
    let view = st.tensor("style").map_err(|e| {
        TtsError::Init(format!(
            "kokoro: voice {voice:?} missing `style` tensor: {e}"
        ))
    })?;

    if view.dtype() != Dtype::F32 {
        return Err(TtsError::Init(format!(
            "kokoro: voice {voice:?} `style` has dtype {:?}, expected F32",
            view.dtype()
        )));
    }
    let shape = view.shape();
    if shape.len() != 2 || shape[1] != STYLE_DIM || shape[0] == 0 {
        return Err(TtsError::Init(format!(
            "kokoro: voice {voice:?} `style` has shape {shape:?}, expected [rows, {STYLE_DIM}] with rows >= 1"
        )));
    }
    let rows = shape[0];

    let floats = view
        .data()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()));
    let voice_style: Vec<[f32; STYLE_DIM]> = floats
        .collect::<Vec<f32>>()
        .chunks_exact(STYLE_DIM)
        .map(|row| row.try_into().expect("STYLE_DIM-sized chunk"))
        .collect();
    Ok((voice_style, rows - 1))
}

/// Read the IPA-character → token-id map from `config.json["vocab"]`.
fn load_vocab(config_path: &Path) -> Result<HashMap<String, i64>, TtsError> {
    #[derive(serde::Deserialize)]
    struct Config {
        vocab: HashMap<String, i64>,
    }

    let path = config_path.display();
    let file = std::fs::File::open(config_path)
        .map_err(|e| TtsError::Init(format!("kokoro: {path}: {e}")))?;
    let Config { vocab } = serde_json::from_reader(file)
        .map_err(|e| TtsError::Init(format!("kokoro: {path}: {e}")))?;
    if vocab.is_empty() {
        return Err(TtsError::Init(format!("kokoro: {path}: vocab is empty")));
    }
    Ok(vocab)
}

/// Where the Kokoro model lives. `source` is either:
/// - an existing local directory containing `model.onnx`, `config.json`, and `voices/*.safetensors`, or
/// - a HuggingFace Hub repo ID in `owner/repo` form (e.g. `"NobodyWho/kokoro-v1"`);
///   the whole repo is downloaded to the user's cache on first use.
#[derive(Clone, Debug)]
pub struct KokoroConfig {
    pub source: String,
    pub voice: String,
    pub language: String,
    pub speed: f32,
}

impl KokoroConfig {
    pub fn new(source: impl AsRef<str>) -> Self {
        Self {
            source: source.as_ref().to_string(),
            voice: "af_heart".into(),
            language: "en-us".into(),
            speed: 1.0,
        }
    }
}
