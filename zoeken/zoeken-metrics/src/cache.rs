//! Pluggable key/value cache (`KvStore`): in-process (moka + optional SQLite) or
//! Valkey/Redis, selected strictly by config. Two backends, per-entry TTL, optional persistence.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use moka::Expiry;
use moka::future::Cache;

#[derive(Debug, thiserror::Error)]
pub enum KvError {
    #[error(
        "the configured cache backend is Valkey/Redis, but zoeken-metrics was built without the `valkey` feature"
    )]
    ValkeyFeatureDisabled,

    #[error("failed to initialize the persistent cache store: {0}")]
    Persistence(#[from] rusqlite::Error),

    #[error("failed to initialize the Valkey/Redis cache backend: {0}")]
    Backend(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum KvConfig {
    InProcess { path: Option<std::path::PathBuf> },
    Valkey { url: String },
}

impl KvConfig {
    pub fn in_process() -> Self {
        KvConfig::InProcess { path: None }
    }
}

/// Key/value cache interface. Async operations; per-entry TTL optional.
pub trait KvStore: Send + Sync {
    fn get(&self, key: &str) -> impl std::future::Future<Output = Option<Vec<u8>>> + Send;

    fn set_ttl(
        &self,
        key: &str,
        value: Vec<u8>,
        ttl: Option<Duration>,
    ) -> impl std::future::Future<Output = ()> + Send;

    fn del(&self, key: &str) -> impl std::future::Future<Output = ()> + Send;
}

/// Cached value with per-entry TTL.
#[derive(Debug, Clone)]
struct Stored {
    bytes: Vec<u8>,
    ttl: Option<Duration>,
}

/// Per-entry expiration policy.
struct PerEntryTtl;

impl Expiry<String, Stored> for PerEntryTtl {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &Stored,
        _created_at: Instant,
    ) -> Option<Duration> {
        value.ttl
    }

    fn expire_after_update(
        &self,
        _key: &String,
        value: &Stored,
        _updated_at: Instant,
        _current_duration: Option<Duration>,
    ) -> Option<Duration> {
        value.ttl
    }
}

/// In-process cache: moka + optional SQLite persistence.
#[derive(Clone)]
pub struct InProcKv {
    cache: Cache<String, Stored>,
    persist: Option<Arc<Mutex<rusqlite::Connection>>>,
}

impl std::fmt::Debug for InProcKv {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InProcKv")
            .field("entries", &self.cache.entry_count())
            .field("persistent", &self.persist.is_some())
            .finish()
    }
}

impl InProcKv {
    /// Memory-only store.
    pub fn new() -> Self {
        Self {
            cache: Self::build_cache(),
            persist: None,
        }
    }

    /// Store with SQLite persistence.
    pub fn persistent(path: impl AsRef<std::path::Path>) -> Result<Self, KvError> {
        let conn = rusqlite::Connection::open(path.as_ref())?;
        Self::init_schema(&conn)?;
        Ok(Self {
            cache: Self::build_cache(),
            persist: Some(Arc::new(Mutex::new(conn))),
        })
    }

    fn build_cache() -> Cache<String, Stored> {
        Cache::builder()
            .max_capacity(100_000)
            .expire_after(PerEntryTtl)
            .build()
    }

