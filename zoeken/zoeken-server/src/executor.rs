//! A network-backed [`EngineExecutor`] for engine HTTP calls.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use url::form_urlencoded;
use wreq::header::{ACCEPT_LANGUAGE, AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use wreq::{Method, Response};
use zoeken_engine_core::{
    Engine, EngineError, EngineResponse, EngineResults, HttpMethod, Processor, RequestParams,
    SearchQueryView, TlsVerify,
};
use zoeken_network::{DEFAULT_NETWORK, NetworkError, NetworkManager, NetworkRequest};
use zoeken_search::{EngineExecResult, EngineExecutor, EngineFuture};

#[derive(Clone)]
pub struct NetworkExecutor {
    networks: Arc<NetworkManager>,
    engine_networks: HashMap<String, String>,
    max_response_bytes: usize,
    response_cache: Arc<ResponseCache>,
}

const DEFAULT_MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

/// Engines whose upstream responses are safe to cache briefly: idempotent
/// instant-answer / infobox lookups that repeat often and query rate-limited
/// public endpoints (Wikidata's WDQS especially). Caching successful responses
/// cuts repeat load — the main lever against WDQS's aggressive HTTP 429s.
const CACHEABLE_ENGINES: &[&str] = &["wikidata", "currency", "dictionary"];

/// Time a cached upstream response stays fresh.
const RESPONSE_CACHE_TTL: Duration = Duration::from_secs(300);

/// Cap on cached entries; the cache is cleared wholesale when it grows past.
const RESPONSE_CACHE_CAPACITY: usize = 512;

struct CachedResponse {
    at: Instant,
    response: EngineResponse,
}

/// A tiny TTL response cache keyed by `engine\u{1f}url\u{1f}body`.
#[derive(Default)]
struct ResponseCache {
    entries: Mutex<HashMap<String, CachedResponse>>,
}

impl ResponseCache {
    fn get(&self, key: &str) -> Option<EngineResponse> {
        let entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;
        (entry.at.elapsed() < RESPONSE_CACHE_TTL).then(|| entry.response.clone())
    }

    fn put(&self, key: String, response: EngineResponse) {
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        if entries.len() >= RESPONSE_CACHE_CAPACITY {
            entries.clear();
        }
        entries.insert(
            key,
            CachedResponse {
                at: Instant::now(),
                response,
            },
        );
    }
}

fn cache_key(engine: &str, url: &str, body: Option<&[u8]>) -> String {
    let body = body.map(<[u8]>::to_vec).unwrap_or_default();
    format!(
        "{engine}\u{1f}{url}\u{1f}{}",
        String::from_utf8_lossy(&body)
    )
}

impl NetworkExecutor {
    pub fn new(networks: Arc<NetworkManager>) -> Self {
        NetworkExecutor {
            networks,
            engine_networks: HashMap::new(),
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            response_cache: Arc::new(ResponseCache::default()),
        }
    }

    /// Set the per-engine network name (from `settings.engines[].network`).
    pub fn with_engine_networks(mut self, engine_networks: HashMap<String, String>) -> Self {
        self.engine_networks = engine_networks;
        self
    }

    pub fn with_max_response_bytes(mut self, max_response_bytes: usize) -> Self {
        self.max_response_bytes = max_response_bytes.max(1);
        self
    }
}

impl EngineExecutor for NetworkExecutor {
    fn execute(&self, engine: Arc<dyn Engine>, query: SearchQueryView) -> EngineFuture {
        let networks = self.networks.clone();
        let engine_name = engine.metadata().name.clone();
        let engine_networks = self.engine_networks.clone();
        let max_response_bytes = self.max_response_bytes;
        let response_cache = self.response_cache.clone();
        Box::pin(async move {
            let mut params = RequestParams {
                query: query.query.clone(),
                pageno: query.pageno,
                safesearch: query.safesearch,
                time_range: query.time_range,
                locale_key: query.locale.clone(),
                engine_data: query.engine_data.clone(),
                ..RequestParams::default()
            };
            if engine_name == "soundcloud"
                && !params.engine_data.contains_key("client_id")
                && let Some(id) = soundcloud_client_id(&networks).await
            {
                params.engine_data.insert("client_id".to_string(), id);
            }

            engine.request(&query, &mut params);

            let network_name = params
                .network
                .clone()
                .or_else(|| engine_networks.get(&engine_name).cloned())
                .unwrap_or_else(|| DEFAULT_NETWORK.to_string());

            let Some(url) = params.url.clone() else {
                if engine.metadata().engine_type != Processor::Online {
                    return EngineExecResult::from_result(
                        engine.response(&EngineResponse::default()),
                    );
                }
                return EngineExecResult::from_result(Ok(EngineResults::new()));
            };

            let request = match build_network_request(&params, &url) {
                Ok(request) => request,
                Err(error) => return EngineExecResult::from_result(Err(error)),
            };

            // Serve a fresh cached upstream response for idempotent engines,
            // skipping the network round-trip entirely.
            let cacheable = CACHEABLE_ENGINES.contains(&engine_name.as_str());
            let key = cacheable.then(|| cache_key(&engine_name, &url, request.body.as_deref()));
            if let Some(key) = &key
                && let Some(cached) = response_cache.get(key)
            {
                return EngineExecResult {
                    result: engine.response(&cached),
                    http_duration: None,
                };
            }

            let http_started = Instant::now();
            let response = match networks.request(&network_name, request).await {
                Ok(response) => response,
                Err(error) => {
                    return EngineExecResult {
                        result: Err(map_network_error(error)),
                        http_duration: Some(http_started.elapsed()),
                    };
                }
            };
            let engine_response = match adapt_response(response, max_response_bytes).await {
                Ok(response) => response,
                Err(error) => {
                    return EngineExecResult {
                        result: Err(error),
                        http_duration: Some(http_started.elapsed()),
                    };
                }
            };
            // Only cache successful responses so a transient rate-limit / error
            // is retried on the next search rather than pinned for the TTL.
            if let Some(key) = key
                && engine_response.status == 200
            {
                response_cache.put(key, engine_response.clone());
            }
            let http_duration = Some(http_started.elapsed());
            EngineExecResult {
                result: engine.response(&engine_response),
                http_duration,
            }
        })
    }
}

fn build_network_request(params: &RequestParams, url: &str) -> Result<NetworkRequest, EngineError> {
    let method = match params.method {
        HttpMethod::Get => Method::GET,
        HttpMethod::Post => Method::POST,
    };

    let mut headers = HeaderMap::new();
    for (name, value) in &params.headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            headers.insert(name, value);
        }
    }
    if let Some(auth) = &params.auth
        && !headers.contains_key(AUTHORIZATION)
        && let Ok(value) = HeaderValue::from_str(auth)
    {
        headers.insert(AUTHORIZATION, value);
    }
    if !params.locale_key.is_empty()
        && !matches!(params.locale_key.as_str(), "all" | "auto")
        && !headers.contains_key(ACCEPT_LANGUAGE)
        && let Ok(value) = HeaderValue::from_str(&browser_accept_language(&params.locale_key))
    {
        headers.insert(ACCEPT_LANGUAGE, value);
    }

    let cookies: Vec<(String, String)> = params
        .cookies
        .iter()
        .map(|(name, value)| (name.clone(), value.clone()))
        .collect();

    let mut request = NetworkRequest::new(method, url.to_string())
        .with_headers(headers)
        .with_cookies(cookies)
        .with_raise_for_httperror(params.raise_for_httperror);
    if params.allow_redirects || params.max_redirects > 0 || params.soft_max_redirects > 0 {
        let max_redirects = params
            .max_redirects
            .max(params.soft_max_redirects)
            .max(u32::from(params.allow_redirects));
        request = request.with_max_redirects(max_redirects as usize);
    }
    match &params.verify {
        TlsVerify::Default => {}
        TlsVerify::Disabled => request = request.with_verify(false),
        TlsVerify::CaFile(_) => request = request.with_verify(true),
    }

    if let Some(json) = &params.json {
        let body = serde_json::to_vec(json).map_err(|e| EngineError::Unexpected(e.to_string()))?;
        set_content_type_if_absent(&mut request, "application/json");
        request = request.with_body(body);
    } else if !params.data.is_empty() {
        let body = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(params.data.iter())
            .finish();
        set_content_type_if_absent(&mut request, "application/x-www-form-urlencoded");
        request = request.with_body(body.into_bytes());
    } else if !params.content.is_empty() {
        request = request.with_body(params.content.clone());
    }

    Ok(request)
}

