//! Typed preferences, cookie codec, and merge resolver.
//! Encodes/decodes preferences to base64url(zlib(json)) and resolves the effective
//! preference across defaults, settings, cookie, and request params layers.

use std::collections::{BTreeMap, HashSet};
use std::io::{Read, Write};

use base64::Engine as _;
use base64::engine::general_purpose::URL_SAFE;
use flate2::Compression;
use flate2::read::ZlibDecoder;
use flate2::write::ZlibEncoder;
use serde::{Deserialize, Serialize};
use zoeken_data::{DataBundle, LocaleMap, detect_language};

use zoeken_query::{FormParams, PreferencesView, SafeSearch};
use zoeken_settings::Settings;

const MAX_COMPRESSED_COOKIE_BYTES: usize = 16 * 1024;
const MAX_DECOMPRESSED_COOKIE_BYTES: usize = 64 * 1024;

/// HTTP method preference: `GET` or `POST`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "UPPERCASE")]
pub enum RequestMethod {
    Get,
    #[default]
    Post,
}

impl RequestMethod {
    /// The uppercase HTTP method token (`GET` or `POST`).
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            RequestMethod::Get => "GET",
            RequestMethod::Post => "POST",
        }
    }

    /// Parse an HTTP method token case-insensitively; unknown tokens yield `None`.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().to_ascii_uppercase().as_str() {
            "GET" => Some(RequestMethod::Get),
            "POST" => Some(RequestMethod::Post),
            _ => None,
        }
    }
}

/// Typed user preferences: theme, locale, categories, engines, safesearch, autocomplete, image_proxy, method.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Preferences {
    pub theme: String,
    pub locale: String,
    pub language: String,
    pub categories: Vec<String>,
    pub engines: Vec<String>,
    pub safesearch: SafeSearch,
    pub autocomplete: String,
    pub image_proxy: bool,
    pub method: RequestMethod,
    pub plugins: BTreeMap<String, bool>,
    #[serde(skip)]
    pub locked: HashSet<String>,
}

impl Default for Preferences {
    fn default() -> Self {
        Preferences {
            theme: "simple".to_string(),
            locale: "all".to_string(),
            language: "all".to_string(),
            categories: vec!["general".to_string()],
            engines: Vec::new(),
            safesearch: SafeSearch::Off,
            autocomplete: "brave".to_string(),
            image_proxy: false,
            method: RequestMethod::Post,
            plugins: BTreeMap::new(),
            locked: HashSet::new(),
        }
    }
}

impl Preferences {
    /// The built-in defaults layer. Equivalent to [`Preferences::default`].
    #[must_use]
    pub fn defaults() -> Self {
        Self::default()
    }
}

impl PreferencesView for Preferences {
    fn is_locked(&self, key: &str) -> bool {
        self.locked.contains(key)
    }
    fn default_language(&self) -> String {
        self.language.clone()
    }
    fn default_safesearch(&self) -> u8 {
        self.safesearch.as_u8()
    }
    fn default_categories(&self) -> Vec<String> {
        self.categories.clone()
    }
}

impl Preferences {
    /// Whether `engine` is enabled.
    #[must_use]
    pub fn is_engine_enabled(&self, engine: &str) -> bool {
        self.engines.iter().any(|e| e == engine)
    }
}

/// Error decoding a preferences cookie.
#[derive(Debug, thiserror::Error)]
pub enum PrefsError {
    #[error("failed to decode preferences cookie: {0}")]
    DecodeFailed(String),
}

/// Encode preferences into base64url(zlib(json)) cookie value.
#[must_use]
pub fn encode_cookie(prefs: &Preferences) -> String {
    // JSON is infallible for this struct (no maps with non-string keys, no
    // types that can fail to serialize), so unwrap is safe here.
    let json = serde_json::to_vec(prefs).expect("Preferences serializes to JSON");

    let mut encoder = ZlibEncoder::new(Vec::new(), Compression::default());
    encoder
        .write_all(&json)
        .expect("writing to an in-memory zlib encoder cannot fail");
    let compressed = encoder
        .finish()
        .expect("finishing an in-memory zlib encoder cannot fail");

    URL_SAFE.encode(compressed)
}

