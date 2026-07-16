// Cache TTL semantics: values expire after TTL and are unchanged before expiry.
// Uses generous timing margins for determinism; tested via public KvStore surface only.

use std::time::Duration;

use proptest::prelude::*;
use zoeken_metrics::cache::{InProcKv, KvStore};

/// TTL long enough for immediate reads before expiry.
const LONG_TTL: Duration = Duration::from_secs(30);

/// Safety margin after TTL before asserting expiry.
const POST_TTL_MARGIN: Duration = Duration::from_millis(150);

/// Non-empty key strategy.
fn key_strategy() -> impl Strategy<Value = String> {
    prop::string::string_regex("[a-zA-Z0-9_./:-]{1,24}").expect("valid key regex")
}

/// Arbitrary opaque payload strategy.
fn value_strategy() -> impl Strategy<Value = Vec<u8>> {
    prop::collection::vec(any::<u8>(), 0..=64)
}

/// Short TTL strategy for expiry tests.
fn short_ttl_ms_strategy() -> impl Strategy<Value = u64> {
    20u64..=50
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn ttl_semantics_hold(
        key in key_strategy(),
        value in value_strategy(),
        short_ttl_ms in short_ttl_ms_strategy(),
    ) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_time()
            .build()
            .expect("build current-thread runtime");

        runtime.block_on(async {
            let kv = InProcKv::new();

            let k_pre = format!("{key}::pre");
            let k_none = format!("{key}::none");
            let k_exp = format!("{key}::exp");
            let k_del = format!("{key}::del");

            kv.set_ttl(&k_pre, value.clone(), Some(LONG_TTL)).await;
            prop_assert_eq!(
                kv.get(&k_pre).await,
                Some(value.clone()),
                "value read before TTL must be unchanged"
            );

            kv.set_ttl(&k_none, value.clone(), None).await;
            let short_ttl = Duration::from_millis(short_ttl_ms);
            kv.set_ttl(&k_exp, value.clone(), Some(short_ttl)).await;

            tokio::time::sleep(short_ttl + POST_TTL_MARGIN).await;

            prop_assert_eq!(
                kv.get(&k_none).await,
                Some(value.clone()),
                "None TTL must not expire on time"
            );

            prop_assert_eq!(
                kv.get(&k_exp).await,
                None,
                "value must expire after TTL"
            );

            kv.set_ttl(&k_del, value.clone(), Some(LONG_TTL)).await;
            kv.del(&k_del).await;
            prop_assert_eq!(
                kv.get(&k_del).await,
                None,
                "deleted key must not be returned"
            );

            Ok(())
        })?;
    }
}
