use std::path::Path;
use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::sqlite::{SqliteConnectOptions, SqliteJournalMode, SqlitePoolOptions};
use sqlx::SqlitePool;

use crate::shared::{
    BlobRow, BudgetRow, HealthRow, MappingRow, PrunableBlob, blocked_retry_after,
    concurrency_limited, favicon_digest, granted, health_retention_bucket, health_snapshot,
    new_lease, rate_limited, refill_tokens, reject_if_newer, sql, supported_version,
    token_retry_after,
};
use crate::{
    EngineHealthSnapshot, EngineHealthUpdate, FaviconData, FaviconLookup, FaviconPolicy, OriginLease,
    OriginPolicy, PermitResult, Storage, StorageError, now_ms,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/sqlite");
static MIGRATION_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

#[derive(Clone)]
pub struct SqliteStorage {
    pool: SqlitePool,
}

impl SqliteStorage {
    pub async fn connect(
        path: impl AsRef<Path>,
        busy_timeout: Duration,
        max_connections: usize,
    ) -> Result<Self, StorageError> {
        let options = SqliteConnectOptions::new()
            .filename(path)
            .create_if_missing(true)
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Wal)
            .busy_timeout(busy_timeout);
        Self::connect_with(options, max_connections).await
    }

    pub async fn in_memory() -> Result<Self, StorageError> {
        let options = SqliteConnectOptions::from_str("sqlite::memory:")?
            .foreign_keys(true)
            .journal_mode(SqliteJournalMode::Memory);
        Self::connect_with(options, 1).await
    }

    async fn connect_with(
        options: SqliteConnectOptions,
        max_connections: usize,
    ) -> Result<Self, StorageError> {
        // Connection setup itself applies WAL/foreign-key pragmas, so serialize
        // it with migrations to avoid a second pool racing the first DDL pass.
        let _migration_guard = MIGRATION_LOCK.lock().await;
        let pool = SqlitePoolOptions::new()
            .max_connections(max_connections.max(1) as u32)
            .connect_with(options)
            .await?;
        reject_newer_schema(&pool).await?;
        MIGRATOR.run(&pool).await?;
        import_legacy_favicons(&pool).await?;
        Ok(Self { pool })
    }
}

async fn reject_newer_schema(pool: &SqlitePool) -> Result<(), StorageError> {
    let exists: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM sqlite_master WHERE type = 'table' AND name = '_sqlx_migrations'",
    )
    .fetch_one(pool)
    .await?;
    if exists == 0 {
        return Ok(());
    }
    let found: i64 = sqlx::query_scalar(sql::MAX_MIGRATION_VERSION)
        .fetch_one(pool)
        .await?;
    reject_if_newer(found, supported_version(&MIGRATOR))
}

async fn import_legacy_favicons(pool: &SqlitePool) -> Result<(), StorageError> {
    let tables: i64 = sqlx::query_scalar(
        r#"
        SELECT COUNT(*)
        FROM sqlite_master
        WHERE type = 'table' AND name IN ('blobs', 'blob_map')
        "#,
    )
    .fetch_one(pool)
    .await?;
    if tables != 2 {
        return Ok(());
    }

    let mut transaction = pool.begin().await?;
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO favicon_blobs (
            digest, size_bytes, mime, data, created_at_ms
        )
        SELECT
            sha256, bytes_c, mime, data,
            CAST(strftime('%s', 'now') AS INTEGER) * 1000
        FROM blobs
        "#,
    )
    .execute(&mut *transaction)
    .await?;
    sqlx::query(
        r#"
        INSERT OR IGNORE INTO favicon_mappings (
            resolver, authority, digest, is_negative, expires_at_ms
        )
        SELECT
            resolver,
            authority,
            CASE WHEN sha256 = 'FALLBACK_ICON' THEN NULL ELSE sha256 END,
            CASE WHEN sha256 = 'FALLBACK_ICON' THEN 1 ELSE 0 END,
            (m_time + 2592000) * 1000
        FROM blob_map
        "#,
    )
    .execute(&mut *transaction)
    .await?;
    transaction.commit().await?;
    Ok(())
}

#[async_trait]
impl Storage for SqliteStorage {
    async fn healthcheck(&self) -> Result<(), StorageError> {
        sqlx::query_scalar::<_, i64>(sql::HEALTHCHECK)
            .fetch_one(&self.pool)
            .await?;
        Ok(())
    }

