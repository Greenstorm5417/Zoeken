use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{FromRow, PgPool};

use crate::{
    EngineHealthSnapshot, EngineHealthUpdate, FaviconData, FaviconLookup, FaviconPolicy,
    OriginLease, OriginPolicy, PermitDecision, PermitResult, Storage, StorageError, new_lease_id,
    now_ms,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/postgres");

#[derive(Clone)]
pub struct PostgresStorage {
    pool: PgPool,
}

#[derive(FromRow)]
struct BudgetRow {
    tokens: f64,
    last_refill_ms: i64,
    blocked_until_ms: Option<i64>,
}

#[derive(FromRow)]
struct FaviconRow {
    is_negative: bool,
    data: Option<Vec<u8>>,
    mime: Option<String>,
}

#[derive(FromRow)]
struct HealthRow {
    bucket: i64,
    successes: i64,
    timeouts: i64,
    errors: i64,
    circuit_status: String,
    cooldown_until_ms: Option<i64>,
    last_error_category: Option<String>,
}

#[derive(FromRow)]
struct PrunableBlob {
    digest: String,
    size_bytes: i64,
}

impl PostgresStorage {
    pub async fn connect(
        url: &str,
        max_connections: usize,
        acquire_timeout: Duration,
    ) -> Result<Self, StorageError> {
        let options =
            PgConnectOptions::from_str(url).map_err(|_| StorageError::InvalidConnectionConfig)?;
        let pool = PgPoolOptions::new()
            .max_connections(max_connections.max(1) as u32)
            .acquire_timeout(acquire_timeout)
            .connect_with(options)
            .await?;

        reject_newer_schema(&pool).await?;
        // SQLx records checksums and serializes each migration with its own
        // PostgreSQL advisory lock, making concurrent replica startup safe.
        MIGRATOR.run(&pool).await?;
        Ok(Self { pool })
    }
}

async fn reject_newer_schema(pool: &PgPool) -> Result<(), StorageError> {
    let exists: bool = sqlx::query_scalar("SELECT to_regclass('_sqlx_migrations') IS NOT NULL")
        .fetch_one(pool)
        .await?;
    if !exists {
        return Ok(());
    }
    let found: i64 = sqlx::query_scalar("SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations")
        .fetch_one(pool)
        .await?;
    let supported = MIGRATOR
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0);
    if found > supported {
        return Err(StorageError::UnsupportedSchema { found, supported });
    }
    Ok(())
}

