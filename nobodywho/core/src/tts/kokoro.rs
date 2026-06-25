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
use misaki_rs::{language::Language, G2P};
use ort::session::Session;
use ort::value::Tensor;
use safetensors::tensor::{Dtype, SafeTensors};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use tracing::{debug, info};

const STYLE_DIM: usize = 256;
const SUPPORTED_LANGS: &[&str] = &["en-us", "en-gb", "es", "fr", "it", "pt-br"];

/// Diphthongs and affricates shared by misaki's `EspeakFallback` (English)
/// and `EspeakG2P` (non-English) e2m tables.
const E2M_SHARED: &[(&str, &str)] = &[
    ("aɪ", "I"),
    ("aʊ", "W"),
    ("ɔɪ", "Y"),
    ("eɪ", "A"),
    ("dʒ", "ʤ"),
    ("tʃ", "ʧ"),
    ("ɚ", "əɹ"),
    ("əl", "ᵊl"),
];

/// Shared consonant cleanup applied to all dialects. `ʰ` stripped because
/// espeak marks aspiration but misaki's lexicon never does.
const E2M_CONSONANT_CLEANUP: &[(&str, &str)] = &[
    ("r", "ɹ"),
    ("x", "k"),
    ("ç", "k"),
    ("ɐ", "ə"),
    ("ɬ", "l"),
    ("ʲ", ""),
    ("ɾ", "T"),
    ("ʔ", "t"),
    ("ʰ", ""),
];

#[derive(Debug, Copy, Clone)]
enum EspeakDialect {
    EnGb,
    EnUs,
    NonEnglish,
}

/// IPA emitted by espeak-ng (the `Translator::text_to_ipa` output).
/// Distinct from `MisakiPhonemes` because Kokoro was trained on a different
/// alphabet — diphthongs/affricates collapsed to single tokens, etc.
#[derive(Debug)]
struct EspeakIpa(String);

impl EspeakIpa {
    /// Run espeak on `text`, swapping `()` to `«»` first so espeak doesn't
    /// interpret them as language-switch markers (misaki/espeak.py:89-106).
    /// Returns the bare espeak error so callers can wrap with the right
    /// context (full-text vs per-OOV-word).
    fn from_text(translator: &Translator, text: &str) -> Result<Self, espeak_ng::Error> {
        let input = text.replace('(', "«").replace(')', "»");
        let raw = translator.text_to_ipa(&input)?;
        Ok(Self(raw.replace('«', "(").replace('»', ")")))
    }
}

/// Phoneme string in misaki's alphabet — what Kokoro's `vocab` indexes
/// against. Produced from text via misaki-rs, or from an `EspeakIpa` via
/// the e2m rules.
#[derive(Debug)]
struct MisakiPhonemes(String);

impl MisakiPhonemes {
    /// Port of misaki's `EspeakFallback.e2m` (English) and `EspeakG2P.e2m`
    /// (non-English) tables.
    fn from_espeak(ipa: &EspeakIpa, dialect: EspeakDialect) -> Self {
        Self(ipa.0.clone())
            .apply(E2M_SHARED)
            .apply(Self::dialect_rules(dialect))
            .apply(E2M_CONSONANT_CLEANUP)
            .rewrite_syllabic_consonants()
    }

