//! Tower middleware for bot detection and rate limiting.

pub mod client_ip;
pub mod config;
pub mod heuristics;
pub mod ip_lists;
pub mod link_token;
pub mod token_bucket;

use std::future::Future;
use std::net::{IpAddr, SocketAddr};
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll};
use std::time::Duration;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use tower::{Layer, Service};

pub use config::{ConfigError, HeaderHeuristics, LimiterConfig, RateLimitConfig};
pub use heuristics::{HeaderView, HeuristicFailure};
pub use link_token::LinkTokenVerifier;
pub use token_bucket::RateLimiter;

/// Features extracted from an inbound request.
#[derive(Debug, Clone)]
pub struct RequestFeatures {
    pub path: String,
    pub client_ip: IpAddr,
    pub headers: HeaderView,
    pub link_token: Option<String>,
}

/// The outcome of evaluating a request.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Allow,
    Block(String),
    TooManyRequests(String),
}

impl Decision {
    pub fn is_allowed(&self) -> bool {
        matches!(self, Decision::Allow)
    }
}

/// Bot detector state and evaluation logic.
#[derive(Debug)]
pub struct Detector {
    config: LimiterConfig,
    rate_limiter: RateLimiter,
    link_tokens: LinkTokenVerifier,
}

impl Detector {
    pub fn new(config: LimiterConfig, token: impl Into<String>) -> Self {
        let idle_timeout = Duration::from_secs(config.state_idle_seconds);
        Self {
            rate_limiter: RateLimiter::with_limits(config.state_capacity, idle_timeout),
            link_tokens: LinkTokenVerifier::with_limits(token, config.state_capacity, idle_timeout),
            config,
        }
    }

    pub fn config(&self) -> &LimiterConfig {
        &self.config
    }

    pub fn link_tokens(&self) -> &LinkTokenVerifier {
        &self.link_tokens
    }

    pub fn rate_limiter(&self) -> &RateLimiter {
        &self.rate_limiter
    }

    pub fn evaluate(&self, features: &RequestFeatures) -> Decision {
        self.evaluate_at(features, self.rate_limiter.now_secs())
    }

    /// Evaluate a request at an explicit time.
    pub fn evaluate_at(&self, features: &RequestFeatures, now: f64) -> Decision {
        if !self.config.enabled {
            return Decision::Allow;
        }

        if features.path == "/healthz" || features.path == "/readyz" {
            return Decision::Allow;
        }
        // Link-token challenge CSS must reach the handler so browsers can verify.
        if features.path.starts_with("/client") && features.path.ends_with(".css") {
            return Decision::Allow;
        }

        let ip = features.client_ip;
        let network =
            ip_lists::client_network(ip, self.config.ipv4_prefix, self.config.ipv6_prefix);
        let net_key = network.to_string();

        if ip_lists::pass_ip(ip, &self.config) {
            tracing::debug!(%network, "PASS: client IP on pass-list");
            return Decision::Allow;
        }

        if ip_lists::block_ip(ip, &self.config) {
            tracing::warn!(%network, "BLOCK: client IP on block-list");
            return Decision::Block(format!("IP {ip} is on the block list"));
        }

        let suspicious = if self.config.link_token {
            self.link_tokens
                .is_suspicious(&net_key, features.link_token.as_deref())
        } else {
            false
        };

        let link_local = ip_lists::is_link_local(ip);
        if !link_local || self.config.filter_link_local {
            let (capacity, refill) = self.config.rate_limit.params(suspicious);
            if !self.rate_limiter.check_at(&net_key, capacity, refill, now) {
                tracing::debug!(%network, suspicious, "BLOCK: rate limit exceeded");
                return Decision::TooManyRequests(format!("too many requests from {network}"));
            }
        }

        if let Err(failure) = heuristics::evaluate(&features.headers, &self.config.heuristics) {
            tracing::debug!(%network, reason = failure.reason(), "BLOCK: header heuristic");
            return Decision::TooManyRequests(failure.reason().to_string());
        }

        Decision::Allow
    }

    pub fn into_layer(self) -> BotDetectLayer {
        BotDetectLayer {
            detector: Arc::new(self),
        }
    }
}

