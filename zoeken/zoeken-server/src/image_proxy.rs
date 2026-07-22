//! The `GET /image_proxy` route: HMAC-gated fetch and validate images.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};
use sha2::{Digest, Sha256};
use zoeken_favicons::{
    DEFAULT_MAX_IMAGE_BYTES, ImageProxyDecision, ImageProxyPolicy, SafeOutboundTransport,
    image_proxy_decision, is_hmac_of, validate_proxy_url,
};
use zoeken_network::{FlightCache, NetworkManager};

use crate::{AppState, parse_pairs};

/// In-process budget for successfully proxied image bodies (byte-weighted).
const IMAGE_CACHE_MAX_BYTES: usize = 64 * 1024 * 1024;
/// TTL for cached proxied images (aligned with browser Cache-Control below).
const IMAGE_CACHE_TTL: Duration = Duration::from_secs(86_400);

#[derive(Debug, Clone)]
pub struct FetchedImage {
    pub status: u16,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
    pub body: Vec<u8>,
}

#[derive(Debug, thiserror::Error)]
pub enum ImageFetchError {
    #[error("failed to fetch image: {0}")]
    Upstream(String),
}

pub type FetchFuture<'a> =
    Pin<Box<dyn Future<Output = Result<FetchedImage, ImageFetchError>> + Send + 'a>>;

pub trait ImageProxyFetcher: Send + Sync {
    fn fetch<'a>(&'a self, url: &'a str) -> FetchFuture<'a>;
}

/// Byte-budgeted singleflight cache in front of any [`ImageProxyFetcher`].
///
/// Search pages hit the same thumbnail URLs repeatedly (re-renders, lightbox,
/// concurrent result cards). Without this, every request re-walks SSRF-safe
/// redirects and re-downloads the body through the outbound pool.
pub struct CachedImageFetcher {
    inner: Arc<dyn ImageProxyFetcher>,
    cache: FlightCache<String, FetchedImage>,
    ttl: Duration,
}

impl CachedImageFetcher {
    #[must_use]
    pub fn new(inner: Arc<dyn ImageProxyFetcher>) -> Self {
        Self::with_limits(inner, IMAGE_CACHE_MAX_BYTES, IMAGE_CACHE_TTL)
    }

    #[must_use]
    pub fn with_limits(
        inner: Arc<dyn ImageProxyFetcher>,
        max_bytes: usize,
        ttl: Duration,
    ) -> Self {
        Self {
            inner,
            cache: FlightCache::new(max_bytes.max(1), |img: &FetchedImage| {
                img.body.len().saturating_add(
                    img.content_type.as_ref().map_or(0, String::len).saturating_add(32),
                )
            }),
            ttl,
        }
    }
}

impl ImageProxyFetcher for CachedImageFetcher {
    fn fetch<'a>(&'a self, url: &'a str) -> FetchFuture<'a> {
        let key = url.to_string();
        Box::pin(async move {
            if let Some(hit) = self.cache.get(&key) {
                metrics::counter!("image_proxy_cache_total", "outcome" => "hit").increment(1);
                return Ok(hit);
            }

            let Some(flight) = self.cache.flight(&key) else {
                return self.inner.fetch(&key).await;
            };
            let _guard = flight.lock().await;

            if let Some(hit) = self.cache.get(&key) {
                metrics::counter!("image_proxy_cache_total", "outcome" => "shared").increment(1);
                return Ok(hit);
            }

            let fetched = match self.inner.fetch(&key).await {
                Ok(fetched) => fetched,
                Err(error) => {
                    self.cache.finish_flight(&key);
                    return Err(error);
                }
            };

            if fetched.status == 200 && !fetched.body.is_empty() {
                self.cache.put(key.clone(), fetched.clone(), self.ttl);
                metrics::counter!("image_proxy_cache_total", "outcome" => "store").increment(1);
            } else {
                metrics::counter!("image_proxy_cache_total", "outcome" => "skip").increment(1);
            }
            self.cache.finish_flight(&key);
            Ok(fetched)
        })
    }
}

pub struct WreqImageFetcher {
    transport: SafeOutboundTransport,
}

