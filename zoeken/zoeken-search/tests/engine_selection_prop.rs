//! Property-based test for engine selection set algebra.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::{Duration, Instant};

use proptest::prelude::*;

use zoeken_engine_core::{
    Engine, EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams, SearchQueryView,
    SuspendConfig,
};
use zoeken_query::SearchQuery;
use zoeken_search::{EnabledEngineSet, EngineRegistry, RegisteredEngine, SelectedEngine};

struct StubEngine {
    meta: EngineMeta,
}

impl StubEngine {
    fn arc(name: &str, categories: &[String]) -> Arc<dyn Engine> {
        Arc::new(StubEngine {
            meta: EngineMeta {
                name: name.to_string(),
                categories: categories.to_vec(),
                ..EngineMeta::default()
            },
        })
    }
}

impl Engine for StubEngine {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }
    fn request(&self, _q: &SearchQueryView, _p: &mut RequestParams) {}
    fn response(&self, _resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        Ok(EngineResults::new())
    }
}

#[derive(Debug, Clone)]
struct EngineSpec {
    name: String,
    categories: Vec<String>,
    disabled: bool,
    suspended: bool,
    required_tokens: Vec<String>,
}

const NAMES: &[&str] = &["a", "b", "c", "d", "e"];
const CATEGORIES: &[&str] = &["general", "images", "news"];
const TOKENS: &[&str] = &["t0", "t1", "t2"];

fn name_strategy() -> impl Strategy<Value = String> {
    prop::sample::select(NAMES).prop_map(String::from)
}

fn categories_strategy() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(
        prop::sample::select(CATEGORIES).prop_map(String::from),
        0..3,
    )
}

fn tokens_strategy() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(prop::sample::select(TOKENS).prop_map(String::from), 0..3)
}

fn engine_spec_strategy() -> impl Strategy<Value = EngineSpec> {
    (
        name_strategy(),
        categories_strategy(),
        any::<bool>(),
        any::<bool>(),
        tokens_strategy(),
    )
        .prop_map(
            |(name, categories, disabled, suspended, required_tokens)| EngineSpec {
                name,
                categories,
                disabled,
                suspended,
                required_tokens,
            },
        )
}

#[derive(Debug, Clone)]
struct Scenario {
    universe: Vec<EngineSpec>,
    query_categories: Vec<String>,
    query_engines: Vec<String>,
    enabled_prefs: Vec<String>,
    available_tokens: Vec<String>,
}

fn scenario_strategy() -> impl Strategy<Value = Scenario> {
    (
        prop::collection::vec(engine_spec_strategy(), 0..8),
        categories_strategy(),
        prop::collection::vec(name_strategy(), 0..5),
        prop::collection::vec(name_strategy(), 0..5),
        tokens_strategy(),
    )
        .prop_map(
            |(universe, query_categories, query_engines, enabled_prefs, available_tokens)| {
                Scenario {
                    universe,
                    query_categories,
                    query_engines,
                    enabled_prefs,
                    available_tokens,
                }
            },
        )
}

fn reference_selection(
    universe: &[EngineSpec],
    query_categories: &[String],
    query_engines: &[String],
    enabled_prefs: &HashSet<String>,
    available_tokens: &HashSet<String>,
) -> Vec<String> {
    universe
        .iter()
        .filter(|s| {
            // An explicit `engines=` selection may summon a disabled engine.
            let explicitly_requested = !query_engines.is_empty() && query_engines.contains(&s.name);
            if (s.disabled && !explicitly_requested) || s.suspended {
                return false;
            }
            if !s
                .required_tokens
                .iter()
                .all(|t| available_tokens.contains(t))
            {
                return false;
            }
            let category_ok = query_categories.is_empty()
                || s.categories.iter().any(|c| query_categories.contains(c));
            let bang_ok = query_engines.is_empty() || query_engines.contains(&s.name);
            let pref_ok = enabled_prefs.contains(&s.name);
            category_ok && bang_ok && pref_ok
        })
        .map(|s| s.name.clone())
        .collect()
}

fn selected_names(selected: &[SelectedEngine]) -> Vec<String> {
    selected.iter().map(|s| s.name.clone()).collect()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_engine_selection_set_algebra(scenario in scenario_strategy()) {
        let now = Instant::now();
        let hour = Duration::from_secs(3600);

        let mut registered = Vec::with_capacity(scenario.universe.len());
        for spec in &scenario.universe {
            let mut re = RegisteredEngine::new(StubEngine::arc(&spec.name, &spec.categories));
            if spec.disabled {
                re = re.disabled();
            }
            if !spec.required_tokens.is_empty() {
                re = re.with_tokens(spec.required_tokens.clone());
            }
            if spec.suspended {
                re.state.lock().unwrap().on_error(
                    now,
                    &SuspendConfig::new(1, hour, hour),
                    "suspended",
                    Some(hour),
                );
            }
            registered.push(re);
        }
        let registry = EngineRegistry::from_engines(registered);

        let query = SearchQuery {
            categories: scenario.query_categories.clone(),
            engines: scenario.query_engines.clone(),
            ..SearchQuery::default()
        };
        let prefs = EnabledEngineSet::new(scenario.enabled_prefs.clone());
        let enabled_set: HashSet<String> = scenario.enabled_prefs.iter().cloned().collect();
        let tokens: HashSet<String> = scenario.available_tokens.iter().cloned().collect();

        let actual = selected_names(&registry.select(&query, &prefs, &tokens, now));
        let expected = reference_selection(
            &scenario.universe,
            &scenario.query_categories,
            &scenario.query_engines,
            &enabled_set,
            &tokens,
        );

        prop_assert_eq!(actual, expected);
    }
}
