//! zoeken-settings: layered YAML settings loading and validation.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::Path;

use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

pub type ExtraMap = BTreeMap<String, Value>;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BoolOrString {
    Bool(bool),
    Str(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum IntOrString {
    Int(i64),
    Str(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum StringOrVec {
    One(String),
    Many(Vec<String>),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Proxies {
    Single(String),
    Map(BTreeMap<String, StringOrVec>),
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub general: GeneralSettings,
    pub brand: BrandSettings,
    pub search: SearchSettings,
    pub server: ServerSettings,
    pub deployment: DeploymentConfig,
    pub ui: UiSettings,
    pub outgoing: OutgoingSettings,
    pub storage: StorageSettings,
    pub cache: CacheSettings,
    /// Replaces, not merges, the default engine set when non-empty.
    pub engines: Vec<EngineSettings>,
    pub plugins: PluginSettings,
    pub lua_plugins: LuaPluginSettings,
    #[serde(rename = "categories_as_tabs")]
    pub categories: CategorySettings,
    pub preferences: PreferencesSettings,
    pub doi_resolvers: BTreeMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_doi_resolver: Option<String>,
    pub hostnames: HostnamesSettings,
    pub limiter: ExtraMap,
    pub favicons: ExtraMap,
}

impl Settings {
    #[must_use]
    pub fn defaults() -> Self {
        Self::default()
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct GeneralSettings {
    pub debug: bool,
    pub instance_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub privacypolicy_url: Option<BoolOrString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contact_url: Option<BoolOrString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub donation_url: Option<BoolOrString>,
    pub enable_metrics: bool,
    pub open_metrics: String,
}

impl Default for GeneralSettings {
    fn default() -> Self {
        Self {
            debug: false,
            instance_name: "Search".to_string(),
            privacypolicy_url: None,
            contact_url: None,
            donation_url: None,
            enable_metrics: true,
            open_metrics: String::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BrandSettings {
    pub issue_url: String,
    pub docs_url: String,
    pub public_instances: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wiki_url: Option<BoolOrString>,
    pub custom: BrandCustom,
    pub pwa_colors: ThemeColors,
    pub new_issue_url: String,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct BrandCustom {
    pub links: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeColors {
    pub theme_color_light: String,
    pub background_color_light: String,
    pub theme_color_dark: String,
    pub background_color_dark: String,
    pub theme_color_black: String,
    pub background_color_black: String,
}

impl Default for ThemeColors {
    fn default() -> Self {
        Self {
            theme_color_light: "#246018".to_string(),
            background_color_light: "#f2f5ee".to_string(),
            theme_color_dark: "#8fd46a".to_string(),
            background_color_dark: "#0f1410".to_string(),
            theme_color_black: "#246018".to_string(),
            background_color_black: "#000".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchSettings {
    pub safe_search: u8,
    pub autocomplete: String,
    pub autocomplete_min: u32,
    pub favicon_resolver: String,
    pub default_lang: String,
    pub languages: Vec<String>,
    pub ban_time_on_fail: f64,
    pub max_ban_time_on_fail: f64,
    pub suspended_times: SuspendedTimes,
    pub formats: Vec<String>,
    pub max_page: u32,
}

impl Default for SearchSettings {
    fn default() -> Self {
        Self {
            safe_search: 0,
            autocomplete: "brave".to_string(),
            autocomplete_min: 1,
            favicon_resolver: "duckduckgo".to_string(),
            default_lang: String::new(),
            languages: Vec::new(),
            ban_time_on_fail: 5.0,
            max_ban_time_on_fail: 120.0,
            suspended_times: SuspendedTimes::default(),
            formats: vec![
                "html".to_string(),
                "csv".to_string(),
                "json".to_string(),
                "rss".to_string(),
            ],
            max_page: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SuspendedTimes {
    #[serde(rename = "SearxEngineAccessDenied")]
    pub access_denied: f64,
    #[serde(rename = "SearxEngineCaptcha")]
    pub captcha: f64,
    #[serde(rename = "SearxEngineTooManyRequests")]
    pub too_many_requests: f64,
    #[serde(rename = "cf_SearxEngineCaptcha")]
    pub cf_captcha: f64,
    #[serde(rename = "cf_SearxEngineAccessDenied")]
    pub cf_access_denied: f64,
    #[serde(rename = "recaptcha_SearxEngineCaptcha")]
    pub recaptcha_captcha: f64,
}

impl Default for SuspendedTimes {
    fn default() -> Self {
        Self {
            access_denied: 86400.0,
            captcha: 86400.0,
            too_many_requests: 3600.0,
            cf_captcha: 1296000.0,
            cf_access_denied: 86400.0,
            recaptcha_captcha: 604800.0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ServerSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub port: Option<IntOrString>,
    pub bind_address: String,
    pub limiter: bool,
    pub public_instance: bool,
    pub secret_key: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub base_url: Option<BoolOrString>,
    pub image_proxy: bool,
    pub http_protocol_version: String,
    pub method: String,
    pub default_http_headers: BTreeMap<String, String>,
}

impl Default for ServerSettings {
    fn default() -> Self {
        Self {
            port: Some(IntOrString::Int(8888)),
            bind_address: "127.0.0.1".to_string(),
            limiter: false,
            public_instance: false,
            secret_key: String::new(),
            base_url: Some(BoolOrString::Bool(false)),
            image_proxy: true,
            http_protocol_version: "1.0".to_string(),
            method: "POST".to_string(),
            default_http_headers: BTreeMap::new(),
        }
    }
}

pub const DEFAULT_MAX_REQUEST_BODY_BYTES: usize = 1024 * 1024;
pub const DEFAULT_MAX_UPSTREAM_RESPONSE_BYTES: usize = 10 * 1024 * 1024;
pub const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 30;
pub const DEFAULT_SHUTDOWN_GRACE_SECONDS: u64 = 30;
pub const DEFAULT_LOG_LEVEL: &str = "info";

/// The built-in same-origin Content-Security-Policy.
#[must_use]
pub fn default_content_security_policy() -> String {
    // Search UIs need remote result favicons + occasional infobox imagery.
    "default-src 'self'; script-src 'self'; \
     style-src 'self' https://fonts.googleapis.com; \
     font-src 'self' https://fonts.gstatic.com; \
     img-src 'self' data: https:; connect-src 'self'; base-uri 'self'; \
     form-action 'self'; frame-ancestors 'none'"
        .to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct DeploymentConfig {
    pub log_level: String,
    pub shutdown_grace_seconds: u64,
    pub max_request_body_bytes: usize,
    pub request_timeout_seconds: u64,
    pub hsts: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content_security_policy: Option<String>,
    pub metrics_enabled: bool,
    pub trusted_proxies: Vec<String>,
}

impl Default for DeploymentConfig {
    fn default() -> Self {
        Self {
            log_level: DEFAULT_LOG_LEVEL.to_string(),
            shutdown_grace_seconds: DEFAULT_SHUTDOWN_GRACE_SECONDS,
            max_request_body_bytes: DEFAULT_MAX_REQUEST_BODY_BYTES,
            request_timeout_seconds: DEFAULT_REQUEST_TIMEOUT_SECONDS,
            hsts: false,
            content_security_policy: Some(default_content_security_policy()),
            metrics_enabled: true,
            trusted_proxies: Vec::new(),
        }
    }
}

impl DeploymentConfig {
    #[must_use]
    pub fn effective_content_security_policy(&self) -> String {
        self.content_security_policy
            .clone()
            .unwrap_or_else(default_content_security_policy)
    }

    #[must_use]
    pub fn effective_max_request_body_bytes(&self) -> usize {
        resolve_max_request_body_bytes(self.max_request_body_bytes)
    }

    #[must_use]
    pub fn effective_request_timeout_seconds(&self) -> u64 {
        resolve_request_timeout_seconds(self.request_timeout_seconds)
    }
}

#[must_use]
pub fn resolve_max_request_body_bytes(configured: usize) -> usize {
    if configured > 0 {
        configured
    } else {
        DEFAULT_MAX_REQUEST_BODY_BYTES
    }
}

#[must_use]
pub fn resolve_request_timeout_seconds(configured: u64) -> u64 {
    if configured > 0 {
        configured
    } else {
        DEFAULT_REQUEST_TIMEOUT_SECONDS
    }
}

const DEFAULT_BIND_PORT: u16 = 8888;

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DeploymentConfigError {
    #[error("malformed bind address '{value}': expected an IP address (e.g. 127.0.0.1 or ::1)")]
    BindAddress { value: String },

    #[error("malformed port '{value}': expected an integer in 0..=65535")]
    Port { value: String },
}

/// Parse and validate `server.bind_address` + `server.port` into a [`SocketAddr`].
pub fn resolve_bind(server: &ServerSettings) -> Result<SocketAddr, DeploymentConfigError> {
    let ip = resolve_bind_ip(&server.bind_address)?;
    let port = resolve_bind_port(server.port.as_ref())?;
    Ok(SocketAddr::new(ip, port))
}

/// Empty/whitespace bind addresses default to IPv4 loopback.
fn resolve_bind_ip(bind_address: &str) -> Result<IpAddr, DeploymentConfigError> {
    let trimmed = bind_address.trim();
    if trimmed.is_empty() {
        return Ok(IpAddr::V4(Ipv4Addr::LOCALHOST));
    }
    trimmed
        .parse::<IpAddr>()
        .map_err(|_| DeploymentConfigError::BindAddress {
            value: bind_address.to_string(),
        })
}

/// `None` or empty bind ports default to [`DEFAULT_BIND_PORT`].
fn resolve_bind_port(port: Option<&IntOrString>) -> Result<u16, DeploymentConfigError> {
    match port {
        None => Ok(DEFAULT_BIND_PORT),
        Some(IntOrString::Int(n)) => u16::try_from(*n).map_err(|_| DeploymentConfigError::Port {
            value: n.to_string(),
        }),
        Some(IntOrString::Str(s)) => {
            let trimmed = s.trim();
            if trimmed.is_empty() {
                return Ok(DEFAULT_BIND_PORT);
            }
            trimmed
                .parse::<u16>()
                .map_err(|_| DeploymentConfigError::Port { value: s.clone() })
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SecretKeyDecision {
    Abort,
    StartWithWarning,
    Start,
}

/// Decide startup behavior from bind address loopback status and secret key presence.
#[must_use]
pub fn secret_key_decision(is_loopback: bool, secret_is_empty: bool) -> SecretKeyDecision {
    match (is_loopback, secret_is_empty) {
        (false, true) => SecretKeyDecision::Abort,
        (true, true) => SecretKeyDecision::StartWithWarning,
        (_, false) => SecretKeyDecision::Start,
    }
}

/// Reject secrets that are empty, too short, or known placeholder values for public binds.
#[must_use]
pub fn secret_key_is_weak(secret: &str) -> bool {
    const MIN_LEN: usize = 16;
    const PLACEHOLDERS: &[&str] = &[
        "secret",
        "changeme",
        "change-me",
        "change-me-to-a-long-random-secret",
        "password",
        "zoeken",
        "searxng",
        "test",
        "default",
        "ci-liveness-check-secret",
    ];
    let trimmed = secret.trim();
    if trimmed.len() < MIN_LEN {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    if PLACEHOLDERS.iter().any(|p| lower == *p) {
        return true;
    }
    // Catch example secrets that only change the suffix of "change-me…".
    lower.starts_with("change-me")
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct UiSettings {
    pub default_theme: String,
    pub default_locale: String,
    pub theme_args: ThemeArgs,
    pub center_alignment: bool,
    pub results_on_new_tab: bool,
    pub query_in_title: bool,
    pub cache_url: String,
    pub search_on_category_select: bool,
    pub hotkeys: String,
    pub url_formatting: String,
}

impl Default for UiSettings {
    fn default() -> Self {
        Self {
            default_theme: "simple".to_string(),
            default_locale: String::new(),
            theme_args: ThemeArgs::default(),
            center_alignment: false,
            results_on_new_tab: false,
            query_in_title: false,
            cache_url: String::new(),
            search_on_category_select: true,
            hotkeys: "default".to_string(),
            url_formatting: "pretty".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct ThemeArgs {
    pub simple_style: String,
}

impl Default for ThemeArgs {
    fn default() -> Self {
        Self {
            simple_style: "auto".to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OutgoingSettings {
    pub useragent_suffix: String,
    pub request_timeout: f64,
    pub enable_http2: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<BoolOrString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_request_timeout: Option<f64>,
    pub pool_connections: u32,
    pub pool_maxsize: u32,
    pub keepalive_expiry: f64,
    pub max_response_bytes: usize,
    pub max_redirects: u32,
    pub retries: u32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxies: Option<Proxies>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ips: Option<StringOrVec>,
    pub using_tor_proxy: bool,
    pub extra_proxy_timeout: u32,
    pub networks: BTreeMap<String, NetworkSettings>,
    pub origin_limits: OriginLimitSettings,
}

impl Default for OutgoingSettings {
    fn default() -> Self {
        Self {
            useragent_suffix: String::new(),
            request_timeout: 3.0,
            enable_http2: true,
            verify: Some(BoolOrString::Bool(true)),
            max_request_timeout: None,
            pool_connections: 100,
            pool_maxsize: 10,
            keepalive_expiry: 5.0,
            max_response_bytes: DEFAULT_MAX_UPSTREAM_RESPONSE_BYTES,
            max_redirects: 30,
            retries: 0,
            proxies: None,
            source_ips: None,
            using_tor_proxy: false,
            extra_proxy_timeout: 0,
            networks: BTreeMap::new(),
            origin_limits: OriginLimitSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct OriginLimitSettings {
    pub requests_per_second: f64,
    pub burst: u32,
    pub max_concurrent: u32,
    pub lease_seconds: u64,
}

impl Default for OriginLimitSettings {
    fn default() -> Self {
        Self {
            requests_per_second: 1.0,
            burst: 2,
            max_concurrent: 2,
            lease_seconds: 15,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct StorageSettings {
    pub backend: String,
    pub sqlite: SqliteStorageSettings,
    pub postgres: PostgresStorageSettings,
}

impl Default for StorageSettings {
    fn default() -> Self {
        Self {
            backend: "sqlite".into(),
            sqlite: SqliteStorageSettings::default(),
            postgres: PostgresStorageSettings::default(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SqliteStorageSettings {
    pub path: String,
    pub busy_timeout_ms: u64,
}

impl Default for SqliteStorageSettings {
    fn default() -> Self {
        Self {
            path: "./zoeken.sqlite3".into(),
            busy_timeout_ms: 5000,
        }
    }
}

#[derive(Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct PostgresStorageSettings {
    pub url: Option<String>,
    pub max_connections: usize,
    pub acquire_timeout_seconds: u64,
}

impl std::fmt::Debug for PostgresStorageSettings {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("PostgresStorageSettings")
            .field("url", &self.url.as_ref().map(|_| "<redacted>"))
            .field("max_connections", &self.max_connections)
            .field("acquire_timeout_seconds", &self.acquire_timeout_seconds)
            .finish()
    }
}

impl Default for PostgresStorageSettings {
    fn default() -> Self {
        Self {
            url: None,
            max_connections: 16,
            acquire_timeout_seconds: 5,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
#[serde(default)]
pub struct CacheSettings {
    pub search: SearchCacheSettings,
    pub autocomplete: AutocompleteCacheSettings,
    pub favicons: FaviconCacheSettings,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct SearchCacheSettings {
    pub ttl_seconds: u64,
    pub structured_ttl_seconds: u64,
    pub max_bytes: usize,
}
impl Default for SearchCacheSettings {
    fn default() -> Self {
        Self {
            ttl_seconds: 60,
            structured_ttl_seconds: 300,
            max_bytes: 134_217_728,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct AutocompleteCacheSettings {
    pub ttl_seconds: u64,
    pub max_entries: u64,
}
impl Default for AutocompleteCacheSettings {
    fn default() -> Self {
        Self {
            ttl_seconds: 300,
            max_entries: 2048,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct FaviconCacheSettings {
    pub positive_ttl_seconds: u64,
    pub negative_ttl_seconds: u64,
    pub max_blob_bytes: usize,
    pub max_total_bytes: usize,
}
impl Default for FaviconCacheSettings {
    fn default() -> Self {
        Self {
            positive_ttl_seconds: 2_592_000,
            negative_ttl_seconds: 86_400,
            max_blob_bytes: 20_480,
            max_total_bytes: 268_435_456,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct NetworkSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub request_timeout: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub enable_http2: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub verify: Option<BoolOrString>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_request_timeout: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_connections: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pool_maxsize: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keepalive_expiry: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_redirects: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retries: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub retry_on_http_error: Option<Vec<u16>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub proxies: Option<Proxies>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_ips: Option<StringOrVec>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub using_tor_proxy: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra_proxy_timeout: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct EngineSettings {
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub engine: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub shortcut: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub categories: Option<StringOrVec>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub timeout: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disabled: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub weight: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<Vec<String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub network: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inactive: Option<bool>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PluginSettings(pub BTreeMap<String, PluginEntry>);

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct PluginEntry {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub active: Option<bool>,
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct LuaPluginSettings {
    pub enabled: bool,
    pub directory: Option<String>,
    pub order: Vec<String>,
}

impl Default for LuaPluginSettings {
    fn default() -> Self {
        Self {
            enabled: true,
            directory: None,
            order: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CategorySettings(pub BTreeMap<String, CategoryTab>);

impl Default for CategorySettings {
    fn default() -> Self {
        let tabs = [
            "general",
            "images",
            "videos",
            "news",
            "map",
            "music",
            "it",
            "science",
            "files",
            "shopping",
            "social media",
        ];
        Self(
            tabs.iter()
                .map(|name| ((*name).to_string(), CategoryTab::default()))
                .collect(),
        )
    }
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
pub struct CategoryTab {
    #[serde(flatten)]
    pub extra: ExtraMap,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct PreferencesSettings {
    pub lock: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct HostnamesSettings {
    #[serde(flatten)]
    pub extra: ExtraMap,
}

/// Read-only view over the process environment used by [`load_settings`].
/// Recognized `APP_*` variables override file values and defaults.
#[derive(Debug, Clone, Default)]
pub struct EnvMap {
    vars: BTreeMap<String, String>,
}

impl EnvMap {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn from_env() -> Self {
        Self {
            vars: std::env::vars().collect(),
        }
    }

    #[must_use]
    pub fn with(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.vars.insert(key.into(), value.into());
        self
    }

    pub fn set(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.vars.insert(key.into(), value.into());
    }

    #[must_use]
    pub fn get(&self, key: &str) -> Option<&str> {
        self.vars.get(key).map(String::as_str)
    }
}

impl<K, V> FromIterator<(K, V)> for EnvMap
where
    K: Into<String>,
    V: Into<String>,
{
    fn from_iter<I: IntoIterator<Item = (K, V)>>(iter: I) -> Self {
        Self {
            vars: iter
                .into_iter()
                .map(|(k, v)| (k.into(), v.into()))
                .collect(),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SettingsError {
    #[error("failed to read settings file '{path}': {source}")]
    FileRead {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("failed to parse settings file '{path}': {source}")]
    FileParse {
        path: String,
        #[source]
        source: serde_yaml_ng::Error,
    },
    #[error("invalid value for 'use_default_settings': expected a boolean or a mapping")]
    InvalidUseDefaultSettings,
    #[error("invalid environment override '{var}': {message}")]
    EnvOverride { var: String, message: String },
    #[error("failed to interpret merged settings: {source}")]
    Deserialize {
        #[source]
        source: serde_yaml_ng::Error,
    },
    #[error("invalid setting '{setting}': {message}")]
    Validation { setting: String, message: String },
}

/// Load, merge, and validate the layered configuration.
pub fn load_settings(path: Option<&Path>, env: &EnvMap) -> Result<Settings, SettingsError> {
    let mut merged = serde_yaml_ng::to_value(Settings::defaults())
        .map_err(|source| SettingsError::Deserialize { source })?;

    if let Some(path) = path {
        let text = std::fs::read_to_string(path).map_err(|source| SettingsError::FileRead {
            path: path.display().to_string(),
            source,
        })?;
        let file_value: Value =
            serde_yaml_ng::from_str(&text).map_err(|source| SettingsError::FileParse {
                path: path.display().to_string(),
                source,
            })?;
        reject_legacy_storage_keys(&file_value, env)?;
        if !file_value.is_null() {
            apply_file_overlay(&mut merged, &file_value)?;
        }
    } else {
        reject_legacy_storage_keys(&Value::Null, env)?;
    }

    apply_env_overrides(&mut merged, env)?;

    let settings: Settings = serde_yaml_ng::from_value(merged)
        .map_err(|source| SettingsError::Deserialize { source })?;
    validate_settings(&settings)?;
    Ok(settings)
}

fn reject_legacy_storage_keys(file: &Value, env: &EnvMap) -> Result<(), SettingsError> {
    let has_legacy_file_key = file.as_mapping().is_some_and(|mapping| {
        ["redis", "valkey"]
            .iter()
            .any(|key| mapping.contains_key(Value::String((*key).to_string())))
    });
    if has_legacy_file_key {
        return Err(SettingsError::Validation {
            setting: "storage".to_string(),
            message: "redis/valkey configuration was removed; migrate to storage.backend with storage.sqlite or storage.postgres".to_string(),
        });
    }

    for variable in ["APP_REDIS_URL", "APP_VALKEY_URL"] {
        if env.get(variable).is_some() {
            return Err(SettingsError::EnvOverride {
                var: variable.to_string(),
                message: "this variable was removed; use APP_STORAGE_BACKEND and APP_POSTGRES_URL or APP_SQLITE_PATH".to_string(),
            });
        }
    }
    Ok(())
}

enum FileMergeMode {
    Overlay {
        remove_engines: Option<Vec<String>>,
        keep_only_engines: Option<Vec<String>>,
    },
    Replace,
}

fn apply_file_overlay(base: &mut Value, file: &Value) -> Result<(), SettingsError> {
    match interpret_use_default_settings(file)? {
        FileMergeMode::Replace => {
            *base = file.clone();
        }
        FileMergeMode::Overlay {
            remove_engines,
            keep_only_engines,
        } => {
            update_settings(
                base,
                file,
                remove_engines.as_deref(),
                keep_only_engines.as_deref(),
            );
        }
    }
    Ok(())
}

fn interpret_use_default_settings(file: &Value) -> Result<FileMergeMode, SettingsError> {
    let Some(map) = file.as_mapping() else {
        return Ok(FileMergeMode::Replace);
    };
    match map.get(Value::String("use_default_settings".to_string())) {
        None | Some(Value::Null) | Some(Value::Bool(false)) => Ok(FileMergeMode::Replace),
        Some(Value::Bool(true)) => Ok(FileMergeMode::Overlay {
            remove_engines: None,
            keep_only_engines: None,
        }),
        Some(Value::Mapping(uds)) => {
            let engines = uds
                .get(Value::String("engines".to_string()))
                .and_then(Value::as_mapping);
            let remove_engines = engines
                .and_then(|e| e.get(Value::String("remove".to_string())))
                .and_then(value_as_string_list);
            let keep_only_engines = engines
                .and_then(|e| e.get(Value::String("keep_only".to_string())))
                .and_then(value_as_string_list);
            Ok(FileMergeMode::Overlay {
                remove_engines,
                keep_only_engines,
            })
        }
        Some(_) => Err(SettingsError::InvalidUseDefaultSettings),
    }
}

/// Deep-merge `user` onto `default_settings`, preserving special-case overlays.
fn update_settings(
    default_settings: &mut Value,
    user: &Value,
    remove_engines: Option<&[String]>,
    keep_only_engines: Option<&[String]>,
) {
    let Some(user_map) = user.as_mapping() else {
        return;
    };

    for (key, value) in user_map {
        if let Some(name) = key.as_str()
            && (name == "use_default_settings" || name == "engines")
        {
            continue;
        }
        let dmap = as_mapping_mut_or_replace(default_settings);
        match dmap.get_mut(key) {
            Some(existing) if value.as_mapping().is_some() && existing.as_mapping().is_some() => {
                update_dict(existing, value);
            }
            _ => {
                dmap.insert(key.clone(), value.clone());
            }
        }
    }

    if let Some(tabs) = user_map.get(Value::String("categories_as_tabs".to_string()))
        && is_truthy(tabs)
    {
        as_mapping_mut_or_replace(default_settings).insert(
            Value::String("categories_as_tabs".to_string()),
            tabs.clone(),
        );
    }

    if let Some(plugins) = user_map.get(Value::String("plugins".to_string()))
        && !plugins.is_null()
    {
        as_mapping_mut_or_replace(default_settings)
            .insert(Value::String("plugins".to_string()), plugins.clone());
    }

    let user_engines = user_map
        .get(Value::String("engines".to_string()))
        .and_then(Value::as_sequence);
    if user_engines.is_some() || remove_engines.is_some() || keep_only_engines.is_some() {
        merge_engines(
            default_settings,
            user_engines,
            remove_engines,
            keep_only_engines,
        );
    }
}

/// Per-engine overlays replace only the named entry fields on a matching default entry.
fn merge_engines(
    default_settings: &mut Value,
    user_engines: Option<&Vec<Value>>,
    remove_engines: Option<&[String]>,
    keep_only_engines: Option<&[String]>,
) {
    let dmap = as_mapping_mut_or_replace(default_settings);
    let mut engines: Vec<Value> = dmap
        .get(Value::String("engines".to_string()))
        .and_then(Value::as_sequence)
        .cloned()
        .unwrap_or_default();

    if let Some(remove) = remove_engines {
        engines.retain(|e| {
            engine_name(e)
                .map(|n| !remove.iter().any(|r| r == n))
                .unwrap_or(true)
        });
    }
    if let Some(keep) = keep_only_engines {
        engines.retain(|e| {
            engine_name(e)
                .map(|n| keep.iter().any(|k| k == n))
                .unwrap_or(false)
        });
    }

    if let Some(user_engines) = user_engines {
        for user_engine in user_engines {
            let Some(name) = engine_name(user_engine) else {
                engines.push(user_engine.clone());
                continue;
            };
            match engines.iter_mut().find(|e| engine_name(e) == Some(name)) {
                Some(existing) => update_dict(existing, user_engine),
                None => engines.push(user_engine.clone()),
            }
        }
    }

    dmap.insert(
        Value::String("engines".to_string()),
        Value::Sequence(engines),
    );
}

fn update_dict(default: &mut Value, user: &Value) {
    let Some(user_map) = user.as_mapping() else {
        return;
    };
    let dmap = as_mapping_mut_or_replace(default);
    for (key, value) in user_map {
        if value.as_mapping().is_some() {
            match dmap.get_mut(key) {
                Some(existing) => update_dict(existing, value),
                None => {
                    let mut nested = Value::Mapping(serde_yaml_ng::Mapping::new());
                    update_dict(&mut nested, value);
                    dmap.insert(key.clone(), nested);
                }
            }
        } else {
            dmap.insert(key.clone(), value.clone());
        }
    }
}

/// Apply recognized `APP_*` environment overrides onto the merged document.
fn apply_env_overrides(merged: &mut Value, env: &EnvMap) -> Result<(), SettingsError> {
    enum Kind {
        Bool,
        IntOrString,
        String,
    }
    const SPECS: &[(&str, &[&str], Kind)] = &[
        ("APP_DEBUG", &["general", "debug"], Kind::Bool),
        ("APP_PORT", &["server", "port"], Kind::IntOrString),
        (
            "APP_BIND_ADDRESS",
            &["server", "bind_address"],
            Kind::String,
        ),
        ("APP_LIMITER", &["server", "limiter"], Kind::Bool),
        (
            "APP_PUBLIC_INSTANCE",
            &["server", "public_instance"],
            Kind::Bool,
        ),
        ("APP_SECRET_KEY", &["server", "secret_key"], Kind::String),
        ("APP_BASE_URL", &["server", "base_url"], Kind::String),
        ("APP_IMAGE_PROXY", &["server", "image_proxy"], Kind::Bool),
        ("APP_METHOD", &["server", "method"], Kind::String),
        ("APP_STORAGE_BACKEND", &["storage", "backend"], Kind::String),
        (
            "APP_SQLITE_PATH",
            &["storage", "sqlite", "path"],
            Kind::String,
        ),
        (
            "APP_POSTGRES_URL",
            &["storage", "postgres", "url"],
            Kind::String,
        ),
        ("APP_LOG_LEVEL", &["deployment", "log_level"], Kind::String),
        (
            "APP_METRICS_ENABLED",
            &["deployment", "metrics_enabled"],
            Kind::Bool,
        ),
    ];

    for (var, path, kind) in SPECS {
        let Some(raw) = env.get(var) else {
            continue;
        };
        let value = match kind {
            Kind::Bool => Value::Bool(parse_env_bool(var, raw)?),
            Kind::String => Value::String(raw.to_string()),
            Kind::IntOrString => match raw.parse::<i64>() {
                Ok(n) => Value::Number(n.into()),
                Err(_) => Value::String(raw.to_string()),
            },
        };
        set_path(merged, path, value);
    }
    Ok(())
}

fn parse_env_bool(var: &str, raw: &str) -> Result<bool, SettingsError> {
    match raw.to_ascii_lowercase().as_str() {
        "1" | "true" | "on" => Ok(true),
        "0" | "false" | "off" => Ok(false),
        other => Err(SettingsError::EnvOverride {
            var: var.to_string(),
            message: format!("expected a boolean (0/1/true/false/on/off), got '{other}'"),
        }),
    }
}

/// Validate the merged, typed settings against schema constraints.
pub fn validate_settings(s: &Settings) -> Result<(), SettingsError> {
    fn invalid(setting: &str, message: String) -> SettingsError {
        SettingsError::Validation {
            setting: setting.to_string(),
            message,
        }
    }

    match s.storage.backend.as_str() {
        "sqlite" if s.storage.sqlite.path.trim().is_empty() => {
            return Err(invalid("storage.sqlite.path", "must not be empty".into()));
        }
        "sqlite" if s.storage.sqlite.busy_timeout_ms == 0 => {
            return Err(invalid(
                "storage.sqlite.busy_timeout_ms",
                "must be greater than zero".into(),
            ));
        }
        "sqlite" => {}
        "postgres" if s.storage.postgres.url.as_deref().is_none_or(str::is_empty) => {
            return Err(invalid(
                "storage.postgres.url",
                "is required when storage.backend is postgres".into(),
            ));
        }
        "postgres" if s.storage.postgres.max_connections == 0 => {
            return Err(invalid(
                "storage.postgres.max_connections",
                "must be greater than zero".into(),
            ));
        }
        "postgres" if s.storage.postgres.acquire_timeout_seconds == 0 => {
            return Err(invalid(
                "storage.postgres.acquire_timeout_seconds",
                "must be greater than zero".into(),
            ));
        }
        "postgres" => {}
        other => {
            return Err(invalid(
                "storage.backend",
                format!("must be 'sqlite' or 'postgres' (got '{other}')"),
            ));
        }
    }
    if s.outgoing.origin_limits.requests_per_second <= 0.0
        || !s.outgoing.origin_limits.requests_per_second.is_finite()
    {
        return Err(invalid(
            "outgoing.origin_limits.requests_per_second",
            "must be finite and greater than zero".into(),
        ));
    }
    if s.outgoing.origin_limits.burst == 0
        || s.outgoing.origin_limits.max_concurrent == 0
        || s.outgoing.origin_limits.lease_seconds == 0
    {
        return Err(invalid(
            "outgoing.origin_limits",
            "burst, max_concurrent, and lease_seconds must be greater than zero".into(),
        ));
    }
    if s.cache.search.ttl_seconds == 0
        || s.cache.search.structured_ttl_seconds == 0
        || s.cache.search.max_bytes == 0
        || s.cache.autocomplete.ttl_seconds == 0
        || s.cache.autocomplete.max_entries == 0
        || s.cache.favicons.positive_ttl_seconds == 0
        || s.cache.favicons.negative_ttl_seconds == 0
        || s.cache.favicons.max_blob_bytes == 0
        || s.cache.favicons.max_total_bytes == 0
    {
        return Err(invalid(
            "cache",
            "all TTL and capacity limits must be greater than zero".into(),
        ));
    }
    if s.search.safe_search > 2 {
        return Err(invalid(
            "search.safe_search",
            format!("must be 0, 1, or 2 (got {})", s.search.safe_search),
        ));
    }
    match s.server.http_protocol_version.as_str() {
        "1.0" | "1.1" => {}
        other => {
            return Err(invalid(
                "server.http_protocol_version",
                format!("must be '1.0' or '1.1' (got '{other}')"),
            ));
        }
    }
    match s.search.favicon_resolver.as_str() {
        "" | "allesedv" | "duckduckgo" | "google" | "yandex" => {}
        other => {
            return Err(invalid(
                "search.favicon_resolver",
                format!(
                    "must be empty or one of allesedv, duckduckgo, google, yandex (got '{other}')"
                ),
            ));
        }
    }
    match s.server.method.as_str() {
        "POST" | "GET" => {}
        other => {
            return Err(invalid(
                "server.method",
                format!("must be 'POST' or 'GET' (got '{other}')"),
            ));
        }
    }
    match s.ui.theme_args.simple_style.as_str() {
        "auto" | "light" | "dark" | "black" => {}
        other => {
            return Err(invalid(
                "ui.theme_args.simple_style",
                format!("must be one of auto, light, dark, black (got '{other}')"),
            ));
        }
    }
    match s.ui.hotkeys.as_str() {
        "default" | "vim" => {}
        other => {
            return Err(invalid(
                "ui.hotkeys",
                format!("must be 'default' or 'vim' (got '{other}')"),
            ));
        }
    }
    match s.ui.url_formatting.as_str() {
        "pretty" | "full" | "host" => {}
        other => {
            return Err(invalid(
                "ui.url_formatting",
                format!("must be one of pretty, full, host (got '{other}')"),
            ));
        }
    }
    for format in &s.search.formats {
        match format.as_str() {
            "html" | "csv" | "json" | "rss" => {}
            other => {
                return Err(invalid(
                    "search.formats",
                    format!("unsupported output format '{other}' (allowed: html, csv, json, rss)"),
                ));
            }
        }
    }
    for (name, value) in &s.server.default_http_headers {
        if http::HeaderName::from_bytes(name.as_bytes()).is_err() {
            return Err(invalid(
                "server.default_http_headers",
                format!("invalid HTTP header name '{name}'"),
            ));
        }
        if http::HeaderValue::from_str(value).is_err() {
            return Err(invalid(
                "server.default_http_headers",
                format!("invalid value for HTTP header '{name}'"),
            ));
        }
    }
    Ok(())
}

fn as_mapping_mut_or_replace(node: &mut Value) -> &mut serde_yaml_ng::Mapping {
    if node.as_mapping().is_none() {
        *node = Value::Mapping(serde_yaml_ng::Mapping::new());
    }
    node.as_mapping_mut()
        .expect("just ensured node is a mapping")
}

fn set_path(root: &mut Value, path: &[&str], value: Value) {
    let Some((head, rest)) = path.split_first() else {
        return;
    };
    let map = as_mapping_mut_or_replace(root);
    let key = Value::String((*head).to_string());
    if rest.is_empty() {
        map.insert(key, value);
        return;
    }
    if map
        .get(&key)
        .map(|v| v.as_mapping().is_none())
        .unwrap_or(true)
    {
        map.insert(key.clone(), Value::Mapping(serde_yaml_ng::Mapping::new()));
    }
    let child = map.get_mut(&key).expect("just inserted child mapping");
    set_path(child, rest, value);
}

fn engine_name(engine: &Value) -> Option<&str> {
    engine
        .as_mapping()?
        .get(Value::String("name".to_string()))?
        .as_str()
}

fn value_as_string_list(value: &Value) -> Option<Vec<String>> {
    let seq = value.as_sequence()?;
    Some(
        seq.iter()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect(),
    )
}

fn is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(b) => *b,
        Value::Mapping(m) => !m.is_empty(),
        Value::Sequence(s) => !s.is_empty(),
        Value::String(s) => !s.is_empty(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn settings_default_round_trips_through_yaml() {
        let settings = Settings::default();
        let yaml = serde_yaml_ng::to_string(&settings).expect("serialize default settings");
        let parsed: Settings =
            serde_yaml_ng::from_str(&yaml).expect("deserialize default settings");
        assert_eq!(settings, parsed);
    }

    #[test]
    fn empty_yaml_deserializes_to_default() {
        let parsed: Settings = serde_yaml_ng::from_str("{}").expect("deserialize empty document");
        assert_eq!(parsed, Settings::default());
    }

    #[test]
    fn partial_general_section_keeps_other_defaults() {
        let yaml = "general:\n  instance_name: \"Example\"\n  debug: true\n";
        let parsed: Settings = serde_yaml_ng::from_str(yaml).expect("deserialize partial general");
        assert_eq!(parsed.general.instance_name, "Example");
        assert!(parsed.general.debug);
    }

    #[test]
    fn engine_entry_captures_overlay_and_extra_keys() {
        let yaml = r#"
engines:
  - name: bing images
    engine: bing_images
    shortcut: bii
    timeout: 4.0
    categories: images
    disabled: true
    base_url: https://cn.bing.com
"#;
        let parsed: Settings = serde_yaml_ng::from_str(yaml).expect("deserialize engine overlay");
        let engine = &parsed.engines[0];
        assert_eq!(engine.name, "bing images");
        assert_eq!(engine.engine.as_deref(), Some("bing_images"));
        assert_eq!(engine.shortcut.as_deref(), Some("bii"));
        assert_eq!(engine.timeout, Some(4.0));
        assert_eq!(engine.disabled, Some(true));
        assert_eq!(
            engine.categories,
            Some(StringOrVec::One("images".to_string()))
        );
        assert!(engine.extra.contains_key("base_url"));
    }

    #[test]
    fn suspended_times_uses_renamed_keys() {
        let yaml = r#"
search:
  suspended_times:
    SearxEngineAccessDenied: 111
    cf_SearxEngineCaptcha: 222
"#;
        let parsed: Settings = serde_yaml_ng::from_str(yaml).expect("deserialize suspended_times");
        assert_eq!(parsed.search.suspended_times.access_denied, 111.0);
        assert_eq!(parsed.search.suspended_times.cf_captcha, 222.0);
    }

    #[test]
    fn server_port_accepts_int_or_string() {
        let as_int: Settings =
            serde_yaml_ng::from_str("server:\n  port: 8888\n").expect("int port");
        assert_eq!(as_int.server.port, Some(IntOrString::Int(8888)));

        let as_str: Settings =
            serde_yaml_ng::from_str("server:\n  port: \"8888\"\n").expect("string port");
        assert_eq!(
            as_str.server.port,
            Some(IntOrString::Str("8888".to_string()))
        );
    }

    #[test]
    fn defaults_match_reference_settings_defaults() {
        let s = Settings::defaults();
        assert_eq!(s.general.instance_name, "Search");
        assert!(!s.general.debug);
        assert!(s.general.enable_metrics);
        assert_eq!(s.general.donation_url, None);
        assert_eq!(s.general.privacypolicy_url, None);
        assert_eq!(s.general.contact_url, None);
        assert_eq!(s.brand.issue_url, "");
        assert_eq!(s.brand.docs_url, "");
        assert_eq!(s.brand.public_instances, "");
        assert_eq!(s.brand.wiki_url, None);
        assert_eq!(s.brand.new_issue_url, "");
        assert_eq!(s.brand.pwa_colors.theme_color_light, "#246018");
        assert_eq!(s.brand.pwa_colors.background_color_light, "#f2f5ee");
        assert_eq!(s.brand.pwa_colors.background_color_black, "#000");
        assert_eq!(s.search.safe_search, 0);
        assert_eq!(s.search.autocomplete_min, 1);
        assert_eq!(s.search.ban_time_on_fail, 5.0);
        assert_eq!(s.search.max_ban_time_on_fail, 120.0);
        assert_eq!(s.search.max_page, 0);
        assert_eq!(
            s.search.formats,
            vec![
                "html".to_string(),
                "csv".to_string(),
                "json".to_string(),
                "rss".to_string()
            ]
        );
        assert_eq!(s.search.suspended_times.access_denied, 86400.0);
        assert_eq!(s.search.suspended_times.captcha, 86400.0);
        assert_eq!(s.search.suspended_times.too_many_requests, 3600.0);
        assert_eq!(s.search.suspended_times.cf_captcha, 1296000.0);
        assert_eq!(s.search.suspended_times.cf_access_denied, 86400.0);
        assert_eq!(s.search.suspended_times.recaptcha_captcha, 604800.0);
        assert_eq!(s.server.port, Some(IntOrString::Int(8888)));
        assert_eq!(s.server.bind_address, "127.0.0.1");
        assert!(!s.server.limiter);
        assert!(!s.server.public_instance);
        assert!(s.server.image_proxy);
        assert_eq!(s.server.base_url, Some(BoolOrString::Bool(false)));
        assert_eq!(s.server.http_protocol_version, "1.0");
        assert_eq!(s.server.method, "POST");
        assert_eq!(s.ui.default_theme, "simple");
        assert_eq!(s.ui.theme_args.simple_style, "auto");
        assert_eq!(s.ui.cache_url, "");
        assert!(s.ui.search_on_category_select);
        assert_eq!(s.ui.hotkeys, "default");
        assert_eq!(s.ui.url_formatting, "pretty");
        assert_eq!(s.outgoing.request_timeout, 3.0);
        assert!(s.outgoing.enable_http2);
        assert_eq!(s.outgoing.verify, Some(BoolOrString::Bool(true)));
        assert_eq!(s.outgoing.max_request_timeout, None);
        assert_eq!(s.outgoing.pool_connections, 100);
        assert_eq!(s.outgoing.pool_maxsize, 10);
        assert_eq!(s.outgoing.keepalive_expiry, 5.0);
        assert_eq!(s.outgoing.max_redirects, 30);
        assert_eq!(s.outgoing.retries, 0);
        assert_eq!(s.outgoing.extra_proxy_timeout, 0);
        assert!(!s.outgoing.using_tor_proxy);
        assert_eq!(s.storage.backend, "sqlite");
        assert_eq!(s.storage.sqlite.path, "./zoeken.sqlite3");
        assert!(s.engines.is_empty());
        assert!(s.plugins.0.is_empty());
        assert!(s.preferences.lock.is_empty());
    }

    #[test]
    fn defaults_populate_categories_as_tabs() {
        let s = Settings::defaults();
        let expected = [
            "general",
            "images",
            "videos",
            "news",
            "map",
            "music",
            "it",
            "science",
            "files",
            "shopping",
            "social media",
        ];
        assert_eq!(s.categories.0.len(), expected.len());
        for name in expected {
            assert!(
                s.categories.0.contains_key(name),
                "missing default category tab: {name}"
            );
        }
    }

    #[test]
    fn empty_yaml_yields_reference_defaults() {
        let parsed: Settings = serde_yaml_ng::from_str("{}").expect("deserialize empty document");
        assert_eq!(parsed.general.instance_name, "Search");
        assert_eq!(parsed.outgoing.request_timeout, 3.0);
        assert_eq!(parsed.ui.default_theme, "simple");
        assert_eq!(parsed, Settings::defaults());
    }

    #[test]
    fn partial_file_overlays_only_named_fields_keeping_reference_defaults() {
        let yaml = "search:\n  autocomplete_min: 2\n";
        let parsed: Settings = serde_yaml_ng::from_str(yaml).expect("deserialize partial search");
        assert_eq!(parsed.search.autocomplete_min, 2);
        assert_eq!(parsed.search.ban_time_on_fail, 5.0);
        assert_eq!(parsed.search.suspended_times.captcha, 86400.0);
        assert_eq!(parsed.general.instance_name, "Search");
    }

    use std::path::PathBuf;

    /// Write `contents` to a unique temp `.yml` file and return its path.
    fn write_temp_yaml(contents: &str) -> PathBuf {
        use std::sync::atomic::{AtomicU64, Ordering};
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "zoeken_settings_test_{}_{}.yml",
            std::process::id(),
            n
        ));
        std::fs::write(&path, contents).expect("write temp settings file");
        path
    }

    #[test]
    fn load_with_no_file_yields_reference_defaults() {
        let settings = load_settings(None, &EnvMap::new()).expect("load defaults only");
        assert_eq!(settings, Settings::defaults());
    }

    #[test]
    fn documented_default_config_matches_typed_defaults() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../default.config.yml");
        let settings =
            load_settings(Some(&path), &EnvMap::new()).expect("load documented default config");
        assert_eq!(settings, Settings::defaults());
    }

    #[test]
    fn load_missing_file_path_is_an_error() {
        let missing = std::env::temp_dir().join("zoeken_settings_does_not_exist_zzz.yml");
        let error = load_settings(Some(&missing), &EnvMap::new())
            .expect_err("an explicit missing settings file must fail");
        assert!(matches!(error, SettingsError::FileRead { .. }));
    }

    #[test]
    fn partial_file_overlays_only_specified_keys() {
        let path =
            write_temp_yaml("search:\n  autocomplete_min: 2\nserver:\n  bind_address: 0.0.0.0\n");
        let settings = load_settings(Some(&path), &EnvMap::new()).expect("load partial file");
        std::fs::remove_file(&path).ok();

        assert_eq!(settings.search.autocomplete_min, 2);
        assert_eq!(settings.server.bind_address, "0.0.0.0");
        assert_eq!(settings.search.ban_time_on_fail, 5.0);
        assert_eq!(settings.server.port, Some(IntOrString::Int(8888)));
        assert_eq!(settings.general.instance_name, "Search");
    }

    #[test]
    fn use_default_settings_true_deep_merges_nested_sections() {
        let path = write_temp_yaml(
            "use_default_settings: true\nsearch:\n  suspended_times:\n    SearxEngineCaptcha: 10\n",
        );
        let settings = load_settings(Some(&path), &EnvMap::new()).expect("load merge mode");
        std::fs::remove_file(&path).ok();

        assert_eq!(settings.search.suspended_times.captcha, 10.0);
        assert_eq!(settings.search.suspended_times.access_denied, 86400.0);
        assert_eq!(settings.search.suspended_times.too_many_requests, 3600.0);
    }

    #[test]
    fn suspended_times_deserializes_searxng_yaml_keys() {
        let yaml = "\
SearxEngineAccessDenied: 11
SearxEngineCaptcha: 22
SearxEngineTooManyRequests: 33
cf_SearxEngineCaptcha: 44
cf_SearxEngineAccessDenied: 55
recaptcha_SearxEngineCaptcha: 66
";
        let parsed: SuspendedTimes =
            serde_yaml_ng::from_str(yaml).expect("parse SearXNG suspended_times keys");

        assert_eq!(parsed.access_denied, 11.0);
        assert_eq!(parsed.captcha, 22.0);
        assert_eq!(parsed.too_many_requests, 33.0);
        assert_eq!(parsed.cf_captcha, 44.0);
        assert_eq!(parsed.cf_access_denied, 55.0);
        assert_eq!(parsed.recaptcha_captcha, 66.0);
    }

    #[test]
    fn env_override_wins_over_file_and_defaults() {
        let path = write_temp_yaml("server:\n  bind_address: 10.0.0.1\n  limiter: false\n");
        let env = EnvMap::new()
            .with("APP_BIND_ADDRESS", "0.0.0.0")
            .with("APP_LIMITER", "true")
            .with("APP_SECRET_KEY", "from-env")
            .with("APP_PORT", "9000");
        let settings = load_settings(Some(&path), &env).expect("load with env overrides");
        std::fs::remove_file(&path).ok();

        assert_eq!(settings.server.bind_address, "0.0.0.0");
        assert!(settings.server.limiter);
        assert_eq!(settings.server.secret_key, "from-env");
        assert_eq!(settings.server.port, Some(IntOrString::Int(9000)));
    }

    #[test]
    fn deployment_config_has_bounded_non_unlimited_defaults() {
        let d = DeploymentConfig::default();
        assert_eq!(d.log_level, "info");
        assert_eq!(d.shutdown_grace_seconds, 30);
        assert_eq!(d.max_request_body_bytes, 1024 * 1024);
        assert!(d.max_request_body_bytes > 0);
        assert_eq!(d.request_timeout_seconds, 30);
        assert!(d.request_timeout_seconds > 0);
        assert!(!d.hsts);
        assert!(d.metrics_enabled);
        assert!(d.trusted_proxies.is_empty());
        assert_eq!(
            d.effective_content_security_policy(),
            default_content_security_policy()
        );
        assert!(
            d.effective_content_security_policy()
                .contains("default-src 'self'")
        );
    }

    #[test]
    fn settings_default_includes_deployment_defaults() {
        let s = Settings::defaults();
        assert_eq!(s.deployment, DeploymentConfig::default());
    }

    #[test]
    fn deployment_config_round_trips_through_yaml() {
        let s = Settings::default();
        let yaml = serde_yaml_ng::to_string(&s).expect("serialize settings");
        let parsed: Settings = serde_yaml_ng::from_str(&yaml).expect("deserialize settings");
        assert_eq!(s.deployment, parsed.deployment);
    }

    #[test]
    fn partial_deployment_file_overlays_only_named_fields() {
        let path = write_temp_yaml(
            "deployment:\n  metrics_enabled: false\n  max_request_body_bytes: 2048\n  hsts: true\n",
        );
        let settings = load_settings(Some(&path), &EnvMap::new()).expect("load partial deployment");
        std::fs::remove_file(&path).ok();

        assert!(!settings.deployment.metrics_enabled);
        assert_eq!(settings.deployment.max_request_body_bytes, 2048);
        assert!(settings.deployment.hsts);
        assert_eq!(settings.deployment.log_level, "info");
        assert_eq!(settings.deployment.request_timeout_seconds, 30);
        assert_eq!(settings.deployment.shutdown_grace_seconds, 30);
    }

    #[test]
    fn deployment_custom_csp_overrides_builtin() {
        let path =
            write_temp_yaml("deployment:\n  content_security_policy: \"default-src 'none'\"\n");
        let settings = load_settings(Some(&path), &EnvMap::new()).expect("load custom csp");
        std::fs::remove_file(&path).ok();

        assert_eq!(
            settings.deployment.effective_content_security_policy(),
            "default-src 'none'"
        );
    }

    #[test]
    fn resolve_body_limit_keeps_positive_configured_value() {
        assert_eq!(resolve_max_request_body_bytes(2048), 2048);
        assert_eq!(resolve_max_request_body_bytes(1), 1);
        assert_eq!(
            resolve_max_request_body_bytes(DEFAULT_MAX_REQUEST_BODY_BYTES),
            DEFAULT_MAX_REQUEST_BODY_BYTES
        );
    }

    #[test]
    fn resolve_body_limit_falls_back_to_default_on_zero() {
        assert_eq!(
            resolve_max_request_body_bytes(0),
            DEFAULT_MAX_REQUEST_BODY_BYTES
        );
        assert!(resolve_max_request_body_bytes(0) > 0);
    }

    #[test]
    fn resolve_request_timeout_keeps_positive_configured_value() {
        assert_eq!(resolve_request_timeout_seconds(45), 45);
        assert_eq!(resolve_request_timeout_seconds(1), 1);
        assert_eq!(
            resolve_request_timeout_seconds(DEFAULT_REQUEST_TIMEOUT_SECONDS),
            DEFAULT_REQUEST_TIMEOUT_SECONDS
        );
    }

    #[test]
    fn resolve_request_timeout_falls_back_to_default_on_zero() {
        assert_eq!(
            resolve_request_timeout_seconds(0),
            DEFAULT_REQUEST_TIMEOUT_SECONDS
        );
        assert!(resolve_request_timeout_seconds(0) > 0);
    }

    #[test]
    fn effective_resource_limits_are_strictly_positive_even_when_config_is_zero() {
        let d = DeploymentConfig {
            max_request_body_bytes: 0,
            request_timeout_seconds: 0,
            ..Default::default()
        };

        assert_eq!(
            d.effective_max_request_body_bytes(),
            DEFAULT_MAX_REQUEST_BODY_BYTES
        );
        assert!(d.effective_max_request_body_bytes() > 0);
        assert_eq!(
            d.effective_request_timeout_seconds(),
            DEFAULT_REQUEST_TIMEOUT_SECONDS
        );
        assert!(d.effective_request_timeout_seconds() > 0);
    }

    #[test]
    fn effective_resource_limits_pass_through_positive_config() {
        let d = DeploymentConfig {
            max_request_body_bytes: 4096,
            request_timeout_seconds: 15,
            ..Default::default()
        };
        assert_eq!(d.effective_max_request_body_bytes(), 4096);
        assert_eq!(d.effective_request_timeout_seconds(), 15);
    }

    fn server_with(bind_address: &str, port: Option<IntOrString>) -> ServerSettings {
        ServerSettings {
            bind_address: bind_address.to_string(),
            port,
            ..ServerSettings::default()
        }
    }

    #[test]
    fn resolve_bind_uses_explicit_address_and_port() {
        let server = server_with("0.0.0.0", Some(IntOrString::Int(9090)));
        let addr = resolve_bind(&server).expect("well-formed bind");
        assert_eq!(addr, "0.0.0.0:9090".parse().unwrap());
    }

    #[test]
    fn resolve_bind_accepts_ipv6_address() {
        let server = server_with("::1", Some(IntOrString::Int(8080)));
        let addr = resolve_bind(&server).expect("well-formed ipv6 bind");
        assert_eq!(addr, "[::1]:8080".parse().unwrap());
    }

    #[test]
    fn resolve_bind_accepts_string_port() {
        let server = server_with("127.0.0.1", Some(IntOrString::Str("8443".to_string())));
        let addr = resolve_bind(&server).expect("string port parses");
        assert_eq!(addr, "127.0.0.1:8443".parse().unwrap());
    }

    #[test]
    fn resolve_bind_defaults_to_loopback_when_address_unset() {
        for empty in ["", "   "] {
            let server = server_with(empty, Some(IntOrString::Int(8888)));
            let addr = resolve_bind(&server).expect("empty address defaults to loopback");
            assert!(addr.ip().is_loopback(), "expected loopback for {empty:?}");
            assert_eq!(addr.ip(), IpAddr::V4(Ipv4Addr::LOCALHOST));
            assert_eq!(addr.port(), 8888);
        }
    }

    #[test]
    fn resolve_bind_defaults_port_when_unset() {
        let none_port = resolve_bind(&server_with("127.0.0.1", None)).expect("none port default");
        assert_eq!(none_port.port(), DEFAULT_BIND_PORT);

        let empty_str = resolve_bind(&server_with(
            "127.0.0.1",
            Some(IntOrString::Str("  ".into())),
        ))
        .expect("empty string port default");
        assert_eq!(empty_str.port(), DEFAULT_BIND_PORT);
    }

    #[test]
    fn resolve_bind_rejects_malformed_address_naming_the_value() {
        let server = server_with("not-an-ip", Some(IntOrString::Int(8888)));
        let err = resolve_bind(&server).expect_err("malformed address must be rejected");
        assert_eq!(
            err,
            DeploymentConfigError::BindAddress {
                value: "not-an-ip".to_string()
            }
        );
        assert!(
            err.to_string().contains("not-an-ip"),
            "message must name the offending value: {err}"
        );
    }

    #[test]
    fn resolve_bind_rejects_out_of_range_int_port_naming_the_value() {
        let server = server_with("127.0.0.1", Some(IntOrString::Int(70_000)));
        let err = resolve_bind(&server).expect_err("out-of-range port must be rejected");
        assert_eq!(
            err,
            DeploymentConfigError::Port {
                value: "70000".to_string()
            }
        );
        assert!(
            err.to_string().contains("70000"),
            "message must name the value: {err}"
        );
    }

    #[test]
    fn resolve_bind_rejects_negative_int_port_naming_the_value() {
        let server = server_with("127.0.0.1", Some(IntOrString::Int(-1)));
        let err = resolve_bind(&server).expect_err("negative port must be rejected");
        assert_eq!(
            err,
            DeploymentConfigError::Port {
                value: "-1".to_string()
            }
        );
        assert!(
            err.to_string().contains("-1"),
            "message must name the value: {err}"
        );
    }

    #[test]
    fn resolve_bind_rejects_non_numeric_string_port_naming_the_value() {
        let server = server_with("127.0.0.1", Some(IntOrString::Str("abc".to_string())));
        let err = resolve_bind(&server).expect_err("non-numeric port must be rejected");
        assert_eq!(
            err,
            DeploymentConfigError::Port {
                value: "abc".to_string()
            }
        );
        assert!(
            err.to_string().contains("abc"),
            "message must name the value: {err}"
        );
    }

    #[test]
    fn secret_key_decision_aborts_on_public_bind_with_empty_secret() {
        assert_eq!(secret_key_decision(false, true), SecretKeyDecision::Abort);
    }

    #[test]
    fn secret_key_decision_warns_on_loopback_bind_with_empty_secret() {
        assert_eq!(
            secret_key_decision(true, true),
            SecretKeyDecision::StartWithWarning
        );
    }

    #[test]
    fn secret_key_decision_starts_on_public_bind_with_secret() {
        // A configured secret allows a public bind to start normally.
        assert_eq!(secret_key_decision(false, false), SecretKeyDecision::Start);
    }

    #[test]
    fn secret_key_decision_starts_on_loopback_bind_with_secret() {
        // A configured secret allows a loopback bind to start normally.
        assert_eq!(secret_key_decision(true, false), SecretKeyDecision::Start);
    }

    #[test]
    fn secret_key_is_weak_rejects_placeholders_and_short_values() {
        assert!(secret_key_is_weak(""));
        assert!(secret_key_is_weak("short"));
        assert!(secret_key_is_weak("changeme"));
        assert!(secret_key_is_weak("change-me-to-a-long-random-secret"));
        assert!(secret_key_is_weak("change-me-but-long-enough-xx"));
        assert!(secret_key_is_weak("ci-liveness-check-secret"));
        assert!(!secret_key_is_weak("a-sufficiently-long-random-secret"));
        assert!(!secret_key_is_weak("ci-liveness-check-secret-ok-16"));
    }

    #[test]
    fn secret_key_sourced_from_env_var() {
        let env = EnvMap::new().with("APP_SECRET_KEY", "from-zoeken-env");
        let settings = load_settings(None, &env).expect("load with APP_SECRET_KEY");
        assert_eq!(settings.server.secret_key, "from-zoeken-env");
    }

    #[test]
    fn secret_key_sourced_from_file() {
        let path = write_temp_yaml("server:\n  secret_key: from-file\n");
        let settings = load_settings(Some(&path), &EnvMap::new()).expect("load secret from file");
        std::fs::remove_file(&path).ok();
        assert_eq!(settings.server.secret_key, "from-file");
    }

    #[test]
    fn deployment_env_overrides_win_over_file_and_defaults() {
        let path = write_temp_yaml("deployment:\n  log_level: warn\n  metrics_enabled: true\n");
        let env = EnvMap::new()
            .with("APP_LOG_LEVEL", "debug")
            .with("APP_METRICS_ENABLED", "false");
        let settings = load_settings(Some(&path), &env).expect("load with deployment env");
        std::fs::remove_file(&path).ok();

        assert_eq!(settings.deployment.log_level, "debug");
        assert!(!settings.deployment.metrics_enabled);
    }

    #[test]
    fn per_engine_overlay_replaces_only_named_attributes() {
        let path = write_temp_yaml(
            "use_default_settings: true\n\
             engines:\n\
             \x20 - name: duckduckgo\n\
             \x20   engine: duckduckgo\n\
             \x20   shortcut: ddg\n\
             \x20   timeout: 5.0\n\
             \x20   disabled: false\n",
        );
        let seeded = load_settings(Some(&path), &EnvMap::new()).expect("seed engine");
        std::fs::remove_file(&path).ok();
        assert_eq!(seeded.engines.len(), 1);
        assert_eq!(seeded.engines[0].timeout, Some(5.0));
        assert_eq!(seeded.engines[0].shortcut.as_deref(), Some("ddg"));

        let mut base = serde_yaml_ng::to_value(&seeded).expect("serialize seeded settings");
        let user: Value =
            serde_yaml_ng::from_str("engines:\n  - name: duckduckgo\n    timeout: 9.0\n")
                .expect("parse overlay");
        update_settings(&mut base, &user, None, None);
        let merged: Settings = serde_yaml_ng::from_value(base).expect("deserialize overlaid");

        assert_eq!(merged.engines.len(), 1);
        let engine = &merged.engines[0];
        assert_eq!(engine.timeout, Some(9.0));
        assert_eq!(engine.shortcut.as_deref(), Some("ddg"));
        assert_eq!(engine.engine.as_deref(), Some("duckduckgo"));
        assert_eq!(engine.disabled, Some(false));
    }

    #[test]
    fn validation_error_names_offending_setting() {
        let path = write_temp_yaml("server:\n  method: PUT\n");
        let err = load_settings(Some(&path), &EnvMap::new()).expect_err("invalid method rejected");
        std::fs::remove_file(&path).ok();

        match err {
            SettingsError::Validation { setting, .. } => {
                assert_eq!(setting, "server.method");
            }
            other => panic!("expected validation error, got {other:?}"),
        }
    }

    #[test]
    fn validation_rejects_out_of_range_safe_search() {
        let path = write_temp_yaml("search:\n  safe_search: 5\n");
        let err = load_settings(Some(&path), &EnvMap::new()).expect_err("invalid safe_search");
        std::fs::remove_file(&path).ok();
        assert!(matches!(
            err,
            SettingsError::Validation { ref setting, .. } if setting == "search.safe_search"
        ));
    }

    #[test]
    fn invalid_use_default_settings_is_rejected() {
        let path = write_temp_yaml("use_default_settings: 42\n");
        let err =
            load_settings(Some(&path), &EnvMap::new()).expect_err("invalid use_default_settings");
        std::fs::remove_file(&path).ok();
        assert!(matches!(err, SettingsError::InvalidUseDefaultSettings));
    }

    #[test]
    fn malformed_bool_env_override_is_rejected() {
        let env = EnvMap::new().with("APP_LIMITER", "maybe");
        let err = load_settings(None, &env).expect_err("malformed bool env");
        assert!(matches!(
            err,
            SettingsError::EnvOverride { ref var, .. } if var == "APP_LIMITER"
        ));
    }

    #[test]
    fn use_default_settings_engines_keep_only_filters_defaults() {
        let seed = write_temp_yaml(
            "use_default_settings: true\n\
             engines:\n\
             \x20 - name: alpha\n\
             \x20   engine: alpha\n\
             \x20 - name: beta\n\
             \x20   engine: beta\n",
        );
        let seeded = load_settings(Some(&seed), &EnvMap::new()).expect("seed two engines");
        std::fs::remove_file(&seed).ok();
        assert_eq!(seeded.engines.len(), 2);

        let mut base = serde_yaml_ng::to_value(&seeded).expect("serialize seeded");
        let user: Value = serde_yaml_ng::from_str(
            "use_default_settings:\n  engines:\n    keep_only:\n      - alpha\n",
        )
        .expect("parse keep_only");
        update_settings(&mut base, &user, None, Some(&["alpha".to_string()]));
        let merged: Settings = serde_yaml_ng::from_value(base).expect("deserialize filtered");

        assert_eq!(merged.engines.len(), 1);
        assert_eq!(merged.engines[0].name, "alpha");
    }
}
