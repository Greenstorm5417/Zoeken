//! Frontend routes: SPA redirects and static assets.

use std::borrow::Cow;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;
use zoeken_server::static_assets::{AssetSource, INDEX_HTML};
use zoeken_server::{AppState, app};

struct IndexAssets;

impl AssetSource for IndexAssets {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        (path == INDEX_HTML).then(|| Cow::Borrowed(&b"<!doctype html><title>Zoeken</title>"[..]))
    }

    fn has_index(&self) -> bool {
        true
    }
}

async fn get(uri: &str) -> axum::response::Response {
    app(AppState::new().expect("state"))
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
        .await
        .unwrap()
}

async fn post(uri: &str) -> axum::response::Response {
    app(AppState::new().expect("state"))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri(uri)
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap()
}

#[tokio::test]
async fn root_with_q_redirects_to_search() {
    let response = get("/?q=rust&format=json").await;
    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/search?q=rust&format=json"
    );
}

#[tokio::test]
async fn root_post_with_q_redirects_to_search() {
    let response = app(AppState::new().expect("state"))
        .oneshot(
            Request::builder()
                .method("POST")
                .uri("/?q=rust")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::PERMANENT_REDIRECT);
    assert_eq!(
        response.headers().get(header::LOCATION).unwrap(),
        "/search?q=rust"
    );
}

#[tokio::test]
async fn info_serves_localized_content() {
    let response = get("/info/en/about").await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_LANGUAGE).unwrap(),
        "en"
    );
}

#[tokio::test]
async fn about_is_an_explicit_spa_route() {
    let assets: Arc<dyn AssetSource> = Arc::new(IndexAssets);
    let response = app(AppState::new().expect("state").with_assets(assets))
        .oneshot(
            Request::builder()
                .uri("/about")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();

    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(
        response.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    assert_eq!(&body[..], b"<!doctype html><title>Zoeken</title>");
}

#[tokio::test]
async fn logo_and_rss_xsl_and_client_css() {
    let logo = get("/logo/32").await;
    assert_eq!(logo.status(), StatusCode::OK);
    assert_eq!(
        logo.headers().get(header::CONTENT_TYPE).unwrap(),
        "image/svg+xml"
    );

    let xsl = get("/rss.xsl").await;
    assert_eq!(xsl.status(), StatusCode::OK);
    assert_eq!(
        xsl.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/xml"
    );
    let body = to_bytes(xsl.into_body(), usize::MAX).await.unwrap();
    assert!(String::from_utf8_lossy(&body).contains("xsl:stylesheet"));

    let css = get("/clientabc.css").await;
    assert_eq!(css.status(), StatusCode::OK);
    assert_eq!(css.headers().get(header::CONTENT_TYPE).unwrap(), "text/css");
    assert_eq!(
        css.headers().get(header::CACHE_CONTROL).unwrap(),
        "no-store, max-age=0"
    );

    let xsl_post = post("/rss.xsl").await;
    assert_eq!(xsl_post.status(), StatusCode::OK);
    assert_eq!(
        xsl_post.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/xml"
    );

    let css_post = post("/clientabc.css").await;
    assert_eq!(css_post.status(), StatusCode::OK);
    assert_eq!(
        css_post.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/css"
    );
}