#[async_trait]
impl Storage for PostgresStorage {
    async fn healthcheck(&self) -> Result<(), StorageError> {
        sqlx::query_scalar::<_, i32>("SELECT 1")
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

        // One transactional lock per origin coordinates all replicas while
        // allowing unrelated origins to proceed independently.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(origin)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM origin_leases WHERE expires_at_ms <= $1")
            .bind(now)
            .execute(&mut *transaction)
            .await?;

        let active: i64 =
            sqlx::query_scalar("SELECT COUNT(*) FROM origin_leases WHERE origin = $1")
                .bind(origin)
                .fetch_one(&mut *transaction)
                .await?;
        if active >= i64::from(policy.max_concurrent) {
            transaction.commit().await?;
            return Ok(PermitResult {
                decision: PermitDecision::ConcurrencyLimited,
                lease: None,
                retry_after: Duration::from_millis(50),
            });
        }

        let budget = sqlx::query_as::<_, BudgetRow>(
            r#"
            SELECT tokens, last_refill_ms, blocked_until_ms
            FROM origin_budgets
            WHERE origin = $1
            FOR UPDATE
            "#,
        )
        .bind(origin)
        .fetch_optional(&mut *transaction)
        .await?;
        if let Some(blocked_until) = budget.as_ref().and_then(|row| row.blocked_until_ms)
            && blocked_until > now
        {
            transaction.commit().await?;
            return Ok(PermitResult {
                decision: PermitDecision::RateLimited,
                lease: None,
                retry_after: Duration::from_millis((blocked_until - now) as u64),
            });
        }
        let (old_tokens, last_refill) = budget.map_or((f64::from(policy.burst), now), |row| {
            (row.tokens, row.last_refill_ms)
        });
        let elapsed_seconds = (now - last_refill).max(0) as f64 / 1000.0;
        let tokens = (old_tokens + elapsed_seconds * policy.requests_per_second)
            .min(f64::from(policy.burst));
        let stored_tokens = if tokens >= 1.0 { tokens - 1.0 } else { tokens };

        sqlx::query(
            r#"
            INSERT INTO origin_budgets (origin, tokens, last_refill_ms, blocked_until_ms)
            VALUES ($1, $2, $3, NULL)
            ON CONFLICT (origin) DO UPDATE SET
                tokens = excluded.tokens,
                last_refill_ms = excluded.last_refill_ms,
                blocked_until_ms = NULL
            "#,
        )
        .bind(origin)
        .bind(stored_tokens)
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        if tokens < 1.0 {
            transaction.commit().await?;
            return Ok(PermitResult {
                decision: PermitDecision::RateLimited,
                lease: None,
                retry_after: Duration::from_secs_f64((1.0 - tokens) / policy.requests_per_second),
            });
        }

        let lease = OriginLease {
            id: new_lease_id(),
            origin: origin.to_string(),
            expires_at_ms: now + policy.lease_duration.as_millis() as i64,
        };
        sqlx::query(
            r#"
            INSERT INTO origin_leases (lease_id, origin, expires_at_ms)
            VALUES ($1, $2, $3)
            "#,
        )
        .bind(&lease.id)
        .bind(&lease.origin)
        .bind(lease.expires_at_ms)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;

        Ok(PermitResult {
            decision: PermitDecision::Granted,
            lease: Some(lease),
            retry_after: Duration::ZERO,
        })
    }

