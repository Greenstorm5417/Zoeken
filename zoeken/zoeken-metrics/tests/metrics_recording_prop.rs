//! Per-engine metrics recording property tests: exactly one record per outcome.

use std::sync::{Arc, Mutex};
use std::time::Duration;

use metrics::{
    Counter, CounterFn, Gauge, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
    SharedString, Unit,
};
use proptest::prelude::*;
use zoeken_engine_core::EngineError;
use zoeken_metrics::{
    CATEGORY_LABEL, ENGINE_ERRORS_TOTAL, ENGINE_LABEL, ENGINE_RESPONSE_TIME_TOTAL,
    EngineMetricsRecorder, categorize_error,
};

/// Metric emission captured for test assertion.
#[derive(Debug, Clone, PartialEq)]
struct Emission {
    name: String,
    labels: Vec<(String, String)>,
}

#[derive(Debug, Default)]
struct Captured {
    histograms: Vec<Emission>,
    counters: Vec<Emission>,
}

#[derive(Clone, Default)]
struct CapturingRecorder {
    inner: Arc<Mutex<Captured>>,
}

fn labels_of(key: &Key) -> Vec<(String, String)> {
    key.labels()
        .map(|l| (l.key().to_string(), l.value().to_string()))
        .collect()
}

struct CounterHandle {
    key: Key,
    inner: Arc<Mutex<Captured>>,
}

impl CounterFn for CounterHandle {
    fn increment(&self, _value: u64) {
        self.inner.lock().unwrap().counters.push(Emission {
            name: self.key.name().to_string(),
            labels: labels_of(&self.key),
        });
    }

    fn absolute(&self, _value: u64) {
        self.inner.lock().unwrap().counters.push(Emission {
            name: self.key.name().to_string(),
            labels: labels_of(&self.key),
        });
    }
}

struct HistogramHandle {
    key: Key,
    inner: Arc<Mutex<Captured>>,
}

impl HistogramFn for HistogramHandle {
    fn record(&self, _value: f64) {
        self.inner.lock().unwrap().histograms.push(Emission {
            name: self.key.name().to_string(),
            labels: labels_of(&self.key),
        });
    }
}

impl Recorder for CapturingRecorder {
    fn describe_counter(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_gauge(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}
    fn describe_histogram(&self, _: KeyName, _: Option<Unit>, _: SharedString) {}

    fn register_counter(&self, key: &Key, _: &Metadata<'_>) -> Counter {
        Counter::from_arc(Arc::new(CounterHandle {
            key: key.clone(),
            inner: self.inner.clone(),
        }))
    }

    fn register_gauge(&self, _: &Key, _: &Metadata<'_>) -> Gauge {
        Gauge::noop()
    }

    fn register_histogram(&self, key: &Key, _: &Metadata<'_>) -> Histogram {
        Histogram::from_arc(Arc::new(HistogramHandle {
            key: key.clone(),
            inner: self.inner.clone(),
        }))
    }
}

fn has_label(labels: &[(String, String)], key: &str, value: &str) -> bool {
    labels.iter().any(|(k, v)| k == key && v == value)
}

/// Test outcome: completion or failure.
#[derive(Debug, Clone)]
enum OutcomeKind {
    Completed,
    Failed(EngineError),
}

/// Arbitrary EngineError strategy.
fn error_strategy() -> impl Strategy<Value = EngineError> {
    let msg = "[a-zA-Z0-9 :/_-]{0,24}";
    prop_oneof![
        Just(EngineError::Timeout),
        msg.prop_map(EngineError::AccessDenied),
        msg.prop_map(EngineError::Captcha),
        msg.prop_map(EngineError::TooManyRequests),
        msg.prop_map(EngineError::Parse),
        msg.prop_map(EngineError::Unexpected),
    ]
}

/// Completion or failure outcome strategy.
fn outcome_strategy() -> impl Strategy<Value = OutcomeKind> {
    prop_oneof![
        Just(OutcomeKind::Completed),
        error_strategy().prop_map(OutcomeKind::Failed),
    ]
}

/// Drive recorder and collect emissions.
fn drive(engine: &str, duration: Duration, outcome: &OutcomeKind) -> Captured {
    let recorder = CapturingRecorder::default();
    let captured = recorder.inner.clone();
    metrics::with_local_recorder(&recorder, || {
        let backend = EngineMetricsRecorder::new();
        match outcome {
            OutcomeKind::Completed => backend.record_timing(engine, duration, None),
            OutcomeKind::Failed(error) => backend.record_error(engine, categorize_error(error)),
        }
    });
    let captured = captured.lock().unwrap();
    Captured {
        histograms: captured.histograms.clone(),
        counters: captured.counters.clone(),
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn exactly_one_record_per_engine_outcome(
        engine in "[a-z][a-z0-9_]{0,19}",
        millis in 0u64..10_000,
        outcome in outcome_strategy(),
    ) {
        let duration = Duration::from_millis(millis);
        let captured = drive(&engine, duration, &outcome);

        match &outcome {
            OutcomeKind::Completed => {
                prop_assert_eq!(
                    captured.histograms.len(),
                    1,
                    "completion must emit exactly one timing sample"
                );
                prop_assert_eq!(&captured.histograms[0].name, ENGINE_RESPONSE_TIME_TOTAL);
                prop_assert!(has_label(&captured.histograms[0].labels, ENGINE_LABEL, &engine));
                prop_assert_eq!(
                    captured.counters.len(),
                    0,
                    "completion must not emit an error counter"
                );
            }
            OutcomeKind::Failed(error) => {
                prop_assert_eq!(
                    captured.counters.len(),
                    1,
                    "failure must emit exactly one error counter"
                );
                prop_assert_eq!(&captured.counters[0].name, ENGINE_ERRORS_TOTAL);
                prop_assert!(has_label(&captured.counters[0].labels, ENGINE_LABEL, &engine));
                let expected_category = categorize_error(error).as_str();
                prop_assert!(has_label(
                    &captured.counters[0].labels,
                    CATEGORY_LABEL,
                    expected_category
                ));
                prop_assert_eq!(
                    captured.histograms.len(),
                    0,
                    "failure must not emit a timing sample"
                );
            }
        }
    }
}