impl WreqImageFetcher {
    pub fn new() -> Self {
        // One shared pooled client for the whole proxy: per-request client
        // construction cost a fresh TLS setup on every image.
        let client = wreq::Client::builder()
            .redirect(wreq::redirect::Policy::none())
            .timeout(Duration::from_secs(15))
            .build()
            .expect("build image proxy HTTP client");
        Self {
            transport: SafeOutboundTransport::Direct(client),
        }
    }

    /// Use an externally built client (e.g. the browser-emulating
    /// `image_proxy` network client) instead of the plain default.
    pub fn with_client(client: wreq::Client) -> Self {
        Self {
            transport: SafeOutboundTransport::Direct(client),
        }
    }

    #[must_use]
    pub fn with_networks(networks: Arc<NetworkManager>) -> Self {
        Self {
            transport: SafeOutboundTransport::Coordinated {
                network: networks,
                network_name: "image_proxy",
                timeout: Duration::from_secs(15),
            },
        }
    }
}

impl Default for WreqImageFetcher {
    fn default() -> Self {
        Self::new()
    }
}

impl ImageProxyFetcher for WreqImageFetcher {
    fn fetch<'a>(&'a self, url: &'a str) -> FetchFuture<'a> {
        let url = url.to_string();
        let max_bytes = DEFAULT_MAX_IMAGE_BYTES as usize;
        Box::pin(async move {
            let fetched = self
                .transport
                .get(&url, max_bytes)
                .await
                .map_err(ImageFetchError::Upstream)?;
            Ok(FetchedImage {
                status: fetched.status,
                content_type: fetched.content_type,
                content_length: fetched.content_length,
                body: fetched.body,
            })
        })
    }
}

pub(crate) fn image_proxy_enabled(
    state: &AppState,
    headers: &HeaderMap,
    params: &[(String, String)],
) -> bool {
    let pref_cookie = crate::preferences::read_pref_cookie(headers);
    let form = zoeken_query::FormParams::from_pairs(params.to_vec());
    let resolved = zoeken_prefs::resolve_with_data(
        &state.pref_defaults,
        &state.settings,
        pref_cookie.as_deref(),
        &form,
        &state.data,
    );
    state.settings.server.image_proxy || resolved.image_proxy
}

fn image_etag(body: &[u8]) -> String {
    let digest = Sha256::digest(body);
    format!("\"{}\"", hex::encode(&digest[..16]))
}

/// `GET /image_proxy?url=...&h=...`: HMAC + prefs gate, then content-type/size policy.
pub async fn image_proxy_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let params = parse_pairs(query.as_deref().unwrap_or(""));

    if !image_proxy_enabled(&state, &headers, &params) {
        return (StatusCode::BAD_REQUEST, "image proxy disabled").into_response();
    }

    let url = params
        .iter()
        .find(|(k, _)| k == "url")
        .map(|(_, v)| v.clone());
    let Some(url) = url.filter(|u| !u.is_empty()) else {
        return (StatusCode::BAD_REQUEST, "missing url parameter").into_response();
    };

    let h = params
        .iter()
        .find(|(k, _)| k == "h")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    if !is_hmac_of(&state.settings.server.secret_key, url.as_bytes(), h) {
        return (StatusCode::BAD_REQUEST, "invalid hmac").into_response();
    }

    if let Err(reason) = validate_proxy_url(&url) {
        return (StatusCode::BAD_REQUEST, reason.reason()).into_response();
    }

    let fetched = match state.image_fetcher.fetch(&url).await {
        Ok(f) => f,
        Err(error) => {
            tracing::debug!(%error, "image proxy upstream fetch failed");
            return (StatusCode::BAD_REQUEST, "failed to fetch image").into_response();
        }
    };

    if fetched.status != 200 {
        let status = StatusCode::from_u16(fetched.status)
            .ok()
            .filter(|s| s.is_client_error() || s.is_server_error())
            .unwrap_or(StatusCode::BAD_REQUEST);
        return (status, "upstream did not return a proxiable image").into_response();
    }

    let size = fetched.content_length.or(Some(fetched.body.len() as u64));

    match image_proxy_decision(fetched.content_type.as_deref(), size, &state.image_policy) {
        ImageProxyDecision::Serve => {
            let etag = image_etag(&fetched.body);
            if headers
                .get(header::IF_NONE_MATCH)
                .and_then(|v| v.to_str().ok())
                .is_some_and(|v| v.split(',').any(|tag| tag.trim() == etag))
            {
                return (
                    StatusCode::NOT_MODIFIED,
                    [
                        (header::ETAG, etag),
                        (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
                    ],
                )
                    .into_response();
            }

            let content_type = fetched
                .content_type
                .unwrap_or_else(|| "application/octet-stream".to_string());
            (
                [
                    (header::CONTENT_TYPE, content_type),
                    (header::ETAG, etag),
                    // Proxied URLs are HMAC-stable per image; let the browser
                    // cache them instead of re-proxying on every render.
                    (header::CACHE_CONTROL, "public, max-age=86400".to_string()),
                ],
                fetched.body,
            )
                .into_response()
        }
        ImageProxyDecision::Reject(reason) => {
            (StatusCode::BAD_REQUEST, reason.reason()).into_response()
        }
    }
}

