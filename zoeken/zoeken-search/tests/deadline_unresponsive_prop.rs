// Deadline property test for responder vs. non-responder classification.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use zoeken_engine_core::{
    Engine, EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};
use zoeken_search::{EngineExecResult, EngineExecutor, EngineFuture, SelectedEngine, run_engines};

use proptest::collection::vec;
use proptest::prelude::*;

struct StubEngine {
    meta: EngineMeta,
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

fn stub(name: &str) -> Arc<dyn Engine> {
    Arc::new(StubEngine {
        meta: EngineMeta {
            name: name.to_string(),
            ..EngineMeta::default()
        },
    })
}

struct LatencyExecutor {
    latencies: HashMap<String, Duration>,
}

impl EngineExecutor for LatencyExecutor {
    fn execute(&self, engine: Arc<dyn Engine>, _query: SearchQueryView) -> EngineFuture {
        let name = engine.metadata().name.clone();
        let latency = self.latencies.get(&name).copied().unwrap_or_default();
        Box::pin(async move {
            tokio::time::sleep(latency).await;
            let mut results = EngineResults::new();
            results.add(Result_::Main(MainResult {
                url: format!("https://{name}.test/"),
                normalized_url: format!("https://{name}.test/"),
                title: name.clone(),
                engine: name,
                ..MainResult::default()
            }));
            EngineExecResult::from_result(Ok(results))
        })
    }
}

const DEADLINE_MS: u64 = 1_000;

#[derive(Debug, Clone)]
struct EngineSpec {
    responds: bool,
    offset_ms: u64,
}

impl EngineSpec {
    fn latency(&self) -> Duration {
        if self.responds {
            Duration::from_millis(DEADLINE_MS - self.offset_ms)
        } else {
            Duration::from_millis(DEADLINE_MS + self.offset_ms)
        }
    }
}

fn engine_spec() -> impl Strategy<Value = EngineSpec> {
    (any::<bool>(), 1u64..DEADLINE_MS, 1u64..=5_000).prop_map(|(responds, fast_off, slow_off)| {
        EngineSpec {
            responds,
            offset_ms: if responds { fast_off } else { slow_off },
        }
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn deadline_yields_responders_and_flags_non_responders(
        specs in vec(engine_spec(), 0..=8),
    ) {
        let named: Vec<(String, EngineSpec)> = specs
            .into_iter()
            .enumerate()
            .map(|(i, spec)| (format!("e{i}"), spec))
            .collect();

        let latencies: HashMap<String, Duration> = named
            .iter()
            .map(|(name, spec)| (name.clone(), spec.latency()))
            .collect();

        let mut expected_fast: Vec<String> = named
            .iter()
            .filter(|(_, spec)| spec.responds)
            .map(|(name, _)| name.clone())
            .collect();
        let mut expected_slow: Vec<String> = named
            .iter()
            .filter(|(_, spec)| !spec.responds)
            .map(|(name, _)| name.clone())
            .collect();
        expected_fast.sort();
        expected_slow.sort();

        let selected: Vec<SelectedEngine> = named
            .iter()
            .map(|(name, _)| SelectedEngine {
                name: name.clone(),
                engine: stub(name),
                timeout: None,
            })
            .collect();

        let executor = Arc::new(LatencyExecutor { latencies });

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .start_paused(true)
            .build()
            .unwrap();

        let report = rt.block_on(async move {
            let deadline = Instant::now() + Duration::from_millis(DEADLINE_MS);
            run_engines(
                executor,
                selected,
                SearchQueryView::default(),
                Duration::from_secs(3_600),
                deadline,
            )
            .await
        });

        let mut responder_names: Vec<String> = report
            .responders()
            .iter()
            .map(|(name, results)| {
                prop_assert!(
                    results
                        .results
                        .iter()
                        .any(|r| matches!(r, Result_::Main(m) if m.title == *name)),
                    "responder {name} is missing its own result in the report",
                );
                Ok((*name).to_string())
            })
            .collect::<Result<Vec<_>, TestCaseError>>()?;
        responder_names.sort();

        let mut unresponsive_names: Vec<String> = report
            .unresponsive_engines()
            .iter()
            .map(|name| name.to_string())
            .collect();
        unresponsive_names.sort();

        prop_assert_eq!(responder_names, expected_fast);
        prop_assert_eq!(unresponsive_names, expected_slow);
    }
}
