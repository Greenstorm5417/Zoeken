//! Concurrent request smoke: fixture-backed search under load.

use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::body::Body;
use axum::http::{Request, StatusCode};
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

#[tokio::test]
async fn concurrent_search_requests_complete_under_budget() {
    let app = test_app();
    let started = Instant::now();
    let mut handles = Vec::with_capacity(32);
    for i in 0..32 {
        let app = app.clone();
        handles.push(tokio::spawn(async move {
            let response = app
                .oneshot(
                    Request::builder()
                        .uri(format!("/search?q=load{i}&format=json"))
                        .body(Body::empty())
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::OK);
        }));
    }
    for handle in handles {
        handle.await.unwrap();
    }
    let elapsed = started.elapsed();
    assert!(
        elapsed.as_secs_f64() < 5.0,
        "32 concurrent fixture searches took {elapsed:?}"
    );
}

#[tokio::test]
async fn healthz_stays_responsive_during_search_burst() {
    let app = test_app();
    let mut search_handles = Vec::new();
    for i in 0..16 {
        let app = app.clone();
        search_handles.push(tokio::spawn(async move {
            app.oneshot(
                Request::builder()
                    .uri(format!("/search?q=burst{i}&format=json"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap()
        }));
    }
    let health = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/healthz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(health.status(), StatusCode::OK);
    for handle in search_handles {
        assert_eq!(handle.await.unwrap().status(), StatusCode::OK);
    }
}
