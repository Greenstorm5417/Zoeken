use proptest::prelude::*;
use zoeken_server::middleware::{LimiterGate, resolve_limiter_gate};

fn explicit_strategy() -> impl Strategy<Value = Option<bool>> {
    prop_oneof![Just(None), Just(Some(true)), Just(Some(false)),]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn limiter_enablement_gate(
        is_loopback in any::<bool>(),
        explicit in explicit_strategy(),
        public_instance in any::<bool>(),
    ) {
        let gate = resolve_limiter_gate(is_loopback, explicit, public_instance);

        let mut expected_enabled = explicit.unwrap_or(!is_loopback);
        let mut expected_warn = !is_loopback && explicit == Some(false);
        if public_instance && !is_loopback && !expected_enabled {
            expected_enabled = true;
            expected_warn = false;
        }

        prop_assert_eq!(
            gate,
            LimiterGate {
                enabled: expected_enabled,
                warn_public_unprotected: expected_warn,
            },
            "gate mismatch for (is_loopback={}, explicit={:?}, public_instance={})",
            is_loopback,
            explicit,
            public_instance,
        );

        if gate.enabled {
            prop_assert!(
                !gate.warn_public_unprotected,
                "an enabled limiter must never warn (is_loopback={}, explicit={:?})",
                is_loopback,
                explicit,
            );
        }
    }
}
