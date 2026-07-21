//! Hash / encode / decode intent detection — digests are computed in the browser.
//!
//! Detects queries like `sha256 foo`, `base64 hello`, `what is x in base64`,
//! `decode base64 …`. Does **not** compute digests server-side.

use zoeken_query::SearchQuery;
use zoeken_results::{Answer, InteractiveAnswer};

use crate::Answerer;

const HASH_ALGS: &[&str] = &["md5", "sha1", "sha224", "sha256", "sha384", "sha512"];
const CODEC_ALGS: &[&str] = &["base64", "hex", "url"];

/// Intent-only crypto answerer (client computes the result).
#[derive(Debug, Default)]
pub struct CryptoAnswerer;

impl CryptoAnswerer {
    pub fn new() -> Self {
        CryptoAnswerer
    }
}

impl Answerer for CryptoAnswerer {
    fn keywords(&self) -> &[&str] {
        &[]
    }

    fn unconditional(&self) -> bool {
        true
    }

    fn answer(&self, query: &SearchQuery) -> Vec<Answer> {
        let Some(parsed) = parse_crypto(&query.query) else {
            return Vec::new();
        };
        let hint = match parsed.mode.as_str() {
            "hash" => format!("{} (client)", parsed.algorithm),
            "encode" => format!("{} encode (client)", parsed.algorithm),
            "decode" => format!("{} decode (client)", parsed.algorithm),
            other => other.to_string(),
        };
        vec![Answer {
            answer: hint,
            engine: "hash_plugin".to_string(),
            interactive: Some(InteractiveAnswer::Crypto {
                mode: parsed.mode,
                algorithm: parsed.algorithm,
                input: parsed.input,
            }),
            ..Answer::default()
        }]
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedCrypto {
    mode: String,
    algorithm: String,
    input: String,
}

fn normalize_alg(raw: &str) -> Option<&'static str> {
    let n = raw.to_ascii_lowercase().replace(['-', '_'], "");
    // "base 64" → caller collapses spaces before calling; also accept "base64"
    let n = n.replace(' ', "");
    for &alg in HASH_ALGS.iter().chain(CODEC_ALGS.iter()) {
        if alg.replace('-', "") == n || alg == n {
            return Some(alg);
        }
    }
    // sha-256 style already stripped
    match n.as_str() {
        "sha1" => Some("sha1"),
        "sha224" => Some("sha224"),
        "sha256" => Some("sha256"),
        "sha384" => Some("sha384"),
        "sha512" => Some("sha512"),
        "md5" => Some("md5"),
        "base64" | "b64" => Some("base64"),
        "hex" | "hexadecimal" => Some("hex"),
        "url" | "uri" | "percent" => Some("url"),
        _ => None,
    }
}

fn is_hash(alg: &str) -> bool {
    HASH_ALGS.contains(&alg)
}

fn is_codec(alg: &str) -> bool {
    CODEC_ALGS.contains(&alg)
}

/// Collapse `base 64` / `base-64` → `base64` without lowercasing the rest.
fn collapse_base64_token(text: &str) -> String {
    let trimmed = text.trim().trim_end_matches('?').trim();
    let lower = trimmed.to_ascii_lowercase();
    if let Some(idx) = lower.find("base 64") {
        let mut out = trimmed.to_string();
        out.replace_range(idx..idx + 7, "base64");
        return out;
    }
    if let Some(idx) = lower.find("base-64") {
        let mut out = trimmed.to_string();
        out.replace_range(idx..idx + 7, "base64");
        return out;
    }
    trimmed.to_string()
}

/// Slice of `original` matching the same byte range as `matched` inside `lowered`.
/// `lowered` must be `original.to_ascii_lowercase()` (same byte length for ASCII algs).
fn preserve_case(original: &str, lowered: &str, matched: &str) -> String {
    if let Some(start) = lowered.rfind(matched) {
        let end = start + matched.len();
        if end <= original.len() {
            return original[start..end].to_string();
        }
    }
    matched.to_string()
}

fn parse_crypto(query: &str) -> Option<ParsedCrypto> {
    let collapsed = collapse_base64_token(query);
    if collapsed.is_empty() {
        return None;
    }
    let q = collapsed.to_ascii_lowercase();

    // `hash <text> with <alg>` / `hash <text> using <alg>`
    if let Some(rest) = q.strip_prefix("hash ") {
        for sep in [" with ", " using "] {
            if let Some((text, alg_raw)) = rest.rsplit_once(sep)
                && let Some(alg) = normalize_alg(alg_raw.trim())
                && is_hash(alg)
                && !text.trim().is_empty()
            {
                return Some(ParsedCrypto {
                    mode: "hash".into(),
                    algorithm: alg.into(),
                    input: preserve_case(&collapsed, &q, text.trim()),
                });
            }
        }
    }

    // `decode <alg> <text>` / `encode <alg> <text>`
    for (prefix, mode) in [("decode ", "decode"), ("encode ", "encode")] {
        if let Some(rest) = q.strip_prefix(prefix) {
            let mut parts = rest.splitn(2, char::is_whitespace);
            let alg_tok = parts.next()?.trim();
            let input = parts.next()?.trim();
            if input.is_empty() {
                continue;
            }
            if let Some(alg) = normalize_alg(alg_tok)
                && is_codec(alg)
            {
                return Some(ParsedCrypto {
                    mode: mode.into(),
                    algorithm: alg.into(),
                    input: preserve_case(&collapsed, &q, input),
                });
            }
        }
    }

    // `<alg> encode <text>` / `<alg> decode <text>` / `url encode …`
    {
        let mut parts = q.splitn(3, char::is_whitespace);
        let a = parts.next().unwrap_or("");
        let b = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        if !rest.is_empty()
            && let Some(alg) = normalize_alg(a)
            && is_codec(alg)
            && (b == "encode" || b == "decode")
        {
            return Some(ParsedCrypto {
                mode: b.to_string(),
                algorithm: alg.into(),
                input: preserve_case(&collapsed, &q, rest),
            });
        }
    }

    // `what is <text> in <alg>` / `what's <text> in <alg>`
    for prefix in ["what is ", "what's ", "whats "] {
        if let Some(rest) = q.strip_prefix(prefix)
            && let Some((text, alg_raw)) = rest.rsplit_once(" in ")
        {
            let text = text.trim();
            if text.is_empty() {
                continue;
            }
            if let Some(alg) = normalize_alg(alg_raw.trim()) {
                if is_hash(alg) {
                    return Some(ParsedCrypto {
                        mode: "hash".into(),
                        algorithm: alg.into(),
                        input: preserve_case(&collapsed, &q, text),
                    });
                }
                if is_codec(alg) {
                    return Some(ParsedCrypto {
                        mode: "encode".into(),
                        algorithm: alg.into(),
                        input: preserve_case(&collapsed, &q, text),
                    });
                }
            }
        }
    }

    // `<alg> <text>` — first token is algorithm
    {
        let mut parts = q.splitn(2, char::is_whitespace);
        let first = parts.next().unwrap_or("");
        let rest = parts.next().unwrap_or("").trim();
        if !rest.is_empty()
            && let Some(alg) = normalize_alg(first)
        {
            let mode = if is_hash(alg) {
                "hash"
            } else if is_codec(alg) {
                "encode"
            } else {
                ""
            };
            if !mode.is_empty() {
                return Some(ParsedCrypto {
                    mode: mode.into(),
                    algorithm: alg.into(),
                    input: preserve_case(&collapsed, &q, rest),
                });
            }
        }
    }

    // `<text> <alg>` — last token is algorithm (skip `random sha256` etc.)
    {
        let mut parts: Vec<&str> = q.split_whitespace().collect();
        if parts.len() >= 2
            && parts[0] != "random"
            && let Some(alg) = normalize_alg(parts[parts.len() - 1])
        {
            parts.pop();
            let input_lower = parts.join(" ");
            if !input_lower.is_empty() && (is_hash(alg) || is_codec(alg)) {
                return Some(ParsedCrypto {
                    mode: if is_hash(alg) {
                        "hash".into()
                    } else {
                        "encode".into()
                    },
                    algorithm: alg.into(),
                    input: preserve_case(&collapsed, &q, &input_lower),
                });
            }
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ask(q: &str) -> Option<ParsedCrypto> {
        parse_crypto(q)
    }

    #[test]
    fn detects_hash_prefix_and_suffix() {
        assert_eq!(
            ask("sha256 abc"),
            Some(ParsedCrypto {
                mode: "hash".into(),
                algorithm: "sha256".into(),
                input: "abc".into(),
            })
        );
        assert_eq!(
            ask("hello sha256"),
            Some(ParsedCrypto {
                mode: "hash".into(),
                algorithm: "sha256".into(),
                input: "hello".into(),
            })
        );
        assert_eq!(
            ask("hash the fox with md5"),
            Some(ParsedCrypto {
                mode: "hash".into(),
                algorithm: "md5".into(),
                input: "the fox".into(),
            })
        );
    }

    #[test]
    fn detects_encode_decode_phrases() {
        assert_eq!(
            ask("base64 hello"),
            Some(ParsedCrypto {
                mode: "encode".into(),
                algorithm: "base64".into(),
                input: "hello".into(),
            })
        );
        assert_eq!(
            ask("base 64 encode hello world"),
            Some(ParsedCrypto {
                mode: "encode".into(),
                algorithm: "base64".into(),
                input: "hello world".into(),
            })
        );
        assert_eq!(
            ask("what is hello in base64"),
            Some(ParsedCrypto {
                mode: "encode".into(),
                algorithm: "base64".into(),
                input: "hello".into(),
            })
        );
        assert_eq!(
            ask("decode base64 aGVsbG8="),
            Some(ParsedCrypto {
                mode: "decode".into(),
                algorithm: "base64".into(),
                input: "aGVsbG8=".into(),
            })
        );
        assert_eq!(
            ask("url encode hello world"),
            Some(ParsedCrypto {
                mode: "encode".into(),
                algorithm: "url".into(),
                input: "hello world".into(),
            })
        );
        assert_eq!(
            ask("hex encode abc"),
            Some(ParsedCrypto {
                mode: "encode".into(),
                algorithm: "hex".into(),
                input: "abc".into(),
            })
        );
    }

    #[test]
    fn ignores_unrelated_queries() {
        assert!(ask("rust programming").is_none());
        assert!(ask("sha256").is_none());
        assert!(ask("").is_none());
        assert!(ask("random sha256").is_none());
    }

    #[test]
    fn answerer_sets_interactive_without_digest() {
        let a = CryptoAnswerer::new();
        let answers = a.answer(&SearchQuery {
            query: "sha256 abc".into(),
            ..SearchQuery::default()
        });
        assert_eq!(answers.len(), 1);
        assert!(answers[0].answer.contains("sha256"));
        assert!(!answers[0].answer.contains("ba7816"));
        match &answers[0].interactive {
            Some(InteractiveAnswer::Crypto {
                mode,
                algorithm,
                input,
            }) => {
                assert_eq!(mode, "hash");
                assert_eq!(algorithm, "sha256");
                assert_eq!(input, "abc");
            }
            other => panic!("expected Crypto, got {other:?}"),
        }
    }
}
