use std::net::SocketAddr;
use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};

use tower::ServiceExt;
use zoeken_botdetect::LimiterConfig;
use zoeken_engine_core::{
    Engine, EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};
use zoeken_search::{
    EngineExecResult, EngineExecutor, EngineFuture, EngineRegistry, RegisteredEngine, Search,
    SearchConfig,
};
use zoeken_server::{AppState, app};

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

fn app_with_config(config: LimiterConfig) -> Router {
    app(AppState::from_search(stub_search()).with_limiter_config(config, ""))
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

fn default_app() -> Router {
    app(AppState::from_search(stub_search()))
}

fn block_list_config() -> LimiterConfig {
    LimiterConfig::from_toml_str(
        r#"
        [botdetection]
        trusted_proxies = ["10.0.0.0/8"]

        [botdetection.ip_lists]
        block_ip = ["198.51.100.0/24"]
        pass_reserved_nets = false
        "#,
    )
    .expect("valid limiter.toml")
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

#[tokio::test]
async fn block_listed_ip_is_rejected_before_handler() {
    let response = app_with_config(block_list_config())
        .oneshot(browser_request("198.51.100.9", "/search?q=rust"))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::FORBIDDEN);
    let body = decoded_body(response).await;
    let text = String::from_utf8(body).unwrap();
    assert!(
        !text.contains("number_of_results"),
        "the search handler must not have produced a body: {text}"
    );
}

#[tokio::test]
async fn clean_request_reaches_search_handler() {
    let response = app_with_config(block_list_config())
        .oneshot(browser_request(
            "203.0.113.50",
            "/search?q=rust&format=json",
        ))
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json"
    );

    let body = decoded_body(response).await;
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["results"].as_array().unwrap().len(), 1);
    assert_eq!(value["results"][0]["engine"], "stub");
}

#[tokio::test]
async fn default_config_serves_requests_without_client_ip() {
    let response = default_app()
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
    assert_eq!(value["results"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn disabled_limiter_allows_blatant_bot() {
    let config = block_list_config().with_enabled(false);
    let response = app_with_config(config)
        .oneshot(
            Request::builder()
                .uri("/search?q=rust&format=json")
                .header("x-real-ip", "198.51.100.9")
                .header(header::USER_AGENT, "curl/8.0")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["results"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn suspicious_ua_blocked_until_link_token_css_ping() {
    let mut config = LimiterConfig {
        link_token: true,
        pass_reserved_nets: false,
        trusted_proxies: vec!["10.0.0.0/8".parse().unwrap()],
        ..LimiterConfig::default()
    };
    // One free request while suspicious, then empty; verified clients refill fast.
    config.rate_limit.suspicious_capacity = 1.0;
    config.rate_limit.suspicious_refill_per_second = 0.0;
    config.rate_limit.capacity = 20.0;
    config.rate_limit.refill_per_second = 1_000_000.0;
    let token = "tok123";
    let detector = Arc::new(zoeken_botdetect::Detector::new(config, token));
    let router = app(AppState::from_search(stub_search()).with_bot_detector(Arc::clone(&detector)));

    let first = router
        .clone()
        .oneshot(browser_request("203.0.113.9", "/search?q=rust&format=json"))
        .await
        .unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let blocked = router
        .clone()
        .oneshot(browser_request("203.0.113.9", "/search?q=rust&format=json"))
        .await
        .unwrap();
    assert_eq!(blocked.status(), StatusCode::TOO_MANY_REQUESTS);

    let css = router
        .clone()
        .oneshot(browser_request(
            "203.0.113.9",
            &format!("/client{token}.css"),
        ))
        .await
        .unwrap();
    assert_eq!(css.status(), StatusCode::OK);
    assert!(detector.link_tokens().is_verified("203.0.113.9/32"));

    tokio::time::sleep(std::time::Duration::from_millis(5)).await;

    let allowed = router
        .oneshot(browser_request("203.0.113.9", "/search?q=rust&format=json"))
        .await
        .unwrap();
    assert_eq!(allowed.status(), StatusCode::OK);
}