    /// Per-dialect IPA → misaki replacements, applied after the shared
    /// diphthong/affricate pass and before shared consonant cleanup.
    /// Order within a slice matters (longer / more specific sequences first).
    fn dialect_rules(dialect: EspeakDialect) -> &'static [(&'static str, &'static str)] {
        match dialect {
            // British: keep length mark `ː`.
            EspeakDialect::EnGb => &[("eə", "ɛː"), ("iə", "ɪə"), ("əʊ", "Q")],
            // American: rhotic + GOAT, then strip every remaining `ː`.
            EspeakDialect::EnUs => &[
                ("oʊ", "O"),
                ("ɜːɹ", "ɜɹ"),
                ("ɜː", "ɜɹ"),
                ("ɪə", "iə"),
                ("ː", ""),
            ],
            // Non-English extras from `EspeakG2P.e2m`. GOAT `oʊ`→`O` applies
            // here too (espeak emits oʊ for Romance-language GOAT-class
            // words; single-char keeps us aligned with the lexicon).
            EspeakDialect::NonEnglish => &[("oʊ", "O"), ("dz", "ʣ"), ("ts", "ʦ"), ("ss", "S")],
        }
    }

    /// Constructor from the misaki-rs token stream (English G2P path).
    /// Each token contributes `phonemes + trailing whitespace`; then we
    /// strip ZWJs that misaki-rs occasionally emits.
    fn from_tokens(tokens: &[misaki_rs::MToken]) -> Self {
        let joined: String = tokens
            .iter()
            .map(|t| format!("{}{}", t.phonemes.as_deref().unwrap_or(""), t.whitespace))
            .collect();
        Self(joined).strip_zwj()
    }

    /// Split CamelCase / snake_case / kebab-case so the G2P sees normal words.
    /// Pre-G2P normalization, returns plain `String` since the result isn't
    /// misaki phonemes yet — just normalized text ready for either backend.
    /// https://github.com/hexgrad/misaki/blob/main/misaki/en.py#L54-L61
    fn normalize_input(text: &str) -> String {
        let re = regex::Regex::new(r"(\p{Ll})(\p{Lu})").unwrap();
        let sep_replaced = text.replace(['_', '-'], " ");
        re.replace_all(&sep_replaced, "$1 $2").into_owned()
    }

    fn apply(mut self, pairs: &[(&str, &str)]) -> Self {
        for (old, new) in pairs {
            self.0 = self.0.replace(old, new);
        }
        self
    }

    /// Strip stray ZWJ (U+200D) characters that misaki-rs occasionally emits.
    fn strip_zwj(mut self) -> Self {
        self.0 = self.0.replace('\u{200d}', "");
        self
    }

    /// Rewrite `<consonant>\u{0329}` (e.g. `n̩`, `l̩`) as `ᵊ<consonant>` to match
    /// misaki's lexicon, which spells syllabic consonants with a leading schwa.
    /// Stray syllabic marks with no preceding consonant are dropped.
    fn rewrite_syllabic_consonants(self) -> Self {
        const SYLLABIC: char = '\u{0329}';
        let mut out = String::with_capacity(self.0.len());
        let mut chars = self.0.chars().peekable();
        while let Some(c) = chars.next() {
            if c == SYLLABIC {
                continue;
            }
            if chars.peek() == Some(&SYLLABIC) {
                chars.next();
                out.push('ᵊ');
            }
            out.push(c);
        }
        Self(out)
    }

    fn as_str(&self) -> &str {
        &self.0
    }

    /// Unwrap for callers that need to stuff the phonemes back into
    /// misaki-rs's `MToken::phonemes: Option<String>` (the OOV path).
    fn into_inner(self) -> String {
        self.0
    }
}

pub(in crate::tts) struct KokoroBackend {
    session: Session,
    voice: KokoroVoice,
    /// IPA character → token id
    vocab: HashMap<String, i64>,
    phonemizer: Phonemizer,
    speed: f32,
}

impl KokoroBackend {
    pub fn new(
        model_dir: &Path,
        voice_name: &str,
        language: &str,
        speed: f32,
        device: TtsDevice,
    ) -> Result<Self, TtsError> {
        let session = ort_util::load_session(&model_dir.join("model.onnx"), device)?;
        let vocab = Self::load_vocab(&model_dir.join("config.json"))?;
        let voice = KokoroVoice::load(&model_dir.join("voices"), voice_name)?;

        if !SUPPORTED_LANGS.contains(&language) {
            return Err(TtsError::UnsupportedLanguage {
                language: language.into(),
                supported: SUPPORTED_LANGS.join(", "),
            });
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
        let phonemizer = Phonemizer::new(dialect, espeak_lang)?;

        info!(
            voice = voice_name,
            language,
            espeak_lang,
            misaki = phonemizer.g2p.is_some(),
            max_input_phonemes = voice.max_input_phonemes(),
            vocab_len = vocab.len(),
            "Loaded Kokoro model"
        );

        Ok(Self {
            session,
            voice,
            vocab,
            phonemizer,
            speed,
        })
    }
}

struct Phonemizer {
    dialect: EspeakDialect,
    translator: Translator,
    g2p: Option<misaki_rs::G2P>,
}

impl Phonemizer {
    fn new(dialect: EspeakDialect, espeak_lang: &str) -> Result<Self, TtsError> {
        let g2p = match dialect {
            EspeakDialect::EnGb => Some(G2P::new(Language::EnglishGB)),
            EspeakDialect::EnUs => Some(G2P::new(Language::EnglishUS)),
            EspeakDialect::NonEnglish => None,
        };
        let translator = init_translator(espeak_lang)?;
        Ok(Self {
            dialect,
            translator,
            g2p,
        })
    }

