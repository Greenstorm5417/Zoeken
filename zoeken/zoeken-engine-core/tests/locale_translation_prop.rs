//! Property test for trait-based locale translation.
//!
//! Checks that resolved values stay within the engine's supported sets or the
//! documented fallbacks.

use std::collections::{HashMap, HashSet};

use proptest::prelude::*;
use zoeken_data::EngineTraits;
use zoeken_engine_core::LocaleTranslate;

const LANGS: &[&str] = &["en", "fr", "de", "zh", "es", "pt", "xx"];
const TERRS: &[&str] = &["US", "GB", "FR", "BE", "CA", "HK", "YY"];

fn arb_locale_tag() -> impl Strategy<Value = String> {
    (
        prop::sample::select(LANGS.to_vec()),
        prop::option::of(prop::sample::select(TERRS.to_vec())),
        prop::sample::select(vec!["-", "_"]),
    )
        .prop_map(|(lang, terr, sep)| match terr {
            Some(t) => format!("{lang}{sep}{t}"),
            None => lang.to_string(),
        })
}

fn arb_engine_value() -> impl Strategy<Value = String> {
    "[a-z]{1,3}(_[A-Z]{1,3})?"
}

fn arb_locale_map() -> impl Strategy<Value = HashMap<String, String>> {
    prop::collection::vec((arb_locale_tag(), arb_engine_value()), 0..12)
        .prop_map(|pairs| pairs.into_iter().collect())
}

fn arb_user_locale() -> impl Strategy<Value = String> {
    prop_oneof![Just("all".to_string()), arb_locale_tag(), "[a-zA-Z_-]{0,8}",]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    /// Checks locale translation stays within supported values or fallbacks.
    #[test]
    fn locale_translation_stays_within_engine_support(
        languages in arb_locale_map(),
        regions in arb_locale_map(),
        all_locale in prop::option::of(arb_engine_value()),
        locale in arb_user_locale(),
        default in prop::option::of(arb_engine_value()),
    ) {
        let traits = EngineTraits {
            all_locale: all_locale.clone(),
            data_type: Some("traits_v1".to_string()),
            languages: languages.clone(),
            regions: regions.clone(),
            custom: serde_json::Value::Null,
        };

        let supported_languages: HashSet<&String> = languages.values().collect();
        let supported_regions: HashSet<&String> = regions.values().collect();

        if let Some(lang) = traits.get_language(&locale, default.as_deref()) {
            let within_support = supported_languages.contains(&lang);
            let is_all_fallback = locale == "all" && all_locale.as_ref() == Some(&lang);
            let is_default_fallback = default.as_deref() == Some(lang.as_str());
            prop_assert!(
                within_support || is_all_fallback || is_default_fallback,
                "language {lang:?} for locale {locale:?} is neither supported \
                 ({supported_languages:?}), the all-locale fallback ({all_locale:?}), \
                 nor the default ({default:?})",
            );
        }

        if let Some(region) = traits.get_region(&locale, default.as_deref()) {
            let within_support = supported_regions.contains(&region);
            let is_all_fallback = locale == "all" && all_locale.as_ref() == Some(&region);
            let is_default_fallback = default.as_deref() == Some(region.as_str());
            prop_assert!(
                within_support || is_all_fallback || is_default_fallback,
                "region {region:?} for locale {locale:?} is neither supported \
                 ({supported_regions:?}), the all-locale fallback ({all_locale:?}), \
                 nor the default ({default:?})",
            );
        }
    }
}
