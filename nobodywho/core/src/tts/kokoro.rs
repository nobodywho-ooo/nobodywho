//! Kokoro TTS via ONNX Runtime + espeak-ng phonemization.
//!
//! Pipeline: text → espeak IPA → phoneme IDs → ONNX → 24 kHz waveform.
//! The voice "style" is a token-count-indexed [510, 256] f32 lookup; the row
//! at index `len(phoneme_ids)` is the conditioning vector for that input
//! length.

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::{ort_util, TtsDevice};
use ahash::AHashMap;
use ort::session::{Session, SessionInputValue, SessionInputs};
use ort::value::{Tensor, Value};
use std::borrow::Cow;
use std::collections::HashMap;
use std::path::Path;
use std::sync::OnceLock;
use tracing::info;

const MODEL_FILE: &str = "model.onnx";
const VOICES_DIR: &str = "voices";

const SAMPLE_RATE: u32 = 24000;
const STYLE_DIM: usize = 256;
/// First dimension of the voice-pack tensor (`[510, 1, 256]` upstream).
const STYLE_ROWS: usize = 510;
/// Maximum un-padded phoneme count: the ONNX run wraps the input in BOS/EOS
/// (id 0) so the actual sequence is two tokens longer.
const MAX_PHONEMES: usize = STYLE_ROWS - 1;

pub(super) struct KokoroBackend {
    session: Session,
    voices: AHashMap<String, Vec<f32>>,
    voice: String,
    language: String,
    speed: f32,
    token_input: &'static str,
}

impl KokoroBackend {
    pub fn new(
        model_dir: &Path,
        voice: String,
        language: String,
        speed: f32,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let model_path = model_dir.join(MODEL_FILE);
        let voices_dir = model_dir.join(VOICES_DIR);

        let voices = load_voices(&voices_dir)?;
        if !voices.contains_key(&voice) {
            return Err(TtsError::Init(format!(
                "kokoro voice {voice:?} not found in {}",
                voices_dir.display()
            )));
        }

        let session = ort_util::load_session(&model_path, device, false)?;
        let token_input = detect_token_input(&session)?;

        info!(
            voices = voices.len(),
            voice,
            language,
            "Loaded Kokoro model"
        );

        Ok(Self {
            session,
            voices,
            voice,
            language,
            speed,
            token_input,
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

        let vocab = phoneme_vocab();
        let mut phoneme_ids: Vec<i64> = Vec::with_capacity(phonemes.chars().count());
        for ch in phonemes.chars() {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            if let Some(&id) = vocab.get(s) {
                phoneme_ids.push(id);
            }
            // Unmapped IPA characters are dropped silently — same as upstream.
        }
        if phoneme_ids.is_empty() {
            return Err(TtsError::Synthesis(
                "kokoro: no phonemes mapped to vocab IDs".into(),
            ));
        }
        if phoneme_ids.len() > MAX_PHONEMES {
            return Err(TtsError::Synthesis(format!(
                "kokoro input is {} phonemes; max {} (chunking not yet implemented)",
                phoneme_ids.len(),
                MAX_PHONEMES
            )));
        }

        // Style row indexed by un-padded phoneme count.
        let style_idx = phoneme_ids.len();
        let voice_pack = &self.voices[&self.voice];
        let style: Vec<f32> = voice_pack
            [style_idx * STYLE_DIM..(style_idx + 1) * STYLE_DIM]
            .to_vec();

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

        Ok((output.1.to_vec(), SAMPLE_RATE))
    }

    fn available_voices(&self) -> Vec<String> {
        let mut names: Vec<String> = self.voices.keys().cloned().collect();
        names.sort();
        names
    }
}

/// Load every `<voice>.bin` under `voices_dir` as a flat `Vec<f32>` of size
/// 510*256. Files must be raw little-endian f32 bytes, matching the
/// onnx-community/kokoro voice export layout (one file per voice).
fn load_voices(voices_dir: &Path) -> Result<AHashMap<String, Vec<f32>>, TtsError> {
    if !voices_dir.is_dir() {
        return Err(TtsError::Init(format!(
            "kokoro: voices directory not found at {}",
            voices_dir.display()
        )));
    }

    let entries = std::fs::read_dir(voices_dir).map_err(|e| {
        TtsError::Init(format!(
            "kokoro: read {}: {e}",
            voices_dir.display()
        ))
    })?;

    let expected_bytes = STYLE_ROWS * STYLE_DIM * std::mem::size_of::<f32>();
    let mut voices = AHashMap::new();
    for entry in entries {
        let entry = entry.map_err(|e| TtsError::Init(format!("kokoro: read entry: {e}")))?;
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("bin") {
            continue;
        }
        let name = match path.file_stem().and_then(|s| s.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };
        let bytes = std::fs::read(&path)
            .map_err(|e| TtsError::Init(format!("kokoro: read voice {name}: {e}")))?;
        if bytes.len() != expected_bytes {
            return Err(TtsError::Init(format!(
                "kokoro: voice {name} has {} bytes, expected {expected_bytes} ({STYLE_ROWS}*{STYLE_DIM} f32 LE)",
                bytes.len()
            )));
        }
        let mut floats = Vec::with_capacity(STYLE_ROWS * STYLE_DIM);
        for chunk in bytes.chunks_exact(4) {
            floats.push(f32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]));
        }
        voices.insert(name, floats);
    }

