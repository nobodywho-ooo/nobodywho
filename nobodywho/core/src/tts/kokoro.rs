//! Kokoro TTS via ONNX Runtime + espeak-ng phonemization.
//!
//! Pipeline: text → espeak IPA → misaki phonemes → phoneme IDs → ONNX → 24 kHz waveform.
//!
//! Kokoro was trained on misaki's phoneme alphabet, which differs from raw
//! espeak IPA: diphthongs and affricates collapse to single tokens
//! (`aɪ`→`I`, `oʊ`→`O`, `dʒ`→`ʤ`, …). [`espeak_ipa_to_misaki`] ports misaki's
//! `EspeakFallback` conversion (the same table kokoroxide/misaki Python use)
//! so the IDs we feed match what the model saw in training. British and
//! American voices need different branches (`əʊ`→`Q` vs `O`, rhotic vowels);
//! the branch is chosen from the configured language.
//!
//! Kokoro doesn't use a single "voice embedding" per voice — it ships a
//! different style vector for each possible input length. So
//! `voices/<voice>.safetensors` holds one `"style"` tensor of shape
//! `[rows, 256]`, and at inference time we pick row `len(phonemes) - 1`
//! (matching upstream `pack[len(ps)-1]`). `max_input_phonemes = rows - 1` is
//! the largest input length the voice has a style row for.

use crate::errors::TtsError;
use crate::tts::backend::TtsBackendImpl;
use crate::tts::{ort_util, TtsDevice, DEFAULT_SAMPLE_RATE};
use espeak_ng::Translator;
use ort::session::Session;
use ort::value::Tensor;
use safetensors::tensor::{Dtype, SafeTensors};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

const STYLE_DIM: usize = 256;
const SUPPORTED_LANGS: &[&str] = &["en-us", "en-gb", "es", "fr", "it", "pt-br"];

#[derive(Debug, Copy, Clone)]
enum EspeakDialect {
    EnGb,
    EnUs,
    NonEnglish,
}

pub(in crate::tts) struct KokoroBackend {
    session: Session,
    voice_style: Vec<[f32; STYLE_DIM]>,
    /// IPA character → token id
    vocab: HashMap<String, i64>,
    phonemizer: Phonemizer,
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
        let session = ort_util::load_session(&model_dir.join("model.onnx"), device)?;
        let vocab = load_vocab(&model_dir.join("config.json"))?;
        let (voice_style, max_input_phonemes) = load_voice(&model_dir.join("voices"), voice)?;

        if !SUPPORTED_LANGS.contains(&language) {
            return Err(TtsError::Init(format!(
                "Language {language:?} not supported. \
                 Supported: {}.",
                SUPPORTED_LANGS.join(", ")
            )));
        }

        let dialect = match language {
            "en-gb" => EspeakDialect::EnGb,
            "en-us" => EspeakDialect::EnUs,
            _ => EspeakDialect::NonEnglish,
        };
        // espeak-ng-rs has a native "en-us" phoneme table; en-gb maps to "en" (no en-gb table).
        // pt-br maps to "pt" (no pt-br table).
        let espeak_lang = match language {
            "en-gb" => "en",
            "pt-br" => "pt",
            other => other,
        };
        let phonemizer = Phonemizer::new(dialect, espeak_lang, espeak_data_dir)?;

        info!(
            voice,
            language,
            espeak_lang,
            misaki = phonemizer.g2p.is_some(),
            max_input_phonemes,
            vocab_len = vocab.len(),
            "Loaded Kokoro model"
        );

        Ok(Self {
            session,
            voice_style,
            vocab,
            phonemizer,
            speed,
            max_input_phonemes,
        })
    }
}

struct Phonemizer {
    dialect: EspeakDialect,
    translator: Translator,
    g2p: Option<misaki_rs::G2P>,
}

impl Phonemizer {
    fn new(
        dialect: EspeakDialect,
        espeak_lang: &str,
        espeak_data_dir: Option<&Path>,
    ) -> Result<Self, TtsError> {
        let g2p = if matches!(dialect, EspeakDialect::EnGb | EspeakDialect::EnUs) {
            use misaki_rs::{language::Language, G2P};
            let lang = match dialect {
                EspeakDialect::EnGb => Language::EnglishGB,
                EspeakDialect::EnUs => Language::EnglishUS,
                EspeakDialect::NonEnglish => unreachable!(),
            };
            Some(G2P::new(lang))
        } else {
            None
        };
        let translator = init_translator(espeak_lang, espeak_data_dir)?;
        Ok(Self {
            dialect,
            translator,
            g2p,
        })
    }

