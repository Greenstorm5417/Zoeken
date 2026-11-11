//! Unicode-aware query tokens that keep technical spellings intact.
//!
//! `C++`, `C#`, `.NET`, `1.84.0`, and `foo_bar` stay one token. Letters outside
//! ASCII stay letters (`café`). Matching is case-folded with Unicode lowercase
//! and never splits a token just to compare ASCII.

use unicode_normalization::UnicodeNormalization;

/// Tokens of `text`, Unicode-lowercased, preserving technical punctuation.
#[must_use]
pub fn tokenize(text: &str) -> Vec<String> {
    let lower = text.nfc().collect::<String>().to_lowercase();
    let chars: Vec<(usize, char)> = lower.char_indices().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < chars.len() {
        let (byte, ch) = chars[index];
        if !token_start(ch, chars.get(index + 1).map(|(_, c)| *c)) {
            index += 1;
            continue;
        }
        let start = byte;
        index += 1;
        while index < chars.len() && token_continue(&chars, index) {
            index += 1;
        }
        let end = chars.get(index).map(|(b, _)| *b).unwrap_or(lower.len());
        let token = &lower[start..end];
        if !token.is_empty() {
            tokens.push(token.to_string());
        }
    }
    tokens
}

fn token_start(ch: char, next: Option<char>) -> bool {
    ch.is_alphanumeric() || ch == '_' || (ch == '.' && next.is_some_and(|n| n.is_alphanumeric()))
}

fn token_continue(chars: &[(usize, char)], index: usize) -> bool {
    let ch = chars[index].1;
    if ch.is_alphanumeric() || ch == '_' || ch == '+' || ch == '#' {
        return true;
    }
    if ch == '-' || ch == '.' {
        return chars
            .get(index + 1)
            .is_some_and(|(_, next)| next.is_alphanumeric());
    }
    false
}

/// Quoted phrases with their quotes removed, original case preserved.
#[must_use]
pub fn quoted_phrases(text: &str) -> Vec<String> {
    let mut phrases = Vec::new();
    let mut rest = text;
    while let Some(start) = rest.find('"') {
        let after = &rest[start + 1..];
        if let Some(end) = after.find('"') {
            let phrase = after[..end].trim();
            if !phrase.is_empty() {
                phrases.push(phrase.to_string());
            }
            rest = &after[end + 1..];
        } else {
            break;
        }
    }
    phrases
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn preserves_technical_tokens() {
        assert_eq!(tokenize("C++ C# .NET"), vec!["c++", "c#", ".net"]);
        assert_eq!(tokenize("rustc 1.84.0"), vec!["rustc", "1.84.0"]);
        assert_eq!(tokenize("foo_bar"), vec!["foo_bar"]);
        assert_eq!(tokenize("std::vector"), vec!["std", "vector"]);
    }

    #[test]
    fn unicode_letters_stay_whole_and_casefold() {
        assert_eq!(tokenize("Café naïve"), vec!["café", "naïve"]);
        assert_eq!(tokenize("cafe\u{0301}"), vec!["café"]);
        assert_eq!(tokenize("東京"), vec!["東京"]);
        assert_eq!(tokenize("Москва Straße"), vec!["москва", "straße"]);
        assert_ne!(tokenize("Straße"), tokenize("strasse"));
        assert_eq!(tokenize("EMAIL"), vec!["email"]);
        assert!(!tokenize("Email").iter().any(|t| t == "ai"));
    }

    #[test]
    fn greek_arabic_hebrew_and_hangul_stay_whole_words() {
        assert_eq!(tokenize("Αθήνα"), vec!["αθήνα"]);
        assert_eq!(tokenize("القاهرة"), vec!["القاهرة"]);
        assert_eq!(tokenize("ירושלים"), vec!["ירושלים"]);
        assert_eq!(tokenize("서울"), vec!["서울"]);
        let decomposed = tokenize("Αθη\u{0301}να");
        assert_eq!(decomposed, tokenize("Αθήνα"));
    }

    #[test]
    fn quoted_phrases_round_trip() {
        assert_eq!(
            quoted_phrases("error \"undefined reference to\" main"),
            vec!["undefined reference to".to_string()]
        );
    }
}
