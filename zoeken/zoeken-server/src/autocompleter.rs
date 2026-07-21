//! `/autocompleter` suggestions route.

use std::sync::Arc;

use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, header};
use axum::response::{IntoResponse, Response};
use zoeken_autocomplete::Suggestion;

use crate::serialize::signed_proxy_url;
use crate::{AppState, parse_pairs};

const SUGGESTIONS_CONTENT_TYPE: &str = "application/x-suggestions+json";

pub async fn autocompleter_get(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
) -> Response {
    let params = parse_pairs(query.as_deref().unwrap_or(""));
    run_autocomplete(&state, &headers, params).await
}

pub async fn autocompleter_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: String,
) -> Response {
    let mut params = parse_pairs(query.as_deref().unwrap_or(""));
    params.extend(parse_pairs(&body));
    run_autocomplete(&state, &headers, params).await
}

async fn run_autocomplete(
    state: &AppState,
    headers: &HeaderMap,
    params: Vec<(String, String)>,
) -> Response {
    let query = param(&params, "q").unwrap_or_default();
    let locale = param(&params, "locale")
        .or_else(|| param(&params, "language"))
        .unwrap_or_default();

    let mut suggestions = state.autocomplete.suggest(&query, &locale).await;
    maybe_proxy_suggestion_images(state, headers, &params, &mut suggestions);

    // Let the browser reuse suggestion responses for repeated prefixes; the
    // upstream lists barely change minute to minute.
    const CACHE: (header::HeaderName, &str) = (header::CACHE_CONTROL, "private, max-age=300");
    if headers
        .get("X-Requested-With")
        .and_then(|value| value.to_str().ok())
        == Some("XMLHttpRequest")
    {
        // SPA: rich suggestion objects (`text` / optional `subtext` / `image`).
        let body = serde_json::to_string(&suggestions).unwrap_or_else(|_| "[]".to_string());
        ([(header::CONTENT_TYPE, "application/json"), CACHE], body).into_response()
    } else {
        // OpenSearch Suggest: keep the second element as plain strings.
        let texts: Vec<&str> = suggestions.iter().map(|s| s.text.as_str()).collect();
        let body = serde_json::json!([query, texts]).to_string();
        (
            [(header::CONTENT_TYPE, SUGGESTIONS_CONTENT_TYPE), CACHE],
            body,
        )
            .into_response()
    }
}

fn maybe_proxy_suggestion_images(
    state: &AppState,
    headers: &HeaderMap,
    params: &[(String, String)],
    suggestions: &mut [Suggestion],
) {
    if !crate::image_proxy::image_proxy_enabled(state, headers, params) {
        return;
    }
    let secret = &state.settings.server.secret_key;
    for suggestion in suggestions {
        let Some(original) = suggestion.image.as_deref() else {
            continue;
        };
        if zoeken_favicons::validate_proxy_url(original).is_ok() {
            suggestion.image = Some(signed_proxy_url("/image_proxy", "url", original, secret));
        } else {
            suggestion.image = None;
        }
    }
}

fn param(params: &[(String, String)], key: &str) -> Option<String> {
    params
        .iter()
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app;
    use axum::body::{Body, to_bytes};
    use axum::http::{Request, StatusCode};
    use tower::ServiceExt;
    use zoeken_autocomplete::{AutocompleteService, StaticBackend, Suggestion};

    fn app_with_autocomplete(service: AutocompleteService) -> axum::Router {
        let state = AppState::new()
            .expect("build app state")
            .with_autocomplete(service);
        app(state)
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn returns_backend_suggestions_for_partial_query() {
        let backend = Arc::new(StaticBackend::new(
            "stub",
            vec!["rust".to_string(), "rustlang".to_string()],
        ));
        let response = app_with_autocomplete(AutocompleteService::with_backend(backend))
            .oneshot(
                Request::builder()
                    .uri("/autocompleter?q=rus")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            SUGGESTIONS_CONTENT_TYPE
        );
        let value = body_json(response).await;
        assert_eq!(value, serde_json::json!(["rus", ["rust", "rustlang"]]));
    }

    #[tokio::test]
    async fn xhr_returns_rich_suggestion_objects() {
        let backend = Arc::new(StaticBackend::with_suggestions(
            "stub",
            vec![Suggestion {
                text: "Albert Einstein".into(),
                subtext: Some("physicist".into()),
                image: Some("https://cdn.example.com/e.jpg".into()),
            }],
        ));
        let response = app_with_autocomplete(AutocompleteService::with_backend(backend))
            .oneshot(
                Request::builder()
                    .uri("/autocompleter?q=ein")
                    .header("X-Requested-With", "XMLHttpRequest")
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
        let value = body_json(response).await;
        let obj = value
            .as_array()
            .and_then(|a| a.first())
            .expect("one suggestion");
        assert_eq!(obj["text"], "Albert Einstein");
        assert_eq!(obj["subtext"], "physicist");
        let image = obj["image"].as_str().expect("image");
        assert!(
            image.starts_with("/image_proxy?url=") && image.contains("cdn.example.com"),
            "suggestion thumbnails should go through the image proxy, got {image}"
        );
    }

    #[tokio::test]
    async fn no_backend_returns_empty_suggestions() {
        let response = app_with_autocomplete(AutocompleteService::disabled())
            .oneshot(
                Request::builder()
                    .uri("/autocompleter?q=rus")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let value = body_json(response).await;
        assert_eq!(value, serde_json::json!(["rus", []]));
    }

    #[tokio::test]
    async fn post_reads_query_from_body() {
        let backend = Arc::new(StaticBackend::new("stub", vec!["rust".to_string()]));
        let response = app_with_autocomplete(AutocompleteService::with_backend(backend))
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/autocompleter")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from("q=rus"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let value = body_json(response).await;
        assert_eq!(value, serde_json::json!(["rus", ["rust"]]));
    }
}
