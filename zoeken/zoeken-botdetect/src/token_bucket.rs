//! Token-bucket rate limiter for per-key request limiting.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use moka::sync::Cache;

use crate::config::{DEFAULT_STATE_CAPACITY, DEFAULT_STATE_IDLE_SECONDS};

/// Mutable state for one token bucket.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BucketState {
    pub tokens: f64,
    pub last: f64,
}

impl BucketState {
    pub fn full(capacity: f64, now: f64) -> Self {
        Self {
            tokens: capacity,
            last: now,
        }
    }
}

pub fn step(
    prev: BucketState,
    capacity: f64,
    refill_per_second: f64,
    now: f64,
    cost: f64,
) -> (bool, BucketState) {
    let elapsed = (now - prev.last).max(0.0);
    let refilled = (prev.tokens + elapsed * refill_per_second).min(capacity);
    if refilled >= cost {
        (
            true,
            BucketState {
                tokens: refilled - cost,
                last: now,
            },
        )
    } else {
        (
            false,
            BucketState {
                tokens: refilled,
                last: now,
            },
        )
    }
}

/// Thread-safe collection of per-key token buckets.
#[derive(Debug)]
pub struct RateLimiter {
    buckets: Cache<String, Arc<Mutex<BucketState>>>,
    base: Instant,
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

impl RateLimiter {
    pub fn new() -> Self {
        Self::with_limits(
            DEFAULT_STATE_CAPACITY,
            Duration::from_secs(DEFAULT_STATE_IDLE_SECONDS),
        )
    }

    pub fn with_limits(max_entries: u64, idle_timeout: Duration) -> Self {
        Self {
            buckets: Cache::builder()
                .max_capacity(max_entries.max(1))
                .time_to_idle(idle_timeout)
                .build(),
            base: Instant::now(),
        }
    }

    pub fn now_secs(&self) -> f64 {
        self.base.elapsed().as_secs_f64()
    }

    pub fn check(&self, key: &str, capacity: f64, refill_per_second: f64) -> bool {
        self.check_at(key, capacity, refill_per_second, self.now_secs())
    }

    pub fn check_at(&self, key: &str, capacity: f64, refill_per_second: f64, now: f64) -> bool {
        let bucket = self.buckets.get_with(key.to_string(), || {
            Arc::new(Mutex::new(BucketState::full(capacity, now)))
        });
        let mut guard = match bucket.lock() {
            Ok(guard) => guard,
            Err(_poisoned) => return true,
        };
        let (allowed, next) = step(*guard, capacity, refill_per_second, now, 1.0);
        *guard = next;
        allowed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_request_is_allowed_with_full_bucket() {
        let prev = BucketState::full(15.0, 0.0);
        let (allowed, next) = step(prev, 15.0, 0.25, 0.0, 1.0);
        assert!(allowed);
        assert_eq!(next.tokens, 14.0);
    }

    #[test]
    fn burst_is_capped_at_capacity() {
        let mut state = BucketState::full(3.0, 0.0);
        for _ in 0..3 {
            let (allowed, next) = step(state, 3.0, 0.5, 0.0, 1.0);
            assert!(allowed);
            state = next;
        }
        let (allowed, _next) = step(state, 3.0, 0.5, 0.0, 1.0);
        assert!(!allowed);
    }

    #[test]
    fn tokens_refill_as_time_advances() {
        let mut state = BucketState::full(2.0, 0.0);
        for _ in 0..2 {
            state = step(state, 2.0, 1.0, 0.0, 1.0).1;
        }
        assert!(!step(state, 2.0, 1.0, 0.0, 1.0).0);
        let (allowed, _next) = step(state, 2.0, 1.0, 1.0, 1.0);
        assert!(allowed);
    }

    #[test]
    fn refill_never_exceeds_capacity() {
        let state = BucketState {
            tokens: 0.0,
            last: 0.0,
        };
        let (_allowed, next) = step(state, 5.0, 10.0, 1_000_000.0, 1.0);
        assert!(next.tokens <= 5.0);
    }

    #[test]
    fn limiter_starts_each_key_with_a_full_bucket() {
        let limiter = RateLimiter::new();
        assert!(limiter.check_at("k", 1.0, 0.0, 0.0));
        assert!(!limiter.check_at("k", 1.0, 0.0, 0.0));
        assert!(limiter.check_at("other", 1.0, 0.0, 0.0));
    }
}