/// Format a locale as a browser-style `Accept-Language` q-list. Real browsers
/// never send a bare `de-DE`; a q-graded list blends in with organic traffic.
fn browser_accept_language(locale: &str) -> String {
    let lang = locale
        .split(['-', '_'])
        .next()
        .unwrap_or(locale)
        .to_ascii_lowercase();
    if lang == "en" {
        if locale == lang {
            "en-US,en;q=0.9".to_string()
        } else {
            format!("{locale},en;q=0.9")
        }
    } else if locale == lang {
        format!("{locale},en;q=0.8")
    } else {
        format!("{locale},{lang};q=0.9,en;q=0.8")
    }
}

/// Set a `Content-Type` header on `request` unless the engine already supplied
/// one. Engines that POST form data or JSON rely on the transport to declare
/// the body encoding (the reference httpx sets this automatically); some
/// upstreams (e.g. the Wikidata SPARQL endpoint) reject a POST without it.
fn set_content_type_if_absent(request: &mut NetworkRequest, content_type: &'static str) {
    if request.headers.contains_key(wreq::header::CONTENT_TYPE) {
        return;
    }
    request.headers.insert(
        wreq::header::CONTENT_TYPE,
        HeaderValue::from_static(content_type),
    );
}

/// Adapt a `wreq` [`Response`] into the engine-facing [`EngineResponse`],
/// reading the full body so the engine can parse it.
async fn adapt_response(
    response: Response,
    max_bytes: usize,
) -> Result<EngineResponse, EngineError> {
    let status = response.status().as_u16();
    let url = response.uri().to_string();

    let mut headers = HashMap::new();
    for (name, value) in response.headers() {
        if let Ok(value) = value.to_str() {
            headers.insert(name.as_str().to_string(), value.to_string());
        }
    }

    let mut body = Vec::new();
    let mut stream = response.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk
            .map_err(|e| EngineError::Unexpected(format!("failed to read response body: {e}")))?;
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err(EngineError::Unexpected(format!(
                "upstream response exceeds {max_bytes} byte limit"
            )));
        }
        body.extend_from_slice(&chunk);
    }

    Ok(EngineResponse {
        status,
        url,
        headers,
        body,
    })
}

