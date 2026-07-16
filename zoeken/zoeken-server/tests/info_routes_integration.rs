use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use axum::response::Response;
use tower::ServiceExt;

use metrics_exporter_prometheus::{PrometheusBuilder, PrometheusHandle};
use zoeken_engine_core::{
    Engine, EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams, SearchQueryView,
};
use zoeken_metrics::{EngineMetricsRecorder, ErrorCategory};
use zoeken_results::{MainResult, Result_};
use zoeken_search::{
    EngineExecResult, EngineExecutor, EngineFuture, EngineRegistry, RegisteredEngine, Search,
    SearchConfig,
};
use zoeken_server::{AppState, app};

const STUB_ENGINE: &str = "stub";

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
            name: STUB_ENGINE.to_string(),
            categories: vec!["general".to_string()],
            ..EngineMeta::default()
        },
    };
    let registry = EngineRegistry::from_engines([RegisteredEngine::new(Arc::new(engine))]);
    let executor: Arc<dyn EngineExecutor> = Arc::new(ImmediateExecutor);
    Search::new(registry, executor, SearchConfig::default())
}

fn test_app() -> Router {
    app(AppState::from_search(stub_search()))
}

async fn get(router: Router, uri: &str) -> Response {
    router
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

fn content_type(response: &Response) -> String {
    response
        .headers()
        .get(header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string()
}

async fn body_json(response: Response) -> serde_json::Value {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&bytes).expect("body should be valid JSON")
}

async fn body_text(response: Response) -> String {
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(bytes.to_vec()).expect("body should be valid UTF-8")
}

#[tokio::test]
async fn config_returns_instance_configuration_json() {
    let response = get(test_app(), "/config").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(content_type(&response), "application/json");

    let value = body_json(response).await;

    assert!(value["instance_name"].is_string(), "instance_name present");
    assert_eq!(value["version"], env!("CARGO_PKG_VERSION"));

    assert!(value["brand"].is_object(), "brand metadata present");
    assert!(value["limiter"].is_object(), "limiter metadata present");
    assert!(value["plugins"].is_array(), "Upstream plugin list present");

    assert!(
        value["default_locale"].is_string(),
        "default_locale present"
    );
    assert!(!value["safe_search"].is_null(), "safe_search present");

    let engines = value["engines"].as_array().expect("engines array");
    assert!(!engines.is_empty(), "at least one engine is configured");
    let stub = engines
        .iter()
        .find(|e| e["name"] == STUB_ENGINE)
        .expect("configured stub engine listed in /config");
    let categories = stub["categories"].as_array().expect("engine categories");
    assert!(
        categories.iter().any(|c| c == "general"),
        "stub engine advertises the general category: {categories:?}"
    );
}

#[tokio::test]
async fn healthz_reports_ok() {
    let response = get(test_app(), "/healthz").await;

    assert_eq!(response.status(), StatusCode::OK);
    let value = body_json(response).await;
    assert_eq!(value["status"], "ok");
}

#[tokio::test]
async fn stats_returns_json_with_engines_array() {
    let response = get(test_app(), "/stats").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(content_type(&response), "application/json");

    let value = body_json(response).await;
    assert_eq!(
        value["engines"],
        serde_json::json!([]),
        "no handle wired => empty engines array"
    );
}

#[tokio::test]
async fn stats_errors_returns_json_with_engines_array() {
    let response = get(test_app(), "/stats/errors").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(content_type(&response), "application/json");

    let value = body_json(response).await;
    assert_eq!(
        value["engines"],
        serde_json::json!([]),
        "no handle wired => empty engines array"
    );
}

#[tokio::test]
async fn metrics_renders_prometheus_text_without_handle() {
    let response = get(test_app(), "/metrics").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(
        content_type(&response).starts_with("text/plain"),
        "metrics content type is text/plain: {}",
        content_type(&response)
    );

    let body = body_text(response).await;
    assert!(
        body.is_empty(),
        "no handle wired => empty exposition, got: {body:?}"
    );
}

fn handle_with_recorded_samples() -> PrometheusHandle {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, || {
        let engine_metrics = EngineMetricsRecorder::new();
        engine_metrics.record_timing(
            STUB_ENGINE,
            Duration::from_millis(250),
            Some(Duration::from_millis(150)),
        );
        engine_metrics.record_error(STUB_ENGINE, ErrorCategory::Timeout);
    });
    handle
}

#[tokio::test]
async fn metrics_renders_injected_handle_exposition() {
    let handle = handle_with_recorded_samples();
    let router = app(AppState::from_search(stub_search()).with_metrics_handle(handle));

    let response = get(router, "/metrics").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert!(content_type(&response).starts_with("text/plain"));

    let body = body_text(response).await;
    // The exposition renders the injected handle's recorded samples.
    assert!(
        body.contains("zoeken_engine_response_time_total_seconds"),
        "exposition includes the total-timing metric: {body}"
    );
    assert!(
        body.contains(&format!("engine=\"{STUB_ENGINE}\"")),
        "exposition carries the stub engine label: {body}"
    );
    assert!(
        body.contains("zoeken_engine_errors_total"),
        "exposition includes the error counter: {body}"
    );
}

#[tokio::test]
async fn stats_derives_engine_timing_from_injected_handle() {
    let handle = handle_with_recorded_samples();
    let router = app(AppState::from_search(stub_search()).with_metrics_handle(handle));

    let response = get(router, "/stats").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(content_type(&response), "application/json");

    let value = body_json(response).await;
    let engines = value["engines"].as_array().expect("engines array");
    let stub = engines
        .iter()
        .find(|e| e["engine"] == STUB_ENGINE)
        .expect("stub engine timing derived from the handle");
    // One total-time observation was recorded (250ms).
    assert_eq!(stub["total_count"], 1, "one total observation recorded");
    assert!(
        stub["total_sum_seconds"].as_f64().unwrap() > 0.0,
        "total time is positive: {stub}"
    );
    // The HTTP leg (150ms) was recorded too.
    assert_eq!(stub["http_count"], 1, "one http observation recorded");
}

#[tokio::test]
async fn stats_errors_derives_counts_from_injected_handle() {
    let handle = handle_with_recorded_samples();
    let router = app(AppState::from_search(stub_search()).with_metrics_handle(handle));

    let response = get(router, "/stats/errors").await;

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(content_type(&response), "application/json");

    let value = body_json(response).await;
    let engines = value["engines"].as_array().expect("engines array");
    let stub = engines
        .iter()
        .find(|e| e["engine"] == STUB_ENGINE)
        .expect("stub engine errors derived from the handle");
    assert_eq!(stub["total"], 1, "one timeout error recorded");
    assert_eq!(
        stub["errors"]["timeout"], 1,
        "the error is categorized as timeout: {stub}"
    );
}
