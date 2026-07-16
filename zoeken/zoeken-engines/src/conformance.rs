//! Fixture-based engine conformance testing.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zoeken_engine_core::{
    Engine, EngineError, EngineResponse, EngineResults, RequestParams, SearchQueryView,
};

/// A recorded conformance case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Fixture {
    pub engine: String,
    #[serde(default)]
    pub case: Option<String>,
    pub query: SearchQueryView,
    #[serde(default)]
    pub golden_request: Option<RequestParams>,
    #[serde(default)]
    pub response: EngineResponse,
    pub golden_results: EngineResults,
}

impl Fixture {
    pub fn capture(
        engine: impl Into<String>,
        query: SearchQueryView,
        response: EngineResponse,
        golden_results: EngineResults,
    ) -> Self {
        Fixture {
            engine: engine.into(),
            case: None,
            query,
            golden_request: None,
            response,
            golden_results,
        }
    }

    pub fn with_case(mut self, case: impl Into<String>) -> Self {
        self.case = Some(case.into());
        self
    }

    pub fn with_golden_request(mut self, request: RequestParams) -> Self {
        self.golden_request = Some(request);
        self
    }

    pub fn label(&self) -> String {
        match &self.case {
            Some(case) => format!("{}/{}", self.engine, case),
            None => self.engine.clone(),
        }
    }

    pub fn load(path: impl AsRef<Path>) -> Result<Self, FixtureError> {
        load_fixture(path)
    }

    pub fn save(&self, path: impl AsRef<Path>) -> Result<(), FixtureError> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(|source| FixtureError::Io {
                path: parent.to_path_buf(),
                source,
            })?;
        }
        let json = serde_json::to_string_pretty(self)
            .map_err(|source| FixtureError::Serialize { source })?;
        fs::write(path, json).map_err(|source| FixtureError::Io {
            path: path.to_path_buf(),
            source,
        })
    }
}

/// Fixture I/O or serialization error.
#[derive(Debug, Error)]
pub enum FixtureError {
    #[error("fixture I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        source: std::io::Error,
    },
    #[error("failed to parse fixture {path}: {source}")]
    Parse {
        path: PathBuf,
        source: serde_json::Error,
    },
    #[error("failed to serialize fixture: {source}")]
    Serialize { source: serde_json::Error },
}

pub fn load_fixture(path: impl AsRef<Path>) -> Result<Fixture, FixtureError> {
    let path = path.as_ref();
    let text = fs::read_to_string(path).map_err(|source| FixtureError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    serde_json::from_str(&text).map_err(|source| FixtureError::Parse {
        path: path.to_path_buf(),
        source,
    })
}

pub fn load_fixtures(dir: impl AsRef<Path>) -> Result<Vec<Fixture>, FixtureError> {
    let dir = dir.as_ref();
    if !dir.exists() {
        return Ok(Vec::new());
    }
    let mut paths: Vec<PathBuf> = Vec::new();
    let entries = fs::read_dir(dir).map_err(|source| FixtureError::Io {
        path: dir.to_path_buf(),
        source,
    })?;
    for entry in entries {
        let entry = entry.map_err(|source| FixtureError::Io {
            path: dir.to_path_buf(),
            source,
        })?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("json") {
            paths.push(path);
        }
    }
    paths.sort();
    paths.into_iter().map(load_fixture).collect()
}

pub fn load_fixtures_for(
    root: impl AsRef<Path>,
    engine: &str,
) -> Result<Vec<Fixture>, FixtureError> {
    load_fixtures(root.as_ref().join(engine))
}

/// Conformance mismatch between engine output and golden fixture.
#[derive(Debug, Clone, Error)]
pub enum ConformanceMismatch {
    #[error(
        "conformance mismatch for `{label}`: parsed results differ from the golden output\n{diff}"
    )]
    Results { label: String, diff: String },
    #[error(
        "conformance mismatch for `{label}`: built request differs from the golden request\n{diff}"
    )]
    Request { label: String, diff: String },
    #[error(
        "conformance failure for `{label}`: engine returned an error while parsing the fixture response: {source}"
    )]
    ResponseError { label: String, source: EngineError },
}