    async fn acquire_origin(
        &self,
        origin: &str,
        policy: &OriginPolicy,
    ) -> Result<PermitResult, StorageError> {
        let now = now_ms();
        let mut transaction = self.pool.begin().await?;
        sqlx::query(sql::DELETE_EXPIRED_LEASES)
            .bind(now)
            .execute(&mut *transaction)
            .await?;

        let active: i64 = sqlx::query_scalar(sql::COUNT_ACTIVE_LEASES)
            .bind(origin)
            .fetch_one(&mut *transaction)
            .await?;
        if active >= i64::from(policy.max_concurrent) {
            transaction.commit().await?;
            return Ok(concurrency_limited());
        }

        let budget = sqlx::query_as::<_, BudgetRow>(sql::SELECT_BUDGET)
            .bind(origin)
            .fetch_optional(&mut *transaction)
            .await?;
        if let Some(retry_after) = blocked_retry_after(budget.as_ref(), now) {
            transaction.commit().await?;
            return Ok(rate_limited(retry_after));
        }
        let (tokens, stored_tokens) = refill_tokens(budget, policy, now);

        sqlx::query(sql::UPSERT_BUDGET)
            .bind(origin)
            .bind(stored_tokens)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        if tokens < 1.0 {
            transaction.commit().await?;
            return Ok(rate_limited(token_retry_after(
                tokens,
                policy.requests_per_second,
            )));
        }

        let lease = new_lease(origin, now, policy.lease_duration);
        sqlx::query(sql::INSERT_LEASE)
            .bind(&lease.id)
            .bind(&lease.origin)
            .bind(lease.expires_at_ms)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(granted(lease))
    }

