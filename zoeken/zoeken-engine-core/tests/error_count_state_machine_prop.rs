use std::time::{Duration, Instant};

use proptest::prelude::*;
use zoeken_engine_core::{EngineState, SuspendConfig};

#[derive(Debug, Clone, Copy)]
struct Event {
    is_error: bool,
    delta_ms: u64,
}

fn event_strategy() -> impl Strategy<Value = Event> {
    (any::<bool>(), 0u64..=25_000).prop_map(|(is_error, delta_ms)| Event { is_error, delta_ms })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Checks `continuous_errors` against the state-machine rules.
    #[test]
    fn error_count_state_machine(
        threshold in 1u32..=5,
        base_secs in 1u64..=10,
        events in proptest::collection::vec(event_strategy(), 1..64),
    ) {
        let cfg = SuspendConfig::new(
            threshold,
            Duration::from_secs(base_secs),
            Duration::from_secs(3600),
        );

        let base = Instant::now();
        let mut state = EngineState::new();
        let mut elapsed_ms: u64 = 0;

        for (i, event) in events.iter().enumerate() {
            elapsed_ms = elapsed_ms.saturating_add(event.delta_ms);
            let now = base + Duration::from_millis(elapsed_ms);

            let was_suspended = state.is_suspended(now);
            let before = state.continuous_errors;

            let expected = if event.is_error {
                if was_suspended {
                    before
                } else {
                    before.saturating_add(1)
                }
            } else {
                0
            };

            if event.is_error {
                state.on_error(now, &cfg, "prop-test error", None);
            } else {
                state.on_success();
            }

            prop_assert_eq!(
                state.continuous_errors,
                expected,
                "event #{} ({}) at t+{}ms: was_suspended={}, before={}, expected={}, got={}",
                i,
                if event.is_error { "error" } else { "success" },
                elapsed_ms,
                was_suspended,
                before,
                expected,
                state.continuous_errors,
            );
        }
    }
}