    async fn release_origin(&self, lease: &OriginLease) -> Result<(), StorageError> {
        sqlx::query("DELETE FROM origin_leases WHERE lease_id = $1")
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
        let updated = sqlx::query(
            "UPDATE origin_leases SET expires_at_ms = $1 WHERE lease_id = $2 AND origin = $3",
        )
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
        sqlx::query(
            r#"
            INSERT INTO origin_budgets
                (origin, tokens, last_refill_ms, blocked_until_ms)
            VALUES ($1, 0, $2, $3)
            ON CONFLICT (origin) DO UPDATE SET
                tokens = 0,
                last_refill_ms = excluded.last_refill_ms,
                blocked_until_ms = GREATEST(
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
        let row = sqlx::query_as::<_, FaviconRow>(
            r#"
            SELECT mapping.is_negative, blob.data, blob.mime
            FROM favicon_mappings AS mapping
            LEFT JOIN favicon_blobs AS blob ON blob.digest = mapping.digest
            WHERE mapping.resolver = $1
              AND mapping.authority = $2
              AND mapping.expires_at_ms > $3
            "#,
        )
        .bind(resolver)
        .bind(authority)
        .bind(now_ms())
        .fetch_optional(&self.pool)
        .await?;

        Ok(match row {
            None => FaviconLookup::Absent,
            Some(row) if row.is_negative => FaviconLookup::KnownMissing,
            Some(row) => match (row.data, row.mime) {
                (Some(data), Some(mime)) => FaviconLookup::Hit(FaviconData { data, mime }),
                _ => FaviconLookup::Absent,
            },
        })
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
            let digest = hex::encode(Sha256::digest(&favicon.data));
            sqlx::query(
                r#"
                INSERT INTO favicon_blobs
                    (digest, size_bytes, mime, data, created_at_ms)
                VALUES ($1, $2, $3, $4, $5)
                ON CONFLICT DO NOTHING
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
        sqlx::query(
            r#"
            INSERT INTO favicon_mappings
                (resolver, authority, digest, is_negative, expires_at_ms)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (resolver, authority) DO UPDATE SET
                digest = excluded.digest,
                is_negative = excluded.is_negative,
                expires_at_ms = excluded.expires_at_ms
            "#,
        )
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
        sqlx::query(
            r#"
            INSERT INTO engine_health (
                engine, bucket, latency_ms_sum, successes, timeouts, errors,
                circuit_status, cooldown_until_ms, last_error_category
            )
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
            ON CONFLICT (engine, bucket) DO UPDATE SET
                latency_ms_sum = engine_health.latency_ms_sum + excluded.latency_ms_sum,
                successes = engine_health.successes + excluded.successes,
                timeouts = engine_health.timeouts + excluded.timeouts,
                errors = engine_health.errors + excluded.errors,
                circuit_status = excluded.circuit_status,
                cooldown_until_ms = excluded.cooldown_until_ms,
                last_error_category = excluded.last_error_category
            "#,
        )
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
        let row = sqlx::query_as::<_, HealthRow>(
            r#"
            SELECT
                bucket,
                successes,
                timeouts,
                errors,
                circuit_status,
                cooldown_until_ms,
                last_error_category
            FROM engine_health
            WHERE engine = $1
            ORDER BY bucket DESC
            LIMIT 1
            "#,
        )
        .bind(engine)
        .fetch_optional(&self.pool)
        .await?;
        Ok(row.map(|row| EngineHealthSnapshot {
            bucket: row.bucket,
            successes: row.successes.max(0) as u64,
            timeouts: row.timeouts.max(0) as u64,
            errors: row.errors.max(0) as u64,
            circuit_status: row.circuit_status,
            cooldown_until_ms: row.cooldown_until_ms,
            last_error_category: row.last_error_category,
        }))
    }

    async fn maintenance(&self, max_total_bytes: usize) -> Result<(), StorageError> {
        let now = now_ms();
        let mut transaction = self.pool.begin().await?;
        sqlx::query("DELETE FROM origin_leases WHERE expires_at_ms <= $1")
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query("DELETE FROM favicon_mappings WHERE expires_at_ms <= $1")
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            r#"
            UPDATE engine_health
            SET cooldown_until_ms = NULL
            WHERE circuit_status = 'open' AND cooldown_until_ms <= $1
            "#,
        )
        .bind(now)
        .execute(&mut *transaction)
        .await?;
        sqlx::query("DELETE FROM engine_health WHERE bucket < $1")
            .bind(now / 3_600_000 - 24 * 7)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(
            r#"
            DELETE FROM favicon_blobs AS blob
            WHERE NOT EXISTS (
                SELECT 1
                FROM favicon_mappings AS mapping
                WHERE mapping.digest = blob.digest
            )
            "#,
        )
        .execute(&mut *transaction)
        .await?;

        let mut total: i64 =
            sqlx::query_scalar("SELECT COALESCE(SUM(size_bytes), 0) FROM favicon_blobs")
                .fetch_one(&mut *transaction)
                .await?;
        if total > max_total_bytes as i64 {
            let blobs = sqlx::query_as::<_, PrunableBlob>(
                "SELECT digest, size_bytes FROM favicon_blobs ORDER BY created_at_ms ASC",
            )
            .fetch_all(&mut *transaction)
            .await?;
            for blob in blobs {
                if total <= max_total_bytes as i64 {
                    break;
                }
                // Deleting mappings fires the orphan-cleanup trigger. The
                // explicit blob delete is idempotent and covers unmapped rows.
                sqlx::query("DELETE FROM favicon_mappings WHERE digest = $1")
                    .bind(&blob.digest)
                    .execute(&mut *transaction)
                    .await?;
                sqlx::query("DELETE FROM favicon_blobs WHERE digest = $1")
                    .bind(&blob.digest)
                    .execute(&mut *transaction)
                    .await?;
                total -= blob.size_bytes;
            }
        }
        transaction.commit().await?;
        Ok(())
    }
}
