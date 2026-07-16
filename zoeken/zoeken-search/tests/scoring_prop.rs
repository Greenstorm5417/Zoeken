//! Property test for weighted scoring and the multi-engine boost.

use std::collections::HashMap;
use std::time::Duration;

use proptest::prelude::*;
use zoeken_engine_core::EngineResults;
use zoeken_results::{MainResult, Result_};
use zoeken_search::execution::{EngineRunOutcome, EngineRunStatus, ExecutionReport};
use zoeken_search::{EngineWeights, NoopRecorder, aggregate};

fn main_result(url: &str) -> Result_ {
    Result_::Main(MainResult {
        url: url.to_string(),
        normalized_url: url.to_string(),
        title: url.to_string(),
        ..MainResult::default()
    })
}

fn completed(engine: &str, results: EngineResults) -> EngineRunOutcome {
    EngineRunOutcome {
        engine: engine.to_string(),
        status: EngineRunStatus::Completed(results),
        duration: Duration::from_millis(5),
        http_duration: None,
    }
}

fn report(outcomes: Vec<EngineRunOutcome>) -> ExecutionReport {
    ExecutionReport { outcomes }
}

fn weights(pairs: &[(String, f64)]) -> EngineWeights {
    EngineWeights::new(pairs.iter().cloned())
}

fn score_of(result: &Result_) -> f64 {
    match result {
        Result_::Main(m) => m.score,
        other => panic!("expected MainResult, got {other:?}"),
    }
}

fn key_of(result: &Result_) -> &str {
    match result {
        Result_::Main(m) => m.normalized_url.as_str(),
        other => panic!("expected MainResult, got {other:?}"),
    }
}

fn approx_eq(a: f64, b: f64) -> bool {
    (a - b).abs() <= 1e-9 + 1e-6 * a.abs().max(b.abs())
}

fn reference_scores(
    engines: &[(String, Vec<String>)],
    weight_of: &HashMap<String, f64>,
) -> HashMap<String, f64> {
    let mut positions: HashMap<String, Vec<usize>> = HashMap::new();
    let mut contributors: HashMap<String, Vec<String>> = HashMap::new();

    for (engine, urls) in engines {
        for (idx, url) in urls.iter().enumerate() {
            let position = idx + 1;
            positions.entry(url.clone()).or_default().push(position);
            let engs = contributors.entry(url.clone()).or_default();
            if !engs.iter().any(|e| e == engine) {
                engs.push(engine.clone());
            }
        }
    }

    let mut scores = HashMap::new();
    for (url, pos) in &positions {
        let mut weight = 1.0_f64;
        for engine in &contributors[url] {
            weight *= weight_of.get(engine).copied().unwrap_or(1.0);
        }
        weight *= pos.len() as f64;

        let mut score = 0.0_f64;
        for &p in pos {
            score += weight / (p.max(1) as f64);
        }
        scores.insert(url.clone(), score);
    }
    scores
}

fn url_strategy() -> impl Strategy<Value = String> {
    (0usize..6).prop_map(|n| format!("https://u{n}.test/"))
}

fn engine_strategy() -> impl Strategy<Value = (f64, Vec<String>)> {
    (0.25f64..4.0, prop::collection::vec(url_strategy(), 0..=6))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn scoring_matches_reference_formula(
        raw_engines in prop::collection::vec(engine_strategy(), 0..=5)
    ) {
        let engines: Vec<(String, Vec<String>)> = raw_engines
            .iter()
            .enumerate()
            .map(|(i, (_w, urls))| (format!("e{i}"), urls.clone()))
            .collect();
        let weight_of: HashMap<String, f64> = raw_engines
            .iter()
            .enumerate()
            .map(|(i, (w, _urls))| (format!("e{i}"), *w))
            .collect();

        let outcomes: Vec<EngineRunOutcome> = engines
            .iter()
            .map(|(name, urls)| {
                let mut results = EngineResults::new();
                for url in urls {
                    results.add(main_result(url));
                }
                completed(name, results)
            })
            .collect();

        let weight_pairs: Vec<(String, f64)> =
            weight_of.iter().map(|(k, v)| (k.clone(), *v)).collect();
        let container = aggregate(report(outcomes), &weights(&weight_pairs), &NoopRecorder);

        let expected = reference_scores(&engines, &weight_of);

        prop_assert_eq!(container.results.len(), expected.len());
        prop_assert_eq!(container.number_of_results, expected.len());

        for result in &container.results {
            let key = key_of(result).to_string();
            let want = *expected
                .get(&key)
                .expect("aggregated URL must appear in reference");
            let got = score_of(result);
            prop_assert!(
                approx_eq(got, want),
                "score mismatch for {key}: got {got}, want {want}"
            );
        }
    }

    #[test]
    fn more_engines_score_strictly_higher(
        engine_weights in prop::collection::vec(1.0f64..4.0, 2..=7)
    ) {
        let url = "https://shared.test/";
        let n_plus_1 = engine_weights.len();
        let n = n_plus_1 - 1;

        let names: Vec<String> = (0..n_plus_1).map(|i| format!("e{i}")).collect();
        let weight_pairs: Vec<(String, f64)> = names
            .iter()
            .zip(engine_weights.iter())
            .map(|(name, w)| (name.clone(), *w))
            .collect();
        let table = weights(&weight_pairs);

        let make_report = |count: usize| {
            let outcomes: Vec<EngineRunOutcome> = names
                .iter()
                .take(count)
                .map(|name| {
                    let mut results = EngineResults::new();
                    results.add(main_result(url));
                    completed(name, results)
                })
                .collect();
            report(outcomes)
        };

        let fewer = aggregate(make_report(n), &table, &NoopRecorder);
        let more = aggregate(make_report(n_plus_1), &table, &NoopRecorder);

        prop_assert_eq!(fewer.results.len(), 1);
        prop_assert_eq!(more.results.len(), 1);

        let fewer_score = score_of(&fewer.results[0]);
        let more_score = score_of(&more.results[0]);
        prop_assert!(
            more_score > fewer_score,
            "N+1={n_plus_1} engine score {more_score} must strictly exceed N={n} engine score {fewer_score}"
        );
    }
}
