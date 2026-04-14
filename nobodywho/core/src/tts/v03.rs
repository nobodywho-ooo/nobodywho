use crate::errors::TtsWorkerError;
use crate::tts::{PreparedTtsRequest, TtsSpeaker, TtsSpeakerProfile, TtsSpeakerWord};

pub(crate) fn process_text(text: &str) -> String {
    let mut normalized = String::new();
    let mut pending_space = false;
    let chars: Vec<char> = text.to_lowercase().chars().collect();
    let mut i = 0;

    while i < chars.len() {
        let c = chars[i];

        if i + 2 < chars.len() && c == '.' && chars[i + 1] == '.' && chars[i + 2] == '.' {
            normalized.push_str("<|ellipsis|>");
            pending_space = false;
            i += 3;
            continue;
        }

        let mapped = match c {
            '.' => Some("<|period|>"),
            '!' => Some("<|exclamation_mark|>"),
            '?' => Some("<|question_mark|>"),
            ',' => Some("<|comma|>"),
            '"' => Some("<|double_quote|>"),
            '„' => Some("<|low_double_quote|>"),
            '¡' => Some("<|inverted_exclamation|>"),
            '¿' => Some("<|inverted_question|>"),
            '…' => Some("<|ellipsis|>"),
            '。' => Some("<|cjk_period|>"),
            '！' => Some("<|cjk_exclamation|>"),
            '？' => Some("<|cjk_question|>"),
            '，' => Some("<|cjk_comma|>"),
            '؟' => Some("<|arabic_question|>"),
            _ => None,
        };

        if let Some(token) = mapped {
            normalized.push_str(token);
            pending_space = false;
            i += 1;
            continue;
        }

        match c {
            '-' | '_' | '/' | '\\' => pending_space = !normalized.is_empty(),
            c if c.is_whitespace() => pending_space = !normalized.is_empty(),
            c if c.is_alphabetic() => {
                if pending_space && !normalized.is_empty() {
                    normalized.push(' ');
                }
                normalized.push(c);
                pending_space = false;
            }
            _ => {}
        }

        i += 1;
    }

    normalized.replace(' ', "<|space|>")
}

pub(crate) fn prompt(request: &PreparedTtsRequest) -> Result<String, TtsWorkerError> {
    match &request.speaker {
        TtsSpeaker::Preset(_) => Ok(build_prompt_no_speaker(&request.processed_text)),
        TtsSpeaker::Profile(profile) => build_prompt(&request.processed_text, profile),
    }
}

fn build_prompt_no_speaker(processed_text: &str) -> String {
    format!("<|im_start|>\n<|text_start|>{processed_text}<|text_end|>\n<|audio_start|>\n")
}

fn build_prompt(
    processed_text: &str,
    profile: &TtsSpeakerProfile,
) -> Result<String, TtsWorkerError> {
    profile.validate()?;

    let speaker_text = profile
        .words
        .iter()
        .map(|w| process_text(&w.word))
        .collect::<Vec<_>>()
        .join("<|space|>");

    let full_text = format!("{}<|space|>{}", speaker_text, processed_text);

    let mut audio_prefix = String::new();
    for (i, word) in profile.words.iter().enumerate() {
        if i > 0 {
            audio_prefix.push_str("<|space|>\n");
        }
        audio_prefix.push_str(&serialize_profile_word(word)?);
    }
    audio_prefix.push_str("<|space|>\n");

    Ok(format!(
        "<|im_start|>\n<|text_start|>{full_text}<|text_end|>\n<|audio_start|>\n{audio_prefix}"
    ))
}

fn serialize_profile_word(word: &TtsSpeakerWord) -> Result<String, TtsWorkerError> {
    let normalized_word = process_text(&word.word);
    if normalized_word.is_empty() {
        return Err(TtsWorkerError::InvalidRequest(format!(
            "speaker profile word {:?} becomes empty after normalization",
            word.word
        )));
    }

    let mut serialized = normalized_word;
    serialized.push_str(&format_duration_token(word.duration));
    for &code in &word.codes {
        serialized.push_str(&format!("<|{code}|>"));
    }
    Ok(serialized)
}

fn format_duration_token(duration: f32) -> String {
    let centiseconds = (duration.max(0.0) * 100.0).round() as i32;
    let centiseconds = centiseconds.clamp(0, 1000);
    format!("<|t_{}.{:02}|>", centiseconds / 100, centiseconds % 100)
}
