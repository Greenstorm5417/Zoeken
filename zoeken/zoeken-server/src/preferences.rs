//! `/preferences` and `/clear_cookies` routes.

use std::sync::Arc;

use axum::extract::{RawQuery, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Response};

use zoeken_prefs::{Preferences, encode_cookie, resolve_with_data};
use zoeken_query::FormParams;

use crate::{AppState, parse_pairs};

const PREFERENCES_COOKIE: &str = "preferences";

const COOKIE_MAX_AGE_SECS: i64 = 60 * 60 * 24 * 365;

/// `GET /preferences` as JSON.
pub async fn preferences_get(State(state): State<Arc<AppState>>, headers: HeaderMap) -> Response {
    let cookie = read_pref_cookie(&headers);
    let prefs = resolve_with_data(
        &state.pref_defaults,
        &state.settings,
        cookie.as_deref(),
        &FormParams::default(),
        &state.data,
    );
    json_response(&prefs)
}

/// `POST /preferences` and save the resulting cookie.
pub async fn preferences_post(
    State(state): State<Arc<AppState>>,
    headers: HeaderMap,
    RawQuery(query): RawQuery,
    body: String,
) -> Response {
    let mut pairs = parse_pairs(query.as_deref().unwrap_or(""));
    pairs.extend(parse_pairs(&body));
    let params = FormParams::from_pairs(pairs);

    let cookie = read_pref_cookie(&headers);
    let prefs = resolve_with_data(
        &state.pref_defaults,
        &state.settings,
        cookie.as_deref(),
        &params,
        &state.data,
    );

    let encoded = encode_cookie(&prefs);
    let set_cookie = format!(
        "{PREFERENCES_COOKIE}={encoded}; Path=/; Max-Age={COOKIE_MAX_AGE_SECS}; SameSite=Lax; HttpOnly"
    );

    let body = serde_json::to_string(&prefs).unwrap_or_else(|_| "{}".to_string());
    (
        StatusCode::OK,
        [
            (header::SET_COOKIE, set_cookie),
            (header::CONTENT_TYPE, "application/json".to_string()),
        ],
        body,
    )
        .into_response()
}

/// `GET`/`POST /clear_cookies`.
pub async fn clear_cookies() -> Response {
    let expire = format!("{PREFERENCES_COOKIE}=; Path=/; Max-Age=0; SameSite=Lax; HttpOnly");
    (
        StatusCode::OK,
        [(header::SET_COOKIE, expire)],
        "preferences cleared",
    )
        .into_response()
}

fn json_response(prefs: &Preferences) -> Response {
    let body = serde_json::to_string(prefs).unwrap_or_else(|_| "{}".to_string());
    ([(header::CONTENT_TYPE, "application/json")], body).into_response()
}

pub(crate) fn read_pref_cookie(headers: &HeaderMap) -> Option<String> {
    headers
        .get_all(header::COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|header| header.split(';'))
        .filter_map(|pair| {
            let (name, value) = pair.split_once('=')?;
            (name.trim() == PREFERENCES_COOKIE).then(|| value.trim().to_string())
        })
        .next()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app;

    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use tower::ServiceExt;
    use zoeken_prefs::{RequestMethod, decode_cookie};
    use zoeken_query::SafeSearch;

    fn set_cookie_value(response: &Response) -> Option<String> {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .flat_map(|s| s.split(';'))
            .filter_map(|pair| {
                let (name, value) = pair.split_once('=')?;
                (name.trim() == PREFERENCES_COOKIE).then(|| value.trim().to_string())
            })
            .next()
    }

    fn raw_set_cookie(response: &Response) -> Option<String> {
        response
            .headers()
            .get_all(header::SET_COOKIE)
            .iter()
            .filter_map(|v| v.to_str().ok())
            .find(|s| s.trim_start().starts_with(PREFERENCES_COOKIE))
            .map(|s| s.to_string())
    }

    async fn body_json(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[tokio::test]
    async fn post_then_get_round_trips_preferences_through_cookie() {
        let app = app(AppState::new().expect("build app state"));

        // Save a set of preferences.
        let post = app
            .clone()
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/preferences")
                    .header(header::CONTENT_TYPE, "application/x-www-form-urlencoded")
                    .body(Body::from(
                        "locale=es&safesearch=2&engines=duckduckgo,brave&method=GET&image_proxy=1",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(post.status(), StatusCode::OK);
        let raw = raw_set_cookie(&post).expect("Set-Cookie carries the preferences cookie");
        assert!(
            raw.contains("HttpOnly"),
            "preferences cookie must be HttpOnly: {raw}"
        );
        let cookie = set_cookie_value(&post).expect("Set-Cookie carries the preferences cookie");

        // The cookie decodes to the saved preferences.
        let decoded = decode_cookie(&cookie).expect("cookie decodes");
        assert_eq!(decoded.locale, "es");
        assert_eq!(decoded.safesearch, SafeSearch::Strict);
        assert_eq!(decoded.engines, vec!["duckduckgo", "brave"]);
        assert_eq!(decoded.method, RequestMethod::Get);
        assert!(decoded.image_proxy);

        // A GET carrying the cookie returns the same effective preferences.
        let get = app
            .oneshot(
                Request::builder()
                    .uri("/preferences")
                    .header(header::COOKIE, format!("{PREFERENCES_COOKIE}={cookie}"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(get.status(), StatusCode::OK);
        assert_eq!(
            get.headers().get(header::CONTENT_TYPE).unwrap(),
            "application/json"
        );
        let value = body_json(get).await;
        assert_eq!(value["locale"], "es");
        assert_eq!(value["safesearch"], "Strict");
        assert_eq!(value["method"], "GET");
        assert_eq!(value["image_proxy"], true);
    }

    /// A `GET` with a malformed preferences cookie falls back to the
    /// defaults+settings preferences instead of erroring.
    #[tokio::test]
    async fn get_with_bad_cookie_falls_back_to_defaults() {
        let app = app(AppState::new().expect("build app state"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/preferences")
                    .header(
                        header::COOKIE,
                        format!("{PREFERENCES_COOKIE}=@@not-a-valid-cookie@@"),
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let value = body_json(response).await;

        // Equal to the defaults+settings resolution (no cookie).
        let expected = resolve_with_data(
            &Preferences::defaults(),
            &zoeken_settings::Settings::default(),
            None,
            &FormParams::default(),
            &zoeken_data::DataBundle::default(),
        );
        assert_eq!(value["locale"], expected.locale);
        assert_eq!(value["theme"], expected.theme);
    }

    /// A `GET` with no cookie returns the defaults+settings preferences.
    #[tokio::test]
    async fn get_without_cookie_returns_defaults() {
        let app = app(AppState::new().expect("build app state"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/preferences")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let value = body_json(response).await;
        let defaults = Preferences::defaults();
        assert_eq!(value["theme"], defaults.theme);
    }

    /// `/clear_cookies` emits a `Set-Cookie` that expires the preferences cookie
    /// (`Max-Age=0`).
    #[tokio::test]
    async fn clear_cookies_emits_expiring_set_cookie() {
        let app = app(AppState::new().expect("build app state"));

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/clear_cookies")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let raw = raw_set_cookie(&response).expect("Set-Cookie clears the preferences cookie");
        assert!(
            raw.contains("Max-Age=0"),
            "clear cookie should expire immediately: {raw}"
        );
        assert!(
            raw.contains("HttpOnly"),
            "clear cookie should remain HttpOnly: {raw}"
        );
    }
}
