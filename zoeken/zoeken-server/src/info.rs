//! Instance-info, health, stats, metrics, and static-document routes.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use base64::Engine as _;
use serde::Serialize;

use zoeken_metrics::{
    CATEGORY_LABEL, ENGINE_ERRORS_TOTAL, ENGINE_LABEL, ENGINE_RESPONSE_TIME_HTTP,
    ENGINE_RESPONSE_TIME_TOTAL,
};
use zoeken_plugins::PluginMetricsSnapshot;

use crate::AppState;

#[derive(Debug, Serialize)]
struct EngineInfo {
    name: String,
    categories: Vec<String>,
    shortcut: String,
    enabled: bool,
    paging: bool,
    language_support: bool,
    languages: Vec<String>,
    regions: Vec<String>,
    safesearch: bool,
    time_range_support: bool,
    timeout: f64,
}

#[derive(Debug, Serialize)]
struct ConfigResponse {
    instance_name: String,
    version: String,
    public_instance: bool,
    engines: Vec<EngineInfo>,
    plugins: Vec<PluginInfo>,
    categories: Vec<String>,
    default_locale: String,
    locales: BTreeMap<String, String>,
    safe_search: u8,
    default_theme: String,
    autocomplete: String,
    autocomplete_min: u32,
    autocomplete_backends: Vec<String>,
    themes: Vec<String>,
    brand: BrandInfo,
    limiter: LimiterInfo,
    doi_resolvers: Vec<String>,
    default_doi_resolver: String,
    categories_as_tabs: Vec<String>,
    ui: UiInfo,
}

#[derive(Debug, Serialize)]
struct UiInfo {
    center_alignment: bool,
    results_on_new_tab: bool,
    query_in_title: bool,
    cache_url: String,
    search_on_category_select: bool,
    hotkeys: String,
    url_formatting: String,
}

#[derive(Debug, Serialize)]
struct PluginInfo {
    id: String,
    name: String,
    description: String,
    enabled: bool,
    default_enabled: bool,
    kind: String,
    keywords: Vec<String>,
    preference_section: String,
    version: String,
    api_version: u32,
    after: Vec<String>,
    before: Vec<String>,
    capabilities: Vec<String>,
}

#[derive(Debug, Serialize)]
#[allow(non_snake_case)]
struct BrandInfo {
    PRIVACYPOLICY_URL: serde_json::Value,
    CONTACT_URL: serde_json::Value,
    GIT_URL: String,
    GIT_BRANCH: String,
    DOCS_URL: String,
}

#[derive(Debug, Serialize)]
struct LimiterInfo {
    enabled: bool,
    #[serde(rename = "botdetection.ip_limit.link_token")]
    link_token: bool,
    #[serde(rename = "botdetection.ip_lists.pass_reserved_nets")]
    pass_reserved_nets: bool,
}

fn engine_infos(state: &AppState) -> Vec<EngineInfo> {
    state
        .search
        .registry()
        .engines()
        .iter()
        .map(|re| {
            let meta = re.engine.metadata();
            EngineInfo {
                name: meta.name.clone(),
                categories: meta.categories.clone(),
                shortcut: meta.shortcut.clone(),
                enabled: !re.disabled,
                paging: meta.paging,
                language_support: meta.language_support,
                languages: state.settings.search.languages.clone(),
                regions: Vec::new(),
                safesearch: meta.safesearch,
                time_range_support: meta.time_range_support,
                timeout: state.settings.outgoing.request_timeout,
            }
        })
        .collect()
}

fn engine_categories(state: &AppState) -> Vec<String> {
    let mut categories: Vec<String> = state
        .search
        .registry()
        .engines()
        .iter()
        .flat_map(|re| re.engine.metadata().categories.clone())
        .collect();
    categories.sort();
    categories.dedup();
    categories
}