pub fn run_conformance(engine: &dyn Engine, fixture: &Fixture) -> Result<(), ConformanceMismatch> {
    if fixture.golden_request.is_some() {
        run_request_conformance(engine, fixture)?;
    }
    run_response_conformance(engine, fixture)
}

/// Compare `engine.response(&fixture.response)` against the fixture's golden
/// results, ignoring any golden request.
pub fn run_response_conformance(
    engine: &dyn Engine,
    fixture: &Fixture,
) -> Result<(), ConformanceMismatch> {
    let produced = engine.response(&fixture.response).map_err(|source| {
        ConformanceMismatch::ResponseError {
            label: fixture.label(),
            source,
        }
    })?;
    if produced == fixture.golden_results {
        return Ok(());
    }
    Err(ConformanceMismatch::Results {
        label: fixture.label(),
        diff: value_diff(&fixture.golden_results, &produced),
    })
}

/// Compare the request `engine.request` builds for `fixture.query` against the
/// fixture's golden request. A no-op when the fixture has no golden request.
///
/// The starting [`RequestParams`] mirror the fields an online processor
/// pre-populates from the query before the engine fills in HTTP details, so the
/// comparison reflects the engine's own request-building contribution.
pub fn run_request_conformance(
    engine: &dyn Engine,
    fixture: &Fixture,
) -> Result<(), ConformanceMismatch> {
    let Some(golden) = &fixture.golden_request else {
        return Ok(());
    };
    let mut params = prepopulated_params(&fixture.query);
    engine.request(&fixture.query, &mut params);
    if &params == golden {
        return Ok(());
    }
    Err(ConformanceMismatch::Request {
        label: fixture.label(),
        diff: value_diff(golden, &params),
    })
}

/// Run conformance for every fixture in `fixtures`, collecting all mismatches.
/// Returns `Ok(())` only when every fixture passes.
pub fn run_all(engine: &dyn Engine, fixtures: &[Fixture]) -> Result<(), Vec<ConformanceMismatch>> {
    let mismatches: Vec<ConformanceMismatch> = fixtures
        .iter()
        .filter_map(|fixture| run_conformance(engine, fixture).err())
        .collect();
    if mismatches.is_empty() {
        Ok(())
    } else {
        Err(mismatches)
    }
}

fn prepopulated_params(query: &SearchQueryView) -> RequestParams {
    RequestParams {
        query: query.query.clone(),
        pageno: query.pageno,
        safesearch: query.safesearch,
        time_range: query.time_range,
        locale_key: query.locale.clone(),
        ..RequestParams::default()
    }
}

fn value_diff<T: Serialize>(expected: &T, actual: &T) -> String {
    let expected_json = serde_json::to_value(expected);
    let actual_json = serde_json::to_value(actual);
    match (expected_json, actual_json) {
        (Ok(expected), Ok(actual)) => {
            let mut diffs = Vec::new();
            collect_diffs("", &expected, &actual, &mut diffs);
            if diffs.is_empty() {
                format!(
                    "  expected: {}\n  actual:   {}",
                    serde_json::to_string(&expected).unwrap_or_default(),
                    serde_json::to_string(&actual).unwrap_or_default()
                )
            } else {
                diffs.join("\n")
            }
        }
        _ => "  <values could not be serialized for diffing>".to_string(),
    }
}

