//! Per-engine timing/error metrics and pluggable KvStore cache.

pub mod cache;
pub mod recorder;

pub use cache::{InProcKv, Kv, KvConfig, KvError, KvStore, build_kv};
pub use recorder::{
    CATEGORY_LABEL, ENGINE_ERRORS_TOTAL, ENGINE_LABEL, ENGINE_RESPONSE_TIME_HTTP,
    ENGINE_RESPONSE_TIME_TOTAL, EngineMetricsRecorder, ErrorCategory, categorize_error,
};

#[cfg(feature = "valkey")]
pub use cache::ValkeyKv;