/// Fetch SoundCloud's guest `client_id` once per process (cached), mirroring the reference `get_client_id`.
async fn soundcloud_client_id(networks: &NetworkManager) -> Option<String> {
    static CACHE: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    CACHE
        .get_or_try_init(|| fetch_soundcloud_client_id(networks))
        .await
        .ok()
        .cloned()
}

/// Scrape the SoundCloud web app and its JS assets for a guest `client_id`.
async fn fetch_soundcloud_client_id(networks: &NetworkManager) -> Result<String, ()> {
    let home = networks
        .request("soundcloud", NetworkRequest::get("https://soundcloud.com/"))
        .await
        .map_err(|_| ())?;
    let html = home.text().await.map_err(|_| ())?;

    for asset_url in soundcloud_asset_urls(&html) {
        let Ok(resp) = networks
            .request("soundcloud", NetworkRequest::get(asset_url))
            .await
        else {
            continue;
        };
        let Ok(js) = resp.text().await else {
            continue;
        };
        if let Some(id) = extract_client_id(&js) {
            return Ok(id);
        }
    }
    Err(())
}

/// The SoundCloud web-app JS asset URLs referenced in the home page HTML.
fn soundcloud_asset_urls(html: &str) -> Vec<String> {
    const PREFIX: &str = "https://a-v2.sndcdn.com/assets/";
    let mut urls = Vec::new();
    let mut rest = html;
    while let Some(pos) = rest.find(PREFIX) {
        let after = &rest[pos..];
        let end = after.find(['"', '\'']).unwrap_or(after.len());
        let url = &after[..end];
        if url.ends_with(".js") {
            urls.push(url.to_string());
        }
        rest = &after[end..];
    }
    urls
}

/// Extract the `client_id:"..."` guest token from a SoundCloud JS asset body.
fn extract_client_id(js: &str) -> Option<String> {
    const KEY: &str = "client_id:\"";
    let start = js.find(KEY)? + KEY.len();
    let tail = &js[start..];
    let end = tail.find('"')?;
    let id = &tail[..end];
    if id.len() >= 20 && id.chars().all(|c| c.is_ascii_alphanumeric()) {
        Some(id.to_string())
    } else {
        None
    }
}

