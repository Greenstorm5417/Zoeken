use proptest::prelude::*;
use zoeken_botdetect::token_bucket::{BucketState, RateLimiter, step};

#[derive(Clone, Copy, Debug)]
struct RefBucket {
    tokens: f64,
    last: f64,
}

impl RefBucket {
    fn full(capacity: f64, now: f64) -> Self {
        Self {
            tokens: capacity,
            last: now,
        }
    }

    fn admit(&mut self, capacity: f64, refill_per_second: f64, now: f64, cost: f64) -> bool {
        let dt = now - self.last;
        let elapsed = if dt > 0.0 { dt } else { 0.0 };
        let refilled = {
            let acc = self.tokens + elapsed * refill_per_second;
            if acc > capacity { capacity } else { acc }
        };
        self.last = now;
        if refilled >= cost {
            self.tokens = refilled - cost;
            true
        } else {
            self.tokens = refilled;
            false
        }
    }
}

fn approx_eq(a: f64, b: f64) -> bool {
    let diff = (a - b).abs();
    diff <= 1e-9 || diff <= 1e-9 * a.abs().max(b.abs())
}

fn request_strategy() -> impl Strategy<Value = (f64, f64)> {
    let delta = prop_oneof![Just(0.0f64), 0.0f64..50.0,];
    let cost = 0.0f64..8.0;
    (delta, cost)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn step_matches_reference_over_request_sequence(
        capacity in 1.0f64..1000.0,
        refill_per_second in 0.0f64..1000.0,
        start in 0.0f64..1000.0,
        requests in proptest::collection::vec(request_strategy(), 1..64),
    ) {
        let mut prod = BucketState::full(capacity, start);
        let mut reference = RefBucket::full(capacity, start);

        let mut now = start;
        for (i, (delta, cost)) in requests.into_iter().enumerate() {
            now += delta;

            let (allowed, next) = step(prod, capacity, refill_per_second, now, cost);
            let ref_allowed = reference.admit(capacity, refill_per_second, now, cost);

            prop_assert_eq!(
                allowed,
                ref_allowed,
                "step {} (t={}, cost={}, cap={}, refill={}): allow/deny mismatch (prod={}, ref={})",
                i, now, cost, capacity, refill_per_second, allowed, ref_allowed
            );

            prop_assert!(
                approx_eq(next.tokens, reference.tokens),
                "step {} (t={}): token state diverged (prod={}, ref={})",
                i, now, next.tokens, reference.tokens
            );
            prop_assert!(
                next.tokens <= capacity + 1e-9,
                "step {}: tokens {} exceeded capacity {}",
                i, next.tokens, capacity
            );

            prod = next;
        }
    }

    #[test]
    fn rate_limiter_check_at_matches_reference(
        capacity in 1.0f64..1000.0,
        refill_per_second in 0.0f64..1000.0,
        start in 0.0f64..1000.0,
        deltas in proptest::collection::vec(prop_oneof![Just(0.0f64), 0.0f64..50.0], 1..64),
    ) {
        let limiter = RateLimiter::new();
        let key = "203.0.113.0/32";
        let mut reference: Option<RefBucket> = None;

        let mut now = start;
        for (i, delta) in deltas.into_iter().enumerate() {
            now += delta;

            let allowed = limiter.check_at(key, capacity, refill_per_second, now);

            let bucket = reference.get_or_insert_with(|| RefBucket::full(capacity, now));
            let ref_allowed = bucket.admit(capacity, refill_per_second, now, 1.0);

            prop_assert_eq!(
                allowed,
                ref_allowed,
                "check_at {} (t={}, cap={}, refill={}): mismatch (prod={}, ref={})",
                i, now, capacity, refill_per_second, allowed, ref_allowed
            );
        }
    }

    #[test]
    fn capacity_is_restored_after_enough_time(
        capacity in 1.0f64..1000.0,
        refill_per_second in 0.01f64..1000.0,
        cost_frac in 0.01f64..1.0,
        t0 in 0.0f64..1000.0,
    ) {
        let cost = capacity * cost_frac;

        let empty = BucketState { tokens: 0.0, last: t0 };
        let (rejected, _after) = step(empty, capacity, refill_per_second, t0, cost);
        prop_assert!(
            !rejected,
            "an empty bucket must reject a cost={} request at t0 (cap={}, refill={})",
            cost, capacity, refill_per_second
        );

        let wait = cost / refill_per_second * (1.0 + 1e-6) + 1e-9;
        let now = t0 + wait;
        let (allowed, _) = step(empty, capacity, refill_per_second, now, cost);
        prop_assert!(
            allowed,
            "after waiting {}s for {} tokens (cap={}, refill={}) the request should be allowed",
            wait, cost, capacity, refill_per_second
        );
    }
}