fn collect_diffs(
    path: &str,
    expected: &serde_json::Value,
    actual: &serde_json::Value,
    out: &mut Vec<String>,
) {
    use serde_json::Value;
    match (expected, actual) {
        (Value::Object(exp), Value::Object(act)) => {
            let mut keys: Vec<&String> = exp.keys().chain(act.keys()).collect();
            keys.sort();
            keys.dedup();
            for key in keys {
                let child = if path.is_empty() {
                    key.clone()
                } else {
                    format!("{path}.{key}")
                };
                match (exp.get(key), act.get(key)) {
                    (Some(e), Some(a)) => collect_diffs(&child, e, a, out),
                    (Some(e), None) => {
                        out.push(format!("  {child}: missing in actual (expected {e})"))
                    }
                    (None, Some(a)) => {
                        out.push(format!("  {child}: unexpected in actual (got {a})"))
                    }
                    (None, None) => {}
                }
            }
        }
        (Value::Array(exp), Value::Array(act)) => {
            if exp.len() != act.len() {
                out.push(format!(
                    "  {}: length differs (expected {}, actual {})",
                    display_path(path),
                    exp.len(),
                    act.len()
                ));
            }
            for (index, (e, a)) in exp.iter().zip(act.iter()).enumerate() {
                collect_diffs(&format!("{path}[{index}]"), e, a, out);
            }
        }
        _ => {
            if expected != actual {
                out.push(format!(
                    "  {}: expected {expected}, actual {actual}",
                    display_path(path)
                ));
            }
        }
    }
}

