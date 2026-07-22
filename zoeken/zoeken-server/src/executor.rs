//! A network-backed [`EngineExecutor`] for engine HTTP calls.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use futures_util::StreamExt;
use url::form_urlencoded;
use wreq::header::{ACCEPT_LANGUAGE, AUTHORIZATION, HeaderMap, HeaderName, HeaderValue};
use wreq::{Method, Response};
use zoeken_engine_core::{
    Engine, EngineError, EngineResponse, EngineResults, HttpMethod, Processor, RequestParams,
    SearchQueryView,
};
use zoeken_engines::engines::soundcloud;
use zoeken_network::{DEFAULT_NETWORK, NetworkError, NetworkManager, NetworkRequest};
use zoeken_search::{EngineExecResult, EngineExecutor, EngineFuture, SuspensionPolicy};

use crate::engine_health::{PendingHealth, circuit_is_open, record_health};
use crate::outbound_cache::{
    ResponseCache, cache_key, response_is_cacheable, response_is_structured,
};

#[derive(Clone)]
pub struct NetworkExecutor {
    networks: Arc<NetworkManager>,
    engine_networks: HashMap<String, String>,
    max_response_bytes: usize,
    response_cache: Arc<ResponseCache>,
    health_policy: SuspensionPolicy,
}

const DEFAULT_MAX_RESPONSE_BYTES: usize = 10 * 1024 * 1024;

