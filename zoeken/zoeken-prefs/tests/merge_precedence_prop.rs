//! Property test: preferences resolve respects merge precedence order.

use std::collections::{BTreeMap, HashSet};

use proptest::prelude::*;
use zoeken_prefs::{Preferences, RequestMethod, encode_cookie, resolve};
use zoeken_query::{FormParams, SafeSearch};
use zoeken_settings::{EngineSettings, Settings};

const KEYS: &[&str] = &[
    "theme",
    "locale",
    "safesearch",
    "engines",
    "image_proxy",
    "method",
];

#[derive(Debug, Clone)]
struct SettingsSpec {
    theme: String,
    default_locale: String,
    default_lang: String,
    safe_search: u8,
    image_proxy: bool,
    method: String,
    engines: Vec<(String, bool)>,
}

#[derive(Debug, Clone)]
struct ParamSpec {
    theme: Option<String>,
    locale: Option<(bool, String)>,
    safesearch: Option<String>,
    engines: Option<String>,
    image_proxy: Option<String>,
    method: Option<String>,
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

/// Mirror of `zoeken_prefs::parse_bool`.
fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

fn arb_theme() -> impl Strategy<Value = String> {
    prop::sample::select(vec!["simple", "dark", "light", "custom"]).prop_map(String::from)
}

fn arb_locale() -> impl Strategy<Value = String> {
    prop::sample::select(vec!["all", "en", "fr", "de", "en-US", "auto"]).prop_map(String::from)
}

fn arb_safesearch() -> impl Strategy<Value = SafeSearch> {
    prop::sample::select(vec![
        SafeSearch::Off,
        SafeSearch::Moderate,
        SafeSearch::Strict,
    ])
}

fn arb_method() -> impl Strategy<Value = RequestMethod> {
    prop::sample::select(vec![RequestMethod::Get, RequestMethod::Post])
}

fn arb_engine_names() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(
        prop::sample::select(vec!["duckduckgo", "brave", "google", "wikipedia"])
            .prop_map(String::from),
        0..4,
    )
}

fn arb_prefs() -> impl Strategy<Value = Preferences> {
    (
        arb_theme(),
        arb_locale(),
        arb_engine_names(),
        arb_safesearch(),
        any::<bool>(),
        arb_method(),
    )
        .prop_map(
            |(theme, locale, engines, safesearch, image_proxy, method)| Preferences {
                theme,
                language: locale.clone(),
                locale,
                categories: vec!["general".to_string()],
                engines,
                safesearch,
                autocomplete: String::new(),
                image_proxy,
                method,
                plugins: BTreeMap::new(),
                locked: HashSet::new(),
            },
        )
}

fn arb_settings_spec() -> impl Strategy<Value = SettingsSpec> {
    (
        prop::sample::select(vec!["", "simple", "settings-theme", "dark"]).prop_map(String::from),
        prop::sample::select(vec!["", "es", "it", "settings-loc"]).prop_map(String::from),
        prop::sample::select(vec!["", "pt", "nl"]).prop_map(String::from),
        0u8..=4,
        any::<bool>(),
        prop::sample::select(vec!["GET", "POST", "get", "put", ""]).prop_map(String::from),
        prop::collection::vec(
            (
                prop::sample::select(vec!["duckduckgo", "brave", "google", "wikipedia"])
                    .prop_map(String::from),
                any::<bool>(),
            ),
            0..4,
        ),
    )
        .prop_map(
            |(theme, default_locale, default_lang, safe_search, image_proxy, method, engines)| {
                SettingsSpec {
                    theme,
                    default_locale,
                    default_lang,
                    safe_search,
                    image_proxy,
                    method,
                    engines,
                }
            },
        )
}

fn arb_param_spec() -> impl Strategy<Value = ParamSpec> {
    (
        prop::option::of(prop::sample::select(vec!["dark", "light", ""]).prop_map(String::from)),
        prop::option::of((
            any::<bool>(),
            prop::sample::select(vec!["en", "fr", "de", "en-US", ""]).prop_map(String::from),
        )),
        prop::option::of(
            prop::sample::select(vec!["0", "1", "2", "3", "x", ""]).prop_map(String::from),
        ),
        prop::option::of(
            prop::sample::select(vec!["duckduckgo,brave", "", "a, b ,,c", "google"])
                .prop_map(String::from),
        ),
        prop::option::of(
            prop::sample::select(vec!["1", "true", "on", "0", "false", "no", "maybe", ""])
                .prop_map(String::from),
        ),
        prop::option::of(
            prop::sample::select(vec!["GET", "POST", "get", "put", ""]).prop_map(String::from),
        ),
    )
        .prop_map(
            |(theme, locale, safesearch, engines, image_proxy, method)| ParamSpec {
                theme,
                locale,
                safesearch,
                engines,
                image_proxy,
                method,
            },
        )
}

fn arb_locked() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(
        prop::sample::select(KEYS.to_vec()).prop_map(String::from),
        0..4,
    )
}

fn build_settings(spec: &SettingsSpec, locked: &[String]) -> Settings {
    let mut settings = Settings::defaults();
    settings.ui.default_theme = spec.theme.clone();
    settings.ui.default_locale = spec.default_locale.clone();
    settings.search.default_lang = spec.default_lang.clone();
    settings.search.safe_search = spec.safe_search;
    settings.server.image_proxy = spec.image_proxy;
    settings.server.method = spec.method.clone();
    settings.engines = spec
        .engines
        .iter()
        .map(|(name, disabled)| EngineSettings {
            name: name.clone(),
            disabled: Some(*disabled),
            ..Default::default()
        })
        .collect();
    settings.preferences.lock = locked.to_vec();
    settings
}