/// `GET /config` as JSON.
pub async fn config(State(state): State<Arc<AppState>>) -> Response {
    let response = ConfigResponse {
        categories: engine_categories(&state),
        engines: engine_infos(&state),
        plugins: plugin_infos(&state),
        instance_name: state.settings.general.instance_name.clone(),
        locales: state
            .data
            .locales
            .locale_names
            .clone()
            .into_iter()
            .collect(),
        default_locale: state.settings.ui.default_locale.clone(),
        autocomplete: state.settings.search.autocomplete.clone(),
        autocomplete_min: state.settings.search.autocomplete_min,
        autocomplete_backends: state.data.autocomplete.backends.clone(),
        themes: vec![
            "simple".to_string(),
            "system".to_string(),
            "light".to_string(),
            "dark".to_string(),
        ],
        safe_search: state.settings.search.safe_search,
        default_theme: state.settings.ui.default_theme.clone(),
        version: env!("CARGO_PKG_VERSION").to_string(),
        brand: BrandInfo {
            PRIVACYPOLICY_URL: serde_json::to_value(&state.settings.general.privacypolicy_url)
                .unwrap_or(serde_json::Value::Null),
            CONTACT_URL: serde_json::to_value(&state.settings.general.contact_url)
                .unwrap_or(serde_json::Value::Null),
            GIT_URL: String::new(),
            GIT_BRANCH: String::new(),
            DOCS_URL: state.settings.brand.docs_url.clone(),
        },
        limiter: LimiterInfo {
            enabled: state.limiter_enabled,
            link_token: state.bot_detector.config().link_token,
            pass_reserved_nets: state.bot_detector.config().pass_reserved_nets,
        },
        doi_resolvers: if state.settings.doi_resolvers.is_empty() {
            state.data.doi_resolvers.resolvers.keys().cloned().collect()
        } else {
            state.settings.doi_resolvers.keys().cloned().collect()
        },
        default_doi_resolver: state
            .settings
            .default_doi_resolver
            .clone()
            .unwrap_or_else(|| state.data.doi_resolvers.default.clone()),
        categories_as_tabs: state.settings.categories.0.keys().cloned().collect(),
        ui: UiInfo {
            center_alignment: state.settings.ui.center_alignment,
            results_on_new_tab: state.settings.ui.results_on_new_tab,
            query_in_title: state.settings.ui.query_in_title,
            cache_url: state.settings.ui.cache_url.clone(),
            search_on_category_select: state.settings.ui.search_on_category_select,
            hotkeys: state.settings.ui.hotkeys.clone(),
            url_formatting: state.settings.ui.url_formatting.clone(),
        },
        public_instance: state.settings.server.public_instance,
    };
    json(&response)
}

/// `GET /healthz` liveness probe.
pub async fn healthz() -> Response {
    (
        [(header::CONTENT_TYPE, "application/json")],
        r#"{"status":"ok"}"#,
    )
        .into_response()
}

/// Serve an information page from the bundled data.
pub async fn info_page(
    State(state): State<Arc<AppState>>,
    Path((locale, page)): Path<(String, String)>,
) -> Response {
    let Some((resolved_locale, info)) = state.data.info_pages.resolve(&locale, &page) else {
        return axum::response::Redirect::temporary(&format!("/{page}")).into_response();
    };
    let mut response = (
        [(header::CONTENT_TYPE, "text/plain; charset=utf-8")],
        format!("{}\n\n{}\n", info.title, info.content),
    )
        .into_response();
    if let Ok(value) = HeaderValue::from_str(resolved_locale) {
        response
            .headers_mut()
            .insert(header::CONTENT_LANGUAGE, value);
    }
    response
}

#[derive(Debug, Default, Serialize)]
struct EngineTiming {
    engine: String,
    total_count: u64,
    total_sum_seconds: f64,
    total_avg_seconds: f64,
    http_count: u64,
    http_sum_seconds: f64,
    http_avg_seconds: f64,
}

#[derive(Debug, Default, Serialize)]
struct StatsResponse {
    engines: Vec<EngineTiming>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    plugins: Vec<PluginMetricsSnapshot>,
}

#[derive(Debug, Default, Serialize)]
struct EngineErrors {
    engine: String,
    errors: BTreeMap<String, u64>,
    total: u64,
}

#[derive(Debug, Default, Serialize)]
struct ErrorStatsResponse {
    engines: Vec<EngineErrors>,
}

/// `GET /stats` timing summaries.
pub async fn stats(State(state): State<Arc<AppState>>) -> Response {
    let rendered = state
        .metrics_handle
        .as_ref()
        .map(|h| h.render())
        .unwrap_or_default();
    let mut response = timing_stats(&rendered);
    response.plugins = state.search.plugins().metrics_snapshots();
    json(&response)
}

/// `GET /stats/errors` error counts.
pub async fn stats_errors(State(state): State<Arc<AppState>>) -> Response {
    let rendered = state
        .metrics_handle
        .as_ref()
        .map(|h| h.render())
        .unwrap_or_default();
    json(&error_stats(&rendered))
}

