// Dedup-merge property test.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use proptest::prelude::*;
use zoeken_engine_core::EngineResults;
use zoeken_results::{MainResult, Result_};
use zoeken_search::{
    EngineRunOutcome, EngineRunStatus, EngineWeights, ExecutionReport, NoopRecorder, aggregate,
};

const POOL: usize = 4;

fn url_for(i: usize) -> String {
    format!("https://site{i}.test/")
}

fn engine_name(i: usize) -> String {
    format!("engine{i}")
}

fn engines_strategy() -> impl Strategy<Value = Vec<Vec<usize>>> {
    prop::collection::vec(prop::collection::vec(0..POOL, 0..=5), 1..=4)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn dedup_merge_uniqueness_and_union(engines in engines_strategy()) {
        let mut outcomes = Vec::new();
        for (ei, urls) in engines.iter().enumerate() {
            let mut results = EngineResults::new();
            for &u in urls {
                let url = url_for(u);
                results.add(Result_::Main(MainResult {
                    url: url.clone(),
                    normalized_url: url,
                    title: format!("t{u}"),
                    ..MainResult::default()
                }));
            }
            outcomes.push(EngineRunOutcome {
                engine: engine_name(ei),
                status: EngineRunStatus::Completed(results),
                duration: Duration::from_millis(1),
                http_duration: None,
            });
        }

        let weights = EngineWeights::new((0..engines.len()).map(|i| (engine_name(i), 1.0)));
        let container = aggregate(ExecutionReport { outcomes }, &weights, &NoopRecorder);

        let mut expected: HashMap<String, (Vec<String>, Vec<usize>)> = HashMap::new();
        for (ei, urls) in engines.iter().enumerate() {
            let name = engine_name(ei);
            for (idx, &u) in urls.iter().enumerate() {
                let position = idx + 1;
                let entry = expected.entry(url_for(u)).or_default();
                if !entry.0.iter().any(|e| e == &name) {
                    entry.0.push(name.clone());
                }
                entry.1.push(position);
            }
        }

        let mut seen: HashSet<String> = HashSet::new();
        for r in &container.results {
            let nu = match r {
                Result_::Main(m) => m.normalized_url.clone(),
                other => panic!("expected Main result, got {other:?}"),
            };
            prop_assert!(
                seen.insert(nu.clone()),
                "normalized_url {nu} appeared in more than one merged result"
            );
        }

        prop_assert_eq!(container.results.len(), expected.len());

        for r in &container.results {
            let m = match r {
                Result_::Main(m) => m,
                other => panic!("expected Main result, got {other:?}"),
            };
            let (exp_engines, exp_positions) = expected
                .get(&m.normalized_url)
                .expect("merged url must exist in the reference model");

            prop_assert_eq!(&m.engines, exp_engines);

            let mut got = m.positions.clone();
            got.sort_unstable();
            let mut want = exp_positions.clone();
            want.sort_unstable();
            prop_assert_eq!(got, want);
        }
    }
}
