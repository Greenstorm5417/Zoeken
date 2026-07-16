//! `GET /favicon_proxy?authority=&h=` — HMAC-gated favicon resolution.

use std::sync::Arc;

use axum::extract::{RawQuery, State};
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};
use zoeken_favicons::{FaviconOutcome, is_hmac_of, validate_proxy_authority};

use crate::info::FAVICON_SVG;
use crate::{AppState, parse_pairs};

/// Serve a resolved favicon for `authority` when HMAC `h` is valid.
pub async fn favicon_proxy_get(
    State(state): State<Arc<AppState>>,
    RawQuery(query): RawQuery,
) -> Response {
    let params = parse_pairs(query.as_deref().unwrap_or(""));
    let authority = params
        .iter()
        .find(|(k, _)| k == "authority")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    let h = params
        .iter()
        .find(|(k, _)| k == "h")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");

    if authority.is_empty() || authority.contains('/') {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if validate_proxy_authority(authority).is_err() {
        return StatusCode::BAD_REQUEST.into_response();
    }
    if !is_hmac_of(&state.settings.server.secret_key, authority.as_bytes(), h) {
        return StatusCode::BAD_REQUEST.into_response();
    }

    match state.favicons.get_favicon(authority).await {
        FaviconOutcome::Serve(favicon) => (
            [
                (header::CONTENT_TYPE, favicon.mime.as_str()),
                (header::CACHE_CONTROL, "max-age=604800"),
            ],
            favicon.data,
        )
            .into_response(),
        FaviconOutcome::Fallback => {
            ([(header::CONTENT_TYPE, "image/svg+xml")], FAVICON_SVG).into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;
    use zoeken_favicons::{
        Favicon, FaviconCache, FaviconService, InMemoryFaviconCache, StaticResolver, new_hmac,
    };
    use zoeken_settings::Settings;

    fn signed_uri(authority: &str, secret: &str) -> String {
        let h = new_hmac(secret, authority.as_bytes());
        format!("/favicon_proxy?authority={authority}&h={h}")
    }

    #[tokio::test]
    async fn missing_hmac_is_rejected() {
        let mut settings = Settings::default();
        settings.server.secret_key = "secret".into();
        let app = app(AppState::new().unwrap().with_settings(settings));
        let response = app
            .oneshot(
                Request::builder()
                    .uri("/favicon_proxy?authority=example.com")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn valid_hmac_serves_cached_favicon() {
        let secret = "secret";
        let mut settings = Settings::default();
        settings.server.secret_key = secret.into();
        let cache = InMemoryFaviconCache::new();
        let fav = Favicon::new(vec![1, 2, 3], "image/png");
        cache.set("stub", "example.com", Some(&fav));
        let favicons = Arc::new(FaviconService::new(
            Arc::new(StaticResolver::failing("stub", "unused")),
            cache,
        ));
        let app = app(AppState::new()
            .unwrap()
            .with_settings(settings)
            .with_favicons(favicons));
        let response = app
            .oneshot(
                Request::builder()
                    .uri(signed_uri("example.com", secret))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn private_authority_is_rejected_even_with_valid_hmac() {
        let secret = "secret";
        let mut settings = Settings::default();
        settings.server.secret_key = secret.into();
        let app = app(AppState::new().unwrap().with_settings(settings));
        let response = app
            .oneshot(
                Request::builder()
                    .uri(signed_uri("127.0.0.1", secret))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn fallback_returns_default_svg() {
        let secret = "secret";
        let mut settings = Settings::default();
        settings.server.secret_key = secret.into();
        let favicons = Arc::new(FaviconService::new(
            Arc::new(StaticResolver::empty("stub")),
            InMemoryFaviconCache::new(),
        ));
        let app = app(AppState::new()
            .unwrap()
            .with_settings(settings)
            .with_favicons(favicons));
        let response = app
            .oneshot(
                Request::builder()
                    .uri(signed_uri("missing.example", secret))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "image/svg+xml"
        );
    }
}
