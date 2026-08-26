use unicode_normalization::UnicodeNormalization;
use unicode_segmentation::UnicodeSegmentation;

pub fn preprocess(text: &str, language: &str) -> String {
    let text: String = text.nfkd().collect();
    let text: String = text
        .graphemes(true)
        .filter(|grapheme| emojis::get(grapheme).is_none())
        .collect();
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
    let collapsed = normalized.split_whitespace().collect::<Vec<_>>().join(" ");
    normalized.clear();
    for character in collapsed.chars() {
        if matches!(character, ',' | '.' | '!' | '?' | ';' | ':' | '\'')
            && normalized.ends_with(' ')
        {
            normalized.pop();
        }
        normalized.push(character);
    }
    let mut normalized = normalized.trim().to_owned();
    if !normalized.is_empty()
        && !normalized.chars().last().is_some_and(|character| {
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
        })
    {
        normalized.push('.');
    }
    format!("<{language}>{normalized}</{language}>")
}