pub fn layer(detector: Arc<Detector>) -> BotDetectLayer {
    BotDetectLayer { detector }
}

/// `tower` layer that installs `BotDetectService`.
#[derive(Clone)]
pub struct BotDetectLayer {
    detector: Arc<Detector>,
}

impl BotDetectLayer {
    pub fn new(detector: Arc<Detector>) -> Self {
        Self { detector }
    }
}

impl<S> Layer<S> for BotDetectLayer {
    type Service = BotDetectService<S>;

    fn layer(&self, inner: S) -> Self::Service {
        BotDetectService {
            inner,
            detector: self.detector.clone(),
        }
    }
}

/// `tower` service that evaluates each request before forwarding it.
#[derive(Clone)]
pub struct BotDetectService<S> {
    inner: S,
    detector: Arc<Detector>,
}

impl<S> Service<Request<Body>> for BotDetectService<S>
where
    S: Service<Request<Body>, Response = Response> + Clone + Send + 'static,
    S::Future: Send + 'static,
{
    type Response = Response;
    type Error = S::Error;
    type Future = Pin<Box<dyn Future<Output = Result<Response, S::Error>> + Send>>;

    fn poll_ready(&mut self, cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        self.inner.poll_ready(cx)
    }

    fn call(&mut self, req: Request<Body>) -> Self::Future {
        let detector = self.detector.clone();
        let clone = self.inner.clone();
        let mut inner = std::mem::replace(&mut self.inner, clone);

        Box::pin(async move {
            match extract_features(&req, &detector.config) {
                None => inner.call(req).await,
                Some(features) => match detector.evaluate(&features) {
                    Decision::Allow => inner.call(req).await,
                    Decision::Block(msg) => Ok((StatusCode::FORBIDDEN, msg).into_response()),
                    Decision::TooManyRequests(msg) => {
                        Ok((StatusCode::TOO_MANY_REQUESTS, msg).into_response())
                    }
                },
            }
        })
    }
}

fn extract_features(req: &Request<Body>, config: &LimiterConfig) -> Option<RequestFeatures> {
    let headers = req.headers();

    let header_str = |name: header::HeaderName| -> Option<String> {
        headers
            .get(&name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    };
    let header_named = |name: &str| -> Option<String> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(|s| s.to_string())
    };

    let peer = req
        .extensions()
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ci| ci.0.ip());
    let x_forwarded_for = header_named("x-forwarded-for")
        .map(|value| client_ip::parse_forwarded_for(&value))
        .unwrap_or_default();
    let x_real_ip = header_named("x-real-ip").and_then(|value| value.trim().parse::<IpAddr>().ok());

    let client_ip =
        client_ip::derive_client_ip(peer, &x_forwarded_for, x_real_ip, &config.trusted_proxies)?;

    let is_secure = match header_named("x-forwarded-proto") {
        Some(proto) => proto.eq_ignore_ascii_case("https"),
        None => req.uri().scheme_str() == Some("https"),
    };

    let view = HeaderView {
        accept: header_str(header::ACCEPT),
        accept_encoding: header_str(header::ACCEPT_ENCODING),
        accept_language: header_str(header::ACCEPT_LANGUAGE),
        connection: header_str(header::CONNECTION),
        user_agent: header_str(header::USER_AGENT),
        sec_fetch_mode: header_named("sec-fetch-mode"),
        is_secure,
    };

    let link_token = header_named("x-link-token").or_else(|| {
        req.uri()
            .query()
            .and_then(|q| url_form_value(q, "link_token"))
    });

    Some(RequestFeatures {
        path: req.uri().path().to_string(),
        client_ip,
        headers: view,
        link_token,
    })
}

