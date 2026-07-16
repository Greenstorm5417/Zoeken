//! Schema golden fixtures: compare JSON key/type shapes (not brittle full strings).

use std::fs;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use metrics_exporter_prometheus::PrometheusBuilder;
use serde_json::Value;
use tower::ServiceExt;
use zoeken_engine_core::{
    Engine, EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams, SearchQueryView,
};
use zoeken_metrics::{EngineMetricsRecorder, ErrorCategory};
use zoeken_results::{Answer, MainResult, Result_};
use zoeken_search::{
    EngineExecResult, EngineExecutor, EngineFuture, EngineRegistry, RegisteredEngine,
    ResultContainer, Search, SearchConfig, UnresponsiveCause, UnresponsiveEngine,
};
use zoeken_server::serialize::{format_csv, format_json_for_query, format_rss};
use zoeken_server::{AppState, app};

fn fixtures_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/fixtures/schema")
}

fn load_fixture(name: &str) -> Value {
    let path = fixtures_dir().join(name);
    let text = fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

/// Recursively assert `actual` has at least the keys/types of `expected` shape.
fn assert_shape(actual: &Value, expected: &Value, path: &str) {
    match (actual, expected) {
        (Value::Object(a), Value::Object(e)) => {
            for (key, e_val) in e {
                let a_val = a
                    .get(key)
                    .unwrap_or_else(|| panic!("missing key {path}.{key}"));
                assert_shape(a_val, e_val, &format!("{path}.{key}"));
            }
        }
        (Value::Array(a), Value::Array(e)) => {
            if e.is_empty() {
                return;
            }
            // When expected is a fixed-length tuple of primitives, compare lengths
            // and each element; otherwise treat e[0] as an element prototype.
            if e.iter().all(|v| !v.is_object() && !v.is_array()) && e.len() > 1 {
                assert_eq!(
                    a.len(),
                    e.len(),
                    "array length mismatch at {path}: actual={} expected={}",
                    a.len(),
                    e.len()
                );
                for (i, (av, ev)) in a.iter().zip(e.iter()).enumerate() {
                    assert_shape(av, ev, &format!("{path}[{i}]"));
                }
                return;
            }
            let prototype = &e[0];
            assert!(!a.is_empty(), "actual array at {path} is empty");
            assert_shape(&a[0], prototype, &format!("{path}[0]"));
        }
        (Value::String(_), Value::String(_)) => {}
        (Value::Number(_), Value::Number(_)) => {}
        (Value::Bool(_), Value::Bool(_)) => {}
        (Value::Null, Value::Null) => {}
        (a, e) => panic!("type mismatch at {path}: actual={a:?} expected_shape={e:?}"),
    }
}

fn sample_container() -> ResultContainer {
    ResultContainer {
        results: vec![Result_::Main(MainResult {
            url: "https://example.test/rust".into(),
            normalized_url: "https://example.test/rust".into(),
            title: "Rust".into(),
            content: "A systems language.".into(),
            engine: "duckduckgo".into(),
            score: 1.0,
            positions: vec![1],
            engines: vec!["duckduckgo".into()],
            ..MainResult::default()
        })],
        answers: vec![Answer {
            answer: "42".into(),
            url: Some("https://answer.test/".into()),
            engine: "calculator".into(),
            ..Answer::default()
        }],
        unresponsive_engines: vec![UnresponsiveEngine {
            engine: "bing".into(),
            cause: UnresponsiveCause::Timeout,
        }],
        number_of_results: 1,
        ..ResultContainer::default()
    }
}

#[test]
fn json_search_response_matches_golden_shape() {
    let body = format_json_for_query("rust", &sample_container());
    let actual: Value = serde_json::from_str(&body).unwrap();
    let expected = load_fixture("search_json_shape.json");
    assert_shape(&actual, &expected, "$");
}

#[test]
fn csv_has_header_and_result_row() {
    let body = format_csv(&sample_container());
    let expected = fs::read_to_string(fixtures_dir().join("search_csv_sample.csv")).unwrap();
    let actual_lines: Vec<&str> = body.lines().collect();
    let expected_lines: Vec<&str> = expected.lines().collect();
    assert_eq!(actual_lines[0], expected_lines[0], "CSV header");
    assert!(
        actual_lines.len() >= 2,
        "CSV should include at least one data row"
    );
    assert!(actual_lines[1].contains("Rust"));
    assert!(actual_lines[1].contains("https://example.test/rust"));
}

#[test]
fn rss_channel_and_item_shape() {
    let body = format_rss(&sample_container());
    assert!(body.contains("<rss version=\"2.0\">"));
    assert!(body.contains("<channel>"));
    assert!(body.contains("<item>"));
    assert!(body.contains("<title>Rust</title>"));
    assert!(body.contains("<link>https://example.test/rust</link>"));
}

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
            name: "stub".into(),
            categories: vec!["general".into()],
            ..EngineMeta::default()
        },
    };
    let registry = EngineRegistry::from_engines([RegisteredEngine::new(Arc::new(engine))]);
    Search::new(
        registry,
        Arc::new(ImmediateExecutor),
        SearchConfig::default(),
    )
}