fn build_params(spec: &ParamSpec) -> FormParams {
    let mut entries: Vec<(String, String)> = Vec::new();
    if let Some(v) = &spec.theme {
        entries.push(("theme".to_string(), v.clone()));
    }
    if let Some((use_language, v)) = &spec.locale {
        let key = if *use_language { "language" } else { "locale" };
        entries.push((key.to_string(), v.clone()));
    }
    if let Some(v) = &spec.safesearch {
        entries.push(("safesearch".to_string(), v.clone()));
    }
    if let Some(v) = &spec.engines {
        entries.push(("engines".to_string(), v.clone()));
    }
    if let Some(v) = &spec.image_proxy {
        entries.push(("image_proxy".to_string(), v.clone()));
    }
    if let Some(v) = &spec.method {
        entries.push(("method".to_string(), v.clone()));
    }
    FormParams::from_pairs(entries)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn resolve_takes_highest_precedence_layer_that_defines_each_field(
        defaults in arb_prefs(),
        cookie_prefs in prop::option::of(arb_prefs()),
        settings_spec in arb_settings_spec(),
        param_spec in arb_param_spec(),
        locked_vec in arb_locked(),
    ) {
        let settings = build_settings(&settings_spec, &locked_vec);
        let params = build_params(&param_spec);

        let cookie_string = cookie_prefs.as_ref().map(encode_cookie);
        let resolved = resolve(&defaults, &settings, cookie_string.as_deref(), &params);

        let locked: HashSet<&str> = locked_vec.iter().map(String::as_str).collect();

        let mut theme = defaults.theme.clone();
        if !settings_spec.theme.is_empty() {
            theme = settings_spec.theme.clone();
        }
        if let Some(c) = &cookie_prefs {
            theme = c.theme.clone();
        }
        if !locked.contains("theme")
            && let Some(v) = &param_spec.theme
            && !v.is_empty()
        {
            theme = v.clone();
        }
        prop_assert_eq!(&resolved.theme, &theme, "theme precedence mismatch");

        let mut locale = defaults.locale.clone();
        if !settings_spec.default_locale.is_empty() {
            locale = settings_spec.default_locale.clone();
        }
        if let Some(c) = &cookie_prefs {
            locale = c.locale.clone();
        }
        if !locked.contains("locale")
            && let Some((false, v)) = &param_spec.locale
            && !v.is_empty()
        {
            locale = v.clone();
        }
        prop_assert_eq!(&resolved.locale, &locale, "locale precedence mismatch");

        let mut language = defaults.language.clone();
        if !settings_spec.default_lang.is_empty() {
            language = settings_spec.default_lang.clone();
        }
        if let Some(c) = &cookie_prefs {
            language = c.language.clone();
        }
        if !locked.contains("language")
            && let Some((true, v)) = &param_spec.locale
            && !v.is_empty()
        {
            language = v.clone();
        }
        prop_assert_eq!(&resolved.language, &language, "language precedence mismatch");

        let mut safesearch = defaults.safesearch;
        if let Some(level) = SafeSearch::from_u8(settings_spec.safe_search) {
            safesearch = level;
        }
        if let Some(c) = &cookie_prefs {
            safesearch = c.safesearch;
        }
        if !locked.contains("safesearch")
            && let Some(v) = &param_spec.safesearch
            && let Some(level) = v.parse::<u8>().ok().and_then(SafeSearch::from_u8)
        {
            safesearch = level;
        }
        prop_assert_eq!(resolved.safesearch, safesearch, "safesearch precedence mismatch");

        let mut image_proxy = settings_spec.image_proxy;
        if let Some(c) = &cookie_prefs {
            image_proxy = c.image_proxy;
        }
        if !locked.contains("image_proxy")
            && let Some(v) = &param_spec.image_proxy
            && let Some(b) = parse_bool(v)
        {
            image_proxy = b;
        }
        prop_assert_eq!(resolved.image_proxy, image_proxy, "image_proxy precedence mismatch");

        let mut method = defaults.method;
        if let Some(m) = RequestMethod::parse(&settings_spec.method) {
            method = m;
        }
        if let Some(c) = &cookie_prefs {
            method = c.method;
        }
        if !locked.contains("method")
            && let Some(v) = &param_spec.method
            && let Some(m) = RequestMethod::parse(v)
        {
            method = m;
        }
        prop_assert_eq!(resolved.method, method, "method precedence mismatch");

        let enabled: Vec<String> = settings_spec
            .engines
            .iter()
            .filter(|(_, disabled)| !*disabled)
            .map(|(name, _)| name.clone())
            .collect();
        let mut engines = defaults.engines.clone();
        if !enabled.is_empty() {
            engines = enabled;
        }
        if let Some(c) = &cookie_prefs {
            engines = c.engines.clone();
        }
        if !locked.contains("engines")
            && let Some(v) = &param_spec.engines
        {
            engines = split_csv(v);
        }
        prop_assert_eq!(&resolved.engines, &engines, "engines precedence mismatch");
    }
}
