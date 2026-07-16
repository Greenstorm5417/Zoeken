//! The random answerer: generates string/int/float/sha256/uuid/color on `random <kind>`.

use rand::Rng;

use zoeken_query::SearchQuery;
use zoeken_results::Answer;

use crate::Answerer;

/// The kind of random value the [`RandomAnswerer`] can produce.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RandomKind {
    /// A random alphanumeric string of 8–32 characters.
    String,
    /// A random 32-bit-range signed integer, rendered in decimal.
    Int,
    /// A random float in `[0, 1)`.
    Float,
    /// A random 256-bit value rendered as 64 lowercase hex digits.
    Sha256,
    /// A random UUID (version 4).
    Uuid,
    /// A random `#RRGGBB` color in uppercase hex.
    Color,
}

const KINDS: [RandomKind; 6] = [
    RandomKind::String,
    RandomKind::Int,
    RandomKind::Float,
    RandomKind::Sha256,
    RandomKind::Uuid,
    RandomKind::Color,
];

const STRING_ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

impl RandomKind {
    /// The keyword token that selects this kind (the second token of the query).
    pub fn token(self) -> &'static str {
        match self {
            RandomKind::String => "string",
            RandomKind::Int => "int",
            RandomKind::Float => "float",
            RandomKind::Sha256 => "sha256",
            RandomKind::Uuid => "uuid",
            RandomKind::Color => "color",
        }
    }

    pub fn parse(token: &str) -> Option<Self> {
        KINDS.into_iter().find(|k| k.token() == token)
    }

    pub fn generate<R: Rng + ?Sized>(self, rng: &mut R) -> String {
        match self {
            RandomKind::String => random_string(rng),
            RandomKind::Int => {
                let n: i64 = rng.random_range(-(1i64 << 31)..=(1i64 << 31));
                n.to_string()
            }
            RandomKind::Float => rng.random::<f64>().to_string(),
            RandomKind::Sha256 => {
                let mut bytes = [0u8; 32];
                rng.fill(&mut bytes);
                to_hex(&bytes)
            }
            RandomKind::Uuid => random_uuid_v4(rng),
            RandomKind::Color => {
                let value: u32 = rng.random_range(0..=0xFF_FFFF);
                format!("#{value:06X}")
            }
        }
    }

    pub fn matches_format(self, value: &str) -> bool {
        match self {
            RandomKind::String => {
                (8..=32).contains(&value.chars().count())
                    && value.bytes().all(|b| STRING_ALPHABET.contains(&b))
            }
            RandomKind::Int => value.parse::<i64>().is_ok(),
            RandomKind::Float => value.parse::<f64>().map(|f| f.is_finite()).unwrap_or(false),
            RandomKind::Sha256 => value.len() == 64 && value.bytes().all(|b| b.is_ascii_hexdigit()),
            RandomKind::Uuid => is_uuid_v4(value),
            RandomKind::Color => {
                value.len() == 7
                    && value.starts_with('#')
                    && value[1..].bytes().all(|b| b.is_ascii_hexdigit())
            }
        }
    }
}

fn random_string<R: Rng + ?Sized>(rng: &mut R) -> String {
    let len = rng.random_range(8..=32);
    (0..len)
        .map(|_| {
            let idx = rng.random_range(0..STRING_ALPHABET.len());
            STRING_ALPHABET[idx] as char
        })
        .collect()
}

fn to_hex(bytes: &[u8]) -> String {
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn random_uuid_v4<R: Rng + ?Sized>(rng: &mut R) -> String {
    let mut bytes = [0u8; 16];
    rng.fill(&mut bytes);
    // Set the version (4) and variant (10xx) bits per RFC 4122.
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    let hex = to_hex(&bytes);
    format!(
        "{}-{}-{}-{}-{}",
        &hex[0..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..32],
    )
}

fn is_uuid_v4(value: &str) -> bool {
    let groups: Vec<&str> = value.split('-').collect();
    if groups.len() != 5 {
        return false;
    }
    let lengths = [8, 4, 4, 4, 12];
    for (group, len) in groups.iter().zip(lengths) {
        if group.len() != len || !group.bytes().all(|b| b.is_ascii_hexdigit()) {
            return false;
        }
    }
    // Version nibble must be '4'.
    let version_ok = groups[2].starts_with('4');
    // Variant nibble must be one of 8, 9, a, b.
    let variant_ok = matches!(
        groups[3].chars().next(),
        Some('8' | '9' | 'a' | 'b' | 'A' | 'B')
    );
    version_ok && variant_ok
}

#[derive(Debug, Clone)]
pub struct RandomAnswerer {
    keywords: Vec<&'static str>,
}

impl RandomAnswerer {
    pub fn new() -> Self {
        RandomAnswerer {
            keywords: vec!["random"],
        }
    }
}

impl Default for RandomAnswerer {
    fn default() -> Self {
        RandomAnswerer::new()
    }
}

impl Answerer for RandomAnswerer {
    fn keywords(&self) -> &[&str] {
        &self.keywords
    }

    fn answer(&self, query: &SearchQuery) -> Vec<Answer> {
        let parts: Vec<&str> = query.query.split_whitespace().collect();
        if parts.len() != 2 || parts[0] != "random" {
            return Vec::new();
        }
        let Some(kind) = RandomKind::parse(parts[1]) else {
            return Vec::new();
        };

        let mut rng = rand::rng();
        vec![Answer {
            answer: kind.generate(&mut rng),
            ..Answer::default()
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            query: text.to_string(),
            ..SearchQuery::default()
        }
    }

    #[test]
    fn parse_resolves_all_kinds() {
        for kind in KINDS {
            assert_eq!(RandomKind::parse(kind.token()), Some(kind));
        }
        assert_eq!(RandomKind::parse("bogus"), None);
    }

    #[test]
    fn generated_values_conform_to_their_kind() {
        let mut rng = rand::rng();
        for kind in KINDS {
            for _ in 0..50 {
                let value = kind.generate(&mut rng);
                assert!(
                    kind.matches_format(&value),
                    "{:?} produced non-conforming value {value:?}",
                    kind
                );
            }
        }
    }

    #[test]
    fn answer_requires_exactly_two_tokens() {
        assert!(RandomAnswerer::new().answer(&query("random")).is_empty());
        assert!(
            RandomAnswerer::new()
                .answer(&query("random uuid extra"))
                .is_empty()
        );
        assert!(
            RandomAnswerer::new()
                .answer(&query("random bogus"))
                .is_empty()
        );
    }

    #[test]
    fn answer_produces_requested_kind() {
        let answers = RandomAnswerer::new().answer(&query("random uuid"));
        assert_eq!(answers.len(), 1);
        assert!(RandomKind::Uuid.matches_format(&answers[0].answer));

        let answers = RandomAnswerer::new().answer(&query("random color"));
        assert!(RandomKind::Color.matches_format(&answers[0].answer));
    }

    #[test]
    fn uuid_format_check_rejects_malformed() {
        assert!(!is_uuid_v4("not-a-uuid"));
        assert!(!is_uuid_v4("12345678-1234-1234-1234-1234567890ab")); // version nibble != 4
        assert!(is_uuid_v4("12345678-1234-4234-8234-1234567890ab"));
    }
}
