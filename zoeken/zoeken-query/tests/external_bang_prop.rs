//! Property test for external bang resolution.

use std::collections::HashMap;

use proptest::prelude::*;
use zoeken_data::{
    BANG_QUERY_PLACEHOLDER, BangEntry, BangTrie, CurrencyTable, DataBundle, EngineTraitsMap,
    LocaleMap, UnitTable, UserAgentPool,
};
use zoeken_query::{QueryFeedback, parse_raw};

const BANGS: &[(&str, &str)] = &[
    ("g", "https://www.google.com/search?q=\u{2}"),
    ("w", "//en.wikipedia.org/wiki/Special:Search?search=\u{2}"),
    ("ddg", "https://duckduckgo.com/?q=\u{2}"),
    ("gh", "https://github.com/search?q=\u{2}"),
    ("a", "https://www.amazon.com/s?k=\u{2}"),
];

fn test_bundle() -> DataBundle {
    let mut bangs = BangTrie::new();
    for (token, template) in BANGS {
        bangs.insert(
            token,
            BangEntry {
                url_template: (*template).to_string(),
                rank: 0,
            },
        );
    }
    DataBundle {
        bangs,
        currencies: CurrencyTable::default(),
        units: UnitTable::default(),
        engine_traits: EngineTraitsMap {
            engines: HashMap::new(),
        },
        useragents: UserAgentPool::default(),
        locales: LocaleMap::default(),
        ..DataBundle::default()
    }
}

fn quote_plus(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            other => {
                out.push('%');
                out.push_str(&format!("{other:02X}"));
            }
        }
    }
    out
}

fn expected_redirect(template: &str, query: &str) -> String {
    let mut url = template.to_string();
    if let Some(rest) = url.strip_prefix("//") {
        url = format!("https://{rest}");
    }
    url.replace(BANG_QUERY_PLACEHOLDER, &quote_plus(query))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

#[test]
    fn known_bang_resolves_to_data_defined_target(
        bang_idx in 0usize..BANGS.len(),
        other in prop::collection::vec("[a-z]{1,8}", 1..6),
        pos_seed in 0usize..64,
    ) {
        let data = test_bundle();
        let (token, template) = BANGS[bang_idx];
        let pos = pos_seed % (other.len() + 1);
        let mut parts = other.clone();
        parts.insert(pos, format!("!!{token}"));
        let text = parts.join(" ");
        let raw = parse_raw(&text, &data).expect("bang parsing should not error");
        let residual = other.join(" ");
        prop_assert_eq!(raw.query(), residual.clone());
        prop_assert_eq!(raw.external_bang.as_deref(), Some(token));
        let expected = expected_redirect(template, &residual);
        prop_assert_eq!(raw.external_redirect.as_deref(), Some(expected.as_str()));
        prop_assert!(
            raw.feedback.is_empty(),
            "known bang should not report feedback, got {:?}", raw.feedback
        );
    }

#[test]
    fn unknown_bang_is_left_in_terms_with_feedback(
        token in "[a-z0-9]{1,10}"
            .prop_filter("must be an unknown bang", |t| {
                !BANGS.iter().any(|(known, _)| known == t)
            }),
        other in prop::collection::vec("[a-z]{1,8}", 0..6),
        pos_seed in 0usize..64,
    ) {
        let data = test_bundle();
        let pos = pos_seed % (other.len() + 1);
        let mut parts = other.clone();
        parts.insert(pos, format!("!!{token}"));
        let text = parts.join(" ");
        let raw = parse_raw(&text, &data).expect("bang parsing should not error");
        prop_assert_eq!(raw.query(), parts.join(" "));
        prop_assert_eq!(raw.external_bang, None);
        prop_assert_eq!(raw.external_redirect, None);
        prop_assert!(
            raw.feedback.contains(&QueryFeedback::UnknownBang { bang: token.clone() }),
            "expected UnknownBang({token}) in feedback, got {:?}", raw.feedback
        );
    }
}
