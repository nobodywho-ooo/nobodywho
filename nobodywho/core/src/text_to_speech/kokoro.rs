//! Kokoro TextToSpeech via ONNX Runtime + espeak-ng phonemization.
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

use crate::errors::TextToSpeechError;
use crate::text_to_speech::architecture::TextToSpeechArchitectureImpl;
use crate::text_to_speech::{TextToSpeechDevice, DEFAULT_SAMPLE_RATE};
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

    /// Split CamelCase / snake_case / kebab-case so the G2P sees normal words,
    /// and fold typographic quotes to ASCII. Word processors and phone
    /// keyboards emit `’` instead of `'` (and `“”` instead of `"`), but
    /// misaki's lexicon only has ASCII-apostrophe entries ("don't", never
    /// "don’t") — without the fold, contractions split and garble
    /// (`don’t` → "don-tee"). Same fold as the supertonic backend and
    /// upstream's old `kokoro.py normalize_text`.
    /// Pre-G2P normalization, returns plain `String` since the result isn't
    /// misaki phonemes yet — just normalized text ready for either backend.
    /// https://github.com/hexgrad/misaki/blob/main/misaki/en.py#L54-L61
    fn normalize_input(text: &str) -> String {
        let re = regex::Regex::new(r"(\p{Ll})(\p{Lu})").unwrap();
        let sep_replaced = text.replace(['_', '-'], " ");
        let quoted = sep_replaced
            .replace(['\u{2018}', '\u{2019}', '´', '`'], "'")
            .replace(['\u{201C}', '\u{201D}'], "\"");
        re.replace_all(&quoted, "$1 $2").into_owned()
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

pub(in crate::text_to_speech) struct KokoroBackend {
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
        device: TextToSpeechDevice,
    ) -> Result<Self, TextToSpeechError> {
        let session = crate::onnx::load_session(&model_dir.join("model.onnx"), device)
            .map_err(TextToSpeechError::Ort)?;
        let vocab = Self::load_vocab(&model_dir.join("config.json"))?;
        let voice = KokoroVoice::load(&model_dir.join("voices"), voice_name)?;

        if !SUPPORTED_LANGS.contains(&language) {
            return Err(TextToSpeechError::UnsupportedLanguage {
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
    fn new(dialect: EspeakDialect, espeak_lang: &str) -> Result<Self, TextToSpeechError> {
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

    fn phonemize(&self, text: &str) -> Result<MisakiPhonemes, TextToSpeechError> {
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
    ) -> Result<MisakiPhonemes, TextToSpeechError> {
        let (_, mut tokens) = g2p
            .g2p(text)
            .map_err(|source| TextToSpeechError::MisakiG2p { source })?;
        self.fill_oov_tokens(&mut tokens)?;
        Ok(MisakiPhonemes::from_tokens(&tokens))
    }

    fn phonemize_non_english(&self, text: &str) -> Result<MisakiPhonemes, TextToSpeechError> {
        let ipa = EspeakIpa::from_text(&self.translator, text)
            .map_err(|source| TextToSpeechError::EspeakPhonemize { source })?;
        Ok(MisakiPhonemes::from_espeak(&ipa, self.dialect))
    }

    fn fill_oov_tokens(&self, tokens: &mut [misaki_rs::MToken]) -> Result<(), TextToSpeechError> {
        for token in tokens.iter_mut() {
            if token.phonemes.as_deref() == Some("❓") {
                let ipa =
                    EspeakIpa::from_text(&self.translator, &token.text).map_err(|source| {
                        TextToSpeechError::EspeakOov {
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
fn init_translator(language: &str) -> Result<Translator, TextToSpeechError> {
    let data_dir = espeak_data_dir();
    std::fs::create_dir_all(&data_dir)
        .map_err(|source| TextToSpeechError::EspeakDataDir { source })?;
    // Extract phonemes + the requested language dict. The language sub-tag
    // ("en-us" → "en") is what bundled-data is keyed on.
    let base_lang = language.split('-').next().unwrap_or(language);
    espeak_ng::install_bundled_language(&data_dir, base_lang).map_err(|source| {
        TextToSpeechError::EspeakInstallLanguage {
            lang: base_lang.into(),
            dir: data_dir.display().to_string(),
            source,
        }
    })?;
    Translator::new(language, Some(data_dir.as_path()))
        .map_err(|source| TextToSpeechError::EspeakInit { source })
}

impl TextToSpeechArchitectureImpl for KokoroBackend {
    /// Synthesize `text`, a piece at a time when its phonemes are more than
    /// one model call takes (the voice's `max_input_phonemes`).
    fn synthesize_raw(&mut self, text: &str) -> Result<Vec<f32>, TextToSpeechError> {
        let phonemes = self.text_to_phonemes(text)?;
        let pieces: Vec<String> = split_phonemes(&phonemes, self.voice.max_input_phonemes(), |c| {
            self.vocab
                .contains_key(c.encode_utf8(&mut [0u8; 4]) as &str)
        })
        .into_iter()
        .map(String::from)
        .collect();
        if pieces.len() > 1 {
            debug!(pieces = pieces.len(), "Kokoro: synthesizing in pieces");
        }

        let mut pcm = Vec::new();
        for piece in &pieces {
            let phoneme_ids = match self.phonemes_to_vocab_ids(piece) {
                // A piece holding only characters Kokoro has no token for says nothing.
                Err(TextToSpeechError::NoVocabMatch) if pieces.len() > 1 => continue,
                result => result?,
            };
            let style = self.voice.style_for_len(phoneme_ids.len()).to_vec();
            pcm.extend(self.run_model(phoneme_ids, style)?);
        }
        if pcm.is_empty() {
            return Err(TextToSpeechError::NoVocabMatch);
        }
        Ok(pcm)
    }

    fn sample_rate(&self) -> u32 {
        DEFAULT_SAMPLE_RATE
    }
}

impl KokoroBackend {
    /// Phonemize `text`, trim it, and check something is left to say.
    fn text_to_phonemes(&self, text: &str) -> Result<String, TextToSpeechError> {
        let phonemes = self.phonemizer.phonemize(text)?;
        let phonemes = phonemes.as_str().trim();
        debug!(
            misaki = self.phonemizer.g2p.is_some(),
            phonemes, "kokoro phonemes"
        );
        if phonemes.is_empty() {
            return Err(TextToSpeechError::NoPhonemes);
        }
        Ok(phonemes.to_string())
    }

    /// Feed `phoneme_ids` + `style` through the ONNX session and extract the
    /// raw PCM. Wraps the token sequence in BOS/EOS (both id 0) to match
    /// upstream's `KModel.forward` — our ONNX export captures only the
    /// `forward_with_tokens` path. See
    /// https://github.com/hexgrad/kokoro/blob/main/kokoro/model.py#L130
    fn run_model(
        &mut self,
        phoneme_ids: Vec<i64>,
        style: Vec<f32>,
    ) -> Result<Vec<f32>, TextToSpeechError> {
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
    fn phonemes_to_vocab_ids(&self, phonemes: &str) -> Result<Vec<i64>, TextToSpeechError> {
        let mut ids: Vec<i64> = Vec::with_capacity(phonemes.len());
        for ch in phonemes.chars() {
            let mut buf = [0u8; 4];
            let s = ch.encode_utf8(&mut buf);
            if let Some(&id) = self.vocab.get(s) {
                ids.push(id);
            }
        }
        if ids.is_empty() {
            return Err(TextToSpeechError::NoVocabMatch);
        }
        if ids.len() > self.voice.max_input_phonemes() {
            return Err(TextToSpeechError::TooManyPhonemes {
                count: ids.len(),
                max: self.voice.max_input_phonemes(),
            });
        }
        Ok(ids)
    }

    /// Read the IPA-character → token-id map from `config.json["vocab"]`.
    fn load_vocab(config_path: &Path) -> Result<HashMap<String, i64>, TextToSpeechError> {
        #[derive(serde::Deserialize)]
        struct Config {
            vocab: HashMap<String, i64>,
        }

        let path = config_path.display().to_string();
        let file =
            std::fs::File::open(config_path).map_err(|source| TextToSpeechError::ConfigOpen {
                path: path.clone(),
                source,
            })?;
        let Config { vocab } =
            serde_json::from_reader(file).map_err(|source| TextToSpeechError::ConfigParse {
                path: path.clone(),
                source,
            })?;
        if vocab.is_empty() {
            return Err(TextToSpeechError::VocabEmpty { path });
        }
        Ok(vocab)
    }
}

/// Split a phoneme string into pieces of at most `max` phonemes, so text longer
/// than one model call can be synthesized a piece at a time. Like upstream
/// Kokoro's pipeline, a piece ends at the last sentence end that fits, else
/// the last clause break, else the last space; only a single word too long
/// for a piece is cut inside the word. `counts` says which characters are
/// phonemes (have a vocab entry); others, such as spaces, don't count.
fn split_phonemes(phonemes: &str, max: usize, counts: impl Fn(char) -> bool) -> Vec<&str> {
    const SENTENCE_ENDS: &[char] = &['.', '!', '?'];
    const CLAUSE_BREAKS: &[char] = &[',', ';', ':', '—', '…'];
    let max = max.max(1);

    let mut pieces = Vec::new();
    let mut rest = phonemes.trim();
    while !rest.is_empty() {
        // Byte offset of the first phoneme past the limit, if there is one.
        let over = rest
            .char_indices()
            .filter(|&(_, c)| counts(c))
            .nth(max)
            .map(|(i, _)| i);
        let Some(over) = over else {
            pieces.push(rest);
            break;
        };

        let window = &rest[..over];
        let after_last = |marks: &[char]| {
            window
                .char_indices()
                .rev()
                .find(|&(_, c)| marks.contains(&c))
                .map(|(i, c)| i + c.len_utf8())
        };
        let end = after_last(SENTENCE_ENDS)
            .or_else(|| after_last(CLAUSE_BREAKS))
            .or_else(|| window.rfind(char::is_whitespace).filter(|&i| i > 0))
            .unwrap_or(over);

        let (piece, tail) = rest.split_at(end);
        if !piece.trim().is_empty() {
            pieces.push(piece.trim());
        }
        rest = tail.trim_start();
    }
    pieces
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
    fn load(voices_dir: &Path, voice: &str) -> Result<Self, TextToSpeechError> {
        let path = voices_dir.join(format!("{voice}.safetensors"));
        let bytes = std::fs::read(&path).map_err(|source| TextToSpeechError::VoiceRead {
            voice: voice.into(),
            source,
        })?;
        let safetensors =
            SafeTensors::deserialize(&bytes).map_err(|source| TextToSpeechError::VoiceParse {
                voice: voice.into(),
                source,
            })?;
        let style_tensor =
            safetensors
                .tensor("style")
                .map_err(|source| TextToSpeechError::VoiceMissingStyle {
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
    ) -> Result<usize, TextToSpeechError> {
        if view.dtype() != Dtype::F32 {
            return Err(TextToSpeechError::VoiceBadDtype {
                voice: voice.into(),
                dtype: view.dtype(),
            });
        }
        let shape = view.shape();
        if shape.len() != 2 || shape[1] != STYLE_DIM || shape[0] < 2 {
            return Err(TextToSpeechError::VoiceBadShape {
                voice: voice.into(),
                shape: shape.to_vec(),
                style_dim: STYLE_DIM,
            });
        }
        Ok(shape[0])
    }

    fn decode_style_rows(view: &safetensors::tensor::TensorView<'_>) -> Vec<[f32; STYLE_DIM]> {
        view.data()
            .as_chunks::<4>()
            .0
            .iter()
            .copied()
            .map(f32::from_le_bytes)
            .collect::<Vec<f32>>()
            .as_chunks::<STYLE_DIM>()
            .0
            .to_vec()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn phonemize_en_us(phonemizer: &Phonemizer, text: &str) -> String {
        phonemizer.phonemize(text).expect("phonemize").into_inner()
    }

    /// Typographic quotes must phonemize exactly like their ASCII
    /// counterparts. Word processors and phone autocorrect emit `’` instead
    /// of `'` (and `“”` instead of `"`), but misaki's lexicon only has
    /// ASCII-apostrophe entries ("don't", never "don’t") — without the fold
    /// in [`MisakiPhonemes::normalize_input`], contractions split and garble
    /// (`don’t` → "don-tee").
    #[test]
    fn phonemizes_typographic_punctuation_like_ascii() {
        // Sandbox-proof espeak data dir: the nix sandbox has no writable
        // $HOME/.cache, and first-use extraction needs somewhere to write.
        std::env::set_var(
            "NOBODYWHO_ESPEAK_DATA_DIR",
            std::env::temp_dir().join("nobodywho-espeak-test"),
        );
        let p = Phonemizer::new(EspeakDialect::EnUs, "en-us").expect("init phonemizer");

        let ascii = phonemize_en_us(&p, "I don't like it, it's Bob's dog.");
        for variant in [
            "I don’t like it, it’s Bob’s dog.", // U+2019
            "I don‘t like it, it‘s Bob‘s dog.", // U+2018
            "I don´t like it, it´s Bob´s dog.", // acute accent
            "I don`t like it, it`s Bob`s dog.", // grave accent
        ] {
            assert_eq!(ascii, phonemize_en_us(&p, variant), "variant: {variant:?}");
        }

        assert_eq!(
            phonemize_en_us(&p, "He said “don’t” and left."),
            phonemize_en_us(&p, "He said \"don't\" and left."),
        );
    }

    /// Unit tests count every non-space character as a phoneme.
    fn counts(c: char) -> bool {
        !c.is_whitespace()
    }

    #[test]
    fn short_phonemes_stay_whole() {
        assert_eq!(
            split_phonemes("həlˈO wˈɜɹld.", 50, counts),
            vec!["həlˈO wˈɜɹld."]
        );
    }

    #[test]
    fn splits_at_sentence_ends_before_clauses() {
        assert_eq!(
            split_phonemes("aa bb, cc. dd ee, ff. gg hh.", 12, counts),
            vec!["aa bb, cc.", "dd ee, ff.", "gg hh."]
        );
    }

    #[test]
    fn falls_back_to_clauses_then_words_then_a_hard_split() {
        assert_eq!(
            split_phonemes("aaa bbb, ccc ddd", 9, counts),
            vec!["aaa bbb,", "ccc ddd"]
        );
        assert_eq!(
            split_phonemes("aaa bbb ccc ddd", 7, counts),
            vec!["aaa bbb", "ccc ddd"]
        );
        assert_eq!(
            split_phonemes("abcdefghij", 4, counts),
            vec!["abcd", "efgh", "ij"]
        );
    }

    #[test]
    fn uncounted_characters_do_not_count_toward_the_limit() {
        assert_eq!(split_phonemes("a b c d e", 4, counts), vec!["a b c d", "e"]);
    }

    #[test]
    fn every_piece_fits_and_nothing_is_lost() {
        let phonemes = (0..120)
            .map(|i| match i % 7 {
                3 => "ðə kˈæt,",
                6 => "sˈæt dˈWn.",
                _ => "wˈʌn mˈɔɹ wˈɜɹd",
            })
            .collect::<Vec<_>>()
            .join(" ");
        let squash = |s: &str| s.chars().filter(|&c| counts(c)).collect::<String>();

        for max in [1, 5, 37, 100, 509] {
            let pieces = split_phonemes(&phonemes, max, counts);
            assert!(pieces.iter().all(|p| !p.is_empty()));
            assert!(
                pieces.iter().all(|p| squash(p).chars().count() <= max),
                "a piece is over {max} phonemes"
            );
            assert_eq!(squash(&pieces.concat()), squash(&phonemes), "max {max}");
        }
    }

    /// Text far over one model call's phoneme limit (509 for the stock
    /// voices) is read a piece at a time instead of failing.
    #[test]
    fn synthesizes_text_longer_than_one_model_call() {
        let Ok(source) = std::env::var("TEST_TTS_SOURCE") else {
            eprintln!("skipping: TEST_TTS_SOURCE is not set");
            return;
        };
        std::env::set_var(
            "NOBODYWHO_ESPEAK_DATA_DIR",
            std::env::temp_dir().join("nobodywho-espeak-test"),
        );
        let tts = crate::text_to_speech::TextToSpeech::new(
            crate::text_to_speech::TextToSpeechConfig::from_source(&source, None)
                .expect("a Kokoro source"),
        )
        .expect("load Kokoro");

        let paragraph = "Lighthouses were once the only way ships could find their way \
            home at night. Each one had its own pattern of flashes, so a captain could tell \
            exactly which coast he was looking at. The keepers lived beside the lamp all \
            year, trimming wicks and polishing glass.";
        let one = tts.synthesize(paragraph).expect("one paragraph");
        let four = tts
            .synthesize([paragraph; 4].join(" "))
            .expect("four paragraphs, well over the phoneme limit");

        assert!(
            four.len() > one.len() * 3,
            "four paragraphs should be about four times as long ({} vs {} bytes)",
            four.len(),
            one.len()
        );
    }
}

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
            voice: "bf_emma".into(),
            language: "en-gb".into(),
            speed: 1.0,
        }
    }
}
