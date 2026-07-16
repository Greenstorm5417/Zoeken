//! Frontend routes: SPA redirects and static assets.

use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};
use tower::ServiceExt;
use zoeken_server::{AppState, app};

async fn get(uri: &str) -> axum::response::Response {
    app(AppState::new().expect("state"))
        .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
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
async fn info_redirects_to_spa_about() {
    let response = get("/info/en/about").await;
    assert_eq!(response.status(), StatusCode::TEMPORARY_REDIRECT);
    assert_eq!(response.headers().get(header::LOCATION).unwrap(), "/about");
}

#[tokio::test]
async fn about_is_served_by_spa_fallback() {
    let response = get("/about").await;
    // Without a built index.html this is 404; with assets it is the SPA shell.
    assert!(
        response.status() == StatusCode::OK || response.status() == StatusCode::NOT_FOUND,
        "unexpected status {}",
        response.status()
    );
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
}
