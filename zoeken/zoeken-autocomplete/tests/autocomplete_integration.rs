//! Integration tests for `zoeken-autocomplete`: service end-to-end through the public API.

use std::sync::Arc;
use std::time::Duration;

use zoeken_autocomplete::{
    AutocompleteBackend, AutocompleteService, BackendError, StaticBackend, SuggestFuture,
    Suggestion, parse_duckduckgo_suggestions, suggestions_from_texts,
};

/// A backend that always fails.
struct ErrorBackend;

impl AutocompleteBackend for ErrorBackend {
    fn name(&self) -> &str {
        "error"
    }
    fn suggest<'a>(&'a self, _query: &'a str, _locale: &'a str) -> SuggestFuture<'a> {
        Box::pin(async { Err(BackendError::Request("upstream refused".to_string())) })
    }
}

struct SlowBackend;

impl AutocompleteBackend for SlowBackend {
    fn name(&self) -> &str {
        "slow"
    }
    fn suggest<'a>(&'a self, _query: &'a str, _locale: &'a str) -> SuggestFuture<'a> {
        Box::pin(async {
            tokio::time::sleep(Duration::from_secs(60)).await;
            Ok(vec![Suggestion::text("too-late")])
        })
    }
}

/// Driving the service through a configured StaticBackend returns its fixed suggestions.
#[tokio::test]
async fn configured_backend_returns_fixed_suggestions() {
    let fixed = vec![
        "rust".to_string(),
        "rustlang".to_string(),
        "rust book".to_string(),
    ];
    let backend = Arc::new(StaticBackend::new("fixture", fixed.clone()));
    let service = AutocompleteService::with_backend(backend);

    let suggestions = service.suggest("rus", "en-US").await;

    assert_eq!(suggestions, suggestions_from_texts(fixed));
    assert!(service.is_enabled());
    assert_eq!(service.backend_name(), Some("fixture"));
}

/// A configured backend with empty list legitimately returns empty.
#[tokio::test]
async fn configured_backend_with_empty_list_returns_empty() {
    let backend = Arc::new(StaticBackend::new("empty-fixture", Vec::new()));
    let service = AutocompleteService::with_backend(backend);

    let suggestions = service.suggest("anything", "en-US").await;

    assert!(suggestions.is_empty());
    // Still an *enabled* service — it simply had nothing to offer.
    assert!(service.is_enabled());
    assert_eq!(service.backend_name(), Some("empty-fixture"));
}

/// A disabled service (no backend) returns an empty list.
#[tokio::test]
async fn disabled_service_returns_empty_list() {
    let service = AutocompleteService::disabled();

    let suggestions = service.suggest("rus", "en-US").await;

    assert!(suggestions.is_empty());
    assert!(!service.is_enabled());
    assert_eq!(service.backend_name(), None);
}

/// The default service is disabled and yields empty for empty inputs.
#[tokio::test]
async fn default_service_returns_empty_list_for_empty_inputs() {
    let service = AutocompleteService::default();

    let suggestions = service.suggest("", "").await;

    assert!(suggestions.is_empty());
    assert!(!service.is_enabled());
}

/// A backend that returns an error yields an empty list.
#[tokio::test]
async fn erroring_backend_returns_empty_list() {
    let service = AutocompleteService::with_backend(Arc::new(ErrorBackend));

    let suggestions = service.suggest("rus", "en-US").await;

    assert!(suggestions.is_empty());
    // The service remains enabled; the error only collapses this call's result.
    assert!(service.is_enabled());
}

/// A backend that sleeps past timeout yields empty and does not stall the caller.
#[tokio::test]
async fn slow_backend_times_out_to_empty_list() {
    let service = AutocompleteService::with_backend(Arc::new(SlowBackend))
        .with_timeout(Duration::from_millis(20));

    let suggestions = service.suggest("rus", "en-US").await;

    assert!(suggestions.is_empty());
}

/// parse_duckduckgo_suggestions extracts strings from type=list payload second element.
#[test]
fn duckduckgo_payload_parses_to_suggestion_list() {
    let payload = serde_json::json!(["rust", ["rust", "rust lang", "rustup", "rust book"]]);

    let suggestions = parse_duckduckgo_suggestions(&payload);

    assert_eq!(
        suggestions,
        vec![
            "rust".to_string(),
            "rust lang".to_string(),
            "rustup".to_string(),
            "rust book".to_string(),
        ]
    );
}

/// A payload lacking the suggestion list parses to empty.
#[test]
fn duckduckgo_payload_without_suggestions_parses_to_empty() {
    assert!(parse_duckduckgo_suggestions(&serde_json::json!(["rus"])).is_empty());
    assert!(parse_duckduckgo_suggestions(&serde_json::json!({})).is_empty());
}
