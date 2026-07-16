//! Property-based test for the preferences cookie codec.
//!
//! Feature: Property 45: Preferences cookie round-trip
//!
//! Validates: Requirements 15.2, 15.3

use std::collections::HashSet;

use proptest::prelude::*;
use zoeken_prefs::{Preferences, RequestMethod, decode_cookie, encode_cookie};
use zoeken_query::SafeSearch;

/// Strategy producing an arbitrary [`SafeSearch`] variant.
fn safesearch_strategy() -> impl Strategy<Value = SafeSearch> {
    prop_oneof![
        Just(SafeSearch::Off),
        Just(SafeSearch::Moderate),
        Just(SafeSearch::Strict),
    ]
}

/// Strategy producing an arbitrary [`RequestMethod`] variant.
fn method_strategy() -> impl Strategy<Value = RequestMethod> {
    prop_oneof![Just(RequestMethod::Get), Just(RequestMethod::Post)]
}

/// Strategy producing an arbitrary [`Preferences`] value across the full
/// preference surface (arbitrary strings, arbitrary string vectors, all enum
/// variants, and both booleans).
fn preferences_strategy() -> impl Strategy<Value = Preferences> {
    (
        any::<String>(),
        any::<String>(),
        any::<String>(),
        prop::collection::vec(any::<String>(), 0..8),
        prop::collection::vec(any::<String>(), 0..8),
        safesearch_strategy(),
        any::<String>(),
        any::<bool>(),
        method_strategy(),
        prop::collection::btree_map(any::<String>(), any::<bool>(), 0..8),
    )
        .prop_map(
            |(
                theme,
                locale,
                language,
                categories,
                engines,
                safesearch,
                autocomplete,
                image_proxy,
                method,
                plugins,
            )| {
                Preferences {
                    theme,
                    locale,
                    language,
                    categories,
                    engines,
                    safesearch,
                    autocomplete,
                    image_proxy,
                    method,
                    plugins,
                    locked: HashSet::new(),
                }
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    /// Feature: Property 45: Preferences cookie round-trip
    ///
    /// For any Preferences value, decoding the encoded cookie yields a
    /// Preferences value equal to the original.
    ///
    /// Validates: Requirements 15.2, 15.3
    #[test]
    fn preferences_cookie_round_trip(prefs in preferences_strategy()) {
        let encoded = encode_cookie(&prefs);
        let decoded = decode_cookie(&encoded).expect("encoded cookie must decode");
        prop_assert_eq!(decoded, prefs);
    }
}
