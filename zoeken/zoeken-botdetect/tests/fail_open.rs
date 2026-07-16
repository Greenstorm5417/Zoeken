//! Integration tests for fail-open behavior on internal errors.

use std::convert::Infallible;
use std::net::SocketAddr;
use std::str::FromStr;
use std::sync::Arc;

use axum::body::{Body, to_bytes};
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use ipnet::IpNet;
use tower::util::BoxCloneService;
use tower::{Layer, ServiceExt, service_fn};

use zoeken_botdetect::{
    BotDetectLayer, BotDetectService, Detector, LimiterConfig, RateLimitConfig,
};

async fn inner_ok(_req: Request<Body>) -> Result<Response, Infallible> {
    Ok((StatusCode::OK, "handler reached").into_response())
}

fn service_with(
    detector: Detector,
) -> BotDetectService<BoxCloneService<Request<Body>, Response, Infallible>> {
    let inner = BoxCloneService::new(service_fn(inner_ok));
    BotDetectLayer::new(Arc::new(detector)).layer(inner)
}

fn enabled_config() -> LimiterConfig {
    let cfg = LimiterConfig {
        pass_reserved_nets: false,
        ..LimiterConfig::default()
    };
    assert!(cfg.enabled, "limiter must be enabled to exercise 13.8");
    cfg
}

fn with_peer(mut req: Request<Body>, peer: &str) -> Request<Body> {
    let addr: SocketAddr = format!("{peer}:12345").parse().unwrap();
    req.extensions_mut().insert(ConnectInfo(addr));
    req
}

#[tokio::test]
async fn fail_open_allows_request_when_client_ip_undeterminable() {
    let service = service_with(Detector::new(enabled_config(), "tok"));

    let req = Request::builder()
        .uri("/search?q=rust")
        .body(Body::empty())
        .unwrap();

    let resp = service.oneshot(req).await.unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "fail-open: request must be allowed when the client IP cannot be determined"
    );
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(
        &body[..],
        b"handler reached",
        "the inner handler must be reached on the fail-open path"
    );
}

#[tokio::test]
async fn fail_open_allows_even_a_bot_like_request_without_client_ip() {
    let service = service_with(Detector::new(enabled_config(), "tok"));

    let req = Request::builder()
        .uri("/search?q=rust")
        .header(header::USER_AGENT, "curl/8.0")
        .body(Body::empty())
        .unwrap();

    let resp = service.oneshot(req).await.unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::OK,
        "fail-open: internal errors must allow requests regardless of headers"
    );
}

#[tokio::test]
async fn blockable_request_with_known_ip_is_still_blocked() {
    let mut cfg = enabled_config();
    cfg.trusted_proxies = vec![IpNet::from_str("10.0.0.0/8").unwrap()];
    cfg.block_ip = vec![IpNet::from_str("198.51.100.0/24").unwrap()];
    let service = service_with(Detector::new(cfg, "tok"));

    let req = with_peer(
        Request::builder()
            .uri("/search?q=rust")
            .header("x-real-ip", "198.51.100.9")
            .header(header::ACCEPT, "text/html")
            .header(header::ACCEPT_ENCODING, "gzip, deflate")
            .header(header::ACCEPT_LANGUAGE, "en-US")
            .header(header::CONNECTION, "keep-alive")
            .header(header::USER_AGENT, "Mozilla/5.0 Firefox/120.0")
            .body(Body::empty())
            .unwrap(),
        "10.0.0.1",
    );

    let resp = service.oneshot(req).await.unwrap();

    assert_eq!(
        resp.status(),
        StatusCode::FORBIDDEN,
        "a determinable, block-listed IP must be rejected"
    );
}

#[tokio::test]
async fn rate_limited_request_with_known_ip_is_still_rejected() {
    let mut cfg = enabled_config();
    cfg.trusted_proxies = vec![IpNet::from_str("10.0.0.0/8").unwrap()];
    cfg.rate_limit = RateLimitConfig {
        capacity: 1.0,
        refill_per_second: 0.0,
        suspicious_capacity: 1.0,
        suspicious_refill_per_second: 0.0,
    };
    let service = service_with(Detector::new(cfg, "tok"));

    let build = || {
        with_peer(
            Request::builder()
                .uri("/search?q=rust")
                .header("x-real-ip", "203.0.113.77")
                .header(header::ACCEPT, "text/html")
                .header(header::ACCEPT_ENCODING, "gzip, deflate")
                .header(header::ACCEPT_LANGUAGE, "en-US")
                .header(header::CONNECTION, "keep-alive")
                .header(header::USER_AGENT, "Mozilla/5.0 Firefox/120.0")
                .body(Body::empty())
                .unwrap(),
            "10.0.0.1",
        )
    };

    let first = service.clone().oneshot(build()).await.unwrap();
    assert_eq!(first.status(), StatusCode::OK);

    let second = service.oneshot(build()).await.unwrap();
    assert_eq!(
        second.status(),
        StatusCode::TOO_MANY_REQUESTS,
        "the enabled limiter must enforce the rate limit"
    );
}
