//! Kokoro TTS via ONNX Runtime + espeak-ng phonemization.
//!
//! Pipeline: text → espeak IPA → phoneme IDs → ONNX → 24 kHz waveform.
//! The voice "style" is a token-count-indexed [rows, dim] f32 lookup; the row
//! at index `len(phoneme_ids)` is the conditioning vector for that input
//! length. Both `rows` and `dim` are derived at load time — `dim` from the
//! ONNX `style` input's shape, `rows` from the voice file byte count — so
//! retrained Kokoro variants with different dimensions just work.

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::{ort_util, TtsDevice, DEFAULT_SAMPLE_RATE};
use ort::session::{Session, SessionInputValue, SessionInputs};
use ort::value::{Tensor, Value};
use std::borrow::Cow;
use std::collections::HashMap;
use std::mem;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

const MODEL_FILE: &str = "model.onnx";
const VOICES_DIR: &str = "voices";
const CONFIG_FILE: &str = "config.json";
const TOKENIZER_FILE: &str = "tokenizer.json";

pub(in crate::tts) struct KokoroBackend {
    session: Session,
    /// Flat f32 style matrix for the selected voice; row `i` is the style
    /// vector for inputs of length `i`.
    voice_style: Vec<f32>,
    /// IPA character → token id, loaded from `config.json` in the model dir.
    vocab: HashMap<String, i64>,
    language: String,
    speed: f32,
    sample_rate: u32,
    token_input: &'static str,
    style_dim: usize,
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
        sample_rate: u32,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let model_path = model_dir.join(MODEL_FILE);
        let voices_dir = model_dir.join(VOICES_DIR);

        let session = ort_util::load_session(&model_path, device, false)?;
        let token_input = detect_token_input(&session)?;
        let style_dim = style_dim_from_session(&session)?;
        let vocab = load_vocab(model_dir)?;

        let (voice_style, max_phonemes) = load_voice(&voices_dir, &voice, style_dim)?;

        info!(
            voice,
            language,
            style_dim,
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
            sample_rate,
            token_input,
            style_dim,
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
            self.voice_style[style_idx * self.style_dim..(style_idx + 1) * self.style_dim].to_vec();

        // ONNX expects the sequence wrapped in BOS/EOS (both id 0).
        let mut tokens: Vec<i64> = Vec::with_capacity(phoneme_ids.len() + 2);
        tokens.push(0);
        tokens.extend(phoneme_ids);
        tokens.push(0);
        let token_len = tokens.len();

        let token_tensor = Tensor::from_array(([1usize, token_len], tokens))
            .map_err(|e| TtsError::Synthesis(format!("tokens tensor: {e}")))?;
        let style_tensor = Tensor::from_array(([1usize, self.style_dim], style))
            .map_err(|e| TtsError::Synthesis(format!("style tensor: {e}")))?;
        let speed_tensor = Tensor::from_array(([1usize], vec![self.speed]))
            .map_err(|e| TtsError::Synthesis(format!("speed tensor: {e}")))?;

        let inputs: Vec<(Cow<'_, str>, SessionInputValue<'_>)> = vec![
            (
                Cow::Borrowed(self.token_input),
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
            pcm_duration_s = pcm.len() as f32 / self.sample_rate as f32,
            "Kokoro: done"
        );
        Ok((pcm, self.sample_rate))
    }
}

/// Load a single `<voice>.bin` file as a flat `Vec<f32>` style matrix and
/// return it together with `max_phonemes` (= style_rows - 1).
/// Files are raw little-endian f32 bytes; matches the onnx-community/kokoro
/// voice export layout.
pub(super) fn load_voice(
    voices_dir: &Path,
    voice: &str,
    style_dim: usize,
) -> Result<(Vec<f32>, usize), TtsError> {
    let path = voices_dir.join(format!("{voice}.bin"));
    let bytes = std::fs::read(&path)
        .map_err(|e| TtsError::Init(format!("kokoro: read voice {voice:?}: {e}")))?;

    let bytes_per_row = style_dim * mem::size_of::<f32>();
    if bytes.len() % bytes_per_row != 0 {
        return Err(TtsError::Init(format!(
            "kokoro: voice {voice:?} has {} bytes, not a multiple of {bytes_per_row} (style_dim={style_dim} × 4)",
            bytes.len()
        )));
    }

    let floats = bytes
        .chunks_exact(4)
        .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
        .collect();
    let max_phonemes = (bytes.len() / bytes_per_row).saturating_sub(1);
    Ok((floats, max_phonemes))
}

/// Read the style-vector dimension from the ONNX `style` input shape. Both
/// known exports declare it as `[1, dim]` with a concrete `dim`; we error
/// out clearly if a future export marks it dynamic.
fn style_dim_from_session(session: &Session) -> Result<usize, TtsError> {
    let style_input = session
        .inputs()
        .iter()
        .find(|i| i.name() == "style")
        .ok_or_else(|| {
            TtsError::Init(
                "kokoro: model does not declare a `style` input; unsupported export".into(),
            )
        })?;
    let shape = style_input
        .dtype()
        .tensor_shape()
        .ok_or_else(|| TtsError::Init("kokoro: `style` input is not a tensor type".into()))?;
    let &dim = shape.last().ok_or_else(|| {
        TtsError::Init("kokoro: `style` input has rank 0; expected at least rank 1".into())
    })?;
    if dim <= 0 {
        return Err(TtsError::Init(format!(
            "kokoro: `style` input last dim is {dim} (dynamic or zero); a concrete style_dim is required"
        )));
    }
    Ok(dim as usize)
}

/// Different ONNX exports of the same Kokoro weights name the token input
/// differently (`tokens` for thewh1teagle's port, `input_ids` for
/// onnx-community's). Pick whichever the loaded model declares.
fn detect_token_input(session: &Session) -> Result<&'static str, TtsError> {
    let names: Vec<&str> = session.inputs().iter().map(|i| i.name()).collect();
    for &candidate in &["tokens", "input_ids"] {
        if names.contains(&candidate) {
            return Ok(candidate);
        }
    }
    Err(TtsError::Init(format!(
        "kokoro: model declares unexpected input names {names:?}; expected one of \"tokens\", \"input_ids\""
    )))
}