fn timing_stats(rendered: &str) -> StatsResponse {
    let total_count = format!("{ENGINE_RESPONSE_TIME_TOTAL}_count");
    let total_sum = format!("{ENGINE_RESPONSE_TIME_TOTAL}_sum");
    let http_count = format!("{ENGINE_RESPONSE_TIME_HTTP}_count");
    let http_sum = format!("{ENGINE_RESPONSE_TIME_HTTP}_sum");

    let mut engines: BTreeMap<String, EngineTiming> = BTreeMap::new();
    for sample in parse_prometheus(rendered) {
        let Some(engine) = label(&sample, ENGINE_LABEL) else {
            continue;
        };
        let entry = engines
            .entry(engine.to_string())
            .or_insert_with(|| EngineTiming {
                engine: engine.to_string(),
                ..EngineTiming::default()
            });
        if sample.name == total_count {
            entry.total_count = sample.value as u64;
        } else if sample.name == total_sum {
            entry.total_sum_seconds = sample.value;
        } else if sample.name == http_count {
            entry.http_count = sample.value as u64;
        } else if sample.name == http_sum {
            entry.http_sum_seconds = sample.value;
        }
    }

    for entry in engines.values_mut() {
        if entry.total_count > 0 {
            entry.total_avg_seconds = entry.total_sum_seconds / entry.total_count as f64;
        }
        if entry.http_count > 0 {
            entry.http_avg_seconds = entry.http_sum_seconds / entry.http_count as f64;
        }
    }

    StatsResponse {
        engines: engines.into_values().collect(),
        plugins: Vec::new(),
    }
}

fn error_stats(rendered: &str) -> ErrorStatsResponse {
    let with_suffix = format!("{ENGINE_ERRORS_TOTAL}_total");

    let mut engines: BTreeMap<String, EngineErrors> = BTreeMap::new();
    for sample in parse_prometheus(rendered) {
        if sample.name != ENGINE_ERRORS_TOTAL && sample.name != with_suffix {
            continue;
        }
        let (Some(engine), Some(category)) =
            (label(&sample, ENGINE_LABEL), label(&sample, CATEGORY_LABEL))
        else {
            continue;
        };
        let count = sample.value as u64;
        let entry = engines
            .entry(engine.to_string())
            .or_insert_with(|| EngineErrors {
                engine: engine.to_string(),
                ..EngineErrors::default()
            });
        *entry.errors.entry(category.to_string()).or_insert(0) += count;
        entry.total += count;
    }

    ErrorStatsResponse {
        engines: engines.into_values().collect(),
    }
}

const METRICS_CONTENT_TYPE: &str = "text/plain; version=0.0.4; charset=utf-8";

/// `GET /metrics` exposition.
pub async fn metrics(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let password = state.settings.general.open_metrics.as_str();
    if !state.metrics_enabled || password.is_empty() {
        return StatusCode::NOT_FOUND.into_response();
    }
    let authorized = headers
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Basic "))
        .and_then(|encoded| {
            base64::engine::general_purpose::STANDARD
                .decode(encoded)
                .ok()
        })
        .and_then(|decoded| String::from_utf8(decoded).ok())
        .and_then(|credentials| {
            credentials
                .split_once(':')
                .map(|(_, pass)| pass.to_string())
        })
        .is_some_and(|presented| presented == password);
    if !authorized {
        return (
            StatusCode::UNAUTHORIZED,
            [(header::WWW_AUTHENTICATE, "Basic realm=\"metrics\"")],
        )
            .into_response();
    }
    let body = state
        .metrics_handle
        .as_ref()
        .map(|h| h.render())
        .unwrap_or_default();
    ([(header::CONTENT_TYPE, METRICS_CONTENT_TYPE)], body).into_response()
}

