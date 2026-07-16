use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use proptest::prelude::*;
use tracing::{Event, Subscriber};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use zoeken_server::middleware::level_filter;
use zoeken_settings::DeploymentConfig;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Severity {
    Error,
    Warn,
    Info,
    Debug,
    Trace,
}

impl Severity {
    fn config_str(self) -> &'static str {
        match self {
            Severity::Error => "error",
            Severity::Warn => "warn",
            Severity::Info => "info",
            Severity::Debug => "debug",
            Severity::Trace => "trace",
        }
    }

    fn rank(self) -> u8 {
        match self {
            Severity::Error => 5,
            Severity::Warn => 4,
            Severity::Info => 3,
            Severity::Debug => 2,
            Severity::Trace => 1,
        }
    }

    fn emit(self) {
        match self {
            Severity::Error => tracing::error!(log_level_prop_marker = true, "record"),
            Severity::Warn => tracing::warn!(log_level_prop_marker = true, "record"),
            Severity::Info => tracing::info!(log_level_prop_marker = true, "record"),
            Severity::Debug => tracing::debug!(log_level_prop_marker = true, "record"),
            Severity::Trace => tracing::trace!(log_level_prop_marker = true, "record"),
        }
    }
}

struct CountingLayer(Arc<AtomicUsize>);

impl<S: Subscriber> Layer<S> for CountingLayer {
    fn on_event(&self, _event: &Event<'_>, _ctx: Context<'_, S>) {
        self.0.fetch_add(1, Ordering::SeqCst);
    }
}

fn record_is_emitted(configured: Severity, record: Severity) -> bool {
    let cfg = DeploymentConfig {
        log_level: configured.config_str().to_string(),
        ..DeploymentConfig::default()
    };
    let filter = level_filter(&cfg);

    let count = Arc::new(AtomicUsize::new(0));
    let subscriber =
        tracing_subscriber::registry().with(CountingLayer(count.clone()).with_filter(filter));

    tracing::subscriber::with_default(subscriber, || record.emit());

    count.load(Ordering::SeqCst) > 0
}

fn severity_strategy() -> impl Strategy<Value = Severity> {
    prop_oneof![
        Just(Severity::Error),
        Just(Severity::Warn),
        Just(Severity::Info),
        Just(Severity::Debug),
        Just(Severity::Trace),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn log_level_filtering(
        configured in severity_strategy(),
        record in severity_strategy(),
    ) {
        let expected_emitted = record.rank() >= configured.rank();

        let actual_emitted = record_is_emitted(configured, record);

        prop_assert_eq!(
            actual_emitted,
            expected_emitted,
            "record {:?} under configured level {:?}: expected emitted={}, got {}",
            record,
            configured,
            expected_emitted,
            actual_emitted,
        );
    }
}

#[test]
fn log_level_filtering_matrix() {
    let all = [
        Severity::Error,
        Severity::Warn,
        Severity::Info,
        Severity::Debug,
        Severity::Trace,
    ];

    for configured in all {
        for record in all {
            let expected = record.rank() >= configured.rank();
            assert_eq!(
                record_is_emitted(configured, record),
                expected,
                "record {record:?} under configured {configured:?}"
            );
        }
    }
}
