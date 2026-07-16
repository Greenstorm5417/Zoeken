//! Property-based test for per-engine filter propagation.

use proptest::prelude::*;

use zoeken_engine_core::{EngineMeta, SafeSearch, SearchQueryView, TimeRange};
use zoeken_search::engine_query_view;

fn safesearch_strategy() -> impl Strategy<Value = SafeSearch> {
    prop_oneof![
        Just(SafeSearch::Off),
        Just(SafeSearch::Moderate),
        Just(SafeSearch::Strict),
    ]
}

fn time_range_strategy() -> impl Strategy<Value = Option<TimeRange>> {
    prop_oneof![
        Just(None),
        Just(Some(TimeRange::Day)),
        Just(Some(TimeRange::Week)),
        Just(Some(TimeRange::Month)),
        Just(Some(TimeRange::Year)),
    ]
}

fn base_view_strategy() -> impl Strategy<Value = SearchQueryView> {
    (safesearch_strategy(), time_range_strategy()).prop_map(|(safesearch, time_range)| {
        SearchQueryView {
            safesearch,
            time_range,
            ..SearchQueryView::default()
        }
    })
}

fn meta_strategy() -> impl Strategy<Value = EngineMeta> {
    (any::<bool>(), any::<bool>()).prop_map(|(safesearch, time_range_support)| EngineMeta {
        name: "eng".to_string(),
        safesearch,
        time_range_support,
        ..EngineMeta::default()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_filter_propagation_to_supporting_engines(
        base in base_view_strategy(),
        meta in meta_strategy(),
    ) {
        let tailored = engine_query_view(&base, &meta);

        if meta.safesearch {
            prop_assert_eq!(tailored.safesearch, base.safesearch);
        } else {
            prop_assert_eq!(tailored.safesearch, SafeSearch::Off);
        }

        if meta.time_range_support {
            prop_assert_eq!(tailored.time_range, base.time_range);
        } else {
            prop_assert_eq!(tailored.time_range, None);
        }

        prop_assert_eq!(&tailored.query, &base.query);
        prop_assert_eq!(tailored.pageno, base.pageno);
        prop_assert_eq!(&tailored.locale, &base.locale);
        prop_assert_eq!(&tailored.categories, &base.categories);
        prop_assert_eq!(&tailored.engines, &base.engines);
    }
}