    if voices.is_empty() {
        return Err(TtsError::Init(format!(
            "kokoro: no voice files (*.bin) found in {}",
            voices_dir.display()
        )));
    }

    Ok(voices)
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

fn phoneme_vocab() -> &'static HashMap<&'static str, i64> {
    static VOCAB: OnceLock<HashMap<&'static str, i64>> = OnceLock::new();
    VOCAB.get_or_init(|| {
        HashMap::from([
            (";", 1),
            (":", 2),
            (",", 3),
            (".", 4),
            ("!", 5),
            ("?", 6),
            ("\u{2014}", 9),    // —
            ("\u{2026}", 10),   // …
            ("\"", 11),
            ("(", 12),
            (")", 13),
            ("\u{201C}", 14),   // “
            ("\u{201D}", 15),   // ”
            (" ", 16),
            ("\u{0303}", 17),   // ̃
            ("ʣ", 18),
            ("ʥ", 19),
            ("ʦ", 20),
            ("ʨ", 21),
            ("ᵝ", 22),
            ("ꭧ", 23),
            ("A", 24),
            ("I", 25),
            ("O", 31),
            ("Q", 33),
            ("S", 35),
            ("T", 36),
            ("W", 39),
            ("Y", 41),
            ("ᵊ", 42),
            ("a", 43),
            ("b", 44),
            ("c", 45),
            ("d", 46),
            ("e", 47),
            ("f", 48),
            ("h", 50),
            ("i", 51),
            ("j", 52),
            ("k", 53),
            ("l", 54),
            ("m", 55),
            ("n", 56),
            ("o", 57),
            ("p", 58),
            ("q", 59),
            ("r", 60),
            ("s", 61),
            ("t", 62),
            ("u", 63),
            ("v", 64),
            ("w", 65),
            ("x", 66),
            ("y", 67),
            ("z", 68),
            ("ɑ", 69),
            ("ɐ", 70),
            ("ɒ", 71),
            ("æ", 72),
            ("β", 75),
            ("ɔ", 76),
            ("ɕ", 77),
            ("ç", 78),
            ("ɖ", 80),
            ("ð", 81),
            ("ʤ", 82),
            ("ə", 83),
            ("ɚ", 85),
            ("ɛ", 86),
            ("ɜ", 87),
            ("ɟ", 90),
            ("ɡ", 92),
            ("ɥ", 99),
            ("ɨ", 101),
            ("ɪ", 102),
            ("ʝ", 103),
            ("ɯ", 110),
            ("ɰ", 111),
            ("ŋ", 112),
            ("ɳ", 113),
            ("ɲ", 114),
            ("ɴ", 115),
            ("ø", 116),
            ("ɸ", 118),
            ("θ", 119),
            ("œ", 120),
            ("ɹ", 123),
            ("ɾ", 125),
            ("ɻ", 126),
            ("ʁ", 128),
            ("ɽ", 129),
            ("ʂ", 130),
            ("ʃ", 131),
            ("ʈ", 132),
            ("ʧ", 133),
            ("ʊ", 135),
            ("ʋ", 136),
            ("ʌ", 138),
            ("ɣ", 139),
            ("ɤ", 140),
            ("χ", 142),
            ("ʎ", 143),
            ("ʒ", 147),
            ("ʔ", 148),
            ("ˈ", 156),
            ("ˌ", 157),
            ("ː", 158),
            ("ʰ", 162),
            ("ʲ", 164),
            ("\u{2193}", 169), // ↓
            ("\u{2192}", 171), // →
            ("\u{2197}", 172), // ↗
            ("\u{2198}", 173), // ↘
            ("ᵻ", 177),
        ])
    })
}