    fn phonemize(&self, text: &str) -> Result<MisakiPhonemes, TtsError> {
        let text = MisakiPhonemes::normalize_input(text);
        match &self.g2p {
            Some(g2p) => self.phonemize_english(g2p, &text),
            None => self.phonemize_non_english(&text),
        }
    }

    fn phonemize_english(
        &self,
        g2p: &misaki_rs::G2P,
        text: &str,
    ) -> Result<MisakiPhonemes, TtsError> {
        let (_, mut tokens) = g2p
            .g2p(text)
            .map_err(|source| TtsError::MisakiG2p { source })?;
        self.fill_oov_tokens(&mut tokens)?;
        Ok(MisakiPhonemes::from_tokens(&tokens))
    }

    fn phonemize_non_english(&self, text: &str) -> Result<MisakiPhonemes, TtsError> {
        let ipa = EspeakIpa::from_text(&self.translator, text)
            .map_err(|source| TtsError::EspeakPhonemize { source })?;
        Ok(MisakiPhonemes::from_espeak(&ipa, self.dialect))
    }

    fn fill_oov_tokens(&self, tokens: &mut [misaki_rs::MToken]) -> Result<(), TtsError> {
        for token in tokens.iter_mut() {
            if token.phonemes.as_deref() == Some("❓") {
                let ipa =
                    EspeakIpa::from_text(&self.translator, &token.text).map_err(|source| {
                        TtsError::EspeakOov {
                            word: token.text.clone(),
                            source,
                        }
                    })?;
                token.phonemes = Some(MisakiPhonemes::from_espeak(&ipa, self.dialect).into_inner());
            }
        }
        Ok(())
    }
}

/// Pick a writable directory for the extracted espeak-ng data.
///
/// Resolution order:
/// 1. `NOBODYWHO_ESPEAK_DATA_DIR` env var — set by the Android JNI bridge
///    from `Context.getCacheDir()` since neither `dirs::cache_dir()` nor
///    `std::env::temp_dir()` give a writable per-app path there.
/// 2. `dirs::cache_dir()` — per-user cache on desktop platforms (skipped
///    on Android; the `dirs` crate isn't a dep there).
/// 3. `std::env::temp_dir()` — last-ditch fallback.
fn espeak_data_dir() -> PathBuf {
    if let Ok(p) = std::env::var("NOBODYWHO_ESPEAK_DATA_DIR") {
        return PathBuf::from(p);
    }
    #[cfg(not(target_os = "android"))]
    if let Some(d) = dirs::cache_dir() {
        return d.join("nobodywho").join("espeak-ng-data");
    }
    std::env::temp_dir().join("nobodywho-espeak-ng-data")
}

/// Extract bundled espeak-ng data on first use and build a Translator, idempotent.
fn init_translator(language: &str) -> Result<Translator, TtsError> {
    let data_dir = espeak_data_dir();
    std::fs::create_dir_all(&data_dir).map_err(|source| TtsError::EspeakDataDir { source })?;
    // Extract phonemes + the requested language dict. The language sub-tag
    // ("en-us" → "en") is what bundled-data is keyed on.
    let base_lang = language.split('-').next().unwrap_or(language);
    espeak_ng::install_bundled_language(&data_dir, base_lang).map_err(|source| {
        TtsError::EspeakInstallLanguage {
            lang: base_lang.into(),
            dir: data_dir.display().to_string(),
            source,
        }
    })?;
    Translator::new(language, Some(data_dir.as_path()))
        .map_err(|source| TtsError::EspeakInit { source })
}

impl TtsBackendImpl for KokoroBackend {
    fn synthesize_raw(&mut self, text: &str) -> Result<Vec<f32>, TtsError> {
        let phoneme_ids = self.text_to_phoneme_ids(text)?;
        let style = self.voice.style_for_len(phoneme_ids.len()).to_vec();
        self.run_model(phoneme_ids, style)
    }