/// Read the IPA-character → token-id map from the model directory.
/// Tries `config.json["vocab"]` (thewh1teagle/kokoro-onnx layout) first,
/// then falls back to `tokenizer.json["model"]["vocab"]`
/// (onnx-community/Kokoro-82M-v1.0-ONNX layout).
pub(super) fn load_vocab(model_dir: &Path) -> Result<HashMap<String, i64>, TtsError> {
    // thewh1teagle/kokoro-onnx: vocab lives at the top level of config.json
    let config_path = model_dir.join(CONFIG_FILE);
    if let Ok(raw) = std::fs::read_to_string(&config_path) {
        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(vocab_value) = json.get("vocab") {
                let vocab: HashMap<String, i64> = serde_json::from_value(vocab_value.clone())
                    .map_err(|e| {
                        TtsError::Init(format!(
                            "kokoro: `vocab` in {} is not {{string: int}}: {e}",
                            config_path.display()
                        ))
                    })?;
                if !vocab.is_empty() {
                    return Ok(vocab);
                }
            }
        }
    }

    // onnx-community/Kokoro-82M-v1.0-ONNX: vocab lives at tokenizer.json["model"]["vocab"]
    let tokenizer_path = model_dir.join(TOKENIZER_FILE);
    let raw = std::fs::read_to_string(&tokenizer_path)
        .map_err(|e| TtsError::Init(format!("kokoro: read {}: {e}", tokenizer_path.display())))?;
    let json: serde_json::Value = serde_json::from_str(&raw)
        .map_err(|e| TtsError::Init(format!("kokoro: parse {}: {e}", tokenizer_path.display())))?;
    let vocab_value = json
        .get("model")
        .and_then(|m| m.get("vocab"))
        .ok_or_else(|| {
            TtsError::Init(format!(
                "kokoro: no vocab found in {} or {}",
                config_path.display(),
                tokenizer_path.display()
            ))
        })?;
    let vocab: HashMap<String, i64> = serde_json::from_value(vocab_value.clone()).map_err(|e| {
        TtsError::Init(format!(
            "kokoro: `vocab` in {} is not {{string: int}}: {e}",
            tokenizer_path.display()
        ))
    })?;
    if vocab.is_empty() {
        return Err(TtsError::Init(format!(
            "kokoro: vocab in {} is empty",
            model_dir.display()
        )));
    }
    Ok(vocab)
}

#[derive(Clone, Debug)]
pub struct KokoroConfig {
    pub model_dir: PathBuf,
    pub voice: String,
    pub language: String,
    pub speed: f32,
    pub sample_rate: u32,
}

impl KokoroConfig {
    pub fn new(model_dir: impl Into<PathBuf>) -> Self {
        Self {
            model_dir: model_dir.into(),
            voice: "af_heart".into(),
            language: "en-us".into(),
            speed: 1.0,
            sample_rate: DEFAULT_SAMPLE_RATE,
        }
    }
}
