//! Per-engine timing/error metrics via the `metrics` facade.

use std::time::Duration;

use metrics::{counter, histogram};
use zoeken_engine_core::{EngineError, ErrorCategory};

use crate::execution::UnresponsiveReason;

/// Histogram name for engine total wall-clock response time (seconds).
pub const ENGINE_RESPONSE_TIME_TOTAL: &str = "zoeken_engine_response_time_total_seconds";

/// Histogram name for engine HTTP response time (seconds).
pub const ENGINE_RESPONSE_TIME_HTTP: &str = "zoeken_engine_response_time_http_seconds";

/// Counter name for categorized per-engine errors.
pub const ENGINE_ERRORS_TOTAL: &str = "zoeken_engine_errors_total";

/// Label key for the engine name.
pub const ENGINE_LABEL: &str = "engine";

/// Label key for [`ErrorCategory`] on the error counter.
pub const CATEGORY_LABEL: &str = "category";

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

/// Records per-engine timing and categorized errors via `metrics` facade.
#[derive(Debug, Clone, Copy, Default)]
pub struct EngineMetricsRecorder;

impl EngineMetricsRecorder {
    pub const fn new() -> Self {
        EngineMetricsRecorder
    }

    pub fn record_timing(&self, engine: &str, total: Duration, http: Option<Duration>) {
        histogram!(ENGINE_RESPONSE_TIME_TOTAL, ENGINE_LABEL => engine.to_owned())
            .record(total.as_secs_f64());
        if let Some(http) = http {
            histogram!(ENGINE_RESPONSE_TIME_HTTP, ENGINE_LABEL => engine.to_owned())
                .record(http.as_secs_f64());
        }
    }

    pub fn record_error(&self, engine: &str, category: ErrorCategory) {
        counter!(
            ENGINE_ERRORS_TOTAL,
            ENGINE_LABEL => engine.to_owned(),
            CATEGORY_LABEL => category.as_str(),
        )
        .increment(1);
    }
}

impl MetricsRecorder for EngineMetricsRecorder {
    fn record_engine(&self, sample: EngineSample<'_>) {
        let EngineSample {
            engine,
            duration,
            http_duration,
            outcome,
        } = sample;

        self.record_timing(engine, duration, http_duration);
        match outcome {
            EngineOutcome::Completed { .. } => {}
            EngineOutcome::Failed { error } => {
                self.record_error(engine, ErrorCategory::from(error));
            }
            EngineOutcome::Unresponsive { reason: _ } => {
                self.record_error(engine, ErrorCategory::Unresponsive);
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

        assert_eq!(captured.counters.len(), 0);
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

        assert!(!captured.histograms.is_empty());
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
            "rate_limited"
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

            assert_eq!(captured.histograms.len(), 1);
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
                CATEGORY_LABEL,
                "unresponsive"
            ));
        }
    }

    #[test]
    fn error_category_from_maps_variants() {
        let cases = [
            (EngineError::Timeout, ErrorCategory::Timeout),
            (
                EngineError::AccessDenied("blocked".into()),
                ErrorCategory::AccessDenied,
            ),
            (
                EngineError::TooManyRequests("429".into()),
                ErrorCategory::RateLimited,
            ),
            (
                EngineError::Captcha("solve me".into()),
                ErrorCategory::Captcha,
            ),
            (EngineError::Parse("bad html".into()), ErrorCategory::Parse),
            (
                EngineError::Unexpected("boom".into()),
                ErrorCategory::Unexpected,
            ),
        ];
        for (error, expected) in cases {
            assert_eq!(ErrorCategory::from(&error), expected);
        }
        assert_eq!(ErrorCategory::Unresponsive.as_str(), "unresponsive");
    }
}
