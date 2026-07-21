//! Privacy-preserving operational storage for Zoeken.
//!
//! The public API contains domain operations only. Queries, result bodies,
//! client identifiers, cookies, and other per-request data cannot be stored
//! through this interface.

mod postgres;
mod sqlite;

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;

pub use postgres::PostgresStorage;
pub use sqlite::SqliteStorage;

#[derive(Debug, thiserror::Error)]
pub enum StorageError {
    #[error("storage operation failed")]
    Database(#[source] sqlx::Error),
    #[error("storage migration failed")]
    Migration(#[source] sqlx::migrate::MigrateError),
    #[error("database schema version {found} is newer than supported version {supported}")]
    UnsupportedSchema { found: i64, supported: i64 },
    #[error("invalid PostgreSQL connection configuration")]
    InvalidConnectionConfig,
}

impl From<sqlx::Error> for StorageError {
    fn from(error: sqlx::Error) -> Self {
        Self::Database(error)
    }
}

impl From<sqlx::migrate::MigrateError> for StorageError {
    fn from(error: sqlx::migrate::MigrateError) -> Self {
        Self::Migration(error)
    }
}

#[derive(Debug, Clone)]
pub struct StorageConfig {
    pub backend: BackendConfig,
}

#[derive(Clone)]
pub enum BackendConfig {
    Sqlite {
        path: PathBuf,
        busy_timeout: Duration,
        max_connections: usize,
    },
    Postgres {
        url: String,
        max_connections: usize,
        acquire_timeout: Duration,
    },
}

impl std::fmt::Debug for BackendConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Sqlite {
                path,
                busy_timeout,
                max_connections,
            } => formatter
                .debug_struct("Sqlite")
                .field("path", path)
                .field("busy_timeout", busy_timeout)
                .field("max_connections", max_connections)
                .finish(),
            Self::Postgres {
                max_connections,
                acquire_timeout,
                ..
            } => formatter
                .debug_struct("Postgres")
                .field("url", &"<redacted>")
                .field("max_connections", max_connections)
                .field("acquire_timeout", acquire_timeout)
                .finish(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FaviconData {
    pub data: Vec<u8>,
    pub mime: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaviconLookup {
    Hit(FaviconData),
    KnownMissing,
    Absent,
}

#[derive(Debug, Clone)]
pub struct FaviconPolicy {
    pub positive_ttl: Duration,
    pub negative_ttl: Duration,
    pub max_blob_bytes: usize,
    pub max_total_bytes: usize,
}

#[derive(Debug, Clone)]
pub struct OriginPolicy {
    pub requests_per_second: f64,
    pub burst: u32,
    pub max_concurrent: u32,
    pub lease_duration: Duration,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OriginLease {
    pub id: String,
    pub origin: String,
    pub expires_at_ms: i64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PermitDecision {
    Granted,
    RateLimited,
    ConcurrencyLimited,
}

#[derive(Debug, Clone)]
pub struct PermitResult {
    pub decision: PermitDecision,
    pub lease: Option<OriginLease>,
    pub retry_after: Duration,
}

#[derive(Debug, Clone)]
pub struct EngineHealthUpdate {
    pub engine: String,
    pub bucket: i64,
    pub latency_ms: u64,
    pub success: bool,
    pub timed_out: bool,
    pub error_category: Option<String>,
    pub circuit_status: String,
    pub cooldown_until_ms: Option<i64>,
}

/// Latest coarse-grained health state for an engine.
///
/// This deliberately contains no request identifiers or precise observation
/// timestamps. `bucket` is an hourly Unix bucket.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineHealthSnapshot {
    pub bucket: i64,
    pub successes: u64,
    pub timeouts: u64,
    pub errors: u64,
    pub circuit_status: String,
    pub cooldown_until_ms: Option<i64>,
    pub last_error_category: Option<String>,
}

#[async_trait]
pub trait Storage: Send + Sync {
    async fn healthcheck(&self) -> Result<(), StorageError>;
    async fn acquire_origin(
        &self,
        origin: &str,
        policy: &OriginPolicy,
    ) -> Result<PermitResult, StorageError>;
    async fn release_origin(&self, lease: &OriginLease) -> Result<(), StorageError>;
    async fn renew_origin(
        &self,
        lease: &OriginLease,
        lease_duration: Duration,
    ) -> Result<bool, StorageError>;
    async fn defer_origin(&self, origin: &str, delay: Duration) -> Result<(), StorageError>;
    async fn favicon_get(
        &self,
        resolver: &str,
        authority: &str,
    ) -> Result<FaviconLookup, StorageError>;
    async fn favicon_put(
        &self,
        resolver: &str,
        authority: &str,
        value: Option<&FaviconData>,
        policy: &FaviconPolicy,
    ) -> Result<bool, StorageError>;
    async fn record_engine_health(&self, update: &EngineHealthUpdate) -> Result<(), StorageError>;
    async fn latest_engine_health(
        &self,
        engine: &str,
    ) -> Result<Option<EngineHealthSnapshot>, StorageError>;
    async fn maintenance(&self, favicon_max_total_bytes: usize) -> Result<(), StorageError>;
}

pub async fn connect(config: &StorageConfig) -> Result<Arc<dyn Storage>, StorageError> {
    match &config.backend {
        BackendConfig::Sqlite {
            path,
            busy_timeout,
            max_connections,
        } => Ok(Arc::new(
            SqliteStorage::connect(path, *busy_timeout, *max_connections).await?,
        )),
        BackendConfig::Postgres {
            url,
            max_connections,
            acquire_timeout,
        } => Ok(Arc::new(
            PostgresStorage::connect(url, *max_connections, *acquire_timeout).await?,
        )),
    }
}

pub(crate) fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};

    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis() as i64)
}

pub(crate) fn new_lease_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static SEQUENCE: AtomicU64 = AtomicU64::new(1);
    format!(
        "{:x}-{:x}",
        now_ms(),
        SEQUENCE.fetch_add(1, Ordering::Relaxed)
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn postgres_url_is_redacted_from_debug_output() {
        let config = BackendConfig::Postgres {
            url: "postgres://secret-user:secret-pass@db.example/zoeken".into(),
            max_connections: 4,
            acquire_timeout: Duration::from_secs(1),
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("<redacted>"));
        assert!(!debug.contains("secret-user"));
        assert!(!debug.contains("secret-pass"));
    }
}
