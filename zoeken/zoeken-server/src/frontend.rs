//! Frontend-adjacent HTTP handlers: SPA index, redirects, logo, link-token CSS.

use std::sync::Arc;

use axum::extract::{Path, RawQuery, Request, State};
use axum::http::{HeaderMap, StatusCode, header};
use axum::response::{IntoResponse, Redirect, Response};

use crate::static_assets::{AssetDecision, build_response, static_fallback};
use crate::{AppState, frontend_index_response, parse_pairs};

/// `GET /` — if `q` present, 308 → `/search?...`; else SPA index.
pub async fn index(State(state): State<Arc<AppState>>, RawQuery(query): RawQuery) -> Response {
    let raw = query.as_deref().unwrap_or("");
    let params = parse_pairs(raw);
    let has_q = params.iter().any(|(k, v)| k == "q" && !v.trim().is_empty());
    if has_q {
        let target = if raw.is_empty() {
            "/search".to_string()
        } else {
            format!("/search?{raw}")
        };
        return Redirect::permanent(&target).into_response();
    }
    frontend_index_response(&state)
}

/// `GET /about` - serve the SPA information view.
pub async fn about(State(state): State<Arc<AppState>>) -> Response {
    frontend_index_response(&state)
}

/// Compatible `GET`/`POST /rss.xsl` static stylesheet endpoint.
pub async fn rss_xsl(State(state): State<Arc<AppState>>) -> Response {
    build_response(
        &AssetDecision::ServeAsset {
            path: "rss.xsl".to_string(),
        },
        state.assets.as_ref(),
    )
}

/// `GET /logo/{resolution}` — brand SVG from the assets directory.
pub async fn logo(State(state): State<Arc<AppState>>, Path(_resolution): Path<String>) -> Response {
    match state.assets.get("zoeken-logo.svg") {
        Some(bytes) => (
            [(header::CONTENT_TYPE, "image/svg+xml")],
            bytes.into_owned(),
        )
            .into_response(),
        None => StatusCode::NOT_FOUND.into_response(),
    }
}

fn client_css_token(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("/client")?;
    let token = rest.strip_suffix(".css")?;
    if token.is_empty() || token.contains('/') {
        return None;
    }
    Some(token)
}

fn empty_client_css() -> Response {
    (
        [
            (header::CONTENT_TYPE, "text/css"),
            (header::CACHE_CONTROL, "no-store, max-age=0"),
        ],
        "",
    )
        .into_response()
}

fn ping_link_token(
    state: &AppState,
    headers: &HeaderMap,
    peer: Option<std::net::SocketAddr>,
    token: &str,
) {
    if !state.bot_detector.config().link_token {
        return;
    }
    let ip = crate::middleware::request_client_ip(
        peer,
        headers,
        &state.bot_detector.config().trusted_proxies,
    );
    if let Some(ip) = ip {
        let cfg = state.bot_detector.config();
        let network =
            zoeken_botdetect::ip_lists::client_network(ip, cfg.ipv4_prefix, cfg.ipv6_prefix);
        let _ = state
            .bot_detector
            .link_tokens()
            .ping(&network.to_string(), Some(token));
    } else {
        let _ = state
            .bot_detector
            .link_tokens()
            .ping("unknown", Some(token));
    }
}

/// Fallback: `/client{token}.css` link-token ping, else static assets / SPA index.
pub async fn client_css_or_static(
    State(state): State<Arc<AppState>>,
    crate::middleware::OptionalPeer(peer): crate::middleware::OptionalPeer,
    req: Request,
) -> Response {
    let path = req.uri().path();
    if let Some(token) = client_css_token(path) {
        ping_link_token(&state, req.headers(), peer, token);
        return empty_client_css();
    }
    static_fallback(State(state.assets.clone()), req).await
}
