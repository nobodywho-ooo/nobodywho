//! Kokoro TTS via ONNX Runtime + espeak-ng phonemization.
//!
//! Pipeline: text → espeak IPA → phoneme IDs → ONNX → 24 kHz waveform.
//!
//! Each voice is a `voices/<voice>.safetensors` file holding one `"style"`
//! tensor of shape `[rows, 256]`. Row `i` is the conditioning vector for an
//! input of `i` un-padded phonemes; `max_phonemes = rows - 1`.

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::{ort_util, TtsDevice, DEFAULT_SAMPLE_RATE};
use ort::session::{Session, SessionInputValue, SessionInputs};
use ort::value::{Tensor, Value};
use safetensors::tensor::{Dtype, SafeTensors};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use tracing::{debug, info};

const STYLE_DIM: usize = 256;

pub(in crate::tts) struct KokoroBackend {
    session: Session,
    /// Flat f32 style matrix for the selected voice; row `i` is the style
    /// vector for inputs of length `i`.
    voice_style: Vec<f32>,
    /// IPA character → token id, loaded from `config.json` in the model dir.
    vocab: HashMap<String, i64>,
    language: String,
    speed: f32,
    /// Maximum un-padded phoneme count. The ONNX run wraps the input in
    /// BOS/EOS (id 0) so the actual sequence is two tokens longer; the style
    /// row is indexed by un-padded count, so this caps at `style_rows - 1`.
    max_phonemes: usize,
}

impl KokoroBackend {
    pub fn new(
        model_dir: &Path,
        voice: String,
        language: String,
        speed: f32,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let session = ort_util::load_session(&model_dir.join("model.onnx"), device, false)?;
        let vocab = load_vocab(&model_dir.join("config.json"))?;
        let (voice_style, max_phonemes) = load_voice(&model_dir.join("voices"), &voice)?;

        info!(
            voice,
            language,
            max_phonemes,
            vocab_len = vocab.len(),
            "Loaded Kokoro model"
        );

        Ok(Self {
            session,
            voice_style,
            vocab,
            language,
            speed,
            max_phonemes,
        })
    }
}

impl TtsBackendImpl for KokoroBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        info!("Kokoro: synthesising");
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
        }
        if phoneme_ids.is_empty() {
            return Err(TtsError::Synthesis(
                "kokoro: no phonemes mapped to vocab IDs".into(),
            ));
        }
        if phoneme_ids.len() > self.max_phonemes {
            return Err(TtsError::Synthesis(format!(
                "kokoro input is {} phonemes; max {} (chunking not yet implemented)",
                phoneme_ids.len(),
                self.max_phonemes
            )));
        }

        // Style row indexed by un-padded phoneme count.
        let style_idx = phoneme_ids.len();
        let style: Vec<f32> =
            self.voice_style[style_idx * STYLE_DIM..(style_idx + 1) * STYLE_DIM].to_vec();

        // ONNX expects the sequence wrapped in BOS/EOS (both id 0).
        let mut tokens: Vec<i64> = Vec::with_capacity(phoneme_ids.len() + 2);
        tokens.push(0);
        tokens.extend(phoneme_ids);
        tokens.push(0);
        let token_len = tokens.len();

        let token_tensor = Tensor::from_array(([1usize, token_len], tokens))
            .map_err(|e| TtsError::Synthesis(format!("tokens tensor: {e}")))?;
        let style_tensor = Tensor::from_array(([1usize, STYLE_DIM], style))
            .map_err(|e| TtsError::Synthesis(format!("style tensor: {e}")))?;
        let speed_tensor = Tensor::from_array(([1usize], vec![self.speed as f64]))
            .map_err(|e| TtsError::Synthesis(format!("speed tensor: {e}")))?;

        let inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> = vec![
            (
                Cow::Borrowed("input_ids"),
                SessionInputValue::Owned(Value::from(token_tensor)),
            ),
            (
                Cow::Borrowed("style"),
                SessionInputValue::Owned(Value::from(style_tensor)),
            ),
            (
                Cow::Borrowed("speed"),
                SessionInputValue::Owned(Value::from(speed_tensor)),
            ),
        ];

        let outputs = self
            .session
            .run(SessionInputs::from(inputs))
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
/// `"style"` with shape `[rows, STYLE_DIM]`. Returns the matrix flattened
/// row-major together with `max_phonemes` (= rows - 1).
pub(super) fn load_voice(voices_dir: &Path, voice: &str) -> Result<(Vec<f32>, usize), TtsError> {
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
    if shape.len() != 2 || shape[1] != STYLE_DIM {
        return Err(TtsError::Init(format!(
            "kokoro: voice {voice:?} `style` has shape {shape:?}, expected [rows, {STYLE_DIM}]"
        )));
    }
    let rows = shape[0];

    let floats: Vec<f32> = view
        .data()
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    Ok((floats, rows.saturating_sub(1)))
}

/// Read the IPA-character → token-id map from `config.json["vocab"]`.
pub(super) fn load_vocab(config_path: &Path) -> Result<HashMap<String, i64>, TtsError> {
    let raw = std::fs::read_to_string(config_path)
        .map_err(|e| TtsError::Init(format!("kokoro: read {}: {e}", config_path.display())))?;
    let json: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| TtsError::Init(format!("kokoro: parse {}: {e}", config_path.display())))?;
    let vocab_value = json.get("vocab").ok_or_else(|| {
        TtsError::Init(format!("kokoro: no `vocab` in {}", config_path.display()))
    })?;
    let vocab: HashMap<String, i64> = serde_json::from_value(vocab_value.clone()).map_err(|e| {
        TtsError::Init(format!(
            "kokoro: `vocab` in {} is not {{string: int}}: {e}",
            config_path.display()
        ))
    })?;
    if vocab.is_empty() {
        return Err(TtsError::Init(format!(
            "kokoro: `vocab` in {} is empty",
            config_path.display()
        )));
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
