//! Property test: answerer invocation rule — invoke when keyword matches or unconditional.

use std::sync::Arc;

use proptest::prelude::*;

use zoeken_answerers::{Answerer, AnswererRegistry};
use zoeken_query::SearchQuery;
use zoeken_results::Answer;

const KEYWORD_POOL: &[&str] = &["foo", "bar", "baz", "qux"];
const TOKEN_POOL: &[&str] = &["foo", "bar", "baz", "qux", "zzz", "none", "other"];

struct TaggedAnswerer {
    id: usize,
    keywords: Vec<&'static str>,
    unconditional: bool,
}

impl Answerer for TaggedAnswerer {
    fn keywords(&self) -> &[&str] {
        &self.keywords
    }

    fn unconditional(&self) -> bool {
        self.unconditional
    }

    fn answer(&self, _query: &SearchQuery) -> Vec<Answer> {
        vec![Answer {
            answer: format!("ans:{}", self.id),
            ..Answer::default()
        }]
    }
}

#[derive(Debug, Clone)]
struct AnswererSpec {
    keywords: Vec<&'static str>,
    unconditional: bool,
}

fn keywords_strategy() -> impl Strategy<Value = Vec<&'static str>> {
    prop::collection::vec(prop::sample::select(KEYWORD_POOL), 0..KEYWORD_POOL.len())
}

fn answerer_spec_strategy() -> impl Strategy<Value = AnswererSpec> {
    (keywords_strategy(), any::<bool>()).prop_map(|(keywords, unconditional)| AnswererSpec {
        keywords,
        unconditional,
    })
}

fn query_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(prop::sample::select(TOKEN_POOL), 0..4)
        .prop_map(|tokens| tokens.join(" "))
}

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        query: text.to_string(),
        ..SearchQuery::default()
    }
}

fn expected_answers(specs: &[AnswererSpec], query_text: &str) -> Vec<String> {
    let first = query_text.split_whitespace().next();
    specs
        .iter()
        .enumerate()
        .filter(|(_, spec)| {
            let matched = match first {
                Some(token) => spec.keywords.contains(&token),
                None => false,
            };
            matched || spec.unconditional
        })
        .map(|(id, _)| format!("ans:{id}"))
        .collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_answerer_invocation_rule(
        specs in prop::collection::vec(answerer_spec_strategy(), 0..6),
        query_text in query_strategy(),
    ) {
        let answerers: Vec<Arc<dyn Answerer>> = specs
            .iter()
            .enumerate()
            .map(|(id, spec)| {
                Arc::new(TaggedAnswerer {
                    id,
                    keywords: spec.keywords.clone(),
                    unconditional: spec.unconditional,
                }) as Arc<dyn Answerer>
            })
            .collect();
        let registry = AnswererRegistry::from_answerers(answerers);
        let actual: Vec<String> = registry
            .ask(&query(&query_text))
            .into_iter()
            .map(|a| a.answer)
            .collect();
        let expected = expected_answers(&specs, &query_text);
        prop_assert_eq!(actual, expected);
    }
}
