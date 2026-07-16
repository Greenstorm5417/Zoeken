//! Property test for form/query parameter mapping.

use std::time::Duration;

use proptest::prelude::*;
use zoeken_data::DataBundle;
use zoeken_query::{FormParams, SafeSearch, StaticPreferences, TimeRange, from_params};

const CATEGORIES: &[&str] = &[
    "general", "images", "videos", "news", "music", "science", "files", "it",
];

const ENGINE_NAMES: &[&str] = &["brave", "bing", "google", "mojeek", "startpage"];

const LANGS: &[&str] = &["all", "en", "de", "fr", "es", "ja", "ru", "en-US", "pt-BR"];

const TIME_RANGES: &[&str] = &["day", "week", "month", "year"];

#[derive(Debug, Clone)]
struct ValidCase {
    q: String,
    pageno: u32,
    safesearch: u8,
    time_range: Option<String>,
    language: String,
    categories: Vec<String>,
    engines: Vec<String>,
    timeout_secs: Option<u32>,
}

impl ValidCase {
    fn to_pairs(&self) -> Vec<(String, String)> {
        let mut pairs = vec![
            ("q".to_string(), self.q.clone()),
            ("pageno".to_string(), self.pageno.to_string()),
            ("safesearch".to_string(), self.safesearch.to_string()),
            ("language".to_string(), self.language.clone()),
            ("categories".to_string(), self.categories.join(",")),
            ("engines".to_string(), self.engines.join(",")),
        ];
        if let Some(tr) = &self.time_range {
            pairs.push(("time_range".to_string(), tr.clone()));
        }
        if let Some(secs) = self.timeout_secs {
            pairs.push(("timeout_limit".to_string(), secs.to_string()));
        }
        pairs
    }

    fn expected_categories(&self) -> Vec<String> {
        let mut out: Vec<String> = Vec::new();
        for c in &self.categories {
            if !out.contains(c) {
                out.push(c.clone());
            }
        }
        out
    }
}

fn arb_valid() -> impl Strategy<Value = ValidCase> {
    (
        "[a-zA-Z0-9]{1,10}",
        1u32..=1000,
        0u8..=2,
        prop::option::of(prop::sample::select(TIME_RANGES.to_vec())),
        prop::sample::select(LANGS.to_vec()),
        prop::collection::vec(prop::sample::select(CATEGORIES.to_vec()), 1..4),
        prop::collection::vec(prop::sample::select(ENGINE_NAMES.to_vec()), 0..4),
        prop::option::of(0u32..=600),
    )
        .prop_map(
            |(q, pageno, safesearch, time_range, language, categories, engines, timeout_secs)| {
                ValidCase {
                    q,
                    pageno,
                    safesearch,
                    time_range: time_range.map(|s| s.to_string()),
                    language: language.to_string(),
                    categories: categories.into_iter().map(|s| s.to_string()).collect(),
                    engines: engines.into_iter().map(|s| s.to_string()).collect(),
                    timeout_secs,
                }
            },
        )
}

#[derive(Debug, Clone)]
enum Invalid {
    EmptyQ,
    Pageno(String),
    Safesearch(String),
    TimeRange(String),
    Language(String),
    Timeout(String),
}

impl Invalid {
    fn expected_name(&self) -> &'static str {
        match self {
            Invalid::EmptyQ => "q",
            Invalid::Pageno(_) => "pageno",
            Invalid::Safesearch(_) => "safesearch",
            Invalid::TimeRange(_) => "time_range",
            Invalid::Language(_) => "language",
            Invalid::Timeout(_) => "timeout_limit",
        }
    }

    fn apply(&self, pairs: &mut Vec<(String, String)>) {
        let (name, value) = match self {
            Invalid::EmptyQ => ("q", String::new()),
            Invalid::Pageno(v) => ("pageno", v.clone()),
            Invalid::Safesearch(v) => ("safesearch", v.clone()),
            Invalid::TimeRange(v) => ("time_range", v.clone()),
            Invalid::Language(v) => ("language", v.clone()),
            Invalid::Timeout(v) => ("timeout_limit", v.clone()),
        };
        set_param(pairs, name, value);
    }
}

fn set_param(pairs: &mut Vec<(String, String)>, name: &str, value: String) {
    if let Some(slot) = pairs.iter_mut().find(|(k, _)| k == name) {
        slot.1 = value;
    } else {
        pairs.push((name.to_string(), value));
    }
}

fn arb_invalid() -> impl Strategy<Value = Invalid> {
    fn pick(options: &[&'static str]) -> impl Strategy<Value = String> {
        prop::sample::select(options.to_vec()).prop_map(|s| s.to_string())
    }
    prop_oneof![
        Just(Invalid::EmptyQ),
        pick(&["0", "abc", "-1", ""]).prop_map(Invalid::Pageno),
        pick(&["3", "4", "9", "x", "-1"]).prop_map(Invalid::Safesearch),
        pick(&["decade", "hour", "weeks", "yesterday"]).prop_map(Invalid::TimeRange),
        pick(&["e", "toolong", "123", "en-USA", "!!"]).prop_map(Invalid::Language),
        pick(&["abc", "-1", "-2.5", "nan"]).prop_map(Invalid::Timeout),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

#[test]
    fn valid_params_map_onto_search_query_fields(case in arb_valid()) {
        let prefs = StaticPreferences::default();
        let params = FormParams::from_pairs(case.to_pairs());

        let sq = from_params(&params, &prefs, &DataBundle::default()).expect("valid params should map without error");

        prop_assert_eq!(&sq.query, &case.q);
        prop_assert_eq!(sq.pageno, case.pageno);
        prop_assert_eq!(sq.safesearch, SafeSearch::from_u8(case.safesearch).unwrap());

        let expected_time_range = case
            .time_range
            .as_deref()
            .map(|t| TimeRange::parse(t).expect("generated time_range is valid"));
        prop_assert_eq!(sq.time_range, expected_time_range);

        if case.language == "all" {
            prop_assert!(sq.locale.is_all(), "locale {:?} should be `all`", sq.locale);
        } else {
            prop_assert_eq!(sq.locale.as_str(), case.language.as_str());
        }

        prop_assert_eq!(sq.categories, case.expected_categories());
        prop_assert_eq!(&sq.engines, &case.engines);

        let expected_timeout = case.timeout_secs.map(|s| Duration::from_secs(s as u64));
        prop_assert_eq!(sq.timeout, expected_timeout);
    }

#[test]
    fn one_invalid_field_rejects_whole_request(
        case in arb_valid(),
        invalid in arb_invalid(),
    ) {
        let prefs = StaticPreferences::default();
        let mut pairs = case.to_pairs();
        invalid.apply(&mut pairs);
        let params = FormParams::from_pairs(pairs);

        let err = from_params(&params, &prefs, &DataBundle::default())
            .expect_err("a single invalid parameter must reject the whole request");

        match err {
            zoeken_query::QueryError::InvalidParameter { name, .. } => {
                prop_assert_eq!(name, invalid.expected_name());
            }
        }
    }
}
