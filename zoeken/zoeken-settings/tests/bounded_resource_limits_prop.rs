// Property-based tests for bounded resource-limit resolution.

use proptest::prelude::*;
use zoeken_settings::{
    DEFAULT_MAX_REQUEST_BODY_BYTES, DEFAULT_REQUEST_TIMEOUT_SECONDS, DeploymentConfig,
    resolve_max_request_body_bytes, resolve_request_timeout_seconds,
};

proptest! {
    #![proptest_config(ProptestConfig { cases: 200, ..ProptestConfig::default() })]

    #[test]
    fn max_request_body_bytes_is_bounded_and_positive(configured in any::<usize>()) {
        let resolved = resolve_max_request_body_bytes(configured);
        prop_assert!(resolved > 0);
        if configured > 0 {
            prop_assert_eq!(resolved, configured);
        } else {
            prop_assert_eq!(resolved, DEFAULT_MAX_REQUEST_BODY_BYTES);
        }
        let cfg = DeploymentConfig {
            max_request_body_bytes: configured,
            ..DeploymentConfig::default()
        };
        prop_assert_eq!(cfg.effective_max_request_body_bytes(), resolved);
    }

    #[test]
    fn request_timeout_seconds_is_bounded_and_positive(configured in any::<u64>()) {
        let resolved = resolve_request_timeout_seconds(configured);
        prop_assert!(resolved > 0);
        if configured > 0 {
            prop_assert_eq!(resolved, configured);
        } else {
            prop_assert_eq!(resolved, DEFAULT_REQUEST_TIMEOUT_SECONDS);
        }
        let cfg = DeploymentConfig {
            request_timeout_seconds: configured,
            ..DeploymentConfig::default()
        };
        prop_assert_eq!(cfg.effective_request_timeout_seconds(), resolved);
    }
}