pub fn default_policy() -> ImageProxyPolicy {
    ImageProxyPolicy::default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use tower::ServiceExt;
    use zoeken_favicons::{DEFAULT_MAX_IMAGE_BYTES, new_hmac};
    use zoeken_settings::Settings;

    struct StubFetcher(FetchedImage);

    impl ImageProxyFetcher for StubFetcher {
        fn fetch<'a>(&'a self, _url: &'a str) -> FetchFuture<'a> {
            let image = self.0.clone();
            Box::pin(async move { Ok(image) })
        }
    }

    struct FailingFetcher;

    impl ImageProxyFetcher for FailingFetcher {
        fn fetch<'a>(&'a self, _url: &'a str) -> FetchFuture<'a> {
            Box::pin(async { Err(ImageFetchError::Upstream("boom".into())) })
        }
    }

    struct CountingFetcher {
        calls: AtomicUsize,
        image: FetchedImage,
    }

    impl ImageProxyFetcher for CountingFetcher {
        fn fetch<'a>(&'a self, _url: &'a str) -> FetchFuture<'a> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let image = self.image.clone();
            Box::pin(async move { Ok(image) })
        }
    }

    fn enabled_state(fetcher: Arc<dyn ImageProxyFetcher>) -> AppState {
        let mut settings = Settings::default();
        settings.server.image_proxy = true;
        settings.server.secret_key = "secret".into();
        AppState::new()
            .expect("build app state")
            .with_image_fetcher(fetcher)
            .with_settings(settings)
    }

    fn signed(url: &str) -> String {
        let h = new_hmac("secret", url.as_bytes());
        let enc: String = url::form_urlencoded::byte_serialize(url.as_bytes()).collect();
        format!("/image_proxy?url={enc}&h={h}")
    }

    async fn get(app: axum::Router, uri: &str) -> (StatusCode, String) {
        let response = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, String::from_utf8_lossy(&body).to_string())
    }

    #[tokio::test]
    async fn serves_allowed_image_within_limits() {
        let fetcher = Arc::new(StubFetcher(FetchedImage {
            status: 200,
            content_type: Some("image/png".into()),
            content_length: Some(3),
            body: vec![1, 2, 3],
        }));
        let response = app(enabled_state(fetcher))
            .oneshot(
                Request::builder()
                    .uri(signed("https://example.com/a.png"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/png"
        );
        assert!(response.headers().get(header::ETAG).is_some());
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(body.to_vec(), vec![1, 2, 3]);
    }

    #[tokio::test]
    async fn cache_dedupes_identical_upstream_fetches() {
        let inner = Arc::new(CountingFetcher {
            calls: AtomicUsize::new(0),
            image: FetchedImage {
                status: 200,
                content_type: Some("image/png".into()),
                content_length: Some(3),
                body: vec![1, 2, 3],
            },
        });
        let cached: Arc<dyn ImageProxyFetcher> =
            Arc::new(CachedImageFetcher::new(Arc::clone(&inner) as Arc<dyn ImageProxyFetcher>));

        let first = cached.fetch("https://cdn.example.com/a.png").await.unwrap();
        let second = cached.fetch("https://cdn.example.com/a.png").await.unwrap();
        assert_eq!(first.body, second.body);
        assert_eq!(inner.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn not_modified_when_etag_matches() {
        let fetcher = Arc::new(StubFetcher(FetchedImage {
            status: 200,
            content_type: Some("image/png".into()),
            content_length: Some(3),
            body: vec![1, 2, 3],
        }));
        let etag = image_etag(&[1, 2, 3]);
        let response = app(enabled_state(fetcher))
            .oneshot(
                Request::builder()
                    .uri(signed("https://example.com/a.png"))
                    .header(header::IF_NONE_MATCH, etag)
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::NOT_MODIFIED);
    }

    #[tokio::test]
    async fn rejects_when_disabled() {
        let fetcher = Arc::new(StubFetcher(FetchedImage {
            status: 200,
            content_type: Some("image/png".into()),
            content_length: Some(3),
            body: vec![1, 2, 3],
        }));
        let mut settings = Settings::default();
        settings.server.secret_key = "secret".into();
        settings.server.image_proxy = false;
        let state = AppState::new()
            .unwrap()
            .with_image_fetcher(fetcher)
            .with_settings(settings);
        let (status, _) = get(app(state), &signed("https://example.com/a.png")).await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rejects_missing_or_bad_hmac() {
        let fetcher = Arc::new(StubFetcher(FetchedImage {
            status: 200,
            content_type: Some("image/png".into()),
            content_length: Some(3),
            body: vec![1, 2, 3],
        }));
        let app = app(enabled_state(fetcher));
        let (status, _) = get(app.clone(), "/image_proxy?url=https://example.com/a.png").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
        let (status, _) = get(app, "/image_proxy?url=https://example.com/a.png&h=deadbeef").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rejects_disallowed_content_type() {
        let fetcher = Arc::new(StubFetcher(FetchedImage {
            status: 200,
            content_type: Some("text/html".into()),
            content_length: Some(3),
            body: vec![1, 2, 3],
        }));
        let (status, _) = get(
            app(enabled_state(fetcher)),
            &signed("https://example.com/a.html"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn rejects_oversized_image() {
        let fetcher = Arc::new(StubFetcher(FetchedImage {
            status: 200,
            content_type: Some("image/png".into()),
            content_length: Some(DEFAULT_MAX_IMAGE_BYTES + 1),
            body: vec![0u8; 4],
        }));
        let (status, _) = get(
            app(enabled_state(fetcher)),
            &signed("https://example.com/big.png"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn missing_url_is_rejected() {
        let (status, _) = get(app(enabled_state(Arc::new(FailingFetcher))), "/image_proxy").await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn upstream_failure_is_rejected() {
        let (status, _) = get(
            app(enabled_state(Arc::new(FailingFetcher))),
            &signed("https://example.com/x.png"),
        )
        .await;
        assert_eq!(status, StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn non_200_upstream_is_not_proxied() {
        let fetcher = Arc::new(StubFetcher(FetchedImage {
            status: 404,
            content_type: Some("image/png".into()),
            content_length: Some(3),
            body: vec![1, 2, 3],
        }));
        let (status, _) = get(
            app(enabled_state(fetcher)),
            &signed("https://example.com/missing.png"),
        )
        .await;
        assert_eq!(status, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn rejects_private_and_localhost_urls() {
        let fetcher = Arc::new(StubFetcher(FetchedImage {
            status: 200,
            content_type: Some("image/png".into()),
            content_length: Some(3),
            body: vec![1, 2, 3],
        }));
        let app = app(enabled_state(fetcher));
        for url in [
            "http://127.0.0.1/a.png",
            "http://10.0.0.5/a.png",
            "http://169.254.169.254/latest",
            "file:///etc/passwd",
        ] {
            let (status, body) = get(app.clone(), &signed(url)).await;
            assert_eq!(status, StatusCode::BAD_REQUEST, "{url} -> {body}");
        }
    }
}