    fn phonemize(&self, text: &str) -> Result<String, TtsError> {
        // https://github.com/hexgrad/misaki/blob/main/misaki/en.py#L54-L61
        let re = regex::Regex::new(r"(\p{Ll})(\p{Lu})").unwrap();
        let sep_replaced = text.replace(['_', '-'], " ");
        let text = re.replace_all(&sep_replaced, "$1 $2");
        if let Some(g2p) = &self.g2p {
            let (_, mut tokens) = g2p
                .g2p(&text)
                .map_err(|e| TtsError::Synthesis(format!("misaki g2p failed: {e}")))?;
            for token in &mut tokens {
                if token.phonemes.as_deref() == Some("❓") {
                    let ipa = self.translator.text_to_ipa(&token.text).map_err(|e| {
                        TtsError::Synthesis(format!("espeak OOV for {:?}: {e}", token.text))
                    })?;
                    token.phonemes = Some(espeak_ipa_to_misaki(&ipa, self.dialect));
                }
            }
            let ps: String = tokens
                .iter()
                .map(|t| format!("{}{}", t.phonemes.as_deref().unwrap_or(""), t.whitespace))
                .collect();
            let ps = zwj_to_single_char(&ps);
            let mut out = ps;
            for p in [" ;", " :", " ,", " .", " !", " ?", " …"] {
                out = out.replace(p, &p[1..]);
            }
            Ok(out)
        } else {
            // Non-English: swap parens to angle quotes so espeak doesn't read
            // `()` as language-switch markers. Mirrors misaki/espeak.py:89-106.
            let espeak_input = text.replace('(', "«").replace(')', "»");
            let raw_ipa = self
                .translator
                .text_to_ipa(&espeak_input)
                .map_err(|e| TtsError::Synthesis(format!("espeak phonemization failed: {e}")))?;
            let raw_ipa = raw_ipa.replace('«', "(").replace('»', ")");
            Ok(espeak_ipa_to_misaki(&raw_ipa, self.dialect))
        }
    }
}

/// Strip any stray ZWJ characters from misaki-rs output.
fn zwj_to_single_char(s: &str) -> String {
    s.replace('\u{200d}', "")
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

/// Convert espeak IPA to misaki's phoneme alphabet. Ports misaki's `EspeakFallback` (English) and `EspeakG2P`
/// (non-English) tables: diphthongs/affricates collapse to single tokens, with
/// dialect-specific branches for GOAT/rhotic vowels (English) and additional
/// affricates `dz→ʣ`, `ts→ʦ`, `ss→S` (non-English).
///
/// espeak-ng-rs emits no tie bars (unlike C espeak's `--tie`), so we match the
/// bare two-char sequences directly. Longest/most-ambiguous sequences first.
fn espeak_ipa_to_misaki(ipa: &str, dialect: EspeakDialect) -> String {
    let mut s = ipa.to_string();

    // Shared diphthongs/affricates — same in misaki's EspeakFallback and
    // EspeakG2P tables.
    for (old, new) in [
        ("aɪ", "I"),
        ("aʊ", "W"),
        ("ɔɪ", "Y"),
        ("eɪ", "A"),
        ("dʒ", "ʤ"),
        ("tʃ", "ʧ"),
        ("ɚ", "əɹ"),
        ("əl", "ᵊl"),
    ] {
        s = s.replace(old, new);
    }

    match dialect {
        EspeakDialect::EnGb => {
            for (old, new) in [("eə", "ɛː"), ("iə", "ɪə"), ("əʊ", "Q")] {
                s = s.replace(old, new);
            }
        }
        EspeakDialect::EnUs => {
            for (old, new) in [("oʊ", "O"), ("ɜːɹ", "ɜɹ"), ("ɜː", "ɜɹ"), ("ɪə", "iə")]
            {
                s = s.replace(old, new);
            }
            s = s.replace('ː', "");
        }
        EspeakDialect::NonEnglish => {
            // misaki/espeak.py EspeakG2P.e2m extras beyond the shared set.
            // GOAT `oʊ`→`O` applies for non-English too (the Romance languages
            // espeak en-US gives produces oʊ for words like Portuguese GOAT;
            // single-char keeps us consistent with the lexicon-trained model).
            for (old, new) in [("oʊ", "O"), ("dz", "ʣ"), ("ts", "ʦ"), ("ss", "S")] {
                s = s.replace(old, new);
            }
        }
    }

    for (old, new) in [
        ("r", "ɹ"),
        ("x", "k"),
        ("ç", "k"),
        ("ɐ", "ə"),
        ("ɬ", "l"),
        ("ʲ", ""),
        ("ɾ", "T"),
        ("ʔ", "t"),
        ("ʰ", ""), // espeak marks aspiration; misaki's lexicon never does
    ] {
        s = s.replace(old, new);
    }

    // Syllabic consonant
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(chars.len());
    let mut i = 0;
    while i < chars.len() {
        if i + 1 < chars.len() && chars[i + 1] == '\u{0329}' {
            out.push('ᵊ');
            out.push(chars[i]);
            i += 2;
        } else {
            out.push(chars[i]);
            i += 1;
        }
    }
    out.replace('\u{0329}', "")
}

impl TtsBackendImpl for KokoroBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<(Vec<f32>, u32), TtsError> {
        let phonemes = self.phonemizer.phonemize(text)?;
        let phonemes = phonemes.trim();
        debug!(
            misaki = self.phonemizer.g2p.is_some(),
            phonemes, "kokoro phonemes"
        );
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

        // Upstream selects pack[len(ps)-1] (kokoro pipeline.py:242), so index
        // by phoneme count minus one. phoneme_ids is non-empty here.
        let style: Vec<f32> = self.voice_style[phoneme_ids.len() - 1].to_vec();

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

#[derive(Clone, Debug)]
pub struct KokoroConfig {
    pub source: String,
    pub voice: String,
    pub language: String,
    pub speed: f32,
    /// Where to extract bundled espeak-ng data on first use. Defaults to
    /// `std::env::temp_dir().join("nobodywho-espeak-ng-data")`. On Android
    /// pass the app's cache dir.
    pub espeak_data_dir: Option<PathBuf>,
}

impl KokoroConfig {
    pub fn new(source: impl AsRef<str>) -> Self {
        Self {
            source: source.as_ref().to_string(),
            voice: "bf_emma".into(),
            language: "en-gb".into(),
            speed: 1.0,
            espeak_data_dir: None,
        }
    }
}
