//! Metrics recording hooks for engine outcomes.

use std::time::Duration;

use zoeken_engine_core::EngineError;
use zoeken_metrics::{EngineMetricsRecorder, ErrorCategory, categorize_error};

use crate::execution::UnresponsiveReason;

/// What happened to an engine during a search run.
#[derive(Debug)]
pub enum EngineOutcome<'a> {
    Completed { results: usize },
    Failed { error: &'a EngineError },
    Unresponsive { reason: UnresponsiveReason },
}

/// A single per-engine measurement passed to the MetricsRecorder.
#[derive(Debug)]
pub struct EngineSample<'a> {
    pub engine: &'a str,
    pub duration: Duration,
    pub http_duration: Option<Duration>,
    pub outcome: EngineOutcome<'a>,
}

pub trait MetricsRecorder: Send + Sync {
    fn record_engine(&self, sample: EngineSample<'_>);
}

/// A MetricsRecorder that discards every sample.
#[derive(Debug, Clone, Copy, Default)]
pub struct NoopRecorder;

impl MetricsRecorder for NoopRecorder {
    fn record_engine(&self, _sample: EngineSample<'_>) {}
}

impl<R: MetricsRecorder + ?Sized> MetricsRecorder for &R {
    fn record_engine(&self, sample: EngineSample<'_>) {
        (**self).record_engine(sample);
    }
}

const fn unresponsive_category(_reason: UnresponsiveReason) -> ErrorCategory {
    ErrorCategory::Unresponsive
}

/// Adapts [`zoeken_metrics::EngineMetricsRecorder`] to the `zoeken-search`
/// [`MetricsRecorder`] trait.
impl MetricsRecorder for EngineMetricsRecorder {
    fn record_engine(&self, sample: EngineSample<'_>) {
        let EngineSample {
            engine,
            duration,
            http_duration,
            outcome,
        } = sample;

        match outcome {
            EngineOutcome::Completed { .. } => {
                self.record_timing(engine, duration, http_duration);
            }
            EngineOutcome::Failed { error } => {
                self.record_timing(engine, duration, http_duration);
                self.record_error(engine, categorize_error(error));
            }
            EngineOutcome::Unresponsive { reason } => {
                self.record_timing(engine, duration, http_duration);
                self.record_error(engine, unresponsive_category(reason));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    use metrics::{
        Counter, CounterFn, Gauge, Histogram, HistogramFn, Key, KeyName, Metadata, Recorder,
        SharedString, Unit,
    };
    use zoeken_metrics::{
        CATEGORY_LABEL, ENGINE_ERRORS_TOTAL, ENGINE_LABEL, ENGINE_RESPONSE_TIME_TOTAL,
    };

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
    struct SpyRecorder {
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

        fn absolute(&self, _value: u64) {}
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

    impl Recorder for SpyRecorder {
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

    fn drive(engine: &str, duration: Duration, outcome: EngineOutcome<'_>) -> Captured {
        let recorder = SpyRecorder::default();
        let captured = recorder.inner.clone();
        metrics::with_local_recorder(&recorder, || {
            EngineMetricsRecorder::new().record_engine(EngineSample {
                engine,
                duration,
                http_duration: None,
                outcome,
            });
        });
        let captured = captured.lock().unwrap();
        Captured {
            histograms: captured.histograms.clone(),
            counters: captured.counters.clone(),
        }
    }

    #[test]
    fn completed_outcome_records_timing_only() {
        let captured = drive(
            "wikipedia",
            Duration::from_millis(120),
            EngineOutcome::Completed { results: 7 },
        );

        assert_eq!(
            captured.counters.len(),
            0,
            "completed must not record an error"
        );
        assert_eq!(captured.histograms.len(), 1);
        assert_eq!(captured.histograms[0].name, ENGINE_RESPONSE_TIME_TOTAL);
        assert!(has_label(
            &captured.histograms[0].labels,
            ENGINE_LABEL,
            "wikipedia"
        ));
    }

    #[test]
    fn failed_outcome_records_timing_and_categorized_error() {
        let error = EngineError::TooManyRequests("429".into());
        let captured = drive(
            "bing",
            Duration::from_millis(50),
            EngineOutcome::Failed { error: &error },
        );

        assert!(
            !captured.histograms.is_empty(),
            "failed records timing as well as an error"
        );
        assert_eq!(captured.histograms[0].name, ENGINE_RESPONSE_TIME_TOTAL);
        assert!(has_label(
            &captured.histograms[0].labels,
            ENGINE_LABEL,
            "bing"
        ));
        assert_eq!(captured.counters.len(), 1);
        assert_eq!(captured.counters[0].name, ENGINE_ERRORS_TOTAL);
        assert!(has_label(
            &captured.counters[0].labels,
            ENGINE_LABEL,
            "bing"
        ));
        assert!(has_label(
            &captured.counters[0].labels,
            CATEGORY_LABEL,
            "too_many_requests"
        ));
    }

    #[test]
    fn unresponsive_outcome_records_timing_and_unresponsive_error() {
        for reason in [
            UnresponsiveReason::EngineTimeout,
            UnresponsiveReason::GlobalDeadline,
        ] {
            let captured = drive(
                "google",
                Duration::from_millis(900),
                EngineOutcome::Unresponsive { reason },
            );

            assert_eq!(
                captured.histograms.len(),
                1,
                "unresponsive still records timing"
            );
            assert_eq!(captured.histograms[0].name, ENGINE_RESPONSE_TIME_TOTAL);
            assert!(has_label(
                &captured.histograms[0].labels,
                ENGINE_LABEL,
                "google"
            ));

            assert_eq!(captured.counters.len(), 1);
            assert_eq!(captured.counters[0].name, ENGINE_ERRORS_TOTAL);
            assert!(has_label(
                &captured.counters[0].labels,
                ENGINE_LABEL,
                "google"
            ));
            assert!(has_label(
                &captured.counters[0].labels,
                CATEGORY_LABEL,
                "unresponsive"
            ));
        }
    }
}
