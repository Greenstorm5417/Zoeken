//! Per-engine timing histograms and categorized error recorder via the
//! `metrics` facade. Pure error categorization function for independent testing.

use std::time::Duration;

use metrics::{counter, histogram};
use zoeken_engine_core::EngineError;

/// Histogram name for engine's total wall-clock response time in seconds (labeled by ENGINE_LABEL).
pub const ENGINE_RESPONSE_TIME_TOTAL: &str = "zoeken_engine_response_time_total_seconds";

/// Histogram name for engine's HTTP response time in seconds (labeled by ENGINE_LABEL).
pub const ENGINE_RESPONSE_TIME_HTTP: &str = "zoeken_engine_response_time_http_seconds";

/// Counter name for categorized per-engine errors (labeled by ENGINE_LABEL and CATEGORY_LABEL).
pub const ENGINE_ERRORS_TOTAL: &str = "zoeken_engine_errors_total";

/// Label key carrying the engine name on every per-engine metric.
pub const ENGINE_LABEL: &str = "engine";

/// Label key carrying the [`ErrorCategory`] on the error counter.
pub const CATEGORY_LABEL: &str = "category";

/// Category for engine outcome on the error counter. First six variants
/// mirror EngineError taxonomy; Unresponsive has no EngineError counterpart.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ErrorCategory {
    /// Request timed out.
    Timeout,
    /// Access blocked.
    AccessDenied,
    /// Too many requests.
    TooManyRequests,
    /// CAPTCHA challenge.
    Captcha,
    /// Failed to parse response.
    Parse,
    /// Other unexpected failure.
    Unexpected,
    /// Engine never responded.
    Unresponsive,
}

impl ErrorCategory {
    /// Stable label value used for this category on error counter.
    pub const fn as_str(self) -> &'static str {
        match self {
            ErrorCategory::Timeout => "timeout",
            ErrorCategory::AccessDenied => "access_denied",
            ErrorCategory::TooManyRequests => "too_many_requests",
            ErrorCategory::Captcha => "captcha",
            ErrorCategory::Parse => "parse",
            ErrorCategory::Unexpected => "unexpected",
            ErrorCategory::Unresponsive => "unresponsive",
        }
    }
}

/// Map EngineError to ErrorCategory. Pure, total function for testability.
pub const fn categorize_error(error: &EngineError) -> ErrorCategory {
    match error {
        EngineError::Timeout => ErrorCategory::Timeout,
        EngineError::AccessDenied(_) | EngineError::CloudflareAccessDenied(_) => {
            ErrorCategory::AccessDenied
        }
        EngineError::TooManyRequests(_) => ErrorCategory::TooManyRequests,
        EngineError::Captcha(_)
        | EngineError::CloudflareCaptcha(_)
        | EngineError::RecaptchaCaptcha(_) => ErrorCategory::Captcha,
        EngineError::Parse(_) => ErrorCategory::Parse,
        EngineError::Unexpected(_) => ErrorCategory::Unexpected,
    }
}

/// Records per-engine timing and categorized errors via `metrics` facade.
/// Zero-sized handle; storage is process-global.
#[derive(Debug, Clone, Copy, Default)]
pub struct EngineMetricsRecorder;

impl EngineMetricsRecorder {
    /// Create a recorder.
    pub const fn new() -> Self {
        EngineMetricsRecorder
    }

    /// Record engine timing into per-engine histograms (total and optional HTTP).
    pub fn record_timing(&self, engine: &str, total: Duration, http: Option<Duration>) {
        histogram!(ENGINE_RESPONSE_TIME_TOTAL, ENGINE_LABEL => engine.to_owned())
            .record(total.as_secs_f64());
        if let Some(http) = http {
            histogram!(ENGINE_RESPONSE_TIME_HTTP, ENGINE_LABEL => engine.to_owned())
                .record(http.as_secs_f64());
        }
    }

    /// Increment per-engine, per-category error counter.
    pub fn record_error(&self, engine: &str, category: ErrorCategory) {
        counter!(
            ENGINE_ERRORS_TOTAL,
            ENGINE_LABEL => engine.to_owned(),
            CATEGORY_LABEL => category.as_str(),
        )
        .increment(1);
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
    struct Observation {
        name: String,
        labels: Vec<(String, String)>,
        value: f64,
    }

    /// Captured emissions for test assertions.
    #[derive(Debug, Default)]
    struct Captured {
        histograms: Vec<Observation>,
        counters: Vec<Observation>,
    }

    /// Capturing metrics recorder for test assertions.
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
        fn increment(&self, value: u64) {
            self.inner.lock().unwrap().counters.push(Observation {
                name: self.key.name().to_string(),
                labels: labels_of(&self.key),
                value: value as f64,
            });
        }

        fn absolute(&self, value: u64) {
            self.inner.lock().unwrap().counters.push(Observation {
                name: self.key.name().to_string(),
                labels: labels_of(&self.key),
                value: value as f64,
            });
        }
    }

