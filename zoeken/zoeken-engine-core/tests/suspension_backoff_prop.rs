use std::time::{Duration, Instant};

use proptest::prelude::*;
use zoeken_engine_core::{EngineState, SuspendConfig, suspend_duration};

/// Independent reference for the capped exponential-backoff duration.
fn expected_capped_backoff(penalty: u32, base: Duration, max: Duration) -> Duration {
    let base_nanos = base.as_nanos();
    if base_nanos == 0 {
        return Duration::ZERO;
    }
    let max_nanos = max.as_nanos();

    let mut scaled = base_nanos;
    for _ in 0..penalty {
        match scaled.checked_mul(2) {
            Some(doubled) => scaled = doubled,
            None => {
                scaled = u128::MAX;
                break;
            }
        }
    }

    let capped = scaled.min(max_nanos);
    let secs = (capped / 1_000_000_000) as u64;
    let nanos = (capped % 1_000_000_000) as u32;
    Duration::new(secs, nanos)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Checks `suspend_duration` against the capped exponential-backoff model.
    #[test]
    fn backoff_duration_matches_capped_exponential(
        penalty in 0u32..=200,
        base_ms in 1u64..=60_000,
        max_ms in 0u64..=600_000,
    ) {
        let base = Duration::from_millis(base_ms);
        let max = Duration::from_millis(max_ms);

        let got = suspend_duration(penalty, base, max);
        let expected = expected_capped_backoff(penalty, base, max);

        prop_assert_eq!(
            got,
            expected,
            "penalty={}, base={:?}, max={:?}: got={:?}, expected={:?}",
            penalty,
            base,
            max,
            got,
            expected
        );

        prop_assert!(
            got <= max,
            "penalty={}, base={:?}, max={:?}: duration {:?} exceeds max",
            penalty,
            base,
            max,
            got
        );
    }

    /// Checks `is_suspended` against the recorded `suspend_end` window.
    #[test]
    fn suspension_gating_matches_end_time(
        suspend_ms in 1u64..=120_000,
        successes in 0usize..=16,
        sample_offsets_ms in proptest::collection::vec(0u64..=240_000, 1..32),
    ) {
        let cfg = SuspendConfig::new(1, Duration::from_secs(5), Duration::from_secs(120));
        let base = Instant::now();

        let suspend = Duration::from_millis(suspend_ms);
        let mut state = EngineState::new();
        state.on_error(base, &cfg, "prop-test suspension", Some(suspend));

        let suspend_end = state
            .suspend_end
            .expect("on_error with an explicit duration must set suspend_end");
        prop_assert_eq!(
            suspend_end,
            base + suspend,
            "explicit suspension end time should be base + {:?}",
            suspend
        );
        prop_assert!(
            state.suspend_reason.is_some(),
            "a suspend reason should be recorded when the engine is suspended"
        );

        for _ in 0..successes {
            state.on_success();
        }
        prop_assert_eq!(
            state.suspend_end,
            Some(suspend_end),
            "intervening successes must not change the suspend end time"
        );

        for offset_ms in sample_offsets_ms {
            let now = base + Duration::from_millis(offset_ms);
            let expected = now < suspend_end;
            prop_assert_eq!(
                state.is_suspended(now),
                expected,
                "at t+{}ms (suspend_ms={}, successes={}): expected is_suspended={}",
                offset_ms,
                suspend_ms,
                successes,
                expected
            );
        }
    }
}
