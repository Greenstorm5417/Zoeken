//! Concurrent engine execution with timeouts and deadlines.

use std::collections::HashMap;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::{Duration, Instant};

use tokio::task::JoinSet;
use zoeken_engine_core::{Engine, EngineError, EngineResults, SearchQueryView};

use crate::selection::SelectedEngine;

/// Boxed execute future: engine result plus optional HTTP-leg timing.
pub struct EngineExecResult {
    pub result: Result<EngineResults, EngineError>,
    pub http_duration: Option<Duration>,
}

impl EngineExecResult {
    pub fn from_result(result: Result<EngineResults, EngineError>) -> Self {
        Self {
            result,
            http_duration: None,
        }
    }
}

pub type EngineFuture = Pin<Box<dyn Future<Output = EngineExecResult> + Send>>;

pub trait EngineExecutor: Send + Sync + 'static {
    fn execute(&self, engine: Arc<dyn Engine>, query: SearchQueryView) -> EngineFuture;
}

/// Why an engine was recorded as unresponsive.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnresponsiveReason {
    /// The engine exceeded its own per-engine timeout.
    EngineTimeout,
    /// The overall request deadline was reached before the engine responded.
    GlobalDeadline,
}

/// The outcome of running one selected engine.
#[derive(Debug)]
pub enum EngineRunStatus {
    /// The engine responded before the deadline with parsed results.
    Completed(EngineResults),
    /// The engine responded before the deadline but returned an error.
    Failed(EngineError),
    /// The engine did not respond before its timeout or the global deadline.
    Unresponsive(UnresponsiveReason),
}

#[derive(Debug)]
pub struct EngineRunOutcome {
    pub engine: String,
    pub status: EngineRunStatus,
    pub duration: Duration,
    pub http_duration: Option<Duration>,
}

#[derive(Debug, Default)]
pub struct ExecutionReport {
    pub outcomes: Vec<EngineRunOutcome>,
}

impl ExecutionReport {
    pub fn unresponsive_engines(&self) -> Vec<&str> {
        self.outcomes
            .iter()
            .filter(|o| matches!(o.status, EngineRunStatus::Unresponsive(_)))
            .map(|o| o.engine.as_str())
            .collect()
    }

    pub fn responders(&self) -> Vec<(&str, &EngineResults)> {
        self.outcomes
            .iter()
            .filter_map(|o| match &o.status {
                EngineRunStatus::Completed(results) => Some((o.engine.as_str(), results)),
                _ => None,
            })
            .collect()
    }
}

pub async fn run_engines(
    executor: Arc<dyn EngineExecutor>,
    selected: Vec<SelectedEngine>,
    query: SearchQueryView,
    default_engine_timeout: Duration,
    deadline: Instant,
) -> ExecutionReport {
    let order: Vec<String> = selected.iter().map(|s| s.name.clone()).collect();

    let mut join_set: JoinSet<EngineRunOutcome> = JoinSet::new();
    for se in selected {
        let executor = executor.clone();
        let engine_query = crate::engine_query_view(&query, se.engine.metadata());
        let engine_timeout = se.timeout.unwrap_or(default_engine_timeout);
        join_set.spawn(run_one(
            executor,
            se,
            engine_query,
            engine_timeout,
            deadline,
        ));
    }

    let mut by_name: HashMap<String, EngineRunOutcome> = HashMap::new();
    while let Some(joined) = join_set.join_next().await {
        match joined {
            Ok(outcome) => {
                by_name.insert(outcome.engine.clone(), outcome);
            }
            Err(join_err) => {
                tracing::warn!(error = %join_err, "engine task failed to join");
            }
        }
    }

    let outcomes = order
        .into_iter()
        .map(|name| {
            by_name.remove(&name).unwrap_or(EngineRunOutcome {
                engine: name,
                status: EngineRunStatus::Unresponsive(UnresponsiveReason::GlobalDeadline),
                duration: Duration::ZERO,
                http_duration: None,
            })
        })
        .collect();

    ExecutionReport { outcomes }
}

