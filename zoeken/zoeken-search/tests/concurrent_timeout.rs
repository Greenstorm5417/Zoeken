//! Integration tests for concurrent execution and timeout classification.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use zoeken_engine_core::{
    Engine, EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};
use zoeken_search::{
    EngineExecResult, EngineExecutor, EngineFuture, EngineRunStatus, ExecutionReport,
    SelectedEngine, UnresponsiveReason, run_engines,
};

struct StubEngine {
    meta: EngineMeta,
}

fn stub(name: &str) -> Arc<dyn Engine> {
    Arc::new(StubEngine {
        meta: EngineMeta {
            name: name.to_string(),
            ..EngineMeta::default()
        },
    })
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

#[derive(Clone)]
enum Behavior {
    Respond(Duration),
}

struct TestExecutor {
    behaviors: HashMap<String, Behavior>,
}

impl EngineExecutor for TestExecutor {
    fn execute(&self, engine: Arc<dyn Engine>, _query: SearchQueryView) -> EngineFuture {
        let name = engine.metadata().name.clone();
        let behavior = self.behaviors.get(&name).cloned();
        Box::pin(async move {
            EngineExecResult::from_result(match behavior {
                Some(Behavior::Respond(delay)) => {
                    tokio::time::sleep(delay).await;
                    let mut results = EngineResults::new();
                    results.add(Result_::Main(MainResult {
                        url: format!("https://{name}.test/"),
                        normalized_url: format!("https://{name}.test/"),
                        title: name.clone(),
                        engine: name,
                        ..MainResult::default()
                    }));
                    Ok(results)
                }
                None => Ok(EngineResults::new()),
            })
        })
    }
}

fn executor(behaviors: impl IntoIterator<Item = (&'static str, Behavior)>) -> Arc<TestExecutor> {
    Arc::new(TestExecutor {
        behaviors: behaviors
            .into_iter()
            .map(|(name, behavior)| (name.to_string(), behavior))
            .collect(),
    })
}

fn selected(names: &[&str]) -> Vec<SelectedEngine> {
    names
        .iter()
        .map(|n| SelectedEngine {
            name: n.to_string(),
            engine: stub(n),
            timeout: None,
        })
        .collect()
}

fn status_for<'a>(report: &'a ExecutionReport, name: &str) -> &'a EngineRunStatus {
    &report
        .outcomes
        .iter()
        .find(|o| o.engine == name)
        .unwrap_or_else(|| panic!("engine {name} in report"))
        .status
}

#[tokio::test(start_paused = true)]
async fn engines_run_concurrently() {
    let names = ["e0", "e1", "e2", "e3", "e4", "e5", "e6", "e7", "e8", "e9"];
    let per_engine = Duration::from_millis(500);
    let exec = executor(names.iter().map(|n| (*n, Behavior::Respond(per_engine))));

    let start = Instant::now();
    let report = run_engines(
        exec,
        selected(&names),
        SearchQueryView::default(),
        Duration::from_secs(10),
        start + Duration::from_secs(2),
    )
    .await;
    let elapsed = start.elapsed();

    assert_eq!(report.responders().len(), names.len());
    assert!(report.unresponsive_engines().is_empty());
    for name in names {
        assert!(
            matches!(status_for(&report, name), EngineRunStatus::Completed(_)),
            "{name} should have completed"
        );
    }
    assert!(
        elapsed < Duration::from_secs(1),
        "expected concurrent (~{per_engine:?}) run, took {elapsed:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn per_engine_timeout_tighter_than_deadline_flags_engine_timeout() {
    let exec = executor([
        ("fast", Behavior::Respond(Duration::from_millis(50))),
        ("slow", Behavior::Respond(Duration::from_secs(5))),
    ]);

    let mut engines = selected(&["fast", "slow"]);
    engines[1].timeout = Some(Duration::from_secs(1));

    let report = run_engines(
        exec,
        engines,
        SearchQueryView::default(),
        Duration::from_secs(10),
        Instant::now() + Duration::from_secs(60),
    )
    .await;

    assert!(matches!(
        status_for(&report, "fast"),
        EngineRunStatus::Completed(_)
    ));
    assert!(matches!(
        status_for(&report, "slow"),
        EngineRunStatus::Unresponsive(UnresponsiveReason::EngineTimeout)
    ));
    assert_eq!(report.unresponsive_engines(), vec!["slow"]);
}

#[tokio::test(start_paused = true)]
async fn global_deadline_flags_slow_engine_while_fast_completes() {
    let exec = executor([
        ("fast", Behavior::Respond(Duration::from_millis(100))),
        ("slow", Behavior::Respond(Duration::from_secs(30))),
    ]);

    let report = run_engines(
        exec,
        selected(&["fast", "slow"]),
        SearchQueryView::default(),
        Duration::from_secs(20),
        Instant::now() + Duration::from_secs(1),
    )
    .await;

    assert!(matches!(
        status_for(&report, "fast"),
        EngineRunStatus::Completed(_)
    ));
    assert!(matches!(
        status_for(&report, "slow"),
        EngineRunStatus::Unresponsive(UnresponsiveReason::GlobalDeadline)
    ));
    assert_eq!(report.responders().len(), 1);
    assert_eq!(report.unresponsive_engines(), vec!["slow"]);
}

#[tokio::test(start_paused = true)]
async fn deadline_tighter_than_engine_timeout_flags_global_deadline() {
    let exec = executor([("slow", Behavior::Respond(Duration::from_secs(30)))]);

    let mut engines = selected(&["slow"]);
    engines[0].timeout = Some(Duration::from_secs(10));

    let report = run_engines(
        exec,
        engines,
        SearchQueryView::default(),
        Duration::from_secs(5),
        Instant::now() + Duration::from_secs(2),
    )
    .await;

    assert!(matches!(
        status_for(&report, "slow"),
        EngineRunStatus::Unresponsive(UnresponsiveReason::GlobalDeadline)
    ));
}
