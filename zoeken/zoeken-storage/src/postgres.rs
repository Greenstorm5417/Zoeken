use std::str::FromStr;
use std::time::Duration;

use async_trait::async_trait;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::PgPool;

use crate::shared::{
    BudgetRow, FaviconJoinRow, HealthRow, PrunableBlob, blocked_retry_after, concurrency_limited,
    favicon_digest, granted, health_retention_bucket, health_snapshot, lookup_from_join, new_lease,
    pg, rate_limited, refill_tokens, reject_if_newer, sql, supported_version, token_retry_after,
};
use crate::{
    EngineHealthSnapshot, EngineHealthUpdate, FaviconData, FaviconLookup, FaviconPolicy, OriginLease,
    OriginPolicy, PermitResult, Storage, StorageError, now_ms,
};

static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations/postgres");

#[derive(Clone)]
pub struct PostgresStorage {
    pool: PgPool,
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
    let found: i64 = sqlx::query_scalar(&pg(sql::MAX_MIGRATION_VERSION))
        .fetch_one(pool)
        .await?;
    reject_if_newer(found, supported_version(&MIGRATOR))
}

#[async_trait]
impl Storage for PostgresStorage {
    async fn healthcheck(&self) -> Result<(), StorageError> {
        sqlx::query_scalar::<_, i32>(sql::HEALTHCHECK)
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
        sqlx::query(&pg(sql::ADVISORY_LOCK))
            .bind(origin)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(&pg(sql::DELETE_EXPIRED_LEASES))
            .bind(now)
            .execute(&mut *transaction)
            .await?;

        let active: i64 = sqlx::query_scalar(&pg(sql::COUNT_ACTIVE_LEASES))
            .bind(origin)
            .fetch_one(&mut *transaction)
            .await?;
        if active >= i64::from(policy.max_concurrent) {
            transaction.commit().await?;
            return Ok(concurrency_limited());
        }

        let budget = sqlx::query_as::<_, BudgetRow>(&pg(sql::SELECT_BUDGET_FOR_UPDATE))
            .bind(origin)
            .fetch_optional(&mut *transaction)
            .await?;
        if let Some(retry_after) = blocked_retry_after(budget.as_ref(), now) {
            transaction.commit().await?;
            return Ok(rate_limited(retry_after));
        }
        let (tokens, stored_tokens) = refill_tokens(budget, policy, now);

        sqlx::query(&pg(sql::UPSERT_BUDGET))
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
        sqlx::query(&pg(sql::INSERT_LEASE))
            .bind(&lease.id)
            .bind(&lease.origin)
            .bind(lease.expires_at_ms)
            .execute(&mut *transaction)
            .await?;
        transaction.commit().await?;
        Ok(granted(lease))
    }

    async fn release_origin(&self, lease: &OriginLease) -> Result<(), StorageError> {
        sqlx::query(&pg(sql::DELETE_LEASE))
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
        let updated = sqlx::query(&pg(sql::RENEW_LEASE))
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
        // Postgres: GREATEST(...); SQLite uses MAX(...).
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
        let row = sqlx::query_as::<_, FaviconJoinRow>(&pg(sql::SELECT_FAVICON_JOIN))
            .bind(resolver)
            .bind(authority)
            .bind(now_ms())
            .fetch_optional(&self.pool)
            .await?;
        Ok(lookup_from_join(row))
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
        sqlx::query(&pg(sql::UPSERT_MAPPING))
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
        sqlx::query(&pg(sql::UPSERT_ENGINE_HEALTH))
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
        let row = sqlx::query_as::<_, HealthRow>(&pg(sql::LATEST_ENGINE_HEALTH))
            .bind(engine)
            .fetch_optional(&self.pool)
            .await?;
        Ok(row.map(health_snapshot))
    }

    async fn maintenance(&self, max_total_bytes: usize) -> Result<(), StorageError> {
        let now = now_ms();
        let mut transaction = self.pool.begin().await?;
        sqlx::query(&pg(sql::DELETE_EXPIRED_LEASES))
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(&pg(sql::DELETE_EXPIRED_MAPPINGS))
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(&pg(sql::CLEAR_EXPIRED_COOLDOWNS))
            .bind(now)
            .execute(&mut *transaction)
            .await?;
        sqlx::query(&pg(sql::DELETE_OLD_HEALTH))
            .bind(health_retention_bucket(now))
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
                // Deleting mappings fires the orphan-cleanup trigger. The
                // explicit blob delete is idempotent and covers unmapped rows.
                sqlx::query(&pg(sql::DELETE_MAPPINGS_BY_DIGEST))
                    .bind(&blob.digest)
                    .execute(&mut *transaction)
                    .await?;
                sqlx::query(&pg(sql::DELETE_BLOB))
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
