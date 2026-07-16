//! Keyword-triggered local instant answers via [`AnswererRegistry`].
//!
//! Two built-in answerers: [`StatisticsAnswerer`] (min/max/avg/sum/prod/range/median)
//! and [`RandomAnswerer`] (string/int/float/sha256/uuid/color).
//! An answerer runs when its trigger keyword matches the query's first token
//! or when configured unconditional.

use std::sync::Arc;

use zoeken_data::DataBundle;
use zoeken_query::SearchQuery;
use zoeken_results::{Answer, Template};

mod random;
mod statistics;

pub use random::{RandomAnswerer, RandomKind};
pub use statistics::{StatisticsAnswerer, StatisticsOp};

/// A keyword-triggered (or unconditional) local instant answerer.
///
/// Declares trigger [`keywords`](Answerer::keywords) and whether it runs
/// [`unconditional`](Answerer::unconditional)ly; [`answer`](Answerer::answer)
/// produces zero or more [`Answer`]s for a query.
/// Answerers are shared across concurrent requests, so must be `Send + Sync`.
pub trait Answerer: Send + Sync {
    /// Trigger keywords this answerer responds to (matched against the first token).
    fn keywords(&self) -> &[&str];

    /// Whether this answerer runs unconditionally. Defaults to `false`.
    fn unconditional(&self) -> bool {
        false
    }

    /// Produce answers for `query`. Empty vector means no answer.
    fn answer(&self, query: &SearchQuery) -> Vec<Answer>;
}

/// The registry of local answerers.
#[derive(Clone, Default)]
pub struct AnswererRegistry {
    answerers: Vec<Arc<dyn Answerer>>,
    data: Arc<DataBundle>,
}

impl AnswererRegistry {
    pub fn new() -> Self {
        AnswererRegistry {
            answerers: Vec::new(),
            data: Arc::new(DataBundle::default()),
        }
    }

    /// A registry pre-loaded with the built-in answerers (statistics + random).
    pub fn with_builtins() -> Self {
        AnswererRegistry::from_answerers([
            Arc::new(StatisticsAnswerer::new()) as Arc<dyn Answerer>,
            Arc::new(RandomAnswerer::new()) as Arc<dyn Answerer>,
        ])
    }

    pub fn register(&mut self, answerer: Arc<dyn Answerer>) -> &mut Self {
        self.answerers.push(answerer);
        self
    }

    pub fn from_answerers(answerers: impl IntoIterator<Item = Arc<dyn Answerer>>) -> Self {
        AnswererRegistry {
            answerers: answerers.into_iter().collect(),
            data: Arc::new(DataBundle::default()),
        }
    }

    pub fn with_data(mut self, data: Arc<DataBundle>) -> Self {
        self.data = data;
        self
    }

    pub fn data(&self) -> &DataBundle {
        &self.data
    }

    pub fn answerers(&self) -> &[Arc<dyn Answerer>] {
        &self.answerers
    }

    pub fn len(&self) -> usize {
        self.answerers.len()
    }

    pub fn is_empty(&self) -> bool {
        self.answerers.is_empty()
    }

    /// Ask every answerer about `query` and collect the answers.
    ///
    /// An answerer is invoked when the first token matches one of its keywords
    /// or it is configured to run unconditionally; all others are skipped.
    /// Each answer is tagged with an `engine` attribution unless already set.
    pub fn ask(&self, query: &SearchQuery) -> Vec<Answer> {
        let first = first_keyword(&query.query);
        let mut out = Vec::new();

        for answerer in &self.answerers {
            let matched = match first {
                Some(token) => answerer.keywords().contains(&token),
                None => false,
            };
            if !matched && !answerer.unconditional() {
                continue;
            }

            for mut answer in answerer.answer(query) {
                if answer.engine.is_empty() {
                    answer.engine = match (matched, first) {
                        (true, Some(token)) => format!("answerer: {token}"),
                        _ => "answerer".to_string(),
                    };
                }
                answer.template = Template::Answer;
                out.push(answer);
            }
        }

        out
    }
}

