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
use crate::onnx::Device as TtsDevice;
use crate::tts::DEFAULT_SAMPLE_RATE;
use espeak_ng::Translator;
use ort::session::Session;
use ort::value::Tensor;
use regex::Regex;
use safetensors::tensor::{Dtype, SafeTensors};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use tracing::{debug, info};

const STYLE_DIM: usize = 256;

pub(in crate::tts) struct KokoroBackend {
    session: Session,
    voice_style: Vec<[f32; STYLE_DIM]>,
    /// IPA character → token id
    vocab: HashMap<String, i64>,
    translator: Translator,
    speed: f32,
    /// Kokoro supports style for finite number of phonemes
    max_input_phonemes: usize,
}

impl KokoroBackend {
    pub fn new(
        model_dir: &Path,
        voice: &str,
        language: &str,
        speed: f32,
        device: TtsDevice,
        espeak_data_dir: Option<&Path>,
    ) -> Result<Self, TtsError> {
        let session = crate::onnx::load_session(&model_dir.join("model.onnx"), device)?;
        let vocab = load_vocab(&model_dir.join("config.json"))?;
        let (voice_style, max_input_phonemes) = load_voice(&model_dir.join("voices"), voice)?;
        let translator = init_translator(language, espeak_data_dir)?;

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
            translator,
            speed,
            max_input_phonemes,
        })
    }
}

/// Extract bundled espeak-ng data on first use and build a Translator.
/// Idempotent — re-running on an existing dir is safe.
fn init_translator(language: &str, dir: Option<&Path>) -> Result<Translator, TtsError> {
    let data_dir: PathBuf = dir
        .map(PathBuf::from)
        .unwrap_or_else(|| std::env::temp_dir().join("nobodywho-espeak-ng-data"));
    std::fs::create_dir_all(&data_dir)
        .map_err(|e| TtsError::Synthesis(format!("create espeak data dir: {e}")))?;
    // Extract phonemes + the requested language dict. The language sub-tag
    // ("en-us" → "en") is what bundled-data is keyed on.
    let base_lang = language.split('-').next().unwrap_or(language);
    espeak_ng::install_bundled_language(&data_dir, base_lang).map_err(|e| {
        TtsError::Synthesis(format!(
            "install_bundled_language({base_lang}) to {}: {e}",
            data_dir.display()
        ))
    })?;
    Translator::new(language, Some(data_dir.as_path()))
        .map_err(|e| TtsError::Synthesis(format!("espeak Translator::new: {e}")))
}

/// Split CamelCase / PascalCase boundaries before passing to espeak.
/// C espeak-ng segments on case transitions internally; the pure-Rust port
/// doesn't (yet) — without this, `"NobodyWho"` is treated as one word and
/// the rule engine inserts a phantom `ˌɪ` between the two halves.
///
/// Handles both `lowerUpper` ("NobodyWho" → "Nobody Who") and the
/// ALLCAPS→Title boundary ("HTMLParser" → "HTML Parser").
fn split_camelcase(text: &str) -> String {
    static LOWER_UPPER: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"([a-z0-9])([A-Z])").unwrap());
    static UPPER_TITLE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"([A-Z])([A-Z][a-z])").unwrap());
    let s = LOWER_UPPER.replace_all(text, "$1 $2");
    UPPER_TITLE.replace_all(&s, "$1 $2").into_owned()
}

/// Kokoro's phoneme vocab includes punctuation tokens (`,`, `.`, `?`, `!`,
/// `;`, `:`, `—`, `…`) that carry prosodic meaning at inference time
/// (pauses, terminal intonation). `Translator::text_to_ipa` strips all
/// punctuation and emits a `\n` between clauses instead — so we walk the
/// input text, collect the terminator chars in order, and splice them back
/// onto the IPA clause boundaries.
fn restore_clause_punctuation(ipa: &str, text: &str) -> String {
    const CLAUSE_PUNCT: &[char] = &[',', '.', ';', ':', '!', '?', '—', '…'];
    let terms: Vec<char> = text.chars().filter(|c| CLAUSE_PUNCT.contains(c)).collect();
    let parts: Vec<&str> = ipa.split('\n').collect();

    let mut out = String::with_capacity(ipa.len() + terms.len() * 2);
    for (i, part) in parts.iter().enumerate() {
        out.push_str(part);
        if let Some(&t) = terms.get(i) {
            out.push(t);
        }
        if i + 1 < parts.len() {
            out.push(' ');
        }
    }
    out
}

impl TtsBackendImpl for KokoroBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        let preproc = split_camelcase(text);
        let raw_ipa = self
            .translator
            .text_to_ipa(&preproc)
            .map_err(|e| TtsError::Synthesis(format!("espeak phonemization failed: {e}")))?;
        let phonemes = restore_clause_punctuation(&raw_ipa, &preproc);
        let phonemes = phonemes.trim();
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

        let tokens = Tensor::from_array(([1usize, token_len], tokens))?;
        let style = Tensor::from_array(([1usize, STYLE_DIM], style))?;
        let speed = Tensor::from_array(([1usize], vec![self.speed as f64]))?;

        let outputs = self
            .session
            .run(ort::inputs!["input_ids" => tokens, "style" => style, "speed" => speed])?;

        let output = outputs[0].try_extract_tensor::<f32>()?;

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
    if shape.len() != 2 || shape[1] != STYLE_DIM || shape[0] < 2 {
        return Err(TtsError::Init(format!(
            "kokoro: voice {voice:?} `style` has shape {shape:?}, expected [rows, {STYLE_DIM}] with rows >= 2"
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
    /// Where to extract bundled espeak-ng data on first use. Defaults to
    /// `std::env::temp_dir().join("nobodywho-espeak-ng-data")`. On Android
    /// pass the app's cache dir (the app is sandboxed and can't write to
    /// `/tmp`).
    pub espeak_data_dir: Option<PathBuf>,
}

impl KokoroConfig {
    pub fn new(source: impl AsRef<str>) -> Self {
        Self {
            source: source.as_ref().to_string(),
            voice: "bf_emma".into(),
            language: "en-us".into(),
            speed: 1.0,
            espeak_data_dir: None,
        }
    }
}
