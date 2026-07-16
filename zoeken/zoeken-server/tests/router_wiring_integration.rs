//! Router wiring integration tests.

use std::borrow::Cow;
use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};

use tower::ServiceExt;
use zoeken_botdetect::{LimiterConfig, RateLimitConfig};
use zoeken_engine_core::{
    Engine, EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};
use zoeken_search::{
    EngineExecResult, EngineExecutor, EngineFuture, EngineRegistry, RegisteredEngine, Search,
    SearchConfig,
};
use zoeken_server::readiness::ReadinessState;
use zoeken_server::static_assets::{AssetSource, INDEX_HTML};
use zoeken_server::{AppState, app};
use zoeken_settings::DeploymentConfig;

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

fn stub_search() -> Search {
    let engine = StubEngine {
        meta: EngineMeta {
            name: "stub".to_string(),
            categories: vec!["general".to_string()],
            ..EngineMeta::default()
        },
    };
    let registry = EngineRegistry::from_engines([RegisteredEngine::new(Arc::new(engine))]);
    let executor: Arc<dyn EngineExecutor> = Arc::new(ImmediateExecutor);
    Search::new(registry, executor, SearchConfig::default())
}

const INDEX_BODY: &[u8] = b"<!doctype html><title>project</title>";
const ASSET_PATH: &str = "assets/main.4af1c2e9.js";
const ASSET_BODY: &[u8] = b"console.log('gs')";

struct MockAssets {
    files: HashMap<String, Vec<u8>>,
}

impl MockAssets {
    fn new(entries: &[(&str, &[u8])]) -> Self {
        let files = entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_vec()))
            .collect();
        Self { files }
    }
}

impl AssetSource for MockAssets {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        self.files.get(path).cloned().map(Cow::Owned)
    }

    fn has_index(&self) -> bool {
        self.files.contains_key(INDEX_HTML)
    }
}

fn mock_assets() -> Arc<dyn AssetSource> {
    Arc::new(MockAssets::new(&[
        (INDEX_HTML, INDEX_BODY),
        (ASSET_PATH, ASSET_BODY),
    ]))
}

async fn decoded_body(response: axum::response::Response) -> Vec<u8> {
    use std::io::Read;

    let is_gzip = response
        .headers()
        .get(header::CONTENT_ENCODING)
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"gzip"));
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    if is_gzip {
        let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).unwrap();
        out
    } else {
        bytes.to_vec()
    }
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

fn browser_request(real_ip: &str, uri: &str) -> Request<Body> {
    let mut req = Request::builder()
        .uri(uri)
        .header("x-real-ip", real_ip)
        .header(header::ACCEPT, "text/html")
        .header(header::ACCEPT_ENCODING, "gzip, deflate")
        .header(header::ACCEPT_LANGUAGE, "en-US")
        .header(header::CONNECTION, "keep-alive")
        .header(
            header::USER_AGENT,
            "Mozilla/5.0 (X11; Linux x86_64) Firefox/120.0",
        )
        .body(Body::empty())
        .unwrap();
    let addr: SocketAddr = "10.0.0.1:12345".parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(addr));
    req
}

fn assert_security_headers(response: &axum::response::Response) {
    let headers = response.headers();
    assert_eq!(
        headers.get(header::X_CONTENT_TYPE_OPTIONS).unwrap(),
        "nosniff",
        "X-Content-Type-Options must be nosniff (Req 16.2)"
    );
    assert_eq!(
        headers.get(header::X_FRAME_OPTIONS).unwrap(),
        "DENY",
        "X-Frame-Options must be DENY (Req 16.2)"
    );
    assert!(
        headers.get(header::CONTENT_SECURITY_POLICY).is_some(),
        "a Content-Security-Policy header must be present (Req 16.2)"
    );
}

fn app_with_assets() -> Router {
    app(AppState::from_search(stub_search()).with_assets(mock_assets()))
}

#[tokio::test]
async fn api_route_takes_precedence_over_static_fallback() {
    let response = app_with_assets().oneshot(get("/config")).await.unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json",
        "the /config API route must win over the SPA fallback"
    );

    let body = decoded_body(response).await;
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert!(
        value.get("engines").is_some() && value.get("brand").is_some(),
        "the body must be the /config JSON, not the SPA index"
    );
    assert_ne!(
        &body[..],
        INDEX_BODY,
        "the SPA index must not be served for an API route"
    );
}

