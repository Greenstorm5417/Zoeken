//! Property test for query selector parsing.

use std::collections::HashMap;

use proptest::prelude::*;
use zoeken_data::{
    BangTrie, CurrencyTable, DataBundle, EngineTraits, EngineTraitsMap, LocaleMap, UnitTable,
    UserAgentPool,
};
use zoeken_query::parse_raw;

const ENGINES: &[&str] = &["bing", "google", "duckduckgo", "brave", "startpage"];
const CATEGORIES: &[&str] = &[
    "general", "images", "videos", "news", "music", "science", "files", "web", "it",
];
const LANGS: &[&str] = &["en", "de", "fr", "es", "ja", "ru"];

#[derive(Debug, Clone)]
enum Expect {
    Engine(String),
    Category(String),
    Language(String),
}

fn test_bundle() -> DataBundle {
    let mut engines = HashMap::new();
    for name in ENGINES {
        engines.insert(
            (*name).to_string(),
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

fn arb_selector() -> impl Strategy<Value = (String, Expect)> {
    let engine = prop::sample::select(ENGINES.to_vec())
        .prop_map(|e| (format!("!{e}"), Expect::Engine(e.to_string())));
    let category = prop::sample::select(CATEGORIES.to_vec())
        .prop_map(|c| (format!("!{c}"), Expect::Category(c.to_string())));
    let language = prop::sample::select(LANGS.to_vec())
        .prop_map(|l| (format!(":{l}"), Expect::Language(l.to_string())));
    prop_oneof![engine, category, language]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

#[test]
    fn selector_sets_target_and_strips_token(
        (token, expect) in arb_selector(),
        other in prop::collection::vec("[a-z]{1,8}", 0..6),
        pos_seed in 0usize..64,
    ) {
        let data = test_bundle();
        let pos = pos_seed % (other.len() + 1);
        let mut parts = other.clone();
        parts.insert(pos, token.clone());
        let text = parts.join(" ");
        let raw = parse_raw(&text, &data).expect("selector parsing should not error");
        prop_assert_eq!(raw.query(), other.join(" "));
        match expect {
            Expect::Engine(e) => {
                prop_assert!(
                    raw.engines.contains(&e),
                    "engines {:?} should contain {}", raw.engines, e
                );
                prop_assert!(raw.specific, "engine selector should mark query specific");
            }
            Expect::Category(c) => {
                prop_assert!(
                    raw.categories.contains(&c),
                    "categories {:?} should contain {}", raw.categories, c
                );
                prop_assert!(raw.specific, "category selector should mark query specific");
            }
            Expect::Language(l) => {
                prop_assert!(
                    raw.languages.contains(&l),
                    "languages {:?} should contain {}", raw.languages, l
                );
            }
        }
    }
}
