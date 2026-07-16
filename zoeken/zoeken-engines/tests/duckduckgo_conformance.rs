//! DuckDuckGo conformance integration tests.

use std::path::PathBuf;

use zoeken_engines::{ConformanceMismatch, DuckDuckGo, Fixture, run_all, run_conformance};

const ENGINE: &str = "duckduckgo";

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

fn ddg_fixtures() -> Vec<Fixture> {
    zoeken_engines::load_fixtures_for(fixtures_root(), ENGINE)
        .expect("load duckduckgo fixtures from disk")
}

#[test]
fn duckduckgo_matches_recorded_golden_output() {
    let fixtures = ddg_fixtures();
    assert!(
        !fixtures.is_empty(),
        "no fixtures found under fixtures/{ENGINE}; run the ignored generate_fixtures test in duckduckgo.rs"
    );

    let engine = DuckDuckGo::new();
    if let Err(mismatches) = run_all(&engine, &fixtures) {
        let report = mismatches
            .iter()
            .map(|m| m.to_string())
            .collect::<Vec<_>>()
            .join("\n");
        panic!("expected all recorded fixtures to pass conformance, but:\n{report}");
    }

    for fixture in &fixtures {
        assert!(
            run_conformance(&engine, fixture).is_ok(),
            "fixture `{}` should pass conformance",
            fixture.label()
        );
    }
}

#[test]
fn duckduckgo_reports_failure_when_golden_output_diverges() {
    let fixtures = ddg_fixtures();
    let engine = DuckDuckGo::new();

    let mut wrong = fixtures
        .iter()
        .find(|f| !f.golden_results.results.is_empty())
        .cloned()
        .expect("at least one duckduckgo fixture should produce main results");

    assert!(
        run_conformance(&engine, &wrong).is_ok(),
        "control fixture `{}` should pass before corruption",
        wrong.label()
    );

    wrong.golden_results.results.pop();

    let err = run_conformance(&engine, &wrong)
        .expect_err("diverged golden output must NOT be reported as a pass");
    match err {
        ConformanceMismatch::Results { label, diff } => {
            assert_eq!(label, wrong.label());
            assert!(
                !diff.is_empty(),
                "results mismatch should carry a field-level diff"
            );
        }
        other => panic!("expected a results mismatch, got: {other:?}"),
    }
}
