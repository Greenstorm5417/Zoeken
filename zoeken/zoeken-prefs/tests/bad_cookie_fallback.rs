//! Tests for bad-cookie fallback: malformed cookies are silently ignored.

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE;

use zoeken_prefs::{Preferences, PrefsError, RequestMethod, decode_cookie, resolve};
use zoeken_query::{FormParams, SafeSearch};
use zoeken_settings::Settings;

fn customized_settings() -> Settings {
    let mut settings = Settings::default();
    settings.ui.default_theme = "settings-theme".to_string();
    settings.search.safe_search = 1; // Moderate
    settings
}

fn malformed_cookies() -> Vec<String> {
    vec![
        "@@invalid@@".to_string(),
        "!!!not base64!!!".to_string(),
        URL_SAFE.encode(b"this is not a compressed prefs payload"),
    ]
}

#[test]
fn bad_cookie_resolves_same_as_no_cookie() {
    let defaults = Preferences::defaults();
    let settings = customized_settings();

    let baseline = resolve(&defaults, &settings, None, &FormParams::default());

    for bad in malformed_cookies() {
        let resolved = resolve(&defaults, &settings, Some(&bad), &FormParams::default());
        assert_eq!(
            resolved, baseline,
            "malformed cookie {bad:?} should be ignored and fall back to defaults+settings"
        );
    }
}

#[test]
fn bad_cookie_fallback_still_applies_settings_layer() {
    let defaults = Preferences::defaults();
    let settings = customized_settings();

    let resolved = resolve(
        &defaults,
        &settings,
        Some("@@invalid@@"),
        &FormParams::default(),
    );

    assert_eq!(resolved.theme, "settings-theme");
    assert_eq!(resolved.safesearch, SafeSearch::Moderate);
}

#[test]
fn decode_cookie_rejects_malformed_values() {
    for bad in malformed_cookies() {
        let result = decode_cookie(&bad);
        assert!(
            matches!(result, Err(PrefsError::DecodeFailed(_))),
            "malformed cookie {bad:?} should decode to Err(PrefsError::DecodeFailed), got {result:?}"
        );
    }
}

#[test]
fn params_still_apply_on_top_of_bad_cookie() {
    let defaults = Preferences::defaults();
    let settings = customized_settings();

    let params = FormParams::from_pairs([
        ("locale".to_string(), "es".to_string()),
        ("safesearch".to_string(), "2".to_string()),
        ("engines".to_string(), "duckduckgo,brave".to_string()),
        ("method".to_string(), "GET".to_string()),
    ]);

    let resolved = resolve(&defaults, &settings, Some("@@invalid@@"), &params);

    assert_eq!(resolved.locale, "es");
    assert_eq!(resolved.safesearch, SafeSearch::Strict);
    assert_eq!(resolved.engines, vec!["duckduckgo", "brave"]);
    assert_eq!(resolved.method, RequestMethod::Get);

    let expected = resolve(&defaults, &settings, None, &params);
    assert_eq!(resolved, expected);

    assert_eq!(resolved.theme, "settings-theme");
}
