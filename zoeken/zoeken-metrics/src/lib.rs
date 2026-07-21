//! Per-engine timing/error metrics.
pub mod recorder;

pub use recorder::{
    CATEGORY_LABEL, ENGINE_ERRORS_TOTAL, ENGINE_LABEL, ENGINE_RESPONSE_TIME_HTTP,
    ENGINE_RESPONSE_TIME_TOTAL, EngineMetricsRecorder, ErrorCategory, categorize_error,
};