impl NetworkExecutor {
    pub fn new(networks: Arc<NetworkManager>) -> Self {
        NetworkExecutor {
            networks,
            engine_networks: HashMap::new(),
            max_response_bytes: DEFAULT_MAX_RESPONSE_BYTES,
            response_cache: Arc::new(ResponseCache::new(
                Duration::from_secs(60),
                Duration::from_secs(300),
                128 * 1024 * 1024,
            )),
            health_policy: SuspensionPolicy::default(),
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

    #[must_use]
    pub fn with_response_cache(
        mut self,
        html_ttl: Duration,
        structured_ttl: Duration,
        max_bytes: usize,
    ) -> Self {
        self.response_cache = Arc::new(ResponseCache::new(html_ttl, structured_ttl, max_bytes));
        self
    }

    #[must_use]
    pub fn with_health_policy(mut self, health_policy: SuspensionPolicy) -> Self {
        self.health_policy = health_policy;
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
        let health_policy = self.health_policy;
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
            engine.prepare_request(&mut params);
            if params.needs_client_id {
                let networks = Arc::clone(&networks);
                let _ = soundcloud::ensure_guest_client_id(&mut params.engine_data, move |url| {
                    let networks = Arc::clone(&networks);
                    let url = url.to_string();
                    async move {
                        let Ok(resp) = networks
                            .request(soundcloud::NAME, NetworkRequest::get(url))
                            .await
                        else {
                            return None;
                        };
                        resp.text().await.ok()
                    }
                })
                .await;
                params.needs_client_id = false;
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

            let key = cache_key(&response_cache.hmac_key, &engine_name, &request, &query);
            if let Some(cached) = response_cache.get(&key) {
                metrics::counter!("engine_response_cache_total", "outcome" => "hit").increment(1);
                return EngineExecResult {
                    result: engine.response(&cached),
                    http_duration: None,
                };
            }

            let Some(flight) = response_cache.flight(&key) else {
                return EngineExecResult::from_result(Err(EngineError::Unexpected(
                    "response cache coordination unavailable".to_string(),
                )));
            };
            let _flight_guard = flight.lock().await;
            if let Some(cached) = response_cache.get(&key) {
                metrics::counter!("engine_singleflight_total", "outcome" => "shared").increment(1);
                return EngineExecResult {
                    result: engine.response(&cached),
                    http_duration: None,
                };
            }

            let storage = networks.coordinator();
            let previous_health = if let Some(storage) = storage.as_ref() {
                match storage.latest_engine_health(&engine_name).await {
                    Ok(snapshot) => snapshot,
                    Err(_) => {
                        response_cache.finish_flight(&key);
                        return EngineExecResult::from_result(Err(EngineError::Unexpected(
                            "outbound coordination storage is unavailable".to_string(),
                        )));
                    }
                }
            } else {
                None
            };
            if circuit_is_open(previous_health.as_ref()) {
                metrics::counter!("engine_circuit_total", "transition" => "rejected").increment(1);
                response_cache.finish_flight(&key);
                return EngineExecResult::from_result(Err(EngineError::AccessDenied(format!(
                    "{engine_name} circuit is cooling down"
                ))));
            }

            let http_started = Instant::now();
            let mut pending_health = PendingHealth::new(
                storage.clone(),
                engine_name.clone(),
                previous_health.clone(),
                health_policy,
            );
            let response = match networks.request(&network_name, request).await {
                Ok(response) => response,
                Err(error) => {
                    pending_health.complete();
                    let mapped = map_network_error(error);
                    record_health(
                        storage.as_deref(),
                        &engine_name,
                        http_started.elapsed(),
                        &Err(mapped.clone()),
                        previous_health.as_ref(),
                        &health_policy,
                    )
                    .await;
                    response_cache.finish_flight(&key);
                    return EngineExecResult {
                        result: Err(mapped),
                        http_duration: Some(http_started.elapsed()),
                    };
                }
            };
            let engine_response = match adapt_response(response, max_response_bytes).await {
                Ok(response) => response,
                Err(error) => {
                    pending_health.complete();
                    record_health(
                        storage.as_deref(),
                        &engine_name,
                        http_started.elapsed(),
                        &Err(error.clone()),
                        previous_health.as_ref(),
                        &health_policy,
                    )
                    .await;
                    response_cache.finish_flight(&key);
                    return EngineExecResult {
                        result: Err(error),
                        http_duration: Some(http_started.elapsed()),
                    };
                }
            };
            let result = engine.response(&engine_response);
            pending_health.complete();
            record_health(
                storage.as_deref(),
                &engine_name,
                http_started.elapsed(),
                &result,
                previous_health.as_ref(),
                &health_policy,
            )
            .await;
            if result.is_ok() && response_is_cacheable(&engine_response) {
                let structured = response_is_structured(&engine_response);
                response_cache.put(key.clone(), engine_response, structured);
                metrics::counter!("engine_response_cache_total", "outcome" => "stored")
                    .increment(1);
            } else {
                metrics::counter!("engine_response_cache_total", "outcome" => "rejected")
                    .increment(1);
            }
            response_cache.finish_flight(&key);
            let http_duration = Some(http_started.elapsed());
            EngineExecResult {
                result,
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
    if params.disable_redirects {
        request = request.with_max_redirects(0);
    } else if params.allow_redirects || params.max_redirects > 0 || params.soft_max_redirects > 0 {
        let max_redirects = params
            .max_redirects
            .max(params.soft_max_redirects)
            .max(u32::from(params.allow_redirects));
        request = request.with_max_redirects(max_redirects as usize);
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
        NetworkError::QueueExpired { .. } => EngineError::QueueExpired,
        _ => EngineError::Unexpected(message),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine_health::cooldown_for;
    use zoeken_search::SuspensionPolicy;
    use zoeken_storage::EngineHealthSnapshot;

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
        let cache = ResponseCache::new(Duration::from_secs(60), Duration::from_secs(300), 4096);
        let secret = [7_u8; 32];
        let request_a = NetworkRequest::post("https://wdqs/sparql").with_body(b"query=A".to_vec());
        let key = cache_key(&secret, "wikidata", &request_a, &SearchQueryView::default());
        assert!(cache.get(&key).is_none(), "cold cache misses");

        let response = EngineResponse {
            status: 200,
            url: "https://wdqs/sparql".to_string(),
            body: b"cached body".to_vec(),
            ..EngineResponse::default()
        };
        cache.put(key.clone(), response.clone(), true);
        assert_eq!(cache.get(&key), Some(response));

        // A different request body is a different cache key (a cache miss).
        let request_b = NetworkRequest::post("https://wdqs/sparql").with_body(b"query=B".to_vec());
        let other = cache_key(&secret, "wikidata", &request_b, &SearchQueryView::default());
        assert!(cache.get(&other).is_none());
    }

    #[test]
    fn response_cache_evicts_oldest_to_stay_within_byte_limit() {
        let cache = ResponseCache::new(Duration::from_secs(60), Duration::from_secs(300), 24);
        for i in 0..4 {
            cache.put(
                format!("k{i}"),
                EngineResponse {
                    status: 200,
                    body: vec![i; 12],
                    ..EngineResponse::default()
                },
                false,
            );
        }
        // Only room for ~2 entries at 12 bytes each within a 24 byte budget;
        // the oldest keys must have been evicted to make room for the newest.
        assert!(cache.get("k0").is_none());
        assert!(cache.get("k3").is_some());
    }

    #[tokio::test]
    async fn identical_cache_keys_share_one_in_flight_lock() {
        let cache = Arc::new(ResponseCache::new(
            Duration::from_secs(60),
            Duration::from_secs(300),
            4096,
        ));
        let first = cache.flight("digest").unwrap();
        let second = cache.flight("digest").unwrap();
        assert!(Arc::ptr_eq(&first, &second));

        let guard = first.lock().await;
        let follower = tokio::spawn(async move {
            let _guard = second.lock().await;
        });
        tokio::task::yield_now().await;
        assert!(!follower.is_finished());
        drop(guard);
        follower.await.unwrap();
    }

    #[test]
    fn personalized_responses_are_not_cacheable() {
        let mut response = EngineResponse {
            status: 200,
            ..EngineResponse::default()
        };
        assert!(response_is_cacheable(&response));
        response
            .headers
            .insert("Set-Cookie".to_string(), "session=secret".to_string());
        assert!(!response_is_cacheable(&response));
    }

    #[test]
    fn duckduckgo_captcha_does_not_open_circuit() {
        let policy = SuspensionPolicy::default();
        assert_eq!(
            cooldown_for(
                "duckduckgo",
                &EngineError::Captcha("duckduckgo".into()),
                None,
                &policy,
            ),
            None
        );
        // Network transport captchas stringify to a message, not the engine id.
        assert_eq!(
            cooldown_for(
                "duckduckgo",
                &EngineError::Captcha("captcha: challenge".into()),
                None,
                &policy,
            ),
            None
        );
    }

    #[test]
    fn captcha_uses_suspended_times_duration() {
        let policy = SuspensionPolicy::default();
        assert_eq!(
            cooldown_for("bing", &EngineError::Captcha("bing".into()), None, &policy,),
            Some(policy.captcha)
        );
    }

    #[test]
    fn recurrent_circuit_failure_escalates_cooldown() {
        let previous = EngineHealthSnapshot {
            bucket: 1,
            successes: 0,
            timeouts: 0,
            errors: 1,
            circuit_status: "half_open".into(),
            cooldown_until_ms: None,
            last_error_category: Some("rate_limited".into()),
        };
        let policy = SuspensionPolicy::default();
        assert_eq!(
            cooldown_for(
                "qwant",
                &EngineError::TooManyRequests("qwant".into()),
                Some(&previous),
                &policy,
            ),
            Some(policy.too_many_requests.saturating_mul(2))
        );
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
                status: 429,
                retry_after: None,
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
