//! Shared row types, SQL (`?` placeholders), and pure helpers for both backends.

use std::time::Duration;

use sha2::{Digest, Sha256};
use sqlx::FromRow;

use crate::{
    EngineHealthSnapshot, FaviconData, FaviconLookup, OriginLease, OriginPolicy, PermitDecision,
    PermitResult, StorageError, new_lease_id,
};

/// Rewrite SQLite-style `?` placeholders to Postgres `$1`, `$2`, …
pub(crate) fn pg(sql: &str) -> String {
    let mut n = 0u32;
    let mut out = String::with_capacity(sql.len() + 16);
    for ch in sql.chars() {
        if ch == '?' {
            n += 1;
            out.push('$');
            out.push_str(&n.to_string());
        } else {
            out.push(ch);
        }
    }
    out
}

#[derive(FromRow)]
pub(crate) struct BudgetRow {
    pub tokens: f64,
    pub last_refill_ms: i64,
    pub blocked_until_ms: Option<i64>,
}

#[derive(FromRow)]
pub(crate) struct MappingRow {
    pub digest: Option<String>,
    pub is_negative: bool,
    pub expires_at_ms: i64,
}

#[derive(FromRow)]
pub(crate) struct BlobRow {
    pub data: Vec<u8>,
    pub mime: String,
}

#[derive(FromRow)]
pub(crate) struct FaviconJoinRow {
    pub is_negative: bool,
    pub data: Option<Vec<u8>>,
    pub mime: Option<String>,
}

#[derive(FromRow)]
pub(crate) struct HealthRow {
    pub bucket: i64,
    pub successes: i64,
    pub timeouts: i64,
    pub errors: i64,
    pub circuit_status: String,
    pub cooldown_until_ms: Option<i64>,
    pub last_error_category: Option<String>,
}

#[derive(FromRow)]
pub(crate) struct PrunableBlob {
    pub digest: String,
    pub size_bytes: i64,
}

pub(crate) mod sql {
    pub const HEALTHCHECK: &str = "SELECT 1";
    pub const DELETE_EXPIRED_LEASES: &str = "DELETE FROM origin_leases WHERE expires_at_ms <= ?";
    pub const COUNT_ACTIVE_LEASES: &str = "SELECT COUNT(*) FROM origin_leases WHERE origin = ?";
    pub const SELECT_BUDGET: &str =
        "SELECT tokens, last_refill_ms, blocked_until_ms FROM origin_budgets WHERE origin = ?";
    pub const SELECT_BUDGET_FOR_UPDATE: &str = "SELECT tokens, last_refill_ms, blocked_until_ms FROM origin_budgets WHERE origin = ? FOR UPDATE";
    pub const UPSERT_BUDGET: &str = "INSERT INTO origin_budgets (origin, tokens, last_refill_ms, blocked_until_ms) VALUES (?, ?, ?, NULL) ON CONFLICT (origin) DO UPDATE SET tokens = excluded.tokens, last_refill_ms = excluded.last_refill_ms, blocked_until_ms = NULL";
    pub const INSERT_LEASE: &str =
        "INSERT INTO origin_leases (lease_id, origin, expires_at_ms) VALUES (?, ?, ?)";
    pub const DELETE_LEASE: &str = "DELETE FROM origin_leases WHERE lease_id = ?";
    pub const RENEW_LEASE: &str =
        "UPDATE origin_leases SET expires_at_ms = ? WHERE lease_id = ? AND origin = ?";
    pub const SELECT_MAPPING: &str = "SELECT digest, is_negative, expires_at_ms FROM favicon_mappings WHERE resolver = ? AND authority = ?";
    pub const SELECT_BLOB: &str = "SELECT data, mime FROM favicon_blobs WHERE digest = ?";
    pub const SELECT_FAVICON_JOIN: &str = "SELECT mapping.is_negative, blob.data, blob.mime FROM favicon_mappings AS mapping LEFT JOIN favicon_blobs AS blob ON blob.digest = mapping.digest WHERE mapping.resolver = ? AND mapping.authority = ? AND mapping.expires_at_ms > ?";
    pub const UPSERT_MAPPING: &str = "INSERT INTO favicon_mappings (resolver, authority, digest, is_negative, expires_at_ms) VALUES (?, ?, ?, ?, ?) ON CONFLICT (resolver, authority) DO UPDATE SET digest = excluded.digest, is_negative = excluded.is_negative, expires_at_ms = excluded.expires_at_ms";
    pub const UPSERT_ENGINE_HEALTH: &str = "INSERT INTO engine_health (engine, bucket, latency_ms_sum, successes, timeouts, errors, circuit_status, cooldown_until_ms, last_error_category) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT (engine, bucket) DO UPDATE SET latency_ms_sum = engine_health.latency_ms_sum + excluded.latency_ms_sum, successes = engine_health.successes + excluded.successes, timeouts = engine_health.timeouts + excluded.timeouts, errors = engine_health.errors + excluded.errors, circuit_status = excluded.circuit_status, cooldown_until_ms = excluded.cooldown_until_ms, last_error_category = excluded.last_error_category";
    pub const LATEST_ENGINE_HEALTH: &str = "SELECT bucket, successes, timeouts, errors, circuit_status, cooldown_until_ms, last_error_category FROM engine_health WHERE engine = ? ORDER BY bucket DESC LIMIT 1";
    pub const DELETE_EXPIRED_MAPPINGS: &str =
        "DELETE FROM favicon_mappings WHERE expires_at_ms <= ?";
    pub const CLEAR_EXPIRED_COOLDOWNS: &str = "UPDATE engine_health SET cooldown_until_ms = NULL WHERE circuit_status = 'open' AND cooldown_until_ms <= ?";
    pub const DELETE_OLD_HEALTH: &str = "DELETE FROM engine_health WHERE bucket < ?";
    pub const SUM_BLOB_BYTES: &str = "SELECT COALESCE(SUM(size_bytes), 0) FROM favicon_blobs";
    pub const LIST_BLOBS_BY_AGE: &str =
        "SELECT digest, size_bytes FROM favicon_blobs ORDER BY created_at_ms ASC";
    pub const DELETE_MAPPINGS_BY_DIGEST: &str = "DELETE FROM favicon_mappings WHERE digest = ?";
    pub const DELETE_BLOB: &str = "DELETE FROM favicon_blobs WHERE digest = ?";
    pub const MAX_MIGRATION_VERSION: &str =
        "SELECT COALESCE(MAX(version), 0) FROM _sqlx_migrations";
    pub const ADVISORY_LOCK: &str = "SELECT pg_advisory_xact_lock(hashtextextended(?, 0))";
}

