use std::sync::Arc;

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};

use tower::ServiceExt;
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

async fn body_string(response: axum::response::Response) -> String {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

#[tokio::test]
async fn get_search_returns_json_with_aggregated_results() {
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
    assert!(
        value["results"].is_array(),
        "results should be a JSON array: {value}"
    );
    assert_eq!(value["results"].as_array().unwrap().len(), 1);
    assert_eq!(value["results"][0]["engine"], "stub");
    assert_eq!(value["results"][0]["url"], "https://stub.test/");
}

#[tokio::test]
async fn post_search_form_body_returns_json() {
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

    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(value["results"].as_array().unwrap().len(), 1);
    assert_eq!(value["results"][0]["engine"], "stub");
}

#[tokio::test]
async fn explicit_json_format_returns_json() {
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
    assert_eq!(value["results"].as_array().unwrap().len(), 1);
}

#[tokio::test]
async fn explicit_rss_format_returns_rss() {
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

    let text = body_string(response).await;
    assert!(text.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
    assert!(text.contains("<rss version=\"2.0\">"));
    assert!(text.contains("<item>"));
}

#[tokio::test]
async fn unsupported_format_returns_client_error_naming_format() {
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

        let status = response.status();
        assert!(
            status.is_client_error(),
            "format={raw} should yield a 4xx client error, got {status}"
        );
        assert_eq!(status, StatusCode::BAD_REQUEST);

        let text = body_string(response).await;
        assert!(
            text.contains(raw),
            "error should name the offending format {raw}: {text}"
        );
    }
}

/// A missing required parameter (`q`) is rejected with `400 Bad Request`.
#[tokio::test]
async fn missing_query_parameter_returns_bad_request_naming_parameter() {
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
    let text = body_string(response).await;
    assert!(
        text.contains('q'),
        "error should name the parameter: {text}"
    );
}