/// The first non-empty whitespace-separated token of `query`.
fn first_keyword(query: &str) -> Option<&str> {
    query.split_whitespace().next()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct FixedAnswerer {
        keywords: Vec<&'static str>,
        unconditional: bool,
        text: String,
    }

    impl FixedAnswerer {
        fn new(keywords: &[&'static str], unconditional: bool, text: &str) -> Arc<Self> {
            Arc::new(FixedAnswerer {
                keywords: keywords.to_vec(),
                unconditional,
                text: text.to_string(),
            })
        }
    }

    impl Answerer for FixedAnswerer {
        fn keywords(&self) -> &[&str] {
            &self.keywords
        }
        fn unconditional(&self) -> bool {
            self.unconditional
        }
        fn answer(&self, _query: &SearchQuery) -> Vec<Answer> {
            vec![Answer {
                answer: self.text.clone(),
                ..Answer::default()
            }]
        }
    }

    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            query: text.to_string(),
            ..SearchQuery::default()
        }
    }

    #[test]
    fn keyword_match_invokes_answerer() {
        let registry = AnswererRegistry::from_answerers([
            FixedAnswerer::new(&["hello"], false, "hi") as Arc<dyn Answerer>,
        ]);
        let answers = registry.ask(&query("hello world"));
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].answer, "hi");
        assert_eq!(answers[0].engine, "answerer: hello");
        assert_eq!(answers[0].template, Template::Answer);
    }

    #[test]
    fn keyword_must_be_first_token() {
        let registry = AnswererRegistry::from_answerers([
            FixedAnswerer::new(&["hello"], false, "hi") as Arc<dyn Answerer>,
        ]);
        assert!(registry.ask(&query("say hello")).is_empty());
    }

    #[test]
    fn no_match_and_no_unconditional_produces_no_answer() {
        let registry = AnswererRegistry::from_answerers([
            FixedAnswerer::new(&["hello"], false, "hi") as Arc<dyn Answerer>,
        ]);
        assert!(registry.ask(&query("goodbye now")).is_empty());
        assert!(registry.ask(&query("")).is_empty());
    }

    #[test]
    fn unconditional_answerer_runs_without_match() {
        let registry =
            AnswererRegistry::from_answerers([
                FixedAnswerer::new(&["never"], true, "always") as Arc<dyn Answerer>
            ]);
        let answers = registry.ask(&query("something unrelated"));
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].answer, "always");
        assert_eq!(answers[0].engine, "answerer");
        assert_eq!(registry.ask(&query("")).len(), 1);
    }

    #[test]
    fn only_matching_and_unconditional_answerers_run() {
        let registry = AnswererRegistry::from_answerers([
            FixedAnswerer::new(&["foo"], false, "foo-answer") as Arc<dyn Answerer>,
            FixedAnswerer::new(&["bar"], false, "bar-answer") as Arc<dyn Answerer>,
            FixedAnswerer::new(&["never"], true, "uncond-answer") as Arc<dyn Answerer>,
        ]);
        let answers: Vec<String> = registry
            .ask(&query("foo 1 2 3"))
            .into_iter()
            .map(|a| a.answer)
            .collect();
        assert_eq!(answers, vec!["foo-answer", "uncond-answer"]);
    }

    #[test]
    fn empty_registry_produces_nothing() {
        assert!(AnswererRegistry::new().ask(&query("anything")).is_empty());
    }

    #[test]
    fn builtins_are_registered() {
        let registry = AnswererRegistry::with_builtins();
        assert_eq!(registry.len(), 2);
        let answers = registry.ask(&query("sum 1 2 3"));
        assert_eq!(answers.len(), 1);
        assert!(answers[0].answer.contains('6'));
        let answers = registry.ask(&query("random uuid"));
        assert_eq!(answers.len(), 1);
    }

    #[test]
    fn registry_retains_data_bundle() {
        let mut data = DataBundle::default();
        data.locales
            .locale_names
            .insert("en".to_string(), "English".to_string());
        let registry = AnswererRegistry::with_builtins().with_data(Arc::new(data));

        assert!(registry.data().locales.resolve("en").is_some());
    }
}