/// Decode a cookie value to typed preferences.
pub fn decode_cookie(value: &str) -> Result<Preferences, PrefsError> {
    let compressed = URL_SAFE
        .decode(value.trim())
        .map_err(|e| PrefsError::DecodeFailed(format!("invalid base64url: {e}")))?;
    if compressed.len() > MAX_COMPRESSED_COOKIE_BYTES {
        return Err(PrefsError::DecodeFailed(
            "compressed preferences exceed size limit".to_string(),
        ));
    }

    let decoder = ZlibDecoder::new(&compressed[..]);
    let mut json = Vec::new();
    decoder
        .take((MAX_DECOMPRESSED_COOKIE_BYTES + 1) as u64)
        .read_to_end(&mut json)
        .map_err(|e| PrefsError::DecodeFailed(format!("invalid compressed payload: {e}")))?;
    if json.len() > MAX_DECOMPRESSED_COOKIE_BYTES {
        return Err(PrefsError::DecodeFailed(
            "decompressed preferences exceed size limit".to_string(),
        ));
    }

    serde_json::from_slice(&json)
        .map_err(|e| PrefsError::DecodeFailed(format!("invalid preferences json: {e}")))
}

/// Resolve effective preferences merging defaults, settings, cookie, and params.
/// Later layers override earlier ones; locked keys in settings are skipped.
#[must_use]
pub fn resolve(
    defaults: &Preferences,
    settings: &Settings,
    cookie: Option<&str>,
    params: &FormParams,
) -> Preferences {
    resolve_inner(defaults, settings, cookie, params, None)
}

#[must_use]
pub fn resolve_with_data(
    defaults: &Preferences,
    settings: &Settings,
    cookie: Option<&str>,
    params: &FormParams,
    data: &DataBundle,
) -> Preferences {
    resolve_inner(defaults, settings, cookie, params, Some(data))
}

fn resolve_inner(
    defaults: &Preferences,
    settings: &Settings,
    cookie: Option<&str>,
    params: &FormParams,
    data: Option<&DataBundle>,
) -> Preferences {
    let mut prefs = defaults.clone();

    apply_settings(&mut prefs, settings);

    let locked: HashSet<&str> = settings
        .preferences
        .lock
        .iter()
        .map(String::as_str)
        .collect();

    if let Some(raw) = cookie
        && let Ok(decoded) = decode_cookie(raw)
    {
        apply_cookie(&mut prefs, decoded, &locked);
    }

    apply_params(&mut prefs, params, &locked);
    prefs.locked = locked.iter().map(|key| (*key).to_string()).collect();
    if let Some(data) = data {
        apply_data(&mut prefs, params, data);
    }

    prefs
}

fn apply_cookie(prefs: &mut Preferences, decoded: Preferences, locked: &HashSet<&str>) {
    if !locked.contains("theme") {
        prefs.theme = decoded.theme;
    }
    if !locked.contains("locale") {
        prefs.locale = decoded.locale;
    }
    if !locked.contains("language") {
        prefs.language = decoded.language;
    }
    if !locked.contains("categories") {
        prefs.categories = decoded.categories;
    }
    if !locked.contains("engines") {
        prefs.engines = decoded.engines;
    }
    if !locked.contains("safesearch") {
        prefs.safesearch = decoded.safesearch;
    }
    if !locked.contains("autocomplete") {
        prefs.autocomplete = decoded.autocomplete;
    }
    if !locked.contains("image_proxy") {
        prefs.image_proxy = decoded.image_proxy;
    }
    if !locked.contains("method") {
        prefs.method = decoded.method;
    }
    if !locked.contains("plugins") {
        prefs.plugins = decoded.plugins;
    }
}

fn apply_settings(prefs: &mut Preferences, settings: &Settings) {
    if !settings.ui.default_theme.is_empty() {
        prefs.theme = settings.ui.default_theme.clone();
    }

    if !settings.ui.default_locale.is_empty() {
        prefs.locale = settings.ui.default_locale.clone();
    }
    if !settings.search.default_lang.is_empty() {
        prefs.language = settings.search.default_lang.clone();
    }

    if let Some(level) = SafeSearch::from_u8(settings.search.safe_search) {
        prefs.safesearch = level;
    }

    prefs.autocomplete = settings.search.autocomplete.clone();
    prefs.image_proxy = settings.server.image_proxy;

    if let Some(method) = RequestMethod::parse(&settings.server.method) {
        prefs.method = method;
    }

    prefs.plugins = settings
        .plugins
        .0
        .iter()
        .filter_map(|(id, entry)| entry.active.map(|active| (normalize_plugin_id(id), active)))
        .collect();

    let enabled: Vec<String> = settings
        .engines
        .iter()
        .filter(|e| e.disabled != Some(true) && e.inactive != Some(true))
        .map(|e| e.engine.clone().unwrap_or_else(|| e.name.clone()))
        .collect();
    if !enabled.is_empty() {
        prefs.engines = enabled;
    }
}