/// `GET /opensearch.xml`.
pub async fn opensearch(State(state): State<Arc<AppState>>) -> Response {
    let name = xml_escape(&state.settings.general.instance_name);
    let body = format!(
        concat!(
            "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n",
            "<OpenSearchDescription xmlns=\"http://a9.com/-/spec/opensearch/1.1/\" ",
            "xmlns:moz=\"http://www.mozilla.org/2006/browser/search/\">\n",
            "  <ShortName>{name}</ShortName>\n",
            "  <Description>{name} search</Description>\n",
            "  <InputEncoding>UTF-8</InputEncoding>\n",
            "  <Url type=\"text/html\" method=\"get\" template=\"/search?q={{searchTerms}}\"/>\n",
            "  <Url type=\"application/json\" method=\"get\" ",
            "template=\"/autocompleter?q={{searchTerms}}\"/>\n",
            "  <moz:SearchForm>/search</moz:SearchForm>\n",
            "</OpenSearchDescription>\n",
        ),
        name = name
    );
    (
        [(
            header::CONTENT_TYPE,
            "application/opensearchdescription+xml",
        )],
        body,
    )
        .into_response()
}

/// `GET /robots.txt` crawler policy.
pub async fn robots() -> Response {
    let body = concat!(
        "User-agent: *\n",
        "Allow: /\n",
        "Disallow: /search\n",
        "Disallow: /autocompleter\n",
        "Disallow: /preferences\n",
        "Disallow: /stats\n",
        "Disallow: /image_proxy\n",
        "Disallow: /*?*q=\n",
    );
    ([(header::CONTENT_TYPE, "text/plain; charset=utf-8")], body).into_response()
}

/// `GET /manifest.json`.
pub async fn manifest(State(state): State<Arc<AppState>>) -> Response {
    let name = &state.settings.general.instance_name;
    let colors = &state.settings.brand.pwa_colors;
    let body = serde_json::json!({
        "name": name,
        "short_name": name,
        "start_url": "/",
        "display": "standalone",
        "theme_color": colors.theme_color_light,
        "background_color": colors.background_color_light,
        "icons": [
            {
                "src": "/favicon.ico",
                "sizes": "any",
                "type": "image/svg+xml",
            }
        ],
    })
    .to_string();
    ([(header::CONTENT_TYPE, "application/manifest+json")], body).into_response()
}

pub(crate) const FAVICON_SVG: &str = concat!(
    "<svg xmlns=\"http://www.w3.org/2000/svg\" viewBox=\"0 0 24 24\" width=\"24\" height=\"24\">",
    "<circle cx=\"10\" cy=\"10\" r=\"6\" fill=\"none\" stroke=\"#3050ff\" stroke-width=\"2\"/>",
    "<line x1=\"15\" y1=\"15\" x2=\"21\" y2=\"21\" stroke=\"#3050ff\" stroke-width=\"2\" ",
    "stroke-linecap=\"round\"/>",
    "</svg>",
);

/// `GET /favicon.ico`.
pub async fn favicon() -> Response {
    ([(header::CONTENT_TYPE, "image/svg+xml")], FAVICON_SVG).into_response()
}

/// `GET /engine_descriptions.json`.
pub async fn engine_descriptions(State(state): State<Arc<AppState>>) -> Response {
    let descriptions: BTreeMap<String, [String; 2]> = state
        .search
        .registry()
        .engines()
        .iter()
        .map(|re| {
            let meta = re.engine.metadata();
            let description = meta
                .about
                .website
                .clone()
                .unwrap_or_else(|| format!("{} search engine", meta.name));
            (
                meta.name.clone(),
                [description, "engine config".to_string()],
            )
        })
        .collect();
    json(&descriptions)
}

fn json<T: Serialize>(value: &T) -> Response {
    let body = serde_json::to_string(value).unwrap_or_else(|_| "{}".to_string());
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

fn plugin_infos(state: &AppState) -> Vec<PluginInfo> {
    state
        .search
        .plugins()
        .infos()
        .into_iter()
        .map(|info| PluginInfo {
            id: info.id,
            name: info.name,
            description: info.description,
            enabled: info.default_enabled,
            default_enabled: info.default_enabled,
            kind: match info.kind {
                zoeken_plugins::PluginKind::ResultPlugin => "result_plugin".to_string(),
                zoeken_plugins::PluginKind::Answerer => "answerer".to_string(),
                zoeken_plugins::PluginKind::Both => "both".to_string(),
            },
            keywords: info.keywords,
            preference_section: info.preference_section,
            version: info.version,
            api_version: info.api_version,
            after: info.after,
            before: info.before,
            capabilities: info.capabilities,
        })
        .collect()
}

fn xml_escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&apos;"),
            c => out.push(c),
        }
    }
    out
}

struct Sample {
    name: String,
    labels: Vec<(String, String)>,
    value: f64,
}