#[tokio::test]
async fn one_app_serves_api_and_static_assets() {
    let router = app_with_assets();

    let config = router.clone().oneshot(get("/config")).await.unwrap();
    assert_eq!(config.status(), StatusCode::OK);
    assert_eq!(
        config.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let asset = router
        .clone()
        .oneshot(get(&format!("/{ASSET_PATH}")))
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::OK);
    assert_eq!(
        asset.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8"
    );
    let asset_body = decoded_body(asset).await;
    assert_eq!(&asset_body[..], ASSET_BODY);

    let spa = router.oneshot(get("/some/client/route")).await.unwrap();
    assert_eq!(spa.status(), StatusCode::OK);
    assert_eq!(
        spa.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    let spa_body = decoded_body(spa).await;
    assert_eq!(&spa_body[..], INDEX_BODY);
}

#[tokio::test]
async fn readyz_reflects_readiness_state() {
    let readiness = ReadinessState::new_not_ready();

    let router = app(AppState::from_search(stub_search())
        .with_assets(mock_assets())
        .with_readiness(readiness.clone()));

    let before = router.oneshot(get("/readyz")).await.unwrap();
    assert_eq!(before.status(), StatusCode::SERVICE_UNAVAILABLE);

    readiness.set_ready();
    let ready_router = app(AppState::from_search(stub_search())
        .with_assets(mock_assets())
        .with_readiness(readiness.clone()));
    let after = ready_router.oneshot(get("/readyz")).await.unwrap();
    assert_eq!(after.status(), StatusCode::OK);
}

#[tokio::test]
async fn metrics_enabled_exposes_endpoint() {
    let response = app_with_assets().oneshot(get("/metrics")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
}

#[tokio::test]
async fn metrics_disabled_returns_not_found() {
    let deployment = DeploymentConfig {
        metrics_enabled: false,
        ..DeploymentConfig::default()
    };
    let router = app(AppState::from_search(stub_search())
        .with_assets(mock_assets())
        .with_deployment(deployment));

    let response = router.oneshot(get("/metrics")).await.unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn security_headers_present_on_success() {
    let response = app_with_assets().oneshot(get("/config")).await.unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_security_headers(&response);
}

#[tokio::test]
async fn security_headers_present_on_error() {
    let response = app_with_assets()
        .oneshot(get("/assets/missing.9999abcd.js"))
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::NOT_FOUND);
    assert_security_headers(&response);
}

#[tokio::test]
async fn oversized_request_body_is_rejected_with_413() {
    let deployment = DeploymentConfig {
        max_request_body_bytes: 16,
        ..DeploymentConfig::default()
    };
    let router = app(AppState::from_search(stub_search())
        .with_assets(mock_assets())
        .with_deployment(deployment));

    let big_body = "q=".to_string() + &"x".repeat(1024);
    let request = Request::builder()
        .method("POST")
        .uri("/search")
        .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
        .body(Body::from(big_body))
        .unwrap();

    let response = router.oneshot(request).await.unwrap();
    assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
}

fn rate_limited_config() -> LimiterConfig {
    LimiterConfig {
        pass_reserved_nets: false,
        trusted_proxies: vec!["10.0.0.0/8".parse().unwrap()],
        rate_limit: RateLimitConfig {
            capacity: 1.0,
            refill_per_second: 0.0,
            suspicious_capacity: 1.0,
            suspicious_refill_per_second: 0.0,
        },
        ..LimiterConfig::default()
    }
}

#[tokio::test]
async fn client_exceeding_rate_limit_is_rejected_with_429() {
    let router = app(AppState::from_search(stub_search())
        .with_assets(mock_assets())
        .with_limiter_config(rate_limited_config(), ""));

    let first = router
        .clone()
        .oneshot(browser_request(
            "203.0.113.77",
            "/search?q=rust&format=json",
        ))
        .await
        .unwrap();
    assert_eq!(
        first.status(),
        StatusCode::OK,
        "the first request must reach the handler"
    );

    let second = router
        .oneshot(browser_request(
            "203.0.113.77",
            "/search?q=rust&format=json",
        ))
        .await
        .unwrap();
    assert_eq!(
        second.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "a client exceeding the rate limit must be rejected with 429"
    );
    assert_security_headers(&second);
}
