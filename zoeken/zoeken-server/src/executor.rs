//! A network-backed [`EngineExecutor`] for engine HTTP calls.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Instant;

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
}

impl NetworkExecutor {
    pub fn new(networks: Arc<NetworkManager>) -> Self {
        NetworkExecutor {
            networks,
            engine_networks: HashMap::new(),
        }
    }

    /// Set the per-engine network name (from `settings.engines[].network`).
    pub fn with_engine_networks(mut self, engine_networks: HashMap<String, String>) -> Self {
        self.engine_networks = engine_networks;
        self
    }
}

impl EngineExecutor for NetworkExecutor {
    fn execute(&self, engine: Arc<dyn Engine>, query: SearchQueryView) -> EngineFuture {
        let networks = self.networks.clone();
        let engine_name = engine.metadata().name.clone();
        let engine_networks = self.engine_networks.clone();
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
            let engine_response = match adapt_response(response).await {
                Ok(response) => response,
                Err(error) => {
                    return EngineExecResult {
                        result: Err(error),
                        http_duration: Some(http_started.elapsed()),
                    };
                }
            };
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
        && let Ok(value) = HeaderValue::from_str(&params.locale_key)
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
async fn adapt_response(response: Response) -> Result<EngineResponse, EngineError> {
    let status = response.status().as_u16();
    let url = response.url().to_string();

    let mut headers = HashMap::new();
    for (name, value) in response.headers() {
        if let Ok(value) = value.to_str() {
            headers.insert(name.as_str().to_string(), value.to_string());
        }
    }

    let body = response
        .bytes()
        .await
        .map_err(|e| EngineError::Unexpected(format!("failed to read response body: {e}")))?
        .to_vec();

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