    fn sample_rate(&self) -> u32 {
        DEFAULT_SAMPLE_RATE
    }
}

impl KokoroBackend {
    /// Run the full text → phoneme-ID pipeline: phonemize, trim, validate
    /// non-empty, then look up each phoneme in the vocab.
    fn text_to_phoneme_ids(&self, text: &str) -> Result<Vec<i64>, TtsError> {
        let phonemes = self.phonemizer.phonemize(text)?;
        let phonemes = phonemes.as_str().trim();
        debug!(
            misaki = self.phonemizer.g2p.is_some(),
            phonemes, "kokoro phonemes"
        );
        if phonemes.is_empty() {
            return Err(TtsError::NoPhonemes);
        }
        self.phonemes_to_vocab_ids(phonemes)
    }

    /// Feed `phoneme_ids` + `style` through the ONNX session and extract the
    /// raw PCM. Wraps the token sequence in BOS/EOS (both id 0) to match
    /// upstream's `KModel.forward` — our ONNX export captures only the
    /// `forward_with_tokens` path. See
    /// https://github.com/hexgrad/kokoro/blob/main/kokoro/model.py#L130
    fn run_model(&mut self, phoneme_ids: Vec<i64>, style: Vec<f32>) -> Result<Vec<f32>, TtsError> {
        let n_phonemes = phoneme_ids.len();
        let tokens = Self::wrap_bos_eos(phoneme_ids);
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
            phoneme_ids = n_phonemes,
            pcm_samples = pcm.len(),
            pcm_duration_s = pcm.len() as f32 / DEFAULT_SAMPLE_RATE as f32,
            "Kokoro: done"
        );
        Ok(pcm)
    }

    fn wrap_bos_eos(ids: Vec<i64>) -> Vec<i64> {
        let mut out = Vec::with_capacity(ids.len() + 2);
        out.push(0);
        out.extend(ids);
        out.push(0);
        out
    }

    /// Look up each phoneme character in Kokoro's vocab and collect the
    /// resulting token IDs. Characters with no vocab entry are dropped
    /// silently, matching upstream — see
    /// https://github.com/hexgrad/kokoro/blob/main/kokoro/model.py#L128
    fn phonemes_to_vocab_ids(&self, phonemes: &str) -> Result<Vec<i64>, TtsError> {
        let mut ids: Vec<i64> = Vec::with_capacity(phonemes.len());
        for ch in phonemes.chars() {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            if let Some(&id) = self.vocab.get(s) {
                ids.push(id);
            }
        }
        if ids.is_empty() {
            return Err(TtsError::NoVocabMatch);
        }
        if ids.len() > self.voice.max_input_phonemes() {
            return Err(TtsError::TooManyPhonemes {
                count: ids.len(),
                max: self.voice.max_input_phonemes(),
            });
        }
        Ok(ids)
    }

    /// Read the IPA-character → token-id map from `config.json["vocab"]`.
    fn load_vocab(config_path: &Path) -> Result<HashMap<String, i64>, TtsError> {
        #[derive(serde::Deserialize)]
        struct Config {
            vocab: HashMap<String, i64>,
        }

        let path = config_path.display().to_string();
        let file = std::fs::File::open(config_path).map_err(|source| TtsError::ConfigOpen {
            path: path.clone(),
            source,
        })?;
        let Config { vocab } =
            serde_json::from_reader(file).map_err(|source| TtsError::ConfigParse {
                path: path.clone(),
                source,
            })?;
        if vocab.is_empty() {
            return Err(TtsError::VocabEmpty { path });
        }
        Ok(vocab)
    }
}

/// A Kokoro voice's style vectors, indexed by input phoneme count.
///
/// Kokoro doesn't use a single voice embedding — it ships one 256-d style
/// vector per possible input length. So `voices/<voice>.safetensors` holds a
/// `[rows, 256]` tensor; at inference we pick row `len(phonemes) - 1`
/// (matching upstream `pack[len(ps)-1]`).
struct KokoroVoice {
    style: Vec<[f32; STYLE_DIM]>,
    /// Largest input length this voice has a style row for (= `rows - 1`).
    max_input_phonemes: usize,
}