fn url_form_value(query: &str, key: &str) -> Option<String> {
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        if it.next() == Some(key) {
            return Some(it.next().unwrap_or("").to_string());
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::SocketAddr;
    use std::str::FromStr;

    use axum::body::to_bytes;
    use axum::extract::ConnectInfo;
    use ipnet::IpNet;
    use tower::ServiceExt;

    fn browser_features(ip: &str, path: &str) -> RequestFeatures {
        RequestFeatures {
            path: path.to_string(),
            client_ip: ip.parse().unwrap(),
            headers: HeaderView {
                accept: Some("text/html".to_string()),
                accept_encoding: Some("gzip, deflate".to_string()),
                accept_language: Some("en-US".to_string()),
                connection: Some("keep-alive".to_string()),
                user_agent: Some("Mozilla/5.0 (X11; Linux x86_64) Firefox/120.0".to_string()),
                sec_fetch_mode: Some("navigate".to_string()),
                is_secure: false,
            },
            link_token: None,
        }
    }

    fn with_peer(mut req: Request<Body>, peer: &str) -> Request<Body> {
        let addr: SocketAddr = format!("{peer}:12345").parse().unwrap();
        req.extensions_mut().insert(ConnectInfo(addr));
        req
    }

    fn base_config() -> LimiterConfig {
        LimiterConfig {
            pass_reserved_nets: false,
            ..LimiterConfig::default()
        }
    }

    #[test]
    fn disabled_limiter_allows_everything() {
        let cfg = base_config().with_enabled(false);
        let detector = Detector::new(cfg, "tok");
        let features = RequestFeatures {
            path: "/search".to_string(),
            client_ip: "203.0.113.1".parse().unwrap(),
            headers: HeaderView::default(),
            link_token: None,
        };
        assert_eq!(detector.evaluate(&features), Decision::Allow);
    }

    #[test]
    fn healthz_is_always_allowed() {
        let detector = Detector::new(base_config(), "tok");
        let mut features = browser_features("203.0.113.1", "/healthz");
        features.headers = HeaderView::default();
        assert_eq!(detector.evaluate(&features), Decision::Allow);
    }

    #[test]
    fn readyz_is_always_allowed() {
        let detector = Detector::new(base_config(), "tok");
        let mut features = browser_features("203.0.113.1", "/readyz");
        features.headers = HeaderView::default();
        assert_eq!(detector.evaluate(&features), Decision::Allow);
    }

    #[test]
    fn pass_list_bypasses_block_and_heuristics() {
        let mut cfg = base_config();
        cfg.pass_ip = vec![IpNet::from_str("203.0.113.0/24").unwrap()];
        cfg.block_ip = vec![IpNet::from_str("203.0.113.0/24").unwrap()];
        let detector = Detector::new(cfg, "tok");
        let features = RequestFeatures {
            path: "/search".to_string(),
            client_ip: "203.0.113.5".parse().unwrap(),
            headers: HeaderView {
                user_agent: Some("curl/8.0".to_string()),
                ..HeaderView::default()
            },
            link_token: None,
        };
        assert_eq!(detector.evaluate(&features), Decision::Allow);
    }

    #[test]
    fn block_list_rejects_before_rate_limit_and_heuristics() {
        let mut cfg = base_config();
        cfg.block_ip = vec![IpNet::from_str("198.51.100.0/24").unwrap()];
        let detector = Detector::new(cfg, "tok");
        let features = browser_features("198.51.100.9", "/search");
        assert!(matches!(detector.evaluate(&features), Decision::Block(_)));
    }

    #[test]
    fn rate_limit_rejects_after_capacity_exhausted() {
        let mut cfg = base_config();
        cfg.rate_limit = RateLimitConfig {
            capacity: 2.0,
            refill_per_second: 0.0,
            suspicious_capacity: 2.0,
            suspicious_refill_per_second: 0.0,
        };
        let detector = Detector::new(cfg, "tok");
        let features = browser_features("203.0.113.7", "/search");
        assert_eq!(detector.evaluate_at(&features, 0.0), Decision::Allow);
        assert_eq!(detector.evaluate_at(&features, 0.0), Decision::Allow);
        assert!(matches!(
            detector.evaluate_at(&features, 0.0),
            Decision::TooManyRequests(_)
        ));
    }

    #[test]
    fn header_heuristics_reject_a_bot() {
        let detector = Detector::new(base_config(), "tok");
        let mut features = browser_features("203.0.113.8", "/search");
        features.headers.user_agent = Some("curl/8.0".to_string());
        assert!(matches!(
            detector.evaluate(&features),
            Decision::TooManyRequests(_)
        ));
    }

    #[test]
    fn link_local_is_exempt_from_rate_limit_by_default() {
        let mut cfg = base_config();
        cfg.rate_limit = RateLimitConfig {
            capacity: 1.0,
            refill_per_second: 0.0,
            suspicious_capacity: 1.0,
            suspicious_refill_per_second: 0.0,
        };
        let detector = Detector::new(cfg, "tok");
        let features = browser_features("169.254.1.1", "/search");
        for _ in 0..5 {
            assert_eq!(detector.evaluate_at(&features, 0.0), Decision::Allow);
        }
    }

    #[test]
    fn suspicious_clients_get_stricter_limits_when_link_token_enabled() {
        let mut cfg = base_config();
        cfg.link_token = true;
        cfg.rate_limit = RateLimitConfig {
            capacity: 10.0,
            refill_per_second: 0.0,
            suspicious_capacity: 1.0,
            suspicious_refill_per_second: 0.0,
        };
        let detector = Detector::new(cfg, "secret");
        let features = browser_features("203.0.113.20", "/search");
        assert_eq!(detector.evaluate_at(&features, 0.0), Decision::Allow);
        assert!(matches!(
            detector.evaluate_at(&features, 0.0),
            Decision::TooManyRequests(_)
        ));
    }

    async fn allow_ok(_req: Request<Body>) -> Result<Response, std::convert::Infallible> {
        Ok((StatusCode::OK, "handler reached").into_response())
    }

    fn service_with(
        detector: Detector,
    ) -> BotDetectService<
        tower::util::BoxCloneService<Request<Body>, Response, std::convert::Infallible>,
    > {
        let inner = tower::util::BoxCloneService::new(tower::service_fn(allow_ok));
        BotDetectLayer::new(Arc::new(detector)).layer(inner)
    }

    #[tokio::test]
    async fn fail_open_when_client_ip_cannot_be_determined() {
        let detector = Detector::new(base_config(), "tok");
        let service = service_with(detector);
        let req = Request::builder()
            .uri("/search?q=rust")
            .body(Body::empty())
            .unwrap();
        let resp = service.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"handler reached");
    }

    #[tokio::test]
    async fn block_listed_ip_via_x_real_ip_is_rejected() {
        let mut cfg = base_config();
        cfg.trusted_proxies = vec![IpNet::from_str("10.0.0.0/8").unwrap()];
        cfg.block_ip = vec![IpNet::from_str("198.51.100.0/24").unwrap()];
        let detector = Detector::new(cfg, "tok");
        let service = service_with(detector);
        let req = with_peer(
            Request::builder()
                .uri("/search?q=rust")
                .header("x-real-ip", "198.51.100.9")
                .header(header::ACCEPT, "text/html")
                .header(header::ACCEPT_ENCODING, "gzip")
                .header(header::ACCEPT_LANGUAGE, "en")
                .header(header::USER_AGENT, "Mozilla/5.0 Firefox/120.0")
                .body(Body::empty())
                .unwrap(),
            "10.0.0.1",
        );
        let resp = service.oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn spoofed_x_real_ip_ignored_from_untrusted_peer() {
        let mut cfg = base_config();
        cfg.trusted_proxies = vec![IpNet::from_str("10.0.0.0/8").unwrap()];
        cfg.block_ip = vec![IpNet::from_str("198.51.100.0/24").unwrap()];
        let detector = Detector::new(cfg, "tok");
        let service = service_with(detector);
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
            "192.0.2.5",
        );
        let resp = service.oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::OK,
            "untrusted peer must not spoof into the block list via X-Real-IP"
        );
    }

    #[tokio::test]
    async fn clean_browser_request_reaches_handler() {
        let mut cfg = base_config();
        cfg.trusted_proxies = vec![IpNet::from_str("10.0.0.0/8").unwrap()];
        let detector = Detector::new(cfg, "tok");
        let service = service_with(detector);
        let req = with_peer(
            Request::builder()
                .uri("/search?q=rust")
                .header("x-real-ip", "203.0.113.50")
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
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