    fn init_schema(conn: &rusqlite::Connection) -> Result<(), rusqlite::Error> {
        conn.execute(
            "CREATE TABLE IF NOT EXISTS kv (
                 key        TEXT PRIMARY KEY,
                 value      BLOB NOT NULL,
                 expires_at INTEGER
             )",
            [],
        )?;
        Ok(())
    }

    fn now_ms() -> i64 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as i64)
            .unwrap_or(0)
    }

    fn persist_get(
        conn: &Arc<Mutex<rusqlite::Connection>>,
        key: &str,
    ) -> Option<(Vec<u8>, Option<Duration>)> {
        let guard = conn.lock().ok()?;
        let row: Option<(Vec<u8>, Option<i64>)> = guard
            .query_row(
                "SELECT value, expires_at FROM kv WHERE key = ?1",
                [key],
                |r| Ok((r.get::<_, Vec<u8>>(0)?, r.get::<_, Option<i64>>(1)?)),
            )
            .ok();

        let (bytes, expires_at) = row?;
        match expires_at {
            Some(deadline) => {
                let now = Self::now_ms();
                if now >= deadline {
                    let _ = guard.execute("DELETE FROM kv WHERE key = ?1", [key]);
                    None
                } else {
                    let remaining = Duration::from_millis((deadline - now) as u64);
                    Some((bytes, Some(remaining)))
                }
            }
            None => Some((bytes, None)),
        }
    }

    fn persist_set(
        conn: &Arc<Mutex<rusqlite::Connection>>,
        key: &str,
        value: &[u8],
        ttl: Option<Duration>,
    ) {
        let expires_at: Option<i64> = ttl.map(|d| Self::now_ms() + d.as_millis() as i64);
        if let Ok(guard) = conn.lock() {
            let _ = guard.execute(
                "INSERT INTO kv (key, value, expires_at) VALUES (?1, ?2, ?3)
                 ON CONFLICT(key) DO UPDATE SET value = ?2, expires_at = ?3",
                rusqlite::params![key, value, expires_at],
            );
        }
    }

    fn persist_del(conn: &Arc<Mutex<rusqlite::Connection>>, key: &str) {
        if let Ok(guard) = conn.lock() {
            let _ = guard.execute("DELETE FROM kv WHERE key = ?1", [key]);
        }
    }
}

impl Default for InProcKv {
    fn default() -> Self {
        Self::new()
    }
}

impl KvStore for InProcKv {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        if let Some(stored) = self.cache.get(key).await {
            return Some(stored.bytes);
        }
        // moka miss: consult persistence, repopulating the hot cache on a hit.
        if let Some(conn) = &self.persist
            && let Some((bytes, remaining)) = Self::persist_get(conn, key)
        {
            self.cache
                .insert(
                    key.to_owned(),
                    Stored {
                        bytes: bytes.clone(),
                        ttl: remaining,
                    },
                )
                .await;
            return Some(bytes);
        }
        None
    }

    async fn set_ttl(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) {
        if let Some(conn) = &self.persist {
            Self::persist_set(conn, key, &value, ttl);
        }
        self.cache
            .insert(key.to_owned(), Stored { bytes: value, ttl })
            .await;
    }

    async fn del(&self, key: &str) {
        if let Some(conn) = &self.persist {
            Self::persist_del(conn, key);
        }
        self.cache.invalidate(key).await;
    }
}

/// Valkey/Redis backend.
#[cfg(feature = "valkey")]
#[derive(Clone)]
pub struct ValkeyKv {
    client: fred::clients::Client,
}

#[cfg(feature = "valkey")]
impl ValkeyKv {
    pub async fn connect(url: &str) -> Result<Self, KvError> {
        use fred::prelude::*;

        let config = Config::from_url(url).map_err(|e| KvError::Backend(e.to_string()))?;
        let client = Builder::from_config(config)
            .build()
            .map_err(|e| KvError::Backend(e.to_string()))?;
        client
            .init()
            .await
            .map_err(|e| KvError::Backend(e.to_string()))?;
        Ok(Self { client })
    }
}

#[cfg(feature = "valkey")]
impl KvStore for ValkeyKv {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        use fred::prelude::*;
        let value: Option<Vec<u8>> = self.client.get(key).await.ok().flatten();
        value
    }

    async fn set_ttl(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) {
        use fred::prelude::*;
        let expiration = ttl.map(|d| Expiration::PX(d.as_millis() as i64));
        let _: Result<(), _> = self.client.set(key, value, expiration, None, false).await;
    }

    async fn del(&self, key: &str) {
        use fred::prelude::*;
        let _: Result<i64, _> = self.client.del(key).await;
    }
}

/// Constructed cache backend.
#[derive(Clone)]
pub enum Kv {
    InProc(InProcKv),
    #[cfg(feature = "valkey")]
    Valkey(ValkeyKv),
}

impl KvStore for Kv {
    async fn get(&self, key: &str) -> Option<Vec<u8>> {
        match self {
            Kv::InProc(k) => k.get(key).await,
            #[cfg(feature = "valkey")]
            Kv::Valkey(k) => k.get(key).await,
        }
    }

    async fn set_ttl(&self, key: &str, value: Vec<u8>, ttl: Option<Duration>) {
        match self {
            Kv::InProc(k) => k.set_ttl(key, value, ttl).await,
            #[cfg(feature = "valkey")]
            Kv::Valkey(k) => k.set_ttl(key, value, ttl).await,
        }
    }