fn apply_params(prefs: &mut Preferences, params: &FormParams, locked: &HashSet<&str>) {
    if !locked.contains("theme")
        && let Some(v) = params.get("theme")
        && !v.is_empty()
    {
        prefs.theme = v.to_string();
    }

    if !locked.contains("locale")
        && let Some(v) = params.get("locale")
        && !v.is_empty()
    {
        prefs.locale = v.to_string();
    }

    if !locked.contains("language")
        && let Some(v) = params.get("language")
        && !v.is_empty()
    {
        prefs.language = v.to_string();
    }

    if !locked.contains("categories")
        && let Some(v) = params.get("categories")
    {
        prefs.categories = split_csv(v);
    }

    if !locked.contains("engines")
        && let Some(v) = params.get("engines")
    {
        prefs.engines = split_csv(v);
    }

    if !locked.contains("safesearch")
        && let Some(v) = params.get("safesearch")
        && let Some(level) = v.parse::<u8>().ok().and_then(SafeSearch::from_u8)
    {
        prefs.safesearch = level;
    }

    if !locked.contains("autocomplete")
        && let Some(v) = params.get("autocomplete")
    {
        prefs.autocomplete = v.to_string();
    }

    if !locked.contains("image_proxy")
        && let Some(v) = params.get("image_proxy")
        && let Some(b) = parse_bool(v)
    {
        prefs.image_proxy = b;
    }

    if !locked.contains("method")
        && let Some(v) = params.get("method")
        && let Some(method) = RequestMethod::parse(v)
    {
        prefs.method = method;
    }

    if !locked.contains("plugins")
        && let Some(v) = params
            .get("enabled_plugins")
            .or_else(|| params.get("plugins"))
    {
        prefs.plugins = split_csv(v)
            .into_iter()
            .map(|id| (normalize_plugin_id(&id), true))
            .collect();
    }

    for (key, value) in params.iter() {
        let Some(id) = key.strip_prefix("plugin_") else {
            continue;
        };
        if locked.contains("plugins") {
            continue;
        }
        if let Some(enabled) = parse_bool(value) {
            prefs.plugins.insert(normalize_plugin_id(id), enabled);
        }
    }
}

fn normalize_plugin_id(id: &str) -> String {
    let id = id.trim();
    let class_stripped = id.strip_suffix(".SXNGPlugin").unwrap_or(id);
    let short = class_stripped
        .strip_suffix(".plugin")
        .unwrap_or(class_stripped)
        .rsplit('.')
        .next()
        .unwrap_or(class_stripped)
        .replace('-', "_");
    // camelCase / PascalCase → snake_case (e.g. infiniteScroll → infinite_scroll)
    let mut out = String::with_capacity(short.len() + 4);
    for (i, ch) in short.chars().enumerate() {
        if ch.is_ascii_uppercase() {
            if i > 0 && !out.ends_with('_') {
                out.push('_');
            }
            out.push(ch.to_ascii_lowercase());
        } else {
            out.push(ch);
        }
    }
    out
}

fn apply_data(prefs: &mut Preferences, params: &FormParams, data: &DataBundle) {
    if let Some(locale) = normalize_locale_preference(&data.locales, &prefs.locale) {
        prefs.locale = locale;
    }

    if prefs.language == "auto"
        && let Some(query) = params.get("q")
        && let Some(detected) = detect_language(query)
    {
        prefs.language = detected.into_string();
    }

    if let Some(language) = normalize_language_preference(&data.locales, &prefs.language) {
        prefs.language = language;
    }
}

fn normalize_locale_preference(locales: &LocaleMap, value: &str) -> Option<String> {
    if locales.locale_names.is_empty() {
        let trimmed = value.trim();
        return (!trimmed.is_empty()).then(|| trimmed.to_string());
    }
    locales.normalize_supported(value)
}

fn normalize_language_preference(locales: &LocaleMap, value: &str) -> Option<String> {
    normalize_locale_preference(locales, value).or_else(|| {
        language_alias(value).and_then(|alias| normalize_locale_preference(locales, alias))
    })
}

fn language_alias(value: &str) -> Option<&'static str> {
    match value {
        "eng" => Some("en"),
        "fra" => Some("fr"),
        "fre" => Some("fr"),
        "deu" => Some("de"),
        "ger" => Some("de"),
        "spa" => Some("es"),
        "ita" => Some("it"),
        "por" => Some("pt"),
        "rus" => Some("ru"),
        "jpn" => Some("ja"),
        "zho" | "cmn" => Some("zh"),
        "ara" => Some("ar"),
        "ell" | "gre" => Some("el"),
        "nld" | "dut" => Some("nl"),
        "pol" => Some("pl"),
        "tur" => Some("tr"),
        _ => None,
    }
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