fn display_path(path: &str) -> &str {
    if path.is_empty() { "<root>" } else { path }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoeken_engine_core::{EngineMeta, HttpMethod, Processor};
    use zoeken_results::{MainResult, Result_};

    struct LineEngine {
        meta: EngineMeta,
    }

    impl LineEngine {
        fn new() -> Self {
            LineEngine {
                meta: EngineMeta {
                    name: "line".to_string(),
                    engine_type: Processor::Online,
                    paging: true,
                    ..EngineMeta::default()
                },
            }
        }
    }

    impl Engine for LineEngine {
        fn metadata(&self) -> &EngineMeta {
            &self.meta
        }

        fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
            p.method = HttpMethod::Get;
            p.url = Some(format!(
                "https://example.test/search?q={}&page={}",
                q.query, q.pageno
            ));
        }

        fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
            let mut results = EngineResults::new();
            for line in resp.text().lines() {
                let line = line.trim();
                if line.is_empty() {
                    continue;
                }
                let (title, url) = line
                    .split_once('|')
                    .ok_or_else(|| EngineError::Parse(format!("bad line: {line}")))?;
                results.add(Result_::Main(MainResult {
                    url: url.to_string(),
                    normalized_url: url.to_string(),
                    title: title.to_string(),
                    engine: "line".to_string(),
                    ..MainResult::default()
                }));
            }
            Ok(results)
        }
    }

    fn sample_query() -> SearchQueryView {
        SearchQueryView {
            query: "rust".to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        }
    }

    fn sample_fixture() -> Fixture {
        let mut golden = EngineResults::new();
        golden.add(Result_::Main(MainResult {
            url: "https://a.test/".to_string(),
            normalized_url: "https://a.test/".to_string(),
            title: "Alpha".to_string(),
            engine: "line".to_string(),
            ..MainResult::default()
        }));
        let response = EngineResponse {
            status: 200,
            url: "https://example.test/".to_string(),
            body: b"Alpha|https://a.test/\n".to_vec(),
            ..EngineResponse::default()
        };
        Fixture::capture("line", sample_query(), response, golden).with_case("basic")
    }

    #[test]
    fn run_conformance_passes_on_matching_golden() {
        let engine = LineEngine::new();
        let fixture = sample_fixture();
        assert!(run_conformance(&engine, &fixture).is_ok());
    }

    #[test]
    fn run_conformance_reports_results_mismatch() {
        let engine = LineEngine::new();
        let mut fixture = sample_fixture();
        if let Result_::Main(r) = &mut fixture.golden_results.results[0] {
            r.title = "WRONG".to_string();
        }
        let err = run_conformance(&engine, &fixture).unwrap_err();
        match err {
            ConformanceMismatch::Results { label, diff } => {
                assert_eq!(label, "line/basic");
                assert!(diff.contains("title"), "diff should mention title: {diff}");
            }
            other => panic!("expected results mismatch, got {other:?}"),
        }
    }

    #[test]
    fn run_conformance_reports_response_error() {
        let engine = LineEngine::new();
        let mut fixture = sample_fixture();
        fixture.response.body = b"no-separator-line".to_vec();
        let err = run_conformance(&engine, &fixture).unwrap_err();
        assert!(matches!(err, ConformanceMismatch::ResponseError { .. }));
    }

    #[test]
    fn request_conformance_compares_built_request() {
        let engine = LineEngine::new();
        let mut params = prepopulated_params(&sample_query());
        engine.request(&sample_query(), &mut params);
        let fixture = sample_fixture().with_golden_request(params);
        assert!(run_conformance(&engine, &fixture).is_ok());
    }

    #[test]
    fn request_conformance_reports_request_mismatch() {
        let engine = LineEngine::new();
        let mut params = prepopulated_params(&sample_query());
        engine.request(&sample_query(), &mut params);
        params.url = Some("https://wrong.test/".to_string());
        let fixture = sample_fixture().with_golden_request(params);
        let err = run_conformance(&engine, &fixture).unwrap_err();
        assert!(matches!(err, ConformanceMismatch::Request { .. }));
    }

    #[test]
    fn fixture_round_trips_through_json() {
        let fixture = sample_fixture();
        let json = serde_json::to_string_pretty(&fixture).unwrap();
        let parsed: Fixture = serde_json::from_str(&json).unwrap();
        assert_eq!(fixture, parsed);
        // The body is stored as a readable string, not a byte array.
        assert!(json.contains("Alpha|https://a.test/"));
    }

    #[test]
    fn load_fixtures_reads_directory_in_order() {
        let dir = std::env::temp_dir().join(format!("zoeken-engines-conf-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        let a = sample_fixture().with_case("a");
        let b = sample_fixture().with_case("b");
        a.save(dir.join("a.json")).unwrap();
        b.save(dir.join("b.json")).unwrap();
        let loaded = load_fixtures(&dir).unwrap();
        assert_eq!(loaded.len(), 2);
        assert_eq!(loaded[0].case.as_deref(), Some("a"));
        assert_eq!(loaded[1].case.as_deref(), Some("b"));
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_fixtures_missing_dir_is_empty() {
        let dir = std::env::temp_dir().join("zoeken-engines-conf-does-not-exist-xyz");
        assert!(load_fixtures(&dir).unwrap().is_empty());
    }

    struct EchoOfflineEngine {
        meta: EngineMeta,
    }

    impl EchoOfflineEngine {
        fn new() -> Self {
            EchoOfflineEngine {
                meta: EngineMeta {
                    name: "echo".to_string(),
                    engine_type: Processor::Offline,
                    ..EngineMeta::default()
                },
            }
        }
    }

    impl Engine for EchoOfflineEngine {
        fn metadata(&self) -> &EngineMeta {
            &self.meta
        }

        fn request(&self, _q: &SearchQueryView, _p: &mut RequestParams) {}

        fn response(&self, _resp: &EngineResponse) -> Result<EngineResults, EngineError> {
            // An offline engine computes its output independent of the HTTP body.
            let mut results = EngineResults::new();
            results.add(Result_::Answer(zoeken_results::Answer {
                answer: "echo: rust".to_string(),
                ..zoeken_results::Answer::default()
            }));
            Ok(results)
        }
    }

    #[test]
    fn harness_is_processor_agnostic_for_offline_engine() {
        let engine = EchoOfflineEngine::new();
        let mut golden = EngineResults::new();
        golden.add(Result_::Answer(zoeken_results::Answer {
            answer: "echo: rust".to_string(),
            ..zoeken_results::Answer::default()
        }));
        // Offline engines have no meaningful HTTP response; the default is fine.
        let fixture = Fixture::capture("echo", sample_query(), EngineResponse::default(), golden)
            .with_case("offline");
        assert!(run_conformance(&engine, &fixture).is_ok());

        // A divergent golden answer is still reported as a results mismatch,
        // confirming golden-output comparison holds for offline processors too.
        let mut wrong = fixture.clone();
        wrong.golden_results.answers[0].answer = "WRONG".to_string();
        assert!(matches!(
            run_conformance(&engine, &wrong),
            Err(ConformanceMismatch::Results { .. })
        ));
    }
}