#[tokio::test]
async fn config_key_presence_matches_golden() {
    let response = app(AppState::from_search(stub_search()))
        .oneshot(
            Request::builder()
                .uri("/config")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    let actual: Value = serde_json::from_slice(&bytes).unwrap();
    let expected = load_fixture("config_keys.json");
    assert_shape(&actual, &expected, "$");
}

#[tokio::test]
async fn stats_and_errors_shape_with_forced_failure() {
    let recorder = PrometheusBuilder::new().build_recorder();
    let handle = recorder.handle();
    metrics::with_local_recorder(&recorder, || {
        let engine_metrics = EngineMetricsRecorder::new();
        engine_metrics.record_timing("stub", Duration::from_millis(100), None);
        engine_metrics.record_error("stub", ErrorCategory::Timeout);
        engine_metrics.record_error("stub", ErrorCategory::AccessDenied);
        // HTTP-ish denial and parser failure paths (7.3 golden coverage).
        engine_metrics.record_error("stub", ErrorCategory::TooManyRequests);
        engine_metrics.record_error("stub", ErrorCategory::Parse);
    });

    let router = app(AppState::from_search(stub_search()).with_metrics_handle(handle));

    let stats = router
        .clone()
        .oneshot(
            Request::builder()
                .uri("/stats")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let stats_body: Value =
        serde_json::from_slice(&to_bytes(stats.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_shape(&stats_body, &load_fixture("stats_shape.json"), "$");

    let errors = router
        .oneshot(
            Request::builder()
                .uri("/stats/errors")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let errors_body: Value =
        serde_json::from_slice(&to_bytes(errors.into_body(), usize::MAX).await.unwrap()).unwrap();
    assert_shape(&errors_body, &load_fixture("stats_errors_shape.json"), "$");
    let stub = errors_body["engines"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["engine"] == "stub")
        .expect("stub engine errors");
    assert_eq!(stub["errors"]["timeout"], 1);
    assert_eq!(stub["errors"]["access_denied"], 1);
    assert_eq!(stub["errors"]["too_many_requests"], 1);
    assert_eq!(stub["errors"]["parse"], 1);
}

#[tokio::test]
async fn engine_descriptions_matches_golden_shape() {
    let response = app(AppState::from_search(stub_search()))
        .oneshot(
            Request::builder()
                .uri("/engine_descriptions.json")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let expected = load_fixture("engine_descriptions_shape.json");
    assert_shape(&actual, &expected, "$");
}

#[tokio::test]
async fn autocomplete_opensearch_shape_matches_golden() {
    use zoeken_autocomplete::{AutocompleteService, StaticBackend};

    let backend = Arc::new(StaticBackend::new(
        "stub",
        vec!["rust".to_string(), "rustlang".to_string()],
    ));
    let response = app(AppState::from_search(stub_search())
        .with_autocomplete(AutocompleteService::with_backend(backend)))
    .oneshot(
        Request::builder()
            .uri("/autocompleter?q=rus")
            .body(Body::empty())
            .unwrap(),
    )
    .await
    .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let actual: Value =
        serde_json::from_slice(&to_bytes(response.into_body(), usize::MAX).await.unwrap()).unwrap();
    let expected = load_fixture("autocomplete_shape.json");
    assert_eq!(actual, expected);
}