    struct HistogramHandle {
        key: Key,
        inner: Arc<Mutex<Captured>>,
    }

    impl HistogramFn for HistogramHandle {
        fn record(&self, value: f64) {
            self.inner.lock().unwrap().histograms.push(Observation {
                name: self.key.name().to_string(),
                labels: labels_of(&self.key),
                value,
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

    fn has_label(obs: &Observation, key: &str, value: &str) -> bool {
        obs.labels.iter().any(|(k, v)| k == key && v == value)
    }

    #[test]
    fn record_timing_records_total_into_engine_histogram() {
        let recorder = CapturingRecorder::default();
        let captured = recorder.inner.clone();

        metrics::with_local_recorder(&recorder, || {
            EngineMetricsRecorder::new().record_timing(
                "wikipedia",
                Duration::from_millis(250),
                None,
            );
        });

        let captured = captured.lock().unwrap();
        assert_eq!(
            captured.histograms.len(),
            1,
            "only the total histogram is recorded when http is None"
        );
        let obs = &captured.histograms[0];
        assert_eq!(obs.name, ENGINE_RESPONSE_TIME_TOTAL);
        assert!(has_label(obs, ENGINE_LABEL, "wikipedia"));
        assert!((obs.value - 0.25).abs() < 1e-9);
    }

    #[test]
    fn record_timing_records_total_and_http_when_http_present() {
        let recorder = CapturingRecorder::default();
        let captured = recorder.inner.clone();

        metrics::with_local_recorder(&recorder, || {
            EngineMetricsRecorder::new().record_timing(
                "google",
                Duration::from_millis(400),
                Some(Duration::from_millis(300)),
            );
        });

        let captured = captured.lock().unwrap();
        assert_eq!(captured.histograms.len(), 2);
        let total = captured
            .histograms
            .iter()
            .find(|o| o.name == ENGINE_RESPONSE_TIME_TOTAL)
            .expect("total histogram recorded");
        let http = captured
            .histograms
            .iter()
            .find(|o| o.name == ENGINE_RESPONSE_TIME_HTTP)
            .expect("http histogram recorded");
        assert!(has_label(total, ENGINE_LABEL, "google"));
        assert!(has_label(http, ENGINE_LABEL, "google"));
        assert!((total.value - 0.4).abs() < 1e-9);
        assert!((http.value - 0.3).abs() < 1e-9);
    }

    #[test]
    fn record_error_increments_labeled_counter() {
        let recorder = CapturingRecorder::default();
        let captured = recorder.inner.clone();

        metrics::with_local_recorder(&recorder, || {
            EngineMetricsRecorder::new().record_error("bing", ErrorCategory::Captcha);
        });

        let captured = captured.lock().unwrap();
        assert_eq!(captured.counters.len(), 1);
        let obs = &captured.counters[0];
        assert_eq!(obs.name, ENGINE_ERRORS_TOTAL);
        assert!(has_label(obs, ENGINE_LABEL, "bing"));
        assert!(has_label(obs, CATEGORY_LABEL, "captcha"));
        assert_eq!(obs.value, 1.0);
    }

    #[test]
    fn categorize_error_maps_each_variant() {
        let cases = [
            (EngineError::Timeout, ErrorCategory::Timeout, "timeout"),
            (
                EngineError::AccessDenied("blocked".into()),
                ErrorCategory::AccessDenied,
                "access_denied",
            ),
            (
                EngineError::TooManyRequests("429".into()),
                ErrorCategory::TooManyRequests,
                "too_many_requests",
            ),
            (
                EngineError::Captcha("solve me".into()),
                ErrorCategory::Captcha,
                "captcha",
            ),
            (
                EngineError::Parse("bad html".into()),
                ErrorCategory::Parse,
                "parse",
            ),
            (
                EngineError::Unexpected("boom".into()),
                ErrorCategory::Unexpected,
                "unexpected",
            ),
        ];

        for (error, expected, label) in cases {
            let category = categorize_error(&error);
            assert_eq!(category, expected, "category for {error:?}");
            assert_eq!(category.as_str(), label);
        }
    }

    #[test]
    fn unresponsive_category_label_is_stable() {
        assert_eq!(ErrorCategory::Unresponsive.as_str(), "unresponsive");
    }
}
