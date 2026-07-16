//! Property test for special-prefix routing.

use std::collections::HashMap;

use proptest::prelude::*;
use zoeken_data::{
    BangTrie, CurrencyTable, DataBundle, EngineTraits, EngineTraitsMap, LocaleMap, UnitTable,
    UserAgentPool,
};
use zoeken_query::{ParseOutcome, SearchQuery, parse_raw};

const CATALOG: &[(&str, &str)] = &[
    ("avg", "statistics"),
    ("min", "statistics"),
    ("max", "statistics"),
    ("sum", "statistics"),
    ("prod", "statistics"),
    ("random", "random"),
    ("hash", "hash"),
    ("convert", "unit_converter"),
];

fn match_answerer(query: &str) -> Option<String> {
    let first = query.split_whitespace().next()?;
    CATALOG
        .iter()
        .find(|(prefix, _)| *prefix == first)
        .map(|(_, id)| (*id).to_string())
}

fn test_bundle() -> DataBundle {
    let mut engines = HashMap::new();
    for name in ["bing", "google", "duckduckgo"] {
        engines.insert(
            name.to_string(),
            EngineTraits {
                all_locale: None,
                data_type: None,
                languages: HashMap::new(),
                regions: HashMap::new(),
                custom: Default::default(),
            },
        );
    }
    DataBundle {
        bangs: BangTrie::new(),
        currencies: CurrencyTable::default(),
        units: UnitTable::default(),
        engine_traits: EngineTraitsMap { engines },
        useragents: UserAgentPool::default(),
        locales: LocaleMap::default(),
        ..DataBundle::default()
    }
}

fn arb_registered_prefix() -> impl Strategy<Value = (String, String)> {
    prop::sample::select(CATALOG.to_vec())
        .prop_map(|(prefix, id)| (prefix.to_string(), id.to_string()))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

#[test]
    fn matching_prefix_routes_to_answerer(
        (prefix, expected_id) in arb_registered_prefix(),
        rest in prop::collection::vec("[a-z]{1,8}", 0..5),
    ) {
        let data = test_bundle();
        let mut parts = vec![prefix.clone()];
        parts.extend(rest.iter().cloned());
        let text = parts.join(" ");
        let raw = parse_raw(&text, &data).expect("parsing should not error");
        prop_assert!(
            raw.external_redirect.is_none(),
            "a plain-word prefix must not trigger an external bang redirect"
        );
        let residual = raw.query();
        prop_assert_eq!(
            residual.split_whitespace().next(),
            Some(prefix.as_str()),
            "the registered prefix should remain the leading residual term"
        );
        let matched = match_answerer(&residual);
        prop_assert_eq!(
            matched.as_deref(),
            Some(expected_id.as_str()),
            "catalog lookup should identify the registered answerer/plugin"
        );
        let outcome = ParseOutcome::resolve(&raw, SearchQuery::default(), matched);
        match outcome {
            ParseOutcome::AnswererRoute { answerer, .. } => {
                prop_assert_eq!(
                    answerer,
                    expected_id,
                    "query should route to the matched answerer/plugin"
                );
            }
            other => prop_assert!(
                false,
                "expected AnswererRoute for a matching prefix, got {:?}",
                other
            ),
        }
    }
}