fn label<'a>(sample: &'a Sample, key: &str) -> Option<&'a str> {
    sample
        .labels
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.as_str())
}

fn parse_prometheus(text: &str) -> Vec<Sample> {
    text.lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .filter_map(parse_sample_line)
        .collect()
}

/// Parse one non-comment Prometheus sample line.
fn parse_sample_line(line: &str) -> Option<Sample> {
    let (name, labels, rest) = if let Some(open) = line.find('{') {
        let name = line[..open].to_string();
        let close = line[open + 1..].find('}')? + open + 1;
        let labels = parse_labels(&line[open + 1..close]);
        (name, labels, line[close + 1..].trim())
    } else {
        let mut parts = line.splitn(2, char::is_whitespace);
        let name = parts.next()?.to_string();
        (name, Vec::new(), parts.next()?.trim())
    };

    let value = rest.split_whitespace().next()?.parse::<f64>().ok()?;
    Some(Sample {
        name,
        labels,
        value,
    })
}

/// Parse the `key="value",key2="value2"` label block into pairs.
fn parse_labels(labels: &str) -> Vec<(String, String)> {
    labels
        .split(',')
        .filter_map(|part| {
            let part = part.trim();
            if part.is_empty() {
                return None;
            }
            let (key, value) = part.split_once('=')?;
            Some((
                key.trim().to_string(),
                value.trim().trim_matches('"').to_string(),
            ))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, app};

    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use zoeken_data::DataBundle;
    use zoeken_network::NetworkManager;
    use zoeken_settings::Settings;

    fn test_app() -> axum::Router {
        app(AppState::new().expect("build app state"))
    }

    async fn get(uri: &str) -> Response {
        test_app()
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap()
    }

    fn content_type(response: &Response) -> &str {
        response
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .unwrap_or("")
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    async fn body_text(response: Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn config_uses_boot_settings() {
        let mut settings = Settings::default();
        settings.general.instance_name = "BootInstance".to_string();
        let networks = NetworkManager::from_settings(&settings.outgoing).unwrap();
        let boot = crate::boot::Boot {
            settings,
            data: DataBundle::default(),
            networks,
        };
        let app = app(AppState::from_boot(boot).unwrap());
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/config")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        let value = body_json(response).await;
        assert_eq!(value["instance_name"], "BootInstance");
    }

    #[tokio::test]
    async fn config_returns_json_instance_configuration() {
        let response = get("/config").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(content_type(&response), "application/json");

        let value = body_json(response).await;
        // Instance identity and Upstream configuration keys are present.
        assert!(value["instance_name"].is_string());
        assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));
        assert!(value["brand"].is_object());
        assert!(value["limiter"].is_object());
        assert!(value["plugins"].is_array());
        // The default engine (DuckDuckGo) is listed with its categories.
        let engines = value["engines"].as_array().expect("engines array");
        assert!(!engines.is_empty(), "at least one engine is configured");
        assert!(engines.iter().any(|e| e["name"] == "duckduckgo"));
        assert_eq!(value["safe_search"], 0);
    }

    #[tokio::test]
    async fn healthz_reports_ok() {
        let response = get("/healthz").await;
        assert_eq!(response.status(), StatusCode::OK);
        let value = body_json(response).await;
        assert_eq!(value["status"], "ok");
    }

    #[tokio::test]
    async fn stats_returns_empty_engines_without_handle() {
        let response = get("/stats").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(content_type(&response), "application/json");
        let value = body_json(response).await;
        // No handle wired in the default state => empty, not fabricated.
        assert_eq!(value["engines"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn stats_errors_returns_empty_engines_without_handle() {
        let response = get("/stats/errors").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(content_type(&response), "application/json");
        let value = body_json(response).await;
        assert_eq!(value["engines"], serde_json::json!([]));
    }

    #[tokio::test]
    async fn metrics_is_hidden_without_a_configured_password() {
        let response = get("/metrics").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn metrics_requires_basic_auth_password() {
        let mut state = AppState::new().expect("build app state");
        state.settings.general.open_metrics = "secret".to_string();
        let app = app(state);
        let unauthorized = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(unauthorized.status(), StatusCode::UNAUTHORIZED);

        let authorized = app
            .oneshot(
                Request::builder()
                    .uri("/metrics")
                    .header(header::AUTHORIZATION, "Basic dXNlcjpzZWNyZXQ=")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(authorized.status(), StatusCode::OK);
        assert_eq!(content_type(&authorized), METRICS_CONTENT_TYPE);
    }

    #[tokio::test]
    async fn opensearch_returns_description_document() {
        let response = get("/opensearch.xml").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            content_type(&response),
            "application/opensearchdescription+xml"
        );
        let body = body_text(response).await;
        assert!(body.contains("<OpenSearchDescription"));
        assert!(body.contains("/search?q="));
    }

    #[tokio::test]
    async fn robots_returns_plaintext_policy() {
        let response = get("/robots.txt").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(content_type(&response), "text/plain; charset=utf-8");
        let body = body_text(response).await;
        assert!(body.contains("User-agent: *"));
        assert!(body.contains("Disallow: /search"));
    }

    #[tokio::test]
    async fn manifest_returns_web_app_manifest() {
        let response = get("/manifest.json").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(content_type(&response), "application/manifest+json");
        let value = body_json(response).await;
        assert!(value["name"].is_string());
        assert_eq!(value["start_url"], "/");
    }

    #[tokio::test]
    async fn favicon_returns_an_icon() {
        let response = get("/favicon.ico").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(content_type(&response), "image/svg+xml");
        let body = body_text(response).await;
        assert!(body.contains("<svg"));
    }

    #[tokio::test]
    async fn engine_descriptions_lists_engines() {
        let response = get("/engine_descriptions.json").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(content_type(&response), "application/json");
        let value = body_json(response).await;
        let engines = value.as_object().expect("engine description map");
        assert!(engines.contains_key("duckduckgo"));
    }

    #[test]
    fn timing_stats_parses_rendered_summary() {
        // A rendered summary exposition for two engines (as the exporter emits
        // histograms): quantile lines plus `_sum`/`_count`.
        let rendered = concat!(
            "# TYPE zoeken_engine_response_time_total_seconds summary\n",
            "zoeken_engine_response_time_total_seconds{engine=\"wikipedia\",quantile=\"0.5\"} 0.25\n",
            "zoeken_engine_response_time_total_seconds_sum{engine=\"wikipedia\"} 0.5\n",
            "zoeken_engine_response_time_total_seconds_count{engine=\"wikipedia\"} 2\n",
            "# TYPE zoeken_engine_response_time_http_seconds summary\n",
            "zoeken_engine_response_time_http_seconds_sum{engine=\"wikipedia\"} 0.3\n",
            "zoeken_engine_response_time_http_seconds_count{engine=\"wikipedia\"} 2\n",
        );
        let stats = timing_stats(rendered);
        assert_eq!(stats.engines.len(), 1);
        let w = &stats.engines[0];
        assert_eq!(w.engine, "wikipedia");
        assert_eq!(w.total_count, 2);
        assert!((w.total_sum_seconds - 0.5).abs() < 1e-9);
        assert!((w.total_avg_seconds - 0.25).abs() < 1e-9);
        assert_eq!(w.http_count, 2);
        assert!((w.http_avg_seconds - 0.15).abs() < 1e-9);
    }

    #[test]
    fn error_stats_parses_counter_lines() {
        let rendered = concat!(
            "# TYPE zoeken_engine_errors_total counter\n",
            "zoeken_engine_errors_total{engine=\"bing\",category=\"captcha\"} 3\n",
            "zoeken_engine_errors_total{engine=\"bing\",category=\"timeout\"} 1\n",
            "zoeken_engine_errors_total{engine=\"google\",category=\"parse\"} 2\n",
        );
        let stats = error_stats(rendered);
        assert_eq!(stats.engines.len(), 2);
        let bing = stats.engines.iter().find(|e| e.engine == "bing").unwrap();
        assert_eq!(bing.total, 4);
        assert_eq!(bing.errors["captcha"], 3);
        assert_eq!(bing.errors["timeout"], 1);
        let google = stats.engines.iter().find(|e| e.engine == "google").unwrap();
        assert_eq!(google.total, 2);
    }

    #[test]
    fn parse_prometheus_skips_comments_and_blanks() {
        let rendered = "# HELP foo bar\n\nfoo_count{engine=\"a\"} 5\n";
        let samples = parse_prometheus(rendered);
        assert_eq!(samples.len(), 1);
        assert_eq!(samples[0].name, "foo_count");
        assert_eq!(label(&samples[0], "engine"), Some("a"));
        assert!((samples[0].value - 5.0).abs() < 1e-9);
    }
}
