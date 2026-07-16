// Property-based tests for startup secret-key decision.

use proptest::prelude::*;
use zoeken_settings::{SecretKeyDecision, secret_key_decision};

fn secret() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        "[ \t\r\n]{1,8}".prop_map(|s| s.to_string()),
        "[a-zA-Z0-9._:/@-]{1,32}".prop_map(|s| s.to_string()),
    ]
}

fn expected(is_loopback: bool, secret_is_empty: bool) -> SecretKeyDecision {
    if !secret_is_empty {
        SecretKeyDecision::Start
    } else if is_loopback {
        SecretKeyDecision::StartWithWarning
    } else {
        SecretKeyDecision::Abort
    }
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn startup_secret_key_decision(is_loopback in any::<bool>(), secret in secret()) {
        let secret_is_empty = secret.is_empty();
        let decision = secret_key_decision(is_loopback, secret_is_empty);
        prop_assert_eq!(decision, expected(is_loopback, secret_is_empty));
        match (is_loopback, secret_is_empty) {
            (false, true) => prop_assert_eq!(decision, SecretKeyDecision::Abort),
            (true, true) => prop_assert_eq!(decision, SecretKeyDecision::StartWithWarning),
            (_, false) => prop_assert_eq!(decision, SecretKeyDecision::Start),
        }
    }
}