async fn run_one(
    executor: Arc<dyn EngineExecutor>,
    se: SelectedEngine,
    query: SearchQueryView,
    engine_timeout: Duration,
    deadline: Instant,
) -> EngineRunOutcome {
    let name = se.name;
    let now = Instant::now();

    let Some(remaining) = deadline.checked_duration_since(now) else {
        return EngineRunOutcome {
            engine: name,
            status: EngineRunStatus::Unresponsive(UnresponsiveReason::GlobalDeadline),
            duration: Duration::ZERO,
            http_duration: None,
        };
    };
    if remaining.is_zero() {
        return EngineRunOutcome {
            engine: name,
            status: EngineRunStatus::Unresponsive(UnresponsiveReason::GlobalDeadline),
            duration: Duration::ZERO,
            http_duration: None,
        };
    }

    // Bound the engine by the smaller of its own timeout and the time left
    // until the global deadline.
    let budget = engine_timeout.min(remaining);
    // When the engine timeout is the tighter bound, a timeout is the engine's
    // fault; otherwise the global deadline cut it off.
    let timeout_reason = if engine_timeout <= remaining {
        UnresponsiveReason::EngineTimeout
    } else {
        UnresponsiveReason::GlobalDeadline
    };

    let started = Instant::now();
    let fut = executor.execute(se.engine, query);
    let (status, http_duration) = match tokio::time::timeout(budget, fut).await {
        Ok(exec) => {
            let status = match exec.result {
                Ok(results) => EngineRunStatus::Completed(results),
                Err(error) => EngineRunStatus::Failed(error),
            };
            (status, exec.http_duration)
        }
        Err(_elapsed) => (EngineRunStatus::Unresponsive(timeout_reason), None),
    };
    let duration = started.elapsed();

    EngineRunOutcome {
        engine: name,
        status,
        duration,
        http_duration,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoeken_engine_core::{EngineMeta, EngineResponse, EngineResults, RequestParams};
    use zoeken_results::{MainResult, Result_};

    /// A trivial engine; the test executor decides its behavior, so this only
    /// needs to supply a name via metadata.
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

    /// Behavior the test executor applies, keyed by engine name.
    #[derive(Clone)]
    enum Behavior {
        /// Sleep `delay`, then return a single result whose title is the name.
        Respond(Duration),
        /// Sleep `delay`, then return an error.
        Fail(Duration),
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
                    Some(Behavior::Fail(delay)) => {
                        tokio::time::sleep(delay).await;
                        Err(EngineError::Unexpected("boom".to_string()))
                    }
                    None => Ok(EngineResults::new()),
                })
            })
        }
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

    fn outcome_for<'a>(report: &'a ExecutionReport, name: &str) -> &'a EngineRunStatus {
        &report
            .outcomes
            .iter()
            .find(|o| o.engine == name)
            .expect("engine in report")
            .status
    }

    #[tokio::test]
    async fn collects_responders_and_failures() {
        let executor = Arc::new(TestExecutor {
            behaviors: HashMap::from([
                (
                    "fast".to_string(),
                    Behavior::Respond(Duration::from_millis(5)),
                ),
                (
                    "broken".to_string(),
                    Behavior::Fail(Duration::from_millis(5)),
                ),
            ]),
        });
        let report = run_engines(
            executor,
            selected(&["fast", "broken"]),
            SearchQueryView::default(),
            Duration::from_secs(5),
            Instant::now() + Duration::from_secs(5),
        )
        .await;

        assert!(matches!(
            outcome_for(&report, "fast"),
            EngineRunStatus::Completed(_)
        ));
        assert!(matches!(
            outcome_for(&report, "broken"),
            EngineRunStatus::Failed(_)
        ));
        assert_eq!(report.responders().len(), 1);
        assert!(report.unresponsive_engines().is_empty());
    }

    #[tokio::test(start_paused = true)]
    async fn slow_engine_past_deadline_is_unresponsive() {
        let executor = Arc::new(TestExecutor {
            behaviors: HashMap::from([
                (
                    "fast".to_string(),
                    Behavior::Respond(Duration::from_millis(10)),
                ),
                (
                    "slow".to_string(),
                    Behavior::Respond(Duration::from_secs(30)),
                ),
            ]),
        });
        // Global deadline of 1s: the 30s engine cannot make it.
        let report = run_engines(
            executor,
            selected(&["fast", "slow"]),
            SearchQueryView::default(),
            Duration::from_secs(10),
            Instant::now() + Duration::from_secs(1),
        )
        .await;

        assert!(matches!(
            outcome_for(&report, "fast"),
            EngineRunStatus::Completed(_)
        ));
        assert!(matches!(
            outcome_for(&report, "slow"),
            EngineRunStatus::Unresponsive(UnresponsiveReason::GlobalDeadline)
        ));
        assert_eq!(report.unresponsive_engines(), vec!["slow"]);
    }

    #[tokio::test(start_paused = true)]
    async fn per_engine_timeout_flags_unresponsive_before_deadline() {
        let executor = Arc::new(TestExecutor {
            behaviors: HashMap::from([(
                "slow".to_string(),
                Behavior::Respond(Duration::from_secs(5)),
            )]),
        });
        // Engine timeout (1s) is tighter than the global deadline (60s).
        let mut engines = selected(&["slow"]);
        engines[0].timeout = Some(Duration::from_secs(1));
        let report = run_engines(
            executor,
            engines,
            SearchQueryView::default(),
            Duration::from_secs(10),
            Instant::now() + Duration::from_secs(60),
        )
        .await;

        assert!(matches!(
            outcome_for(&report, "slow"),
            EngineRunStatus::Unresponsive(UnresponsiveReason::EngineTimeout)
        ));
    }

    #[tokio::test]
    async fn outcomes_follow_selection_order() {
        let executor = Arc::new(TestExecutor {
            behaviors: HashMap::from([
                (
                    "a".to_string(),
                    Behavior::Respond(Duration::from_millis(20)),
                ),
                ("b".to_string(), Behavior::Respond(Duration::from_millis(1))),
                (
                    "c".to_string(),
                    Behavior::Respond(Duration::from_millis(10)),
                ),
            ]),
        });
        let report = run_engines(
            executor,
            selected(&["a", "b", "c"]),
            SearchQueryView::default(),
            Duration::from_secs(5),
            Instant::now() + Duration::from_secs(5),
        )
        .await;

        let order: Vec<&str> = report.outcomes.iter().map(|o| o.engine.as_str()).collect();
        assert_eq!(order, vec!["a", "b", "c"]);
    }
}
