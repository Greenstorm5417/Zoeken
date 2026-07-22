//! `zoeken-server` axum application.

pub mod autocompleter;
pub mod boot;
mod engine_health;
pub mod executor;
pub mod favicon_proxy;
pub mod frontend;
pub mod image_proxy;
pub mod info;
pub mod middleware;
mod outbound_cache;
pub mod preferences;
pub mod readiness;
pub mod serialize;
pub mod serve;
pub mod static_assets;
mod ahmia_filter;
mod tracker_cleanup;

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::extract::{FromRef, RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::get;
use url::form_urlencoded;
use zoeken_engine_core::SuspendConfig;
use zoeken_network::{NetworkError, NetworkManager};
use zoeken_prefs::Preferences;
use zoeken_query::FormParams;
use zoeken_search::{
    EnabledEngineSet, EngineExecutor, EnginePreferences, MetricsRecorder, NoopRecorder, Search,
    SearchConfig, SuspensionPolicy,
};
use zoeken_settings::{DeploymentConfig, OutgoingSettings, Settings};

use crate::executor::NetworkExecutor;
use crate::image_proxy::{ImageProxyFetcher, WreqImageFetcher};
use crate::readiness::ReadinessState;
use crate::static_assets::{AssetSource, DirAssets, INDEX_HTML};
use metrics_exporter_prometheus::PrometheusHandle;
use zoeken_autocomplete::AutocompleteService;
use zoeken_botdetect::{Detector, LimiterConfig};
use zoeken_data::DataBundle;
use zoeken_favicons::{
    FaviconProvider, FaviconService, HttpFaviconResolver, ImageProxyPolicy, InMemoryFaviconCache,
    StaticResolver, StorageFaviconService,
};
use zoeken_storage::FaviconPolicy;

/// Type-erased favicon service held on [`AppState`].
pub type AppFaviconService = dyn FaviconProvider;

fn default_assets_dir() -> DirAssets {
    DirAssets::new(PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets"))
}

#[derive(Clone)]
pub struct AppState {
    search: Search,
    recorder: Arc<dyn MetricsRecorder>,
    image_fetcher: Arc<dyn ImageProxyFetcher>,
    image_policy: ImageProxyPolicy,
    favicons: Arc<AppFaviconService>,
    autocomplete: AutocompleteService,
    pref_defaults: Preferences,
    settings: Settings,
    bot_detector: Arc<Detector>,
    metrics_handle: Option<PrometheusHandle>,
    assets: Arc<dyn AssetSource>,
    readiness: ReadinessState,
    deployment: DeploymentConfig,
    metrics_enabled: bool,
    limiter_enabled: bool,
    data: Arc<DataBundle>,
}

fn image_policy_from_settings(settings: &Settings) -> ImageProxyPolicy {
    let mut policy = ImageProxyPolicy::default();
    if let Some(max) = settings
        .favicons
        .get("max_image_bytes")
        .and_then(|v| v.as_u64())
        .or_else(|| settings.favicons.get("max_bytes").and_then(|v| v.as_u64()))
    {
        policy.max_bytes = max;
    }
    policy
}

#[derive(Debug, thiserror::Error)]
pub enum LimiterLoadError {
    #[error("invalid limiter.toml: {0}")]
    Parse(#[from] zoeken_botdetect::ConfigError),
    #[error("failed to read limiter config `{path}`: {source}")]
    Read {
        path: String,
        #[source]
        source: std::io::Error,
    },
    #[error("limiter.{key} must be a string")]
    InvalidType { key: &'static str },
}

#[derive(Debug, thiserror::Error)]
pub enum AppStateError {
    #[error(transparent)]
    Limiter(#[from] LimiterLoadError),
}

/// Union `deployment.trusted_proxies` into the limiter config (idempotent).
fn merge_deployment_trusted_proxies(config: &mut LimiterConfig, settings: &Settings) {
    if settings.deployment.trusted_proxies.is_empty() {
        return;
    }
    let from_settings =
        crate::middleware::parse_trusted_proxies(&settings.deployment.trusted_proxies);
    for net in from_settings {
        if !config
            .trusted_proxies
            .iter()
            .any(|existing| existing == &net)
        {
            config.trusted_proxies.push(net);
        }
    }
}

fn limiter_from_settings(
    settings: &Settings,
    data: &DataBundle,
) -> Result<Arc<Detector>, LimiterLoadError> {
    let mut config = if let Some(value) = settings.limiter.get("toml") {
        let text = value
            .as_str()
            .ok_or(LimiterLoadError::InvalidType { key: "toml" })?;
        LimiterConfig::from_toml_str(text)?
    } else if let Some(value) = settings.limiter.get("file") {
        let path = value
            .as_str()
            .ok_or(LimiterLoadError::InvalidType { key: "file" })?;
        let text = std::fs::read_to_string(path).map_err(|source| LimiterLoadError::Read {
            path: path.to_string(),
            source,
        })?;
        LimiterConfig::from_toml_str(&text)?
    } else {
        LimiterConfig::from_toml_str(&data.limiter_toml)?
    };

    // Operator-facing `deployment.trusted_proxies` is always unioned into the
    // limiter list (bundled limiter.toml only trusts loopback by default).
    merge_deployment_trusted_proxies(&mut config, settings);

    let link_token = settings
        .limiter
        .get("link_token")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    Ok(Arc::new(Detector::new(config, link_token)))
}

fn default_favicon_service(settings: &Settings) -> Arc<AppFaviconService> {
    Arc::new(FaviconService::new(
        Arc::new(HttpFaviconResolver::for_provider(
            &settings.search.favicon_resolver,
        )),
        InMemoryFaviconCache::new(),
    ))
}

fn persistent_favicon_service(
    settings: &Settings,
    storage: Arc<dyn zoeken_storage::Storage>,
    networks: Arc<NetworkManager>,
) -> Arc<AppFaviconService> {
    Arc::new(StorageFaviconService::new(
        Arc::new(HttpFaviconResolver::for_provider_with_network(
            &settings.search.favicon_resolver,
            networks,
        )),
        storage,
        FaviconPolicy {
            positive_ttl: Duration::from_secs(settings.cache.favicons.positive_ttl_seconds),
            negative_ttl: Duration::from_secs(settings.cache.favicons.negative_ttl_seconds),
            max_blob_bytes: settings.cache.favicons.max_blob_bytes,
            max_total_bytes: settings.cache.favicons.max_total_bytes,
        },
    ))
}

fn search_config_from_settings(settings: &Settings) -> SearchConfig {
    let default_engine_timeout = duration_from_secs_f64(settings.outgoing.request_timeout);
    let max_request_timeout = settings
        .outgoing
        .max_request_timeout
        .map(duration_from_secs_f64)
        .unwrap_or(default_engine_timeout)
        .max(default_engine_timeout);
    SearchConfig {
        default_engine_timeout,
        max_request_timeout,
        suspension: SuspensionPolicy {
            config: SuspendConfig::new(
                1,
                duration_from_secs_f64(settings.search.ban_time_on_fail),
                duration_from_secs_f64(settings.search.max_ban_time_on_fail),
            ),
            access_denied: duration_from_secs_f64(settings.search.suspended_times.access_denied),
            captcha: duration_from_secs_f64(settings.search.suspended_times.captcha),
            too_many_requests: duration_from_secs_f64(
                settings.search.suspended_times.too_many_requests,
            ),
            cf_captcha: duration_from_secs_f64(settings.search.suspended_times.cf_captcha),
            cf_access_denied: duration_from_secs_f64(
                settings.search.suspended_times.cf_access_denied,
            ),
            recaptcha_captcha: duration_from_secs_f64(
                settings.search.suspended_times.recaptcha_captcha,
            ),
        },
    }
}

fn duration_from_secs_f64(value: f64) -> Duration {
    if value.is_finite() && value > 0.0 {
        Duration::from_secs_f64(value)
    } else {
        Duration::ZERO
    }
}

/// Resolve settings-derived fields (`hostnames`, DOI resolver, Tor flag) onto
/// a `DataBundle` clone. Feeds `/config` (`info.rs`'s `hostnames`/`doi_resolvers`)
/// and the SPA client-features that read them.
fn resolved_data_bundle(settings: &Settings, data: &DataBundle) -> DataBundle {
    let mut data = data.clone();
    data.plugin_data.doi_resolver = settings
        .default_doi_resolver
        .as_ref()
        .and_then(|name| settings.doi_resolvers.get(name))
        .or_else(|| settings.doi_resolvers.values().next())
        .or_else(|| {
            data.doi_resolvers
                .resolvers
                .get(&data.doi_resolvers.default)
        })
        .or_else(|| data.doi_resolvers.resolvers.values().next())
        .cloned();
    data.plugin_data.hostnames = hostnames_rules_from_settings(&settings.hostnames.extra);
    data.plugin_data.using_tor_proxy = settings.outgoing.using_tor_proxy;
    data
}

fn hostnames_rules_from_settings(
    extra: &BTreeMap<String, serde_yaml_ng::Value>,
) -> zoeken_data::HostnamesRules {
    zoeken_data::HostnamesRules {
        replace: hostnames_replace(extra.get("replace")),
        remove: yaml_string_list(extra.get("remove")),
        high_priority: yaml_string_list(extra.get("high_priority")),
        low_priority: yaml_string_list(extra.get("low_priority")),
    }
}

fn hostnames_replace(value: Option<&serde_yaml_ng::Value>) -> Vec<(String, String)> {
    let Some(value) = value else {
        return Vec::new();
    };
    match value {
        serde_yaml_ng::Value::Mapping(map) => map
            .iter()
            .filter_map(|(key, value)| Some((yaml_string(key)?, yaml_string(value)?)))
            .collect(),
        serde_yaml_ng::Value::Sequence(seq) => seq
            .iter()
            .filter_map(|item| match item {
                serde_yaml_ng::Value::Mapping(map) => {
                    let pattern = map
                        .get(serde_yaml_ng::Value::String("pattern".to_string()))
                        .and_then(yaml_string)?;
                    let replacement = map
                        .get(serde_yaml_ng::Value::String("replacement".to_string()))
                        .and_then(yaml_string)?;
                    Some((pattern, replacement))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn yaml_string_list(value: Option<&serde_yaml_ng::Value>) -> Vec<String> {
    match value {
        Some(serde_yaml_ng::Value::Sequence(seq)) => seq.iter().filter_map(yaml_string).collect(),
        Some(value) => yaml_string(value).into_iter().collect(),
        None => Vec::new(),
    }
}

fn yaml_string(value: &serde_yaml_ng::Value) -> Option<String> {
    match value {
        serde_yaml_ng::Value::String(value) => Some(value.clone()),
        serde_yaml_ng::Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}


impl AppState {
    pub fn new() -> Result<Self, NetworkError> {
        let outgoing = OutgoingSettings::default();
        let networks = Arc::new(NetworkManager::from_settings(&outgoing)?);
        let executor: Arc<dyn EngineExecutor> = Arc::new(NetworkExecutor::new(networks));

        let registry = zoeken_engines::registry_from_settings(&Settings::default());
        let settings = Settings::default();
        let data = Arc::new(resolved_data_bundle(
            &settings,
            &zoeken_data::load_embedded_bundle()
                .expect("compile-time validated embedded data must load"),
        ));
        let search = Search::new(registry, executor, SearchConfig::default());

        Ok(AppState::from_search(search).with_data(data))
    }

    pub fn from_search(search: Search) -> Self {
        AppState {
            search,
            recorder: Arc::new(NoopRecorder),
            image_fetcher: Arc::new(WreqImageFetcher::new()),
            image_policy: ImageProxyPolicy::default(),
            favicons: Arc::new(FaviconService::new(
                Arc::new(StaticResolver::empty("stub")),
                InMemoryFaviconCache::new(),
            )),
            autocomplete: AutocompleteService::disabled(),
            pref_defaults: Preferences::defaults(),
            settings: Settings::default(),
            bot_detector: Arc::new(Detector::new(LimiterConfig::default(), String::new())),
            metrics_handle: None,
            assets: Arc::new(default_assets_dir()),
            readiness: ReadinessState::new_not_ready(),
            deployment: DeploymentConfig::default(),
            metrics_enabled: DeploymentConfig::default().metrics_enabled,
            limiter_enabled: true,
            data: Arc::new(DataBundle::default()),
        }
    }

    pub fn from_boot(boot: crate::boot::Boot) -> Result<Self, AppStateError> {
        let crate::boot::Boot {
            settings,
            data,
            networks,
        } = boot;

        let data = Arc::new(resolved_data_bundle(&settings, &data));
        let networks = Arc::new(networks);

        let engine_networks: std::collections::HashMap<String, String> = settings
            .engines
            .iter()
            .filter_map(|e| e.network.as_ref().map(|net| (e.name.clone(), net.clone())))
            .collect();
        let executor: Arc<dyn EngineExecutor> = Arc::new(
            NetworkExecutor::new(Arc::clone(&networks))
                .with_engine_networks(engine_networks)
                .with_max_response_bytes(settings.outgoing.max_response_bytes)
                .with_response_cache(
                    Duration::from_secs(settings.cache.search.ttl_seconds),
                    Duration::from_secs(settings.cache.search.structured_ttl_seconds),
                    settings.cache.search.max_bytes,
                ),
        );

        let registry = zoeken_engines::registry_from_settings(&settings);
        let search_config = search_config_from_settings(&settings);
        let search = Search::new(registry, executor, search_config);

        let autocomplete = zoeken_autocomplete::service_for(
            Some(&settings.search.autocomplete),
            Arc::clone(&networks),
        )
        .with_cache(
            Duration::from_secs(settings.cache.autocomplete.ttl_seconds),
            settings.cache.autocomplete.max_entries as usize,
        );

        let deployment = settings.deployment.clone();
        let metrics_enabled = deployment.metrics_enabled && settings.general.enable_metrics;
        let image_policy = image_policy_from_settings(&settings);
        let bot_detector = limiter_from_settings(&settings, &data)?;

        Ok(AppState {
            search,
            recorder: Arc::new(zoeken_metrics::EngineMetricsRecorder::new()),
            // Reuse the browser-emulating `image_proxy` network client so
            // proxied image fetches look like a real browser and share a pool.
            image_fetcher: Arc::new(WreqImageFetcher::with_networks(Arc::clone(&networks))),
            image_policy,
            favicons: networks.coordinator().map_or_else(
                || default_favicon_service(&settings),
                |storage| persistent_favicon_service(&settings, storage, Arc::clone(&networks)),
            ),
            autocomplete,
            pref_defaults: Preferences::defaults(),
            settings,
            bot_detector,
            metrics_handle: None,
            assets: Arc::new(default_assets_dir()),
            readiness: ReadinessState::new_not_ready(),
            deployment,
            metrics_enabled,
            limiter_enabled: true,
            data,
        })
    }

    pub fn with_recorder(mut self, recorder: Arc<dyn MetricsRecorder>) -> Self {
        self.recorder = recorder;
        self
    }

    pub fn with_image_fetcher(mut self, fetcher: Arc<dyn ImageProxyFetcher>) -> Self {
        self.image_fetcher = fetcher;
        self
    }

    pub fn with_image_policy(mut self, policy: ImageProxyPolicy) -> Self {
        self.image_policy = policy;
        self
    }

    pub fn with_favicons(mut self, favicons: Arc<AppFaviconService>) -> Self {
        self.favicons = favicons;
        self
    }

    pub fn with_autocomplete(mut self, autocomplete: AutocompleteService) -> Self {
        self.autocomplete = autocomplete;
        self
    }

    pub fn with_data(mut self, data: Arc<DataBundle>) -> Self {
        self.data = data;
        self
    }

    pub fn with_settings(mut self, settings: Settings) -> Self {
        self.settings = settings;
        self
    }

    pub fn with_pref_defaults(mut self, defaults: Preferences) -> Self {
        self.pref_defaults = defaults;
        self
    }

    pub fn with_limiter_config(
        mut self,
        config: LimiterConfig,
        link_token: impl Into<String>,
    ) -> Self {
        self.bot_detector = Arc::new(Detector::new(config, link_token));
        self
    }

    pub fn with_bot_detector(mut self, detector: Arc<Detector>) -> Self {
        self.bot_detector = detector;
        self
    }

    pub fn with_metrics_handle(mut self, handle: PrometheusHandle) -> Self {
        self.metrics_handle = Some(handle);
        self
    }

    pub fn with_assets(mut self, assets: Arc<dyn AssetSource>) -> Self {
        self.assets = assets;
        self
    }

    pub fn with_readiness(mut self, readiness: ReadinessState) -> Self {
        self.readiness = readiness;
        self
    }

    pub fn with_deployment(mut self, cfg: DeploymentConfig) -> Self {
        self.metrics_enabled = cfg.metrics_enabled;
        self.deployment = cfg;
        self
    }

    pub fn with_metrics_enabled(mut self, enabled: bool) -> Self {
        self.metrics_enabled = enabled;
        self
    }

    pub fn with_limiter_enabled(mut self, enabled: bool) -> Self {
        self.limiter_enabled = enabled;
        self
    }
}

impl FromRef<Arc<AppState>> for ReadinessState {
    fn from_ref(state: &Arc<AppState>) -> Self {
        state.readiness.clone()
    }
}

pub fn app(state: AppState) -> Router {
    let bot_detector = state.bot_detector.clone();
    let deployment = state.deployment.clone();
    let limiter_enabled = state.limiter_enabled;
    let default_http_headers = state.settings.server.default_http_headers.clone();
    let http_protocol_version = state.settings.server.http_protocol_version.clone();

    let router = Router::new()
        .route("/", get(frontend::index).post(frontend::index))
        .route("/search", get(search_get).post(search_post))
        .route(
            "/autocompleter",
            get(autocompleter::autocompleter_get).post(autocompleter::autocompleter_post),
        )
        .route("/image_proxy", get(image_proxy::image_proxy_get))
        .route("/favicon_proxy", get(favicon_proxy::favicon_proxy_get))
        .route(
            "/preferences",
            get(preferences::preferences_get).post(preferences::preferences_post),
        )
        .route("/clear_cookies", get(preferences::clear_cookies))
        .route("/info/{locale}/{page}", get(info::info_page))
        .route("/about", get(frontend::about))
        .route("/rss.xsl", get(frontend::rss_xsl).post(frontend::rss_xsl))
        .route("/logo/{resolution}", get(frontend::logo))
        .route("/config", get(info::config))
        .route("/bangs", get(info::bangs))
        .route("/healthz", get(info::healthz))
        .route("/readyz", get(readiness::readyz))
        .route("/stats", get(info::stats))
        .route("/stats/errors", get(info::stats_errors))
        .route("/metrics", get(info::metrics))
        .route("/opensearch.xml", get(info::opensearch))
        .route("/robots.txt", get(info::robots))
        .route("/sitemap.xml", get(info::sitemap))
        .route("/manifest.json", get(info::manifest))
        .route("/favicon.ico", get(info::favicon))
        .route("/engine_descriptions.json", get(info::engine_descriptions))
        // Axum forbids `{param}` plus a suffix in one path segment, so the compatible
        // GET/POST `/client{token}.css` endpoint is handled by this fallback.
        .fallback(frontend::client_css_or_static)
        .with_state(Arc::new(state));

    let limiter = limiter_enabled.then(|| zoeken_botdetect::layer(bot_detector));
    crate::middleware::apply_middleware(
        router,
        &deployment,
        &default_http_headers,
        &http_protocol_version,
        limiter,
    )
}

async fn search_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let params = FormParams::from_pairs(parse_pairs(query.as_deref().unwrap_or("")));
    run_search(&state, &headers, params).await
}

async fn search_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: String,
) -> Response {
    let mut pairs = parse_pairs(query.as_deref().unwrap_or(""));
    pairs.extend(parse_pairs(&body));
    run_search(&state, &headers, FormParams::from_pairs(pairs)).await
}

async fn run_search(state: &AppState, headers: &HeaderMap, params: FormParams) -> Response {
    let format = match serialize::OutputFormat::from_param(params.get("format")) {
        Ok(format) => format,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };
    if !state
        .settings
        .search
        .formats
        .iter()
        .any(|allowed| allowed == format.as_str())
    {
        return (
            StatusCode::BAD_REQUEST,
            format!("output format disabled by settings: '{}'", format.as_str()),
        )
            .into_response();
    }

    if matches!(format, serialize::OutputFormat::Html) {
        return frontend_index_response(state, headers);
    }

    let pref_cookie = preferences::read_pref_cookie(headers);
    let resolved_prefs = zoeken_prefs::resolve_with_data(
        &state.pref_defaults,
        &state.settings,
        pref_cookie.as_deref(),
        &params,
        &state.data,
    );

    let query = match zoeken_query::from_params(&params, &resolved_prefs, &state.data) {
        Ok(query) => query,
        Err(error) => return (StatusCode::BAD_REQUEST, error.to_string()).into_response(),
    };

    if state.settings.search.max_page > 0 && query.pageno > state.settings.search.max_page {
        return (
            StatusCode::BAD_REQUEST,
            format!(
                "pageno exceeds configured maximum of {}",
                state.settings.search.max_page
            ),
        )
            .into_response();
    }
    if let Some(external) = &query.external_bang {
        return axum::response::Redirect::temporary(&external.target_url).into_response();
    }

    let tokens = request_tokens(&params);
    let mut container = state
        .search
        .run(
            &query,
            &engine_preferences(&resolved_prefs),
            &tokens,
            state.recorder.as_ref(),
        )
        .await;
    if resolved_prefs
        .plugins
        .get("tracker_url_remover")
        .copied()
        .unwrap_or(true)
    {
        tracker_cleanup::strip_trackers(&mut container, &state.data.tracker_patterns);
    }
    if resolved_prefs.plugins.get("ahmia_filter").copied().unwrap_or(true) {
        ahmia_filter::filter_blacklisted_onions(
            &mut container,
            &state.data.ahmia_blacklist,
            state.settings.outgoing.using_tor_proxy,
        );
    }

    if query.redirect.is_some()
        && let Some(target) = container.results.iter().find_map(|result| match result {
            zoeken_results::Result_::Main(result) => Some(result.url.as_str()),
            zoeken_results::Result_::Image(result) => Some(result.url.as_str()),
            _ => None,
        })
    {
        return axum::response::Redirect::temporary(target).into_response();
    }

    // JSON (Req 14.2), CSV (Req 14.3), and RSS (Req 14.4) are served. The match
    // keeps the format → serializer wiring explicit.
    match format {
        serialize::OutputFormat::Html => frontend_index_response(state, headers),
        serialize::OutputFormat::Json => {
            let body = serialize::format_json_for_query_with_proxies(
                &query.query,
                &container,
                serialize::ProxySettings {
                    secret_key: &state.settings.server.secret_key,
                    image_proxy: resolved_prefs.image_proxy,
                    favicon_proxy: !state.settings.search.favicon_resolver.is_empty(),
                },
            );
            ([(header::CONTENT_TYPE, "application/json")], body).into_response()
        }
        serialize::OutputFormat::Csv => {
            let body = serialize::format_csv(&container);
            (
                [
                    (header::CONTENT_TYPE, "application/csv".to_string()),
                    (
                        header::CONTENT_DISPOSITION,
                        format!("attachment;Filename=search_-_{}.csv", query.query),
                    ),
                ],
                body,
            )
                .into_response()
        }
        serialize::OutputFormat::Rss => {
            let body = serialize::format_rss(&container);
            (
                [(header::CONTENT_TYPE, "application/rss+xml; charset=utf-8")],
                body,
            )
                .into_response()
        }
    }
}

#[derive(Debug, Clone)]
enum RequestEnginePreferences {
    All,
    Enabled(EnabledEngineSet),
}

impl EnginePreferences for RequestEnginePreferences {
    fn is_engine_enabled(&self, engine: &str) -> bool {
        match self {
            RequestEnginePreferences::All => true,
            RequestEnginePreferences::Enabled(enabled) => enabled.is_engine_enabled(engine),
        }
    }
}

fn engine_preferences(prefs: &Preferences) -> RequestEnginePreferences {
    if prefs.engines.is_empty() {
        RequestEnginePreferences::All
    } else {
        RequestEnginePreferences::Enabled(EnabledEngineSet::new(prefs.engines.clone()))
    }
}

fn request_tokens(params: &FormParams) -> HashSet<String> {
    params
        .get("tokens")
        .or_else(|| params.get("engine_tokens"))
        .map(split_csv_set)
        .unwrap_or_default()
}

fn split_csv_set(value: &str) -> HashSet<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .collect()
}

pub(crate) fn frontend_index_response(state: &AppState, headers: &HeaderMap) -> Response {
    match state.assets.get(INDEX_HTML) {
        Some(bytes) => {
            let html = String::from_utf8_lossy(&bytes);
            let origin = configured_or_request_origin(state, headers).unwrap_or_default();
            let body = html.replace("__ORIGIN__", &origin);
            ([(header::CONTENT_TYPE, "text/html; charset=utf-8")], body).into_response()
        }
        None => (
            StatusCode::INTERNAL_SERVER_ERROR,
            "frontend assets missing: index.html",
        )
            .into_response(),
    }
}

fn configured_or_request_origin(state: &AppState, headers: &HeaderMap) -> Option<String> {
    let base = match state.settings.server.base_url.as_ref() {
        Some(zoeken_settings::BoolOrString::Str(url)) if !url.is_empty() => Some(url.as_str()),
        _ => None,
    };
    crate::middleware::instance_origin(base, state.deployment.hsts, headers)
}

pub(crate) fn parse_pairs(raw: &str) -> Vec<(String, String)> {
    form_urlencoded::parse(raw.as_bytes())
        .into_owned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_pairs_preserves_order_and_duplicates() {
        let pairs = parse_pairs("q=rust&category_general=1&category_general=0");
        assert_eq!(
            pairs,
            vec![
                ("q".to_string(), "rust".to_string()),
                ("category_general".to_string(), "1".to_string()),
                ("category_general".to_string(), "0".to_string()),
            ]
        );
    }

    #[test]
    fn parse_pairs_decodes_percent_and_plus() {
        let pairs = parse_pairs("q=rust+lang%20rocks");
        assert_eq!(
            pairs,
            vec![("q".to_string(), "rust lang rocks".to_string())]
        );
    }

    #[test]
    fn deployment_trusted_proxies_merge_into_non_empty_limiter_list() {
        use std::str::FromStr;

        use ipnet::IpNet;

        let settings: Settings = serde_yaml_ng::from_str(
            r#"
deployment:
  trusted_proxies:
    - 10.0.0.0/8
    - 127.0.0.0/8
"#,
        )
        .expect("settings parse");

        let data = zoeken_data::load_embedded_bundle().expect("embedded data");
        let mut config = LimiterConfig::from_toml_str(&data.limiter_toml).expect("bundled limiter");
        assert!(
            !config.trusted_proxies.is_empty(),
            "bundled limiter must already list loopback"
        );
        let before = config.trusted_proxies.len();
        merge_deployment_trusted_proxies(&mut config, &settings);

        let docker_bridge = IpNet::from_str("10.0.0.0/8").unwrap();
        assert!(
            config.trusted_proxies.iter().any(|n| n == &docker_bridge),
            "settings CIDR must be unioned even when limiter.toml is non-empty"
        );
        // 127.0.0.0/8 was already present — merge must not duplicate it.
        assert_eq!(config.trusted_proxies.len(), before + 1);
    }

}

#[cfg(test)]
mod route_tests {
    use super::*;

    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;
    use zoeken_engine_core::{
        Engine, EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams,
        SearchQueryView,
    };
    use zoeken_results::{MainResult, Result_};
    use zoeken_search::{EngineExecResult, EngineFuture, EngineRegistry, RegisteredEngine};

    struct StubEngine {
        meta: EngineMeta,
    }

    impl Engine for StubEngine {
        fn metadata(&self) -> &EngineMeta {
            &self.meta
        }
        fn request(&self, _q: &SearchQueryView, _p: &mut RequestParams) {}
        fn response(&self, _resp: &EngineResponse) -> Result<EngineResults, EngineError> {
            Ok(EngineResults::new())
        }
    }

    struct ImmediateExecutor;

    impl EngineExecutor for ImmediateExecutor {
        fn execute(&self, engine: Arc<dyn Engine>, _query: SearchQueryView) -> EngineFuture {
            let name = engine.metadata().name.clone();
            Box::pin(async move {
                let mut results = EngineResults::new();
                results.add(Result_::Main(MainResult {
                    url: format!("https://{name}.test/"),
                    normalized_url: format!("https://{name}.test/"),
                    title: name.clone(),
                    engine: name,
                    ..MainResult::default()
                }));
                EngineExecResult::from_result(Ok(results))
            })
        }
    }

    fn test_app() -> Router {
        let engine = StubEngine {
            meta: EngineMeta {
                name: "stub".to_string(),
                categories: vec!["general".to_string()],
                ..EngineMeta::default()
            },
        };
        let registry = EngineRegistry::from_engines([RegisteredEngine::new(Arc::new(engine))]);
        let executor: Arc<dyn EngineExecutor> = Arc::new(ImmediateExecutor);
        let search = Search::new(registry, executor, SearchConfig::default());
        app(AppState::from_search(search))
    }

    #[tokio::test]
    async fn get_search_flows_through_to_json_response() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/search?q=rust&format=json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["query"], "rust");
        assert_eq!(value["results"].as_array().unwrap().len(), 1);
        assert_eq!(value["results"][0]["engine"], "stub");
    }

    #[tokio::test]
    async fn post_search_reads_form_body() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/search?format=json")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("q=rust"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[tokio::test]
    async fn explicit_json_format_is_served() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/search?q=rust&format=json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
    }

    #[tokio::test]
    async fn csv_format_is_served() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/search?q=rust&format=csv")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/csv"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        // The header row is present and the stub engine's result is a data row.
        let mut lines = text.lines();
        assert_eq!(
            lines.next().unwrap(),
            "title,url,content,host,engine,score,type"
        );
        let row = lines.next().expect("one data row for the stub result");
        assert!(
            row.starts_with("stub,"),
            "row should carry the stub result: {row}"
        );
    }

    /// `format=rss` is served as RSS with an `application/rss+xml` content type.
    #[tokio::test]
    async fn rss_format_is_served() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/search?q=rust&format=rss")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/rss+xml; charset=utf-8"
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        // A well-formed RSS 2.0 feed carrying the stub engine's result as an item.
        assert!(text.contains("<rss version=\"2.0\">"));
        assert!(text.contains("<item>"));
        assert!(text.contains("<title>stub</title>"));
    }

    /// An unsupported output format is rejected with a client-error status and
    /// an error message naming the format, before the search runs.
    #[tokio::test]
    async fn unsupported_format_is_rejected_with_client_error() {
        for raw in ["xml", "bogus"] {
            let response = test_app()
                .oneshot(
                    Request::builder()
                        .uri(format!("/search?q=rust&format={raw}"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();

            assert_eq!(
                response.status(),
                StatusCode::BAD_REQUEST,
                "format={raw} should be a client error"
            );
            let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
            let text = String::from_utf8(body.to_vec()).unwrap();
            assert!(
                text.contains(raw),
                "error should name the offending format {raw}: {text}"
            );
        }
    }

    /// A request that fails to parse into a query is rejected with
    /// `400 Bad Request` naming the offending parameter, before the search runs.
    #[tokio::test]
    async fn missing_query_is_rejected_with_bad_request() {
        let response = test_app()
            .oneshot(
                Request::builder()
                    .uri("/search?format=json")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let text = String::from_utf8(body.to_vec()).unwrap();
        // The error message names the offending parameter (`q`).
        assert!(
            text.contains('q'),
            "error should name the parameter: {text}"
        );
    }
}