    async fn release_origin(&self, lease: &OriginLease) -> Result<(), StorageError> {
        sqlx::query(sql::DELETE_LEASE)
            .bind(&lease.id)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn renew_origin(
        &self,
        lease: &OriginLease,
        lease_duration: Duration,
    ) -> Result<bool, StorageError> {
        let updated = sqlx::query(sql::RENEW_LEASE)
            .bind(now_ms().saturating_add(lease_duration.as_millis() as i64))
            .bind(&lease.id)
            .bind(&lease.origin)
            .execute(&self.pool)
            .await?;
        Ok(updated.rows_affected() == 1)
    }

    async fn defer_origin(&self, origin: &str, delay: Duration) -> Result<(), StorageError> {
        let now = now_ms();
        let until = now.saturating_add(delay.as_millis() as i64);
        // SQLite: MAX(...); Postgres uses GREATEST(...).
        sqlx::query(
            r#"
            INSERT INTO origin_budgets
                (origin, tokens, last_refill_ms, blocked_until_ms)
            VALUES (?, 0, ?, ?)
            ON CONFLICT (origin) DO UPDATE SET
                tokens = 0,
                last_refill_ms = excluded.last_refill_ms,
                blocked_until_ms = MAX(
                    COALESCE(origin_budgets.blocked_until_ms, 0),
                    excluded.blocked_until_ms
                )
            "#,
        )
        .bind(origin)
        .bind(now)
        .bind(until)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    async fn favicon_get(
        &self,
        resolver: &str,
        authority: &str,
    ) -> Result<FaviconLookup, StorageError> {
        let mapping = sqlx::query_as::<_, MappingRow>(sql::SELECT_MAPPING)
            .bind(resolver)
            .bind(authority)
            .fetch_optional(&self.pool)
            .await?;
        let Some(mapping) = mapping.filter(|row| row.expires_at_ms > now_ms()) else {
            return Ok(FaviconLookup::Absent);
        };
        if mapping.is_negative {
            return Ok(FaviconLookup::KnownMissing);
        }
        let Some(digest) = mapping.digest else {
            return Ok(FaviconLookup::Absent);
        };
        let blob = sqlx::query_as::<_, BlobRow>(sql::SELECT_BLOB)
            .bind(digest)
            .fetch_optional(&self.pool)
            .await?;
        Ok(blob.map_or(FaviconLookup::Absent, |row| {
            FaviconLookup::Hit(FaviconData {
                data: row.data,
                mime: row.mime,
            })
        }))
    }

    async fn favicon_put(
        &self,
        resolver: &str,
        authority: &str,
        value: Option<&FaviconData>,
        policy: &FaviconPolicy,
    ) -> Result<bool, StorageError> {
        if value.is_some_and(|favicon| favicon.data.len() > policy.max_blob_bytes) {
            return Ok(false);
        }

        let now = now_ms();
        let mut transaction = self.pool.begin().await?;
        let (digest, is_negative, ttl) = if let Some(favicon) = value {
            let digest = favicon_digest(&favicon.data);
            sqlx::query(
                r#"
                INSERT OR IGNORE INTO favicon_blobs
                    (digest, size_bytes, mime, data, created_at_ms)
                VALUES (?, ?, ?, ?, ?)
                "#,
            )
            .bind(&digest)
            .bind(favicon.data.len() as i64)
            .bind(&favicon.mime)
            .bind(&favicon.data)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
            (Some(digest), false, policy.positive_ttl)
        } else {
            (None, true, policy.negative_ttl)
        };
        sqlx::query(sql::UPSERT_MAPPING)
            .bind(resolver)
            .bind(authority)
            .bind(digest)
            .bind(is_negative)
            .bind(now + ttl.as_millis() as i64)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(true)
    }

    async fn record_engine_health(&self, update: &EngineHealthUpdate) -> Result<(), StorageError> {
        sqlx::query(sql::UPSERT_ENGINE_HEALTH)
            .bind(&update.engine)
            .bind(update.bucket)
            .bind(update.latency_ms as i64)
            .bind(update.success as i64)
            .bind(update.timed_out as i64)
            .bind((!update.success) as i64)
            .bind(&update.circuit_status)
            .bind(update.cooldown_until_ms)
            .bind(&update.error_category)
            .execute(&self.pool)
            .await?;
        Ok(())
    }

    async fn latest_engine_health(
        &self,
        engine: &str,
    ) -> Result<Option<EngineHealthSnapshot>, StorageError> {
        let row = sqlx::query_as::<_, HealthRow>(sql::LATEST_ENGINE_HEALTH)
            .bind(engine)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(health_snapshot))
    }

    async fn maintenance(&self, max_total_bytes: usize) -> Result<(), StorageError> {
        let now = now_ms();
        let mut transaction = self.pool.begin().await?;
        sqlx::query(sql::DELETE_EXPIRED_LEASES)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(sql::DELETE_EXPIRED_MAPPINGS)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(sql::CLEAR_EXPIRED_COOLDOWNS)
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        // Retain seven days of hourly aggregates; older data has no bearing on
        // active cooldowns and is unnecessary operational history.
        sqlx::query(sql::DELETE_OLD_HEALTH)
            .bind(health_retention_bucket(now))
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            r#"
            DELETE FROM favicon_blobs
            WHERE digest NOT IN (
                SELECT digest FROM favicon_mappings WHERE digest IS NOT NULL
            )
            "#,
        )
        .execute(&mut *transaction)
        .await?;

        let mut total: i64 = sqlx::query_scalar(sql::SUM_BLOB_BYTES)
            .fetch_one(&mut *transaction)
            .await?;
        if total > max_total_bytes as i64 {
            let blobs = sqlx::query_as::<_, PrunableBlob>(sql::LIST_BLOBS_BY_AGE)
                .fetch_all(&mut *transaction)
                .await?;
            for blob in blobs {
                if total <= max_total_bytes as i64 {
                    break;
                }
                sqlx::query(sql::DELETE_MAPPINGS_BY_DIGEST)
                    .bind(&blob.digest)
                    .execute(&mut *transaction)
                    .await?;
                sqlx::query(sql::DELETE_BLOB)
                    .bind(blob.digest)
                    .execute(&mut *transaction)
                    .await?;
                total -= blob.size_bytes;
            }
        }
        transaction.commit().await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::PermitDecision;

    #[tokio::test]
    async fn contract_covers_origin_and_favicon_operations() {
        let storage = SqliteStorage::in_memory().await.unwrap();
        storage.healthcheck().await.unwrap();
        let origin_policy = OriginPolicy {
            requests_per_second: 1.0,
            burst: 2,
            max_concurrent: 1,
            lease_duration: Duration::from_secs(15),
        };
        let first = storage
            .acquire_origin("https://example.com", &origin_policy)
            .await
            .unwrap();
        assert_eq!(first.decision, PermitDecision::Granted);
        assert!(
            storage
                .renew_origin(first.lease.as_ref().unwrap(), Duration::from_secs(15))
                .await
                .unwrap()
        );
        assert_eq!(
            storage
                .acquire_origin("https://example.com", &origin_policy)
                .await
                .unwrap()
                .decision,
            PermitDecision::ConcurrencyLimited
        );
        storage
            .release_origin(first.lease.as_ref().unwrap())
            .await
            .unwrap();

        let favicon_policy = FaviconPolicy {
            positive_ttl: Duration::from_secs(60),
            negative_ttl: Duration::from_secs(10),
            max_blob_bytes: 100,
            max_total_bytes: 1_000,
        };
        let favicon = FaviconData {
            data: vec![1, 2, 3],
            mime: "image/png".into(),
        };
        storage
            .favicon_put("duckduckgo", "example.com", Some(&favicon), &favicon_policy)
            .await
            .unwrap();
        assert_eq!(
            storage
                .favicon_get("duckduckgo", "example.com")
                .await
                .unwrap(),
            FaviconLookup::Hit(favicon)
        );
    }

    #[tokio::test]
    async fn schema_contains_only_operational_tables() {
        let storage = SqliteStorage::in_memory().await.unwrap();
        let names: Vec<String> =
            sqlx::query_scalar("SELECT name FROM sqlite_master WHERE type = 'table'")
                .fetch_all(&storage.pool)
                .await
                .unwrap();
        assert!(!names.iter().any(|name| {
            ["query", "request", "client", "session", "result"]
                .iter()
                .any(|forbidden| name.contains(forbidden))
        }));
        for table in names {
            if table.starts_with("sqlite_") || table.starts_with("_sqlx_") {
                continue;
            }
            let quoted = table.replace('"', "\"\"");
            let columns: Vec<String> =
                sqlx::query_scalar(&format!("SELECT name FROM pragma_table_info(\"{quoted}\")"))
                    .fetch_all(&storage.pool)
                    .await
                    .unwrap();
            assert!(!columns.iter().any(|column| {
                [
                    "query",
                    "request_id",
                    "client_ip",
                    "user_agent",
                    "cookie",
                    "response_body",
                ]
                .contains(&column.as_str())
            }));
        }
    }

    #[tokio::test]
    async fn retry_after_defers_the_whole_origin() {
        let storage = SqliteStorage::in_memory().await.unwrap();
        let policy = OriginPolicy {
            requests_per_second: 100.0,
            burst: 10,
            max_concurrent: 10,
            lease_duration: Duration::from_secs(1),
        };
        storage
            .defer_origin("https://example.com", Duration::from_secs(30))
            .await
            .unwrap();
        let result = storage
            .acquire_origin("https://example.com", &policy)
            .await
            .unwrap();
        assert_eq!(result.decision, PermitDecision::RateLimited);
        assert!(result.retry_after >= Duration::from_secs(29));

        let independent = storage
            .acquire_origin("https://example.org", &policy)
            .await
            .unwrap();
        assert_eq!(independent.decision, PermitDecision::Granted);
    }

    #[tokio::test]
    async fn update_trigger_removes_replaced_orphan_blob() {
        let storage = SqliteStorage::in_memory().await.unwrap();
        let policy = FaviconPolicy {
            positive_ttl: Duration::from_secs(60),
            negative_ttl: Duration::from_secs(10),
            max_blob_bytes: 100,
            max_total_bytes: 1_000,
        };
        for data in [vec![1, 2, 3], vec![4, 5, 6]] {
            storage
                .favicon_put(
                    "resolver",
                    "example.com",
                    Some(&FaviconData {
                        data,
                        mime: "image/png".into(),
                    }),
                    &policy,
                )
                .await
                .unwrap();
        }
        let blobs: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM favicon_blobs")
            .fetch_one(&storage.pool)
            .await
            .unwrap();
        assert_eq!(blobs, 1);
    }

    #[tokio::test]
    async fn health_is_aggregated_and_latest_state_is_readable() {
        let storage = SqliteStorage::in_memory().await.unwrap();
        for success in [false, true] {
            storage
                .record_engine_health(&EngineHealthUpdate {
                    engine: "brave".into(),
                    bucket: 123,
                    latency_ms: 10,
                    success,
                    timed_out: false,
                    error_category: (!success).then(|| "captcha".into()),
                    circuit_status: if success { "half_open" } else { "open" }.into(),
                    cooldown_until_ms: None,
                })
                .await
                .unwrap();
        }
        let health = storage
            .latest_engine_health("brave")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(health.successes, 1);
        assert_eq!(health.errors, 1);
        assert_eq!(health.circuit_status, "half_open");
    }

    #[tokio::test]
    async fn unsupported_future_schema_is_rejected() {
        let storage = SqliteStorage::in_memory().await.unwrap();
        sqlx::query(
            r#"
            INSERT INTO _sqlx_migrations
                (version, description, installed_on, success, checksum, execution_time)
            SELECT 999, 'future', installed_on, success, checksum, execution_time
            FROM _sqlx_migrations
            LIMIT 1
            "#,
        )
        .execute(&storage.pool)
        .await
        .unwrap();
        assert!(matches!(
            reject_newer_schema(&storage.pool).await,
            Err(StorageError::UnsupportedSchema { found: 999, .. })
        ));
    }

    #[tokio::test]
    async fn inaccessible_database_path_fails_startup() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("missing-parent/zoeken.sqlite3");
        let result = SqliteStorage::connect(path, Duration::from_millis(50), 1).await;
        assert!(result.is_err());
    }
}