impl KokoroVoice {
    /// Load `<voices_dir>/<voice>.safetensors`.
    fn load(voices_dir: &Path, voice: &str) -> Result<Self, TtsError> {
        let path = voices_dir.join(format!("{voice}.safetensors"));
        let bytes = std::fs::read(&path).map_err(|source| TtsError::VoiceRead {
            voice: voice.into(),
            source,
        })?;
        let safetensors =
            SafeTensors::deserialize(&bytes).map_err(|source| TtsError::VoiceParse {
                voice: voice.into(),
                source,
            })?;
        let style_tensor =
            safetensors
                .tensor("style")
                .map_err(|source| TtsError::VoiceMissingStyle {
                    voice: voice.into(),
                    source,
                })?;
        let rows = Self::validate_style_shape(&style_tensor, voice)?;
        Ok(Self {
            style: Self::decode_style_rows(&style_tensor),
            max_input_phonemes: rows - 1,
        })
    }

    /// Pick the style row for an input of `n_phonemes` phonemes. Upstream
    /// uses `pack[len(ps)-1]` (kokoro pipeline.py:242). Caller must ensure
    /// `1 <= n_phonemes <= self.max_input_phonemes()`.
    fn style_for_len(&self, n_phonemes: usize) -> &[f32; STYLE_DIM] {
        &self.style[n_phonemes - 1]
    }

    fn max_input_phonemes(&self) -> usize {
        self.max_input_phonemes
    }

    fn validate_style_shape(
        view: &safetensors::tensor::TensorView<'_>,
        voice: &str,
    ) -> Result<usize, TtsError> {
        if view.dtype() != Dtype::F32 {
            return Err(TtsError::VoiceBadDtype {
                voice: voice.into(),
                dtype: view.dtype(),
            });
        }
        let shape = view.shape();
        if shape.len() != 2 || shape[1] != STYLE_DIM || shape[0] < 2 {
            return Err(TtsError::VoiceBadShape {
                voice: voice.into(),
                shape: shape.to_vec(),
                style_dim: STYLE_DIM,
            });
        }
        Ok(shape[0])
    }

    fn decode_style_rows(view: &safetensors::tensor::TensorView<'_>) -> Vec<[f32; STYLE_DIM]> {
        view.data()
            .chunks_exact(4)
            .map(|c| f32::from_le_bytes(c.try_into().unwrap()))
            .collect::<Vec<f32>>()
            .chunks_exact(STYLE_DIM)
            .map(|row| row.try_into().expect("STYLE_DIM-sized chunk"))
            .collect()
    }
}

/// Configuration for a Kokoro TTS model.
///
/// Build one with [`KokoroConfig::new`] and then override fields as needed.
/// `voice` and `language` must agree (see the language/voice table in the TTS docs).
#[derive(Clone, Debug)]
pub struct KokoroConfig {
    /// HuggingFace repo id (`owner/repo`) or path to a local model directory.
    pub source: String,

    /// Voice name. Must match a voice available for the chosen `language`
    /// (e.g. `af_heart` for `en-us`, `bf_emma` for `en-gb`, `ff_siwis` for `fr`).
    /// Defaults to `bf_emma`.
    pub voice: String,

    /// Language code. Must agree with `voice` (e.g. `en-us`, `en-gb`, `es`, `fr`,
    /// `it`, `pt-br`). Japanese (`ja`) and Chinese (`zh`) are not supported.
    /// Defaults to `en-gb`.
    pub language: String,

    /// Speech speed multiplier. Values greater than `1.0` speed the audio up,
    /// less than `1.0` slow it down. Defaults to `1.0`.
    pub speed: f32,
}

impl KokoroConfig {
    /// Create a config with defaults for the given `source`.
    ///
    /// `source` is a HuggingFace repo id (`owner/repo`) or a local model directory.
    pub fn new(source: impl AsRef<str>) -> Self {
        Self {
            source: source.as_ref().to_string(),
            voice: "bf_emma".into(),
            language: "en-gb".into(),
            speed: 1.0,
        }
    }
}