pub(crate) fn concurrency_limited() -> PermitResult {
    PermitResult {
        decision: PermitDecision::ConcurrencyLimited,
        lease: None,
        retry_after: Duration::from_millis(50),
    }
}

pub(crate) fn rate_limited(retry_after: Duration) -> PermitResult {
    PermitResult {
        decision: PermitDecision::RateLimited,
        lease: None,
        retry_after,
    }
}

pub(crate) fn granted(lease: OriginLease) -> PermitResult {
    PermitResult {
        decision: PermitDecision::Granted,
        lease: Some(lease),
        retry_after: Duration::ZERO,
    }
}

pub(crate) fn blocked_retry_after(budget: Option<&BudgetRow>, now: i64) -> Option<Duration> {
    let blocked_until = budget.and_then(|row| row.blocked_until_ms)?;
    (blocked_until > now).then(|| Duration::from_millis((blocked_until - now) as u64))
}

/// Refill the token bucket. Caller persists `stored_tokens`, then checks `tokens < 1.0`.
pub(crate) fn refill_tokens(
    budget: Option<BudgetRow>,
    policy: &OriginPolicy,
    now: i64,
) -> (f64, f64) {
    let (old_tokens, last_refill) = budget.map_or((f64::from(policy.burst), now), |row| {
        (row.tokens, row.last_refill_ms)
    });
    let elapsed = (now - last_refill).max(0) as f64 / 1000.0;
    let tokens = (old_tokens + elapsed * policy.requests_per_second).min(f64::from(policy.burst));
    let stored = if tokens >= 1.0 { tokens - 1.0 } else { tokens };
    (tokens, stored)
}

pub(crate) fn new_lease(origin: &str, now: i64, lease_duration: Duration) -> OriginLease {
    OriginLease {
        id: new_lease_id(),
        origin: origin.to_string(),
        expires_at_ms: now + lease_duration.as_millis() as i64,
    }
}

pub(crate) fn token_retry_after(tokens: f64, rps: f64) -> Duration {
    Duration::from_secs_f64((1.0 - tokens) / rps)
}

pub(crate) fn favicon_digest(data: &[u8]) -> String {
    hex::encode(Sha256::digest(data))
}

pub(crate) fn lookup_from_join(row: Option<FaviconJoinRow>) -> FaviconLookup {
    match row {
        None => FaviconLookup::Absent,
        Some(row) if row.is_negative => FaviconLookup::KnownMissing,
        Some(row) => match (row.data, row.mime) {
            (Some(data), Some(mime)) => FaviconLookup::Hit(FaviconData { data, mime }),
            _ => FaviconLookup::Absent,
        },
    }
}

pub(crate) fn health_snapshot(row: HealthRow) -> EngineHealthSnapshot {
    EngineHealthSnapshot {
        bucket: row.bucket,
        successes: row.successes.max(0) as u64,
        timeouts: row.timeouts.max(0) as u64,
        errors: row.errors.max(0) as u64,
        circuit_status: row.circuit_status,
        cooldown_until_ms: row.cooldown_until_ms,
        last_error_category: row.last_error_category,
    }
}

pub(crate) fn reject_if_newer(found: i64, supported: i64) -> Result<(), StorageError> {
    if found > supported {
        Err(StorageError::UnsupportedSchema { found, supported })
    } else {
        Ok(())
    }
}

pub(crate) fn supported_version(migrator: &sqlx::migrate::Migrator) -> i64 {
    migrator
        .iter()
        .map(|migration| migration.version)
        .max()
        .unwrap_or(0)
}

pub(crate) fn health_retention_bucket(now_ms: i64) -> i64 {
    now_ms / 3_600_000 - 24 * 7
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pg_rewrites_placeholders_in_order() {
        assert_eq!(pg("a = ? AND b = ?"), "a = $1 AND b = $2");
        assert_eq!(pg("no placeholders"), "no placeholders");
    }
}
