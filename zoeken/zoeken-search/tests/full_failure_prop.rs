// Property test for total aggregation under full failure.

use std::time::Duration;

use proptest::prelude::*;
use zoeken_engine_core::{EngineError, EngineResults};
use zoeken_search::{
    EngineRunOutcome, EngineRunStatus, EngineWeights, ExecutionReport, NoopRecorder,
    UnresponsiveCause, UnresponsiveEngine, UnresponsiveReason, aggregate,
};

#[derive(Debug, Clone)]
enum EngineFate {
    Failed(EngineError),
    EngineTimeout,
    GlobalDeadline,
    CompletedEmpty,
}

fn engine_error_strategy() -> impl Strategy<Value = EngineError> {
    prop_oneof![
        ".*".prop_map(EngineError::AccessDenied),
        ".*".prop_map(EngineError::Captcha),
        ".*".prop_map(EngineError::TooManyRequests),
        ".*".prop_map(EngineError::Parse),
        Just(EngineError::Timeout),
        ".*".prop_map(EngineError::Unexpected),
    ]
}

fn engine_fate_strategy() -> impl Strategy<Value = EngineFate> {
    prop_oneof![
        engine_error_strategy().prop_map(EngineFate::Failed),
        Just(EngineFate::EngineTimeout),
        Just(EngineFate::GlobalDeadline),
        Just(EngineFate::CompletedEmpty),
    ]
}

fn build_outcome(
    engine: String,
    fate: &EngineFate,
) -> (EngineRunOutcome, Option<UnresponsiveEngine>) {
    let (status, expected) = match fate {
        EngineFate::Failed(err) => (
            EngineRunStatus::Failed(err.clone()),
            Some(UnresponsiveEngine {
                engine: engine.clone(),
                cause: UnresponsiveCause::Error(err.to_string()),
            }),
        ),
        EngineFate::EngineTimeout => (
            EngineRunStatus::Unresponsive(UnresponsiveReason::EngineTimeout),
            Some(UnresponsiveEngine {
                engine: engine.clone(),
                cause: UnresponsiveCause::Timeout,
            }),
        ),
        EngineFate::GlobalDeadline => (
            EngineRunStatus::Unresponsive(UnresponsiveReason::GlobalDeadline),
            Some(UnresponsiveEngine {
                engine: engine.clone(),
                cause: UnresponsiveCause::DeadlineExceeded,
            }),
        ),
        EngineFate::CompletedEmpty => (EngineRunStatus::Completed(EngineResults::new()), None),
    };
    (
        EngineRunOutcome {
            engine,
            status,
            duration: Duration::from_millis(1),
            http_duration: None,
        },
        expected,
    )
}

fn assert_total_on_full_failure(
    outcomes: Vec<EngineRunOutcome>,
    expected_unresponsive: Vec<UnresponsiveEngine>,
    weights: &EngineWeights,
) -> Result<(), TestCaseError> {
    let report = ExecutionReport { outcomes };
    let container = aggregate(report, weights, &NoopRecorder);

    prop_assert!(container.results.is_empty());
    prop_assert_eq!(container.number_of_results, 0);

    prop_assert_eq!(container.unresponsive_engines, expected_unresponsive);
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn aggregation_total_on_full_failure(fates in prop::collection::vec(engine_fate_strategy(), 0..12)) {
        let mut outcomes = Vec::with_capacity(fates.len());
        let mut expected_unresponsive = Vec::new();
        let mut names = Vec::with_capacity(fates.len());
        for (i, fate) in fates.iter().enumerate() {
            let name = format!("engine_{i}");
            names.push(name.clone());
            let (outcome, expected) = build_outcome(name, fate);
            outcomes.push(outcome);
            if let Some(e) = expected {
                expected_unresponsive.push(e);
            }
        }

        assert_total_on_full_failure(
            clone_outcomes(&outcomes),
            expected_unresponsive.clone(),
            &EngineWeights::default(),
        )?;

        let populated = EngineWeights::new(names.into_iter().map(|n| (n, 2.0)));
        assert_total_on_full_failure(outcomes, expected_unresponsive, &populated)?;
    }
}

fn clone_outcomes(outcomes: &[EngineRunOutcome]) -> Vec<EngineRunOutcome> {
    outcomes
        .iter()
        .map(|o| {
            let status = match &o.status {
                EngineRunStatus::Completed(_) => EngineRunStatus::Completed(EngineResults::new()),
                EngineRunStatus::Failed(err) => EngineRunStatus::Failed(err.clone()),
                EngineRunStatus::Unresponsive(reason) => EngineRunStatus::Unresponsive(*reason),
            };
            EngineRunOutcome {
                engine: o.engine.clone(),
                status,
                duration: o.duration,
                http_duration: None,
            }
        })
        .collect()
}
