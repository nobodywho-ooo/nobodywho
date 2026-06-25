use regex::Regex;
use unicode_normalization::UnicodeNormalization;

pub(super) fn normalize_tts_text(text: &str) -> String {
    let text: String = text.nfkd().collect();
    let text = remove_emoji(&text);
    let text = normalize_tts_symbols(&text);
    let text = normalize_tts_spacing(&text);
    ensure_terminal_punctuation(&text)
}

fn remove_emoji(text: &str) -> String {
    Regex::new(r"[\x{1F600}-\x{1F64F}\x{1F300}-\x{1F5FF}\x{1F680}-\x{1F6FF}\x{1F700}-\x{1F77F}\x{1F780}-\x{1F7FF}\x{1F800}-\x{1F8FF}\x{1F900}-\x{1F9FF}\x{1FA00}-\x{1FA6F}\x{1FA70}-\x{1FAFF}\x{2600}-\x{26FF}\x{2700}-\x{27BF}\x{1F1E6}-\x{1F1FF}]+")
        .unwrap()
        .replace_all(text, "")
        .to_string()
}

fn normalize_tts_symbols(text: &str) -> String {
    let mut normalized = String::new();
    for character in text.chars() {
        match character {
            '–' | '‑' | '—' => normalized.push('-'),
            '_' | '[' | ']' | '|' | '/' | '#' | '→' | '←' => normalized.push(' '),
            '\u{201C}' | '\u{201D}' => normalized.push('"'),
            '\u{2018}' | '\u{2019}' | '´' | '`' => normalized.push('\''),
            '♥' | '☆' | '♡' | '©' | '\\' => {}
            '@' => normalized.push_str(" at "),
            _ => normalized.push(character),
        }
    }
    normalized
}

fn normalize_tts_spacing(text: &str) -> String {
    let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
    let mut normalized = String::new();

    for character in collapsed.chars() {
        if is_closing_punctuation(character) && normalized.ends_with(' ') {
            normalized.pop();
        }
        normalized.push(character);
    }

    normalized.trim().to_string()
}

fn ensure_terminal_punctuation(text: &str) -> String {
    let mut text = text.trim().to_string();
    if !text.is_empty() && !text.chars().last().is_some_and(is_terminal_punctuation) {
        text.push('.');
    }
    text
}

fn is_closing_punctuation(character: char) -> bool {
    matches!(character, ',' | '.' | '!' | '?' | ';' | ':' | '\'')
}

fn is_terminal_punctuation(character: char) -> bool {
    matches!(
        character,
        '.' | '!'
            | '?'
            | ';'
            | ':'
            | ','
            | '\''
            | '"'
            | ')'
            | ']'
            | '}'
            | '…'
            | '。'
            | '」'
            | '』'
            | '】'
            | '〉'
            | '》'
            | '›'
            | '»'
    )
}

pub(super) fn chunk_text(text: &str, max_len: usize) -> Vec<String> {
    let text = text.trim();
    if text.is_empty() {
        return vec![String::new()];
    }

    let mut chunks = Vec::new();
    for paragraph in Regex::new(r"\n\s*\n").unwrap().split(text) {
        let paragraph = paragraph.trim();
        if paragraph.is_empty() {
            continue;
        }
        if paragraph.len() <= max_len {
            chunks.push(paragraph.to_string());
            continue;
        }
        push_sentence_chunks(paragraph, max_len, &mut chunks);
    }

    if chunks.is_empty() {
        vec![String::new()]
    } else {
        chunks
    }
}

fn push_sentence_chunks(text: &str, max_len: usize, chunks: &mut Vec<String>) {
    let mut current = String::new();
    let mut current_len = 0;

    for sentence in split_sentences(text) {
        let sentence = sentence.trim();
        if sentence.is_empty() {
            continue;
        }
        if sentence.len() > max_len {
            flush_chunk(&mut current, &mut current_len, chunks);
            push_long_sentence(sentence, max_len, chunks);
            continue;
        }
        if current_len + sentence.len() + 1 > max_len {
            flush_chunk(&mut current, &mut current_len, chunks);
        }
        if !current.is_empty() {
            current.push(' ');
            current_len += 1;
        }
        current.push_str(sentence);
        current_len += sentence.len();
    }

    flush_chunk(&mut current, &mut current_len, chunks);
}

fn push_long_sentence(sentence: &str, max_len: usize, chunks: &mut Vec<String>) {
    let mut current = String::new();
    let mut current_len = 0;
    for part in sentence.split(',').flat_map(str::split_whitespace) {
        if current_len + part.len() + 1 > max_len {
            flush_chunk(&mut current, &mut current_len, chunks);
        }
        if !current.is_empty() {
            current.push(' ');
            current_len += 1;
        }
        current.push_str(part);
        current_len += part.len();
    }
    flush_chunk(&mut current, &mut current_len, chunks);
}

fn flush_chunk(current: &mut String, current_len: &mut usize, chunks: &mut Vec<String>) {
    if !current.is_empty() {
        chunks.push(current.trim().to_string());
        current.clear();
        *current_len = 0;
    }
}

fn split_sentences(text: &str) -> Vec<String> {
    let re = Regex::new(r"([.!?])\s+").unwrap();
    let matches: Vec<_> = re.find_iter(text).collect();
    if matches.is_empty() {
        return vec![text.to_string()];
    }

    let mut sentences = Vec::new();
    let mut last_end = 0;
    for matched in matches {
        let punctuation = matched.as_str().chars().next().unwrap_or('.');
        if should_split_after_punctuation(&text[..matched.start()], punctuation) {
            sentences.push(text[last_end..matched.end()].to_string());
            last_end = matched.end();
        }
    }

    if last_end < text.len() {
        sentences.push(text[last_end..].to_string());
    }
    if sentences.is_empty() {
        vec![text.to_string()]
    } else {
        sentences
    }
}

fn should_split_after_punctuation(text_before_punctuation: &str, punctuation: char) -> bool {
    punctuation != '.' || !looks_like_abbreviation(text_before_punctuation)
}

fn looks_like_abbreviation(text_before_punctuation: &str) -> bool {
    let Some(previous_word) = text_before_punctuation.split_whitespace().last() else {
        return false;
    };
    let letters = previous_word
        .chars()
        .filter(|character| character.is_alphabetic())
        .count();
    previous_word.contains('.') || letters <= 2
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chunk_text_keeps_abbreviations_together() {
        assert_eq!(
            split_sentences("Dr. Smith arrived. Hello there."),
            vec![
                "Dr. Smith arrived. ".to_string(),
                "Hello there.".to_string()
            ]
        );
    }
}