fn parse_bool(value: &str) -> Option<bool> {
    match value.trim().to_ascii_lowercase().as_str() {
        "1" | "true" | "on" | "yes" => Some(true),
        "0" | "false" | "off" | "no" => Some(false),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoeken_data::{DataBundle, LocaleMap};

    fn sample() -> Preferences {
        Preferences {
            theme: "simple".to_string(),
            locale: "en-US".to_string(),
            language: "en".to_string(),
            categories: vec!["general".to_string(), "images".to_string()],
            engines: vec!["duckduckgo".to_string(), "wikipedia".to_string()],
            safesearch: SafeSearch::Moderate,
            autocomplete: "duckduckgo".to_string(),
            image_proxy: true,
            method: RequestMethod::Get,
            plugins: BTreeMap::new(),
            locked: HashSet::new(),
        }
    }

    #[test]
    fn encode_decode_round_trip() {
        let prefs = sample();
        let encoded = encode_cookie(&prefs);
        let decoded = decode_cookie(&encoded).expect("valid cookie decodes");
        assert_eq!(decoded, prefs);
    }

    #[test]
    fn encoded_cookie_is_url_safe() {
        let encoded = encode_cookie(&sample());
        assert!(!encoded.contains('+'));
        assert!(!encoded.contains('/'));
    }

    #[test]
    fn default_preferences_round_trip() {
        let prefs = Preferences::default();
        let decoded = decode_cookie(&encode_cookie(&prefs)).expect("decodes");
        assert_eq!(decoded, prefs);
    }

    #[test]
    fn decode_rejects_bad_base64() {
        let err = decode_cookie("!!!not base64!!!");
        assert!(matches!(err, Err(PrefsError::DecodeFailed(_))));
    }

    #[test]
    fn decode_rejects_bad_compression() {
        let garbage = URL_SAFE.encode(b"this is not zlib compressed data");
        assert!(matches!(
            decode_cookie(&garbage),
            Err(PrefsError::DecodeFailed(_))
        ));
    }

    #[test]
    fn resolve_uses_settings_over_defaults() {
        let defaults = Preferences::defaults();
        let mut settings = Settings::defaults();
        settings.ui.default_theme = "custom".to_string();
        settings.ui.default_locale = "de".to_string();
        settings.search.default_lang = "de-DE".to_string();
        settings.search.safe_search = 2;
        settings.server.image_proxy = true;
        settings.server.method = "GET".to_string();

        let resolved = resolve(&defaults, &settings, None, &FormParams::default());
        assert_eq!(resolved.theme, "custom");
        assert_eq!(resolved.locale, "de");
        assert_eq!(resolved.language, "de-DE");
        assert_eq!(resolved.safesearch, SafeSearch::Strict);
        assert!(resolved.image_proxy);
        assert_eq!(resolved.method, RequestMethod::Get);
    }

    #[test]
    fn resolve_cookie_overrides_settings() {
        let defaults = Preferences::defaults();
        let mut settings = Settings::defaults();
        settings.ui.default_theme = "custom".to_string();

        let cookie_prefs = Preferences {
            theme: "from-cookie".to_string(),
            locale: "fr".to_string(),
            ..Preferences::default()
        };
        let cookie = encode_cookie(&cookie_prefs);

        let resolved = resolve(&defaults, &settings, Some(&cookie), &FormParams::default());
        assert_eq!(resolved.theme, "from-cookie");
        assert_eq!(resolved.locale, "fr");
    }

    #[test]
    fn resolve_params_override_cookie() {
        let defaults = Preferences::defaults();
        let settings = Settings::defaults();

        let cookie_prefs = Preferences {
            locale: "fr".to_string(),
            safesearch: SafeSearch::Off,
            ..Preferences::default()
        };
        let cookie = encode_cookie(&cookie_prefs);

        let params = FormParams::from_pairs([
            ("locale".to_string(), "es".to_string()),
            ("safesearch".to_string(), "2".to_string()),
            ("engines".to_string(), "duckduckgo,brave".to_string()),
        ]);

        let resolved = resolve(&defaults, &settings, Some(&cookie), &params);
        assert_eq!(resolved.locale, "es");
        assert_eq!(resolved.safesearch, SafeSearch::Strict);
        assert_eq!(resolved.engines, vec!["duckduckgo", "brave"]);
    }

    #[test]
    fn resolve_params_support_language_and_plugins() {
        let defaults = Preferences::defaults();
        let settings = Settings::defaults();
        let params = FormParams::from_pairs([
            ("language".to_string(), "fr-BE".to_string()),
            (
                "enabled_plugins".to_string(),
                "hash_plugin,self_info".to_string(),
            ),
            ("plugin_hash_plugin".to_string(), "0".to_string()),
        ]);

        let resolved = resolve(&defaults, &settings, None, &params);
        assert_eq!(resolved.language, "fr-BE");
        assert_eq!(resolved.plugins.get("self_info"), Some(&true));
        assert_eq!(resolved.plugins.get("hash_plugin"), Some(&false));
    }

    #[test]
    fn resolve_with_data_normalizes_locale_and_detects_auto_language() {
        let data = DataBundle {
            locales: LocaleMap::from_owned(
                [
                    ("en".to_string(), "English".to_string()),
                    ("fr".to_string(), "French".to_string()),
                ]
                .into_iter()
                .collect(),
                Vec::new(),
            ),
            ..Default::default()
        };
        let mut settings = Settings::defaults();
        settings.ui.default_locale = "fr-FR".to_string();
        settings.search.default_lang = "auto".to_string();
        let params = FormParams::from_pairs([(
            "q".to_string(),
            "This is a simple sentence written in English".to_string(),
        )]);

        let resolved = resolve_with_data(&Preferences::defaults(), &settings, None, &params, &data);

        assert_eq!(resolved.locale, "fr");
        assert_eq!(resolved.language, "en");
    }

    #[test]
    fn resolve_full_precedence_chain() {
        let mut defaults = Preferences::defaults();
        defaults.locale = "default-loc".to_string();

        let mut settings = Settings::defaults();
        settings.ui.default_locale = "settings-loc".to_string();

        let cookie = encode_cookie(&Preferences {
            locale: "cookie-loc".to_string(),
            ..Preferences::default()
        });

        let params = FormParams::from_pairs([("locale".to_string(), "params-loc".to_string())]);

        let resolved = resolve(&defaults, &settings, Some(&cookie), &params);
        assert_eq!(resolved.locale, "params-loc");
    }

    #[test]
    fn resolve_locked_key_ignores_param() {
        let defaults = Preferences::defaults();
        let mut settings = Settings::defaults();
        settings.search.safe_search = 2;
        settings.preferences.lock = vec!["safesearch".to_string()];

        let params = FormParams::from_pairs([("safesearch".to_string(), "0".to_string())]);

        let resolved = resolve(&defaults, &settings, None, &params);
        assert_eq!(resolved.safesearch, SafeSearch::Strict);
    }

    #[test]
    fn resolve_locked_key_ignores_cookie() {
        let defaults = Preferences::defaults();
        let mut settings = Settings::defaults();
        settings.search.safe_search = 2;
        settings.preferences.lock = vec!["safesearch".to_string()];
        let cookie = encode_cookie(&Preferences {
            safesearch: SafeSearch::Off,
            ..Preferences::default()
        });

        let resolved = resolve(&defaults, &settings, Some(&cookie), &FormParams::default());
        assert_eq!(resolved.safesearch, SafeSearch::Strict);
    }

    #[test]
    fn resolve_bad_cookie_falls_back_to_defaults_and_settings() {
        let defaults = Preferences::defaults();
        let mut settings = Settings::defaults();
        settings.ui.default_theme = "settings-theme".to_string();
        settings.search.safe_search = 1;

        let resolved = resolve(
            &defaults,
            &settings,
            Some("@@ totally invalid cookie @@"),
            &FormParams::default(),
        );

        let expected = resolve(&defaults, &settings, None, &FormParams::default());
        assert_eq!(resolved, expected);
        assert_eq!(resolved.theme, "settings-theme");
        assert_eq!(resolved.safesearch, SafeSearch::Moderate);
    }

    #[test]
    fn is_engine_enabled_reflects_engines_list() {
        let prefs = Preferences {
            engines: vec!["alpha".to_string()],
            ..Preferences::default()
        };
        assert!(prefs.is_engine_enabled("alpha"));
        assert!(!prefs.is_engine_enabled("beta"));
    }

    #[test]
    fn request_method_parse_is_case_insensitive() {
        assert_eq!(RequestMethod::parse("get"), Some(RequestMethod::Get));
        assert_eq!(RequestMethod::parse("Post"), Some(RequestMethod::Post));
        assert_eq!(RequestMethod::parse("put"), None);
    }

    #[test]
    fn normalize_plugin_id_snake_cases_camel() {
        assert_eq!(normalize_plugin_id("infiniteScroll"), "infinite_scroll");
        assert_eq!(normalize_plugin_id("oa_doi_rewrite"), "oa_doi_rewrite");
        assert_eq!(normalize_plugin_id("tracker-url-remover"), "tracker_url_remover");
    }
}