/// Map a [`NetworkError`] onto the engine error taxonomy so the suspend/penalty
/// machine can classify access/rate-limit/CAPTCHA failures.
fn map_network_error(error: NetworkError) -> EngineError {
    let message = error.to_string();
    match error {
        NetworkError::AccessDenied { .. } => EngineError::AccessDenied(message),
        NetworkError::CloudflareAccessDenied { .. } => EngineError::CloudflareAccessDenied(message),
        NetworkError::TooManyRequests { .. } => EngineError::TooManyRequests(message),
        NetworkError::Captcha { .. } => EngineError::Captcha(message),
        NetworkError::CloudflareCaptcha { .. } => EngineError::CloudflareCaptcha(message),
        NetworkError::RecaptchaCaptcha { .. } => EngineError::RecaptchaCaptcha(message),
        _ => EngineError::Unexpected(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A `POST` engine request with form data and cookies is translated into a
    /// `NetworkRequest` with a URL-encoded body and the cookies carried through.
    #[test]
    fn builds_post_form_request() {
        let mut params = RequestParams {
            method: HttpMethod::Post,
            ..RequestParams::default()
        };
        params.data.insert("q".to_string(), "rust lang".to_string());
        params
            .headers
            .insert("Referer".to_string(), "https://example.test/".to_string());
        params.cookies.insert("kl".to_string(), "wt-wt".to_string());

        let request = build_network_request(&params, "https://example.test/search").unwrap();

        assert_eq!(request.method, Method::POST);
        assert_eq!(request.url, "https://example.test/search");
        let body = String::from_utf8(request.body.clone().expect("body")).unwrap();
        // The single form field is URL-encoded (space -> `+`).
        assert_eq!(body, "q=rust+lang");
        assert_eq!(
            request.cookies,
            vec![("kl".to_string(), "wt-wt".to_string())]
        );
    }

    /// A JSON body takes precedence over form data and is serialized verbatim.
    #[test]
    fn builds_json_body_request() {
        let params = RequestParams {
            json: Some(serde_json::json!({ "q": "rust" })),
            ..Default::default()
        };

        let request = build_network_request(&params, "https://example.test/api").unwrap();
        let body = String::from_utf8(request.body.clone().expect("body")).unwrap();
        assert_eq!(body, r#"{"q":"rust"}"#);
    }

    #[test]
    fn response_cache_serves_within_ttl_and_keys_on_body() {
        let cache = ResponseCache::default();
        let key = cache_key("wikidata", "https://wdqs/sparql", Some(b"query=A"));
        assert!(cache.get(&key).is_none(), "cold cache misses");

        let response = EngineResponse {
            status: 200,
            url: "https://wdqs/sparql".to_string(),
            body: b"cached body".to_vec(),
            ..EngineResponse::default()
        };
        cache.put(key.clone(), response.clone());
        assert_eq!(cache.get(&key), Some(response));

        // A different request body is a different cache key (a cache miss).
        let other = cache_key("wikidata", "https://wdqs/sparql", Some(b"query=B"));
        assert!(cache.get(&other).is_none());
    }

    #[test]
    fn response_cache_evicts_wholesale_past_capacity() {
        let cache = ResponseCache::default();
        for i in 0..=RESPONSE_CACHE_CAPACITY {
            cache.put(format!("k{i}"), EngineResponse::default());
        }
        // The wholesale clear on overflow keeps the map bounded.
        assert!(cache.entries.lock().unwrap().len() <= RESPONSE_CACHE_CAPACITY);
    }

    #[test]
    fn only_curated_engines_are_cacheable() {
        assert!(CACHEABLE_ENGINES.contains(&"wikidata"));
        // A general web engine must never be cached (results would go stale and
        // caching mixes users' identical queries in surprising ways).
        assert!(!CACHEABLE_ENGINES.contains(&"duckduckgo"));
        assert!(!CACHEABLE_ENGINES.contains(&"soundcloud"));
    }

    /// `Accept-Language` is emitted as a browser-style q-graded list, never a
    /// bare locale tag.
    #[test]
    fn accept_language_is_browser_shaped() {
        assert_eq!(browser_accept_language("en"), "en-US,en;q=0.9");
        assert_eq!(browser_accept_language("en-GB"), "en-GB,en;q=0.9");
        assert_eq!(browser_accept_language("de"), "de,en;q=0.8");
        assert_eq!(browser_accept_language("de-DE"), "de-DE,de;q=0.9,en;q=0.8");
        assert_eq!(browser_accept_language("fr-FR"), "fr-FR,fr;q=0.9,en;q=0.8");
    }

    /// Network access/rate-limit/CAPTCHA errors map onto the matching engine
    /// error variants.
    #[test]
    fn maps_network_errors_to_engine_errors() {
        assert!(matches!(
            map_network_error(NetworkError::AccessDenied {
                name: "n".to_string(),
                status: 403
            }),
            EngineError::AccessDenied(_)
        ));
        assert!(matches!(
            map_network_error(NetworkError::TooManyRequests {
                name: "n".to_string(),
                status: 429
            }),
            EngineError::TooManyRequests(_)
        ));
        assert!(matches!(
            map_network_error(NetworkError::Captcha {
                name: "n".to_string(),
                status: 503
            }),
            EngineError::Captcha(_)
        ));
        assert!(matches!(
            map_network_error(NetworkError::CloudflareCaptcha {
                name: "n".to_string(),
                status: 503
            }),
            EngineError::CloudflareCaptcha(_)
        ));
        assert!(matches!(
            map_network_error(NetworkError::CloudflareAccessDenied {
                name: "n".to_string(),
                status: 403
            }),
            EngineError::CloudflareAccessDenied(_)
        ));
        assert!(matches!(
            map_network_error(NetworkError::RecaptchaCaptcha {
                name: "n".to_string(),
                status: 503
            }),
            EngineError::RecaptchaCaptcha(_)
        ));
    }
}