    async fn del(&self, key: &str) {
        match self {
            Kv::InProc(k) => k.del(key).await,
            #[cfg(feature = "valkey")]
            Kv::Valkey(k) => k.del(key).await,
        }
    }
}

/// Construct the cache backend selected by config. Returns exact backend
/// matching config; no cross-backend fallback.
pub async fn build_kv(config: &KvConfig) -> Result<Kv, KvError> {
    match config {
        KvConfig::InProcess { path } => match path {
            Some(path) => Ok(Kv::InProc(InProcKv::persistent(path)?)),
            None => Ok(Kv::InProc(InProcKv::new())),
        },
        KvConfig::Valkey { url } => {
            #[cfg(feature = "valkey")]
            {
                Ok(Kv::Valkey(ValkeyKv::connect(url).await?))
            }
            #[cfg(not(feature = "valkey"))]
            {
                let _ = url;
                Err(KvError::ValkeyFeatureDisabled)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db_path(tag: &str) -> std::path::PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("zoeken-metrics-kv-{tag}-{nanos}.sqlite"))
    }

    #[tokio::test]
    async fn build_kv_in_process_returns_in_process_backend() {
        let kv = build_kv(&KvConfig::in_process())
            .await
            .expect("in-process backend always builds");
        assert!(matches!(kv, Kv::InProc(_)));
    }

    #[cfg(not(feature = "valkey"))]
    #[tokio::test]
    async fn build_kv_valkey_without_feature_is_error_not_fallback() {
        let result = build_kv(&KvConfig::Valkey {
            url: "redis://127.0.0.1:6379".into(),
        })
        .await;
        // Strict: it must NOT silently fall back to the in-process backend.
        assert!(matches!(result, Err(KvError::ValkeyFeatureDisabled)));
    }

    #[tokio::test]
    async fn value_read_before_ttl_elapses_is_unchanged() {
        let kv = InProcKv::new();
        kv.set_ttl("k", b"hello".to_vec(), Some(Duration::from_secs(30)))
            .await;
        assert_eq!(kv.get("k").await, Some(b"hello".to_vec()));
    }

    #[tokio::test]
    async fn value_without_ttl_persists_in_memory() {
        let kv = InProcKv::new();
        kv.set_ttl("k", b"v".to_vec(), None).await;
        assert_eq!(kv.get("k").await, Some(b"v".to_vec()));
    }

    #[tokio::test]
    async fn value_stops_being_returned_after_ttl_elapses() {
        let kv = InProcKv::new();
        kv.set_ttl("k", b"bye".to_vec(), Some(Duration::from_millis(50)))
            .await;
        assert_eq!(kv.get("k").await, Some(b"bye".to_vec()));
        tokio::time::sleep(Duration::from_millis(120)).await;
        kv.cache.run_pending_tasks().await;
        assert_eq!(kv.get("k").await, None);
    }

    #[tokio::test]
    async fn del_removes_the_value() {
        let kv = InProcKv::new();
        kv.set_ttl("k", b"v".to_vec(), None).await;
        kv.del("k").await;
        assert_eq!(kv.get("k").await, None);
    }

    #[tokio::test]
    async fn persistent_value_survives_a_new_store_instance() {
        let path = temp_db_path("persist");
        {
            let kv = InProcKv::persistent(&path).expect("open persistent store");
            kv.set_ttl("pk", b"durable".to_vec(), None).await;
        }
        let kv2 = InProcKv::persistent(&path).expect("reopen persistent store");
        assert_eq!(kv2.get("pk").await, Some(b"durable".to_vec()));
        let _ = std::fs::remove_file(&path);
    }

    #[tokio::test]
    async fn persistent_expired_value_is_not_returned() {
        let path = temp_db_path("persist-ttl");
        {
            let kv = InProcKv::persistent(&path).expect("open persistent store");
            kv.set_ttl("pk", b"stale".to_vec(), Some(Duration::from_millis(40)))
                .await;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
        let kv2 = InProcKv::persistent(&path).expect("reopen persistent store");
        assert_eq!(kv2.get("pk").await, None);
        let _ = std::fs::remove_file(&path);
    }
}
