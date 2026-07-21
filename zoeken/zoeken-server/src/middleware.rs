//! Middleware: security headers, request tracing, resource limits, and limiter assembly.

use std::collections::BTreeMap;
use std::fmt;
use std::net::{IpAddr, SocketAddr};
use std::time::Duration;

use axum::Router;
use axum::http::header::{
    self, CONTENT_SECURITY_POLICY, HeaderName, HeaderValue, STRICT_TRANSPORT_SECURITY,
    X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, Method, Request, Response, StatusCode};
use axum::response::IntoResponse;
use ipnet::IpNet;
use tower_http::classify::{ServerErrorsAsFailures, SharedClassifier};
use tower_http::compression::CompressionLayer;
use tower_http::limit::RequestBodyLimitLayer;
use tower_http::set_header::SetResponseHeaderLayer;
use tower_http::timeout::TimeoutLayer;
use tower_http::trace::{
    DefaultOnBodyChunk, DefaultOnEos, DefaultOnFailure, DefaultOnRequest, MakeSpan, OnResponse,
    TraceLayer,
};
use tracing::Span;
use tracing::field::Empty;
use tracing_subscriber::filter::LevelFilter;
use url::Url;
use zoeken_botdetect::BotDetectLayer;
use zoeken_botdetect::client_ip;
use zoeken_botdetect::config::parse_ip_or_net;
use zoeken_settings::DeploymentConfig;

const HSTS_VALUE: &str = "max-age=63072000; includeSubDomains";

/// Build the security headers applied to every response.
#[must_use]
pub fn security_headers(cfg: &DeploymentConfig) -> Vec<(HeaderName, HeaderValue)> {
    let mut headers: Vec<(HeaderName, HeaderValue)> = Vec::with_capacity(4);

    headers.push((X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff")));
    headers.push((X_FRAME_OPTIONS, HeaderValue::from_static("DENY")));

    // The CSP is operator-configurable and therefore not a compile-time static;
    // guard against a value that cannot be represented as a header value.
    if let Ok(csp) = HeaderValue::from_str(&cfg.effective_content_security_policy()) {
        headers.push((CONTENT_SECURITY_POLICY, csp));
    }

    if cfg.hsts {
        headers.push((
            STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static(HSTS_VALUE),
        ));
    }

    headers
}

struct RequestFields<'a> {
    method: &'a Method,
    path: &'a str,
}

impl<'a> RequestFields<'a> {
    fn from_request<B>(request: &'a Request<B>) -> Self {
        Self {
            method: request.method(),
            path: request.uri().path(),
        }
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RedactingMakeSpan;

impl<B> MakeSpan<B> for RedactingMakeSpan {
    fn make_span(&mut self, request: &Request<B>) -> Span {
        let fields = RequestFields::from_request(request);
        tracing::info_span!(
            "http.request",
            method = %fields.method,
            path = %fields.path,
            status = Empty,
            latency_ms = Empty,
        )
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct RedactingOnResponse;

impl<B> OnResponse<B> for RedactingOnResponse {
    fn on_response(self, response: &Response<B>, latency: Duration, span: &Span) {
        // `as_u64` on the millisecond count is saturating for any realistic
        // request duration; casting the `u128` is safe here.
        let latency_ms = u64::try_from(latency.as_millis()).unwrap_or(u64::MAX);
        span.record("status", response.status().as_u16());
        span.record("latency_ms", latency_ms);
    }
}

pub type RequestTraceLayer = TraceLayer<
    SharedClassifier<ServerErrorsAsFailures>,
    RedactingMakeSpan,
    DefaultOnRequest,
    RedactingOnResponse,
    DefaultOnBodyChunk,
    DefaultOnEos,
    DefaultOnFailure,
>;

/// Build the request-tracing layer.
#[must_use]
pub fn trace_layer() -> RequestTraceLayer {
    TraceLayer::new_for_http()
        .make_span_with(RedactingMakeSpan)
        .on_response(RedactingOnResponse)
}

/// Build the severity filter from the configured log level.
#[must_use]
pub fn level_filter(cfg: &DeploymentConfig) -> LevelFilter {
    cfg.log_level
        .parse::<LevelFilter>()
        .unwrap_or(LevelFilter::INFO)
}

pub const INTERNAL_ERROR_BODY: &str = r#"{"error":"internal server error"}"#;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ClientError {
    BadRequest(String),
    PayloadTooLarge,
    Timeout,
    TooManyRequests,
    NotFound,
    MethodNotAllowed,
}

impl ClientError {
    #[must_use]
    pub fn status(&self) -> StatusCode {
        match self {
            ClientError::BadRequest(_) => StatusCode::BAD_REQUEST,
            ClientError::PayloadTooLarge => StatusCode::PAYLOAD_TOO_LARGE,
            ClientError::Timeout => StatusCode::REQUEST_TIMEOUT,
            ClientError::TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            ClientError::NotFound => StatusCode::NOT_FOUND,
            ClientError::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED,
        }
    }

    #[must_use]
    pub fn message(&self) -> &str {
        match self {
            ClientError::BadRequest(detail) => detail,
            ClientError::PayloadTooLarge => "payload too large",
            ClientError::Timeout => "request timeout",
            ClientError::TooManyRequests => "too many requests",
            ClientError::NotFound => "not found",
            ClientError::MethodNotAllowed => "method not allowed",
        }
    }
}

pub enum ErrorKind<'a> {
    Server {
        method: &'a Method,
        path: &'a str,
        cause: &'a (dyn fmt::Display + 'a),
    },
    Client(ClientError),
}

impl<'a> ErrorKind<'a> {
    #[must_use]
    pub fn server(method: &'a Method, path: &'a str, cause: &'a (dyn fmt::Display + 'a)) -> Self {
        ErrorKind::Server {
            method,
            path,
            cause,
        }
    }

    #[must_use]
    pub fn client(error: ClientError) -> Self {
        ErrorKind::Client(error)
    }
}

fn json_error_body(message: &str) -> String {
    serde_json::json!({ "error": message }).to_string()
}

/// Produce the HTTP response for a classified failure.
#[must_use]
pub fn error_response(kind: ErrorKind<'_>) -> axum::response::Response {
    match kind {
        ErrorKind::Server {
            method,
            path,
            cause,
        } => {
            tracing::error!(
                %method,
                path = %path,
                error = %cause,
                "internal server error while handling request"
            );
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                [(header::CONTENT_TYPE, "application/json")],
                INTERNAL_ERROR_BODY,
            )
                .into_response()
        }
        ErrorKind::Client(error) => {
            let status = error.status();
            let body = json_error_body(error.message());
            (status, [(header::CONTENT_TYPE, "application/json")], body).into_response()
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LimiterGate {
    pub enabled: bool,
    pub warn_public_unprotected: bool,
}

/// Decide the limiter's enabled state and public-exposure warning.
///
/// When `public_instance` is set on a non-loopback bind, the limiter is force-enabled
/// even if `server.limiter` would otherwise leave it off (SearXNG public-instance expectation).
#[must_use]
pub fn resolve_limiter_gate(
    is_loopback: bool,
    explicit: Option<bool>,
    public_instance: bool,
) -> LimiterGate {
    let mut gate = if is_loopback {
        LimiterGate {
            enabled: explicit.unwrap_or(false),
            warn_public_unprotected: false,
        }
    } else {
        match explicit {
            None => LimiterGate {
                enabled: true,
                warn_public_unprotected: false,
            },
            Some(false) => LimiterGate {
                enabled: false,
                warn_public_unprotected: true,
            },
            Some(true) => LimiterGate {
                enabled: true,
                warn_public_unprotected: false,
            },
        }
    };

    if public_instance && !is_loopback && !gate.enabled {
        gate.enabled = true;
        gate.warn_public_unprotected = false;
    }

    gate
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scheme {
    Http,
    Https,
}

impl Scheme {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Scheme::Http => "http",
            Scheme::Https => "https",
        }
    }

    #[must_use]
    fn from_forwarded_proto(value: &str) -> Option<Scheme> {
        let first = value.split(',').next().unwrap_or("").trim();
        if first.eq_ignore_ascii_case("https") {
            Some(Scheme::Https)
        } else if first.eq_ignore_ascii_case("http") {
            Some(Scheme::Http)
        } else {
            None
        }
    }
}

/// Parse trusted proxy entries into networks.
#[must_use]
pub fn parse_trusted_proxies(entries: &[String]) -> Vec<IpNet> {
    entries.iter().filter_map(|e| parse_ip_or_net(e)).collect()
}

/// Check if the peer is a configured trusted proxy.
#[must_use]
pub fn is_trusted_proxy(peer: Option<IpAddr>, trusted_proxies: &[IpNet]) -> bool {
    match peer {
        Some(ip) => {
            let ip = client_ip::normalize(ip);
            trusted_proxies.iter().any(|net| net.contains(&ip))
        }
        None => false,
    }
}

/// Derive the real client IP from request headers and peer address.
#[must_use]
pub fn request_client_ip(
    peer: Option<SocketAddr>,
    headers: &HeaderMap,
    trusted_proxies: &[IpNet],
) -> Option<IpAddr> {
    let header_named = |name: &str| -> Option<String> {
        headers
            .get(name)
            .and_then(|v| v.to_str().ok())
            .map(str::to_string)
    };

    let x_forwarded_for = header_named("x-forwarded-for")
        .map(|value| client_ip::parse_forwarded_for(&value))
        .unwrap_or_default();
    let x_real_ip = header_named("x-real-ip").and_then(|value| value.trim().parse::<IpAddr>().ok());

    client_ip::derive_client_ip(
        peer.map(|p| p.ip()),
        &x_forwarded_for,
        x_real_ip,
        trusted_proxies,
    )
}

/// Optional TCP peer from `ConnectInfo` (absent in oneshot tests / without the layer).
#[derive(Debug, Clone, Copy)]
pub struct OptionalPeer(pub Option<SocketAddr>);

impl<S> axum::extract::FromRequestParts<S> for OptionalPeer
where
    S: Send + Sync,
{
    type Rejection = std::convert::Infallible;

    async fn from_request_parts(
        parts: &mut axum::http::request::Parts,
        _state: &S,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(
            parts
                .extensions
                .get::<axum::extract::ConnectInfo<SocketAddr>>()
                .map(|ci| ci.0),
        ))
    }
}

/// Decide the request scheme, trusting forwarded header only if peer is trusted.
#[must_use]
pub fn forwarded_scheme(
    is_trusted_proxy: bool,
    xfp_header: Option<&str>,
    conn_scheme: Scheme,
) -> Scheme {
    if is_trusted_proxy
        && let Some(header) = xfp_header
        && let Some(scheme) = Scheme::from_forwarded_proto(header)
    {
        return scheme;
    }
    conn_scheme
}

/// Build the origin for self-referential URLs.
#[must_use]
pub fn self_origin(base_url: Option<&str>, scheme: Scheme, host: &str) -> String {
    if let Some(raw) = base_url
        && let Ok(url) = Url::parse(raw)
        && let Some(url_host) = url.host_str()
    {
        return match url.port() {
            Some(port) => format!("{}://{}:{}", url.scheme(), url_host, port),
            None => format!("{}://{}", url.scheme(), url_host),
        };
    }
    format!("{}://{}", scheme.as_str(), host)
}

/// Build the public origin for this request (configured `base_url`, else Host).
#[must_use]
pub fn instance_origin(base_url: Option<&str>, hsts: bool, headers: &HeaderMap) -> Option<String> {
    let host = headers
        .get(header::HOST)
        .and_then(|value| value.to_str().ok())
        .filter(|host| !host.is_empty());
    let forwarded = headers
        .get("x-forwarded-proto")
        .and_then(|value| value.to_str().ok());
    let fallback = if hsts { Scheme::Https } else { Scheme::Http };
    let scheme = forwarded_scheme(true, forwarded, fallback);
    match (base_url, host) {
        (Some(base), _) if !base.is_empty() => {
            let origin = self_origin(Some(base), scheme, host.unwrap_or("localhost"));
            Some(origin)
        }
        (_, Some(host)) => Some(self_origin(None, scheme, host)),
        _ => None,
    }
}

/// Build a self-referential absolute URL from origin and path.
#[must_use]
pub fn absolute_url(base_url: Option<&str>, scheme: Scheme, host: &str, path: &str) -> String {
    let origin = self_origin(base_url, scheme, host);
    if path.is_empty() {
        origin
    } else if let Some(rest) = path.strip_prefix('/') {
        format!("{origin}/{rest}")
    } else {
        format!("{origin}/{path}")
    }
}

/// Assemble the Tower middleware stack around the router.
pub fn apply_middleware(
    router: Router,
    cfg: &DeploymentConfig,
    default_http_headers: &BTreeMap<String, String>,
    http_protocol_version: &str,
    limiter: Option<BotDetectLayer>,
) -> Router {
    let mut router = router;

    if let Some(limiter) = limiter {
        router = router.layer(limiter);
    }

    router = router
        .layer(CompressionLayer::new())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(cfg.effective_request_timeout_seconds()),
        ))
        .layer(RequestBodyLimitLayer::new(
            cfg.effective_max_request_body_bytes(),
        ));

    for (name, value) in default_http_headers {
        if let (Ok(name), Ok(value)) = (
            HeaderName::from_bytes(name.as_bytes()),
            HeaderValue::from_str(value),
        ) {
            router = router.layer(SetResponseHeaderLayer::overriding(name, value));
        }
    }

    for (name, value) in security_headers(cfg) {
        router = router.layer(SetResponseHeaderLayer::overriding(name, value));
    }

    let response_version = if http_protocol_version == "1.0" {
        axum::http::Version::HTTP_10
    } else {
        axum::http::Version::HTTP_11
    };
    router = router.layer(axum::middleware::from_fn(
        move |request: axum::extract::Request, next: axum::middleware::Next| async move {
            let mut response = next.run(request).await;
            *response.version_mut() = response_version;
            response
        },
    ));

    router = router.layer(trace_layer());

    router
}

#[cfg(test)]
mod reverse_proxy_tests {
    use super::*;

    use axum::http::HeaderMap;

    fn nets(entries: &[&str]) -> Vec<IpNet> {
        parse_trusted_proxies(&entries.iter().map(|s| (*s).to_string()).collect::<Vec<_>>())
    }

    fn headers(pairs: &[(&str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(
                HeaderName::from_bytes(name.as_bytes()).unwrap(),
                HeaderValue::from_str(value).unwrap(),
            );
        }
        map
    }

    fn peer(addr: &str) -> Option<SocketAddr> {
        Some(SocketAddr::new(addr.parse().unwrap(), 12345))
    }

    fn ip(addr: &str) -> IpAddr {
        addr.parse().unwrap()
    }

    #[test]
    fn trusted_proxy_selects_forwarded_client_over_peer() {
        let trusted = nets(&["10.0.0.0/8"]);
        let hdrs = headers(&[("x-forwarded-for", "203.0.113.9, 10.0.0.1")]);
        let derived = request_client_ip(peer("10.0.0.1"), &hdrs, &trusted);
        assert_eq!(derived, Some(ip("203.0.113.9")));
    }

    #[test]
    fn without_trusted_proxies_uses_peer_and_ignores_forwarded_for() {
        let hdrs = headers(&[("x-forwarded-for", "203.0.113.9")]);
        let derived = request_client_ip(peer("198.51.100.7"), &hdrs, &[]);
        assert_eq!(
            derived,
            Some(ip("198.51.100.7")),
            "a spoofable X-Forwarded-For must be ignored without trusted proxies"
        );
    }

    #[test]
    fn spoofed_forwarded_headers_ignored_when_peer_not_in_trusted_list() {
        let trusted = nets(&["10.0.0.0/8", "127.0.0.0/8"]);
        let hdrs = headers(&[
            ("x-forwarded-for", "203.0.113.9"),
            ("x-real-ip", "198.51.100.1"),
        ]);
        let derived = request_client_ip(peer("192.0.2.5"), &hdrs, &trusted);
        assert_eq!(
            derived,
            Some(ip("192.0.2.5")),
            "remote clients must not spoof XFF/X-Real-IP even when trusted_proxies is non-empty"
        );
    }

    #[test]
    fn no_source_yields_none() {
        assert_eq!(request_client_ip(None, &HeaderMap::new(), &[]), None);
    }

    #[test]
    fn is_trusted_proxy_matches_configured_networks() {
        let trusted = nets(&["10.0.0.0/8", "192.0.2.1"]);
        assert!(is_trusted_proxy(Some(ip("10.9.9.9")), &trusted));
        assert!(is_trusted_proxy(Some(ip("192.0.2.1")), &trusted));
        assert!(!is_trusted_proxy(Some(ip("203.0.113.1")), &trusted));
        assert!(!is_trusted_proxy(None, &trusted));
        assert!(is_trusted_proxy(Some(ip("::ffff:10.0.0.5")), &trusted));
    }

    #[test]
    fn forwarded_proto_trusted_only_from_a_trusted_proxy() {
        assert_eq!(
            forwarded_scheme(true, Some("https"), Scheme::Http),
            Scheme::Https
        );
        assert_eq!(
            forwarded_scheme(false, Some("https"), Scheme::Http),
            Scheme::Http
        );
    }

    #[test]
    fn forwarded_proto_falls_back_when_absent_or_unrecognised() {
        assert_eq!(forwarded_scheme(true, None, Scheme::Https), Scheme::Https);
        assert_eq!(
            forwarded_scheme(true, Some("gopher"), Scheme::Http),
            Scheme::Http
        );
        assert_eq!(
            forwarded_scheme(true, Some("https, http"), Scheme::Http),
            Scheme::Https
        );
        assert_eq!(
            forwarded_scheme(true, Some("HTTPS"), Scheme::Http),
            Scheme::Https
        );
    }

    #[test]
    fn self_origin_prefers_configured_base_url() {
        assert_eq!(
            self_origin(
                Some("https://search.example.org"),
                Scheme::Http,
                "127.0.0.1"
            ),
            "https://search.example.org"
        );
        assert_eq!(
            self_origin(Some("https://search.example.org:8443/"), Scheme::Http, "x"),
            "https://search.example.org:8443"
        );
    }

    #[test]
    fn self_origin_falls_back_to_derived_scheme_and_host() {
        assert_eq!(
            self_origin(None, Scheme::Https, "example.test:8888"),
            "https://example.test:8888"
        );
        assert_eq!(
            self_origin(Some("not a url"), Scheme::Http, "example.test"),
            "http://example.test"
        );
    }

    #[test]
    fn instance_origin_uses_host_when_base_url_missing() {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_static("zoeken.test"));
        assert_eq!(
            instance_origin(None, false, &headers).as_deref(),
            Some("http://zoeken.test")
        );
        headers.insert(
            HeaderName::from_static("x-forwarded-proto"),
            HeaderValue::from_static("https"),
        );
        assert_eq!(
            instance_origin(None, false, &headers).as_deref(),
            Some("https://zoeken.test")
        );
    }

    #[test]
    fn absolute_url_joins_origin_and_path() {
        assert_eq!(
            absolute_url(Some("https://ex.org"), Scheme::Http, "h", "/search"),
            "https://ex.org/search"
        );
        assert_eq!(
            absolute_url(None, Scheme::Http, "ex.org", "search"),
            "http://ex.org/search"
        );
        assert_eq!(
            absolute_url(None, Scheme::Https, "ex.org", ""),
            "https://ex.org"
        );
    }

    #[test]
    fn parse_trusted_proxies_skips_invalid_entries() {
        let parsed = parse_trusted_proxies(&[
            "10.0.0.0/8".to_string(),
            "not-an-ip".to_string(),
            "192.0.2.5".to_string(),
        ]);
        // The bare address becomes a host network; the invalid entry is dropped.
        assert_eq!(parsed.len(), 2);
        assert!(is_trusted_proxy(Some(ip("10.1.2.3")), &parsed));
        assert!(is_trusted_proxy(Some(ip("192.0.2.5")), &parsed));
    }
}

#[cfg(test)]
mod limiter_gate_tests {
    use super::*;

    #[test]
    fn public_bind_without_explicit_setting_enables_without_warning() {
        let gate = resolve_limiter_gate(false, None, false);
        assert_eq!(
            gate,
            LimiterGate {
                enabled: true,
                warn_public_unprotected: false
            }
        );
    }

    #[test]
    fn public_bind_explicitly_disabled_stays_off_and_warns() {
        let gate = resolve_limiter_gate(false, Some(false), false);
        assert_eq!(
            gate,
            LimiterGate {
                enabled: false,
                warn_public_unprotected: true
            }
        );
    }

    #[test]
    fn public_instance_force_enables_when_explicitly_disabled() {
        let gate = resolve_limiter_gate(false, Some(false), true);
        assert_eq!(
            gate,
            LimiterGate {
                enabled: true,
                warn_public_unprotected: false
            }
        );
    }

    #[test]
    fn public_bind_explicitly_enabled_is_enabled_without_warning() {
        let gate = resolve_limiter_gate(false, Some(true), false);
        assert_eq!(
            gate,
            LimiterGate {
                enabled: true,
                warn_public_unprotected: false
            }
        );
    }

    #[test]
    fn loopback_defaults_off_without_warning() {
        let gate = resolve_limiter_gate(true, None, false);
        assert_eq!(
            gate,
            LimiterGate {
                enabled: false,
                warn_public_unprotected: false
            }
        );
    }

    #[test]
    fn loopback_honors_explicit_setting_without_warning() {
        assert_eq!(
            resolve_limiter_gate(true, Some(true), false),
            LimiterGate {
                enabled: true,
                warn_public_unprotected: false
            }
        );
        assert_eq!(
            resolve_limiter_gate(true, Some(false), false),
            LimiterGate {
                enabled: false,
                warn_public_unprotected: false
            }
        );
    }

    #[test]
    fn warning_only_ever_fires_for_public_explicitly_disabled() {
        for is_loopback in [true, false] {
            for explicit in [None, Some(true), Some(false)] {
                let gate = resolve_limiter_gate(is_loopback, explicit, false);
                let expected_warn = !is_loopback && explicit == Some(false);
                assert_eq!(
                    gate.warn_public_unprotected, expected_warn,
                    "warn flag mismatch for (is_loopback={is_loopback}, explicit={explicit:?})"
                );
                if gate.enabled {
                    assert!(!gate.warn_public_unprotected);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoeken_settings::{DeploymentConfig, default_content_security_policy};

    fn find<'a>(
        headers: &'a [(HeaderName, HeaderValue)],
        name: &HeaderName,
    ) -> Option<&'a HeaderValue> {
        headers.iter().find(|(n, _)| n == name).map(|(_, v)| v)
    }

    /// The value of `name` as a string, or `None` if absent/non-ASCII.
    fn value_str<'a>(
        headers: &'a [(HeaderName, HeaderValue)],
        name: &HeaderName,
    ) -> Option<&'a str> {
        find(headers, name).and_then(|v| v.to_str().ok())
    }

    #[test]
    fn always_includes_content_type_options_frame_options_and_csp() {
        let cfg = DeploymentConfig::default();
        let headers = security_headers(&cfg);

        assert_eq!(
            value_str(&headers, &X_CONTENT_TYPE_OPTIONS),
            Some("nosniff")
        );
        assert_eq!(value_str(&headers, &X_FRAME_OPTIONS), Some("DENY"));
        assert!(find(&headers, &CONTENT_SECURITY_POLICY).is_some());
    }

    #[test]
    fn csp_reflects_the_effective_policy() {
        let cfg = DeploymentConfig::default();
        let headers = security_headers(&cfg);
        assert_eq!(
            value_str(&headers, &CONTENT_SECURITY_POLICY),
            Some(default_content_security_policy().as_str())
        );

        let overridden = DeploymentConfig {
            content_security_policy: Some("default-src 'none'".to_string()),
            ..Default::default()
        };
        let headers = security_headers(&overridden);
        assert_eq!(
            value_str(&headers, &CONTENT_SECURITY_POLICY),
            Some("default-src 'none'")
        );
    }

    #[test]
    fn hsts_is_omitted_by_default() {
        let cfg = DeploymentConfig::default();
        assert!(!cfg.hsts);
        let headers = security_headers(&cfg);
        assert!(find(&headers, &STRICT_TRANSPORT_SECURITY).is_none());
    }

    #[test]
    fn hsts_is_present_only_when_enabled() {
        let cfg = DeploymentConfig {
            hsts: true,
            ..Default::default()
        };
        let headers = security_headers(&cfg);
        assert_eq!(
            value_str(&headers, &STRICT_TRANSPORT_SECURITY),
            Some(HSTS_VALUE)
        );
    }

    #[test]
    fn invalid_csp_is_skipped_without_dropping_other_headers() {
        let cfg = DeploymentConfig {
            content_security_policy: Some("default-src 'self'\n".to_string()),
            ..Default::default()
        };
        let headers = security_headers(&cfg);

        assert!(find(&headers, &CONTENT_SECURITY_POLICY).is_none());
        assert!(find(&headers, &X_CONTENT_TYPE_OPTIONS).is_some());
        assert!(find(&headers, &X_FRAME_OPTIONS).is_some());
    }
}

#[cfg(test)]
mod trace_tests {
    use super::*;

    use std::collections::BTreeMap;
    use std::sync::{Arc, Mutex};

    use tracing::Subscriber;
    use tracing::field::{Field, Visit};
    use tracing::span::{Attributes, Id, Record};
    use tracing_subscriber::layer::{Context, Layer};
    use tracing_subscriber::prelude::*;
    use tracing_subscriber::registry::LookupSpan;

    fn sensitive_request() -> Request<()> {
        Request::builder()
            .method("POST")
            .uri("/search?q=topsecretquery")
            .header(
                "cookie",
                "prefs=eyJ0aGVtZSI6ImRhcmsifQ; session=COOKIESECRET",
            )
            .header("authorization", "Bearer SECRETKEYVALUE")
            .body(())
            .unwrap()
    }

    #[test]
    fn request_fields_extract_only_method_and_path() {
        let request = sensitive_request();
        let fields = RequestFields::from_request(&request);

        assert_eq!(fields.method, Method::POST);
        assert_eq!(fields.path, "/search");
    }

    #[derive(Default)]
    struct Captured {
        fields: BTreeMap<String, String>,
    }

    struct FieldVisitor<'a>(&'a mut BTreeMap<String, String>);

    impl Visit for FieldVisitor<'_> {
        fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
            self.0
                .insert(field.name().to_string(), format!("{value:?}"));
        }
        fn record_str(&mut self, field: &Field, value: &str) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
        fn record_u64(&mut self, field: &Field, value: u64) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
        fn record_i64(&mut self, field: &Field, value: i64) {
            self.0.insert(field.name().to_string(), value.to_string());
        }
    }

    #[derive(Clone)]
    struct CaptureLayer(Arc<Mutex<Captured>>);

    impl<S> Layer<S> for CaptureLayer
    where
        S: Subscriber + for<'a> LookupSpan<'a>,
    {
        fn on_new_span(&self, attrs: &Attributes<'_>, _id: &Id, _ctx: Context<'_, S>) {
            let mut captured = self.0.lock().unwrap();
            attrs.record(&mut FieldVisitor(&mut captured.fields));
        }

        fn on_record(&self, _id: &Id, values: &Record<'_>, _ctx: Context<'_, S>) {
            let mut captured = self.0.lock().unwrap();
            values.record(&mut FieldVisitor(&mut captured.fields));
        }
    }

    fn capture(body: impl FnOnce()) -> BTreeMap<String, String> {
        let captured = Arc::new(Mutex::new(Captured::default()));
        let subscriber = tracing_subscriber::registry().with(CaptureLayer(captured.clone()));
        tracing::subscriber::with_default(subscriber, body);
        let guard = captured.lock().unwrap();
        guard.fields.clone()
    }

    #[test]
    fn span_records_allowlisted_fields_and_nothing_sensitive() {
        let fields = capture(|| {
            let mut make = RedactingMakeSpan;
            let request = sensitive_request();
            let span = make.make_span(&request);
            let response = Response::builder().status(503).body(()).unwrap();
            RedactingOnResponse.on_response(&response, Duration::from_millis(42), &span);
        });

        assert_eq!(fields.get("method").map(String::as_str), Some("POST"));
        assert_eq!(fields.get("path").map(String::as_str), Some("/search"));
        assert_eq!(fields.get("status").map(String::as_str), Some("503"));
        assert_eq!(fields.get("latency_ms").map(String::as_str), Some("42"));

        assert!(!fields.contains_key("cookie") && !fields.contains_key("authorization"));

        for value in fields.values() {
            assert!(!value.contains("COOKIESECRET"));
            assert!(!value.contains("SECRETKEYVALUE"));
            assert!(!value.contains("topsecretquery"));
        }
    }

    #[test]
    fn trace_layer_builds() {
        let _layer: RequestTraceLayer = trace_layer();
    }
}

#[cfg(test)]
mod error_response_tests {
    use super::*;

    use axum::body::to_bytes;

    async fn body_text(response: axum::response::Response) -> String {
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        String::from_utf8(bytes.to_vec()).unwrap()
    }

    #[tokio::test]
    async fn server_failure_returns_generic_500_body_without_the_cause() {
        let method = Method::GET;
        let cause =
            "SearchOrchestratorError at /home/app/zoeken-search/src/lib.rs:42 (stack overflow)";
        let response = error_response(ErrorKind::server(&method, "/search", &cause));

        assert_eq!(response.status(), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json"),
        );

        let body = body_text(response).await;
        assert_eq!(body, INTERNAL_ERROR_BODY);
        assert!(!body.contains("SearchOrchestratorError"));
        assert!(!body.contains("zoeken-search/src/lib.rs"));
        assert!(!body.contains("stack"));
        assert!(!body.contains("42"));
    }

    #[tokio::test]
    async fn server_failure_body_is_identical_regardless_of_cause() {
        let method = Method::POST;
        let a = error_response(ErrorKind::server(&method, "/a", &"cause one: DbError"));
        let b = error_response(ErrorKind::server(&method, "/b", &"cause two: TimeoutError"));
        assert_eq!(body_text(a).await, body_text(b).await);
    }

    #[tokio::test]
    async fn bad_request_names_the_clients_mistake() {
        let response = error_response(ErrorKind::client(ClientError::BadRequest(
            "unsupported format 'yaml'".to_string(),
        )));

        assert_eq!(response.status(), StatusCode::BAD_REQUEST);
        let body = body_text(response).await;
        assert!(body.contains("unsupported format 'yaml'"));
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["error"], "unsupported format 'yaml'");
    }

    #[tokio::test]
    async fn client_errors_carry_the_expected_status() {
        let cases = [
            (ClientError::PayloadTooLarge, StatusCode::PAYLOAD_TOO_LARGE),
            (ClientError::Timeout, StatusCode::REQUEST_TIMEOUT),
            (ClientError::TooManyRequests, StatusCode::TOO_MANY_REQUESTS),
            (ClientError::NotFound, StatusCode::NOT_FOUND),
            (
                ClientError::MethodNotAllowed,
                StatusCode::METHOD_NOT_ALLOWED,
            ),
        ];

        for (error, expected) in cases {
            let message = error.message().to_string();
            let response = error_response(ErrorKind::client(error));
            assert_eq!(response.status(), expected);
            let body = body_text(response).await;
            let value: serde_json::Value = serde_json::from_str(&body).unwrap();
            assert_eq!(value["error"], message);
        }
    }

    #[tokio::test]
    async fn caller_supplied_detail_cannot_break_the_json_body() {
        let response = error_response(ErrorKind::client(ClientError::BadRequest(
            r#"bad "value": {injected: true}"#.to_string(),
        )));
        let body = body_text(response).await;
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["error"], r#"bad "value": {injected: true}"#);
        assert!(value.get("injected").is_none());
    }
}

#[cfg(test)]
mod apply_middleware_tests {
    use super::*;

    use std::net::SocketAddr;
    use std::str::FromStr;
    use std::sync::Arc;

    use axum::body::{Body, to_bytes};
    use axum::extract::ConnectInfo;
    use axum::http::Request;
    use axum::routing::{get, post};
    use ipnet::IpNet;
    use tower::ServiceExt;
    use zoeken_botdetect::{Detector, LimiterConfig};
    use zoeken_settings::DeploymentConfig;

    fn tight_cfg() -> DeploymentConfig {
        DeploymentConfig {
            max_request_body_bytes: 8,
            request_timeout_seconds: 1,
            ..DeploymentConfig::default()
        }
    }

    async fn ok_handler() -> &'static str {
        "ok"
    }

    async fn slow_handler() -> &'static str {
        tokio::time::sleep(Duration::from_secs(30)).await;
        "slow"
    }

    async fn echo_handler(body: String) -> String {
        body
    }

    fn app(cfg: &DeploymentConfig, limiter: Option<BotDetectLayer>) -> Router {
        let router = Router::new()
            .route("/", get(ok_handler))
            .route("/slow", get(slow_handler))
            .route("/echo", post(echo_handler));
        apply_middleware(
            router,
            cfg,
            &std::collections::BTreeMap::new(),
            "1.1",
            limiter,
        )
    }

    fn assert_security_headers(headers: &HeaderMap) {
        assert_eq!(
            headers
                .get(&X_CONTENT_TYPE_OPTIONS)
                .and_then(|v| v.to_str().ok()),
            Some("nosniff"),
            "X-Content-Type-Options must be present on every response"
        );
        assert_eq!(
            headers.get(&X_FRAME_OPTIONS).and_then(|v| v.to_str().ok()),
            Some("DENY"),
            "X-Frame-Options must be present on every response"
        );
        assert!(
            headers.get(&CONTENT_SECURITY_POLICY).is_some(),
            "Content-Security-Policy must be present on every response"
        );
    }

    #[tokio::test]
    async fn successful_response_carries_security_headers() {
        let response = app(&tight_cfg(), None)
            .oneshot(Request::builder().uri("/").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_security_headers(response.headers());
    }

    #[tokio::test]
    async fn oversized_body_is_rejected_with_413_and_carries_headers() {
        let response = app(&tight_cfg(), None)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/echo")
                    .body(Body::from(
                        "this body is definitely longer than eight bytes",
                    ))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
        assert_security_headers(response.headers());
    }

    #[tokio::test]
    async fn small_body_within_limit_reaches_handler() {
        let response = app(&tight_cfg(), None)
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/echo")
                    .body(Body::from("tiny"))
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"tiny");
    }

    #[tokio::test]
    async fn slow_request_times_out_with_408_and_carries_headers() {
        let response = app(&tight_cfg(), None)
            .oneshot(Request::builder().uri("/slow").body(Body::empty()).unwrap())
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::REQUEST_TIMEOUT);
        assert_security_headers(response.headers());
    }

    #[tokio::test]
    async fn limiter_runs_ahead_of_handlers_and_its_rejection_carries_headers() {
        let mut limiter_cfg = LimiterConfig {
            pass_reserved_nets: false,
            ..LimiterConfig::default()
        };
        limiter_cfg.block_ip = vec![IpNet::from_str("203.0.113.0/24").unwrap()];
        let detector = Detector::new(limiter_cfg, String::new());
        let limiter = zoeken_botdetect::layer(Arc::new(detector));

        let mut request = Request::builder().uri("/").body(Body::empty()).unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([203, 0, 113, 9], 34567))));

        let response = app(&tight_cfg(), Some(limiter))
            .oneshot(request)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::FORBIDDEN);
        assert_security_headers(response.headers());
    }

    #[tokio::test]
    async fn limiter_allows_clean_request_through_to_handler() {
        let limiter_cfg = LimiterConfig {
            pass_reserved_nets: false,
            ..LimiterConfig::default()
        };
        let detector = Detector::new(limiter_cfg, String::new());
        let limiter = zoeken_botdetect::layer(Arc::new(detector));

        let mut request = Request::builder()
            .uri("/")
            .header(header::ACCEPT, "text/html")
            .header(header::ACCEPT_ENCODING, "gzip, deflate")
            .header(header::ACCEPT_LANGUAGE, "en-US")
            .header(header::CONNECTION, "keep-alive")
            .header(header::USER_AGENT, "Mozilla/5.0 Firefox/120.0")
            .body(Body::empty())
            .unwrap();
        request
            .extensions_mut()
            .insert(ConnectInfo(SocketAddr::from(([198, 51, 100, 7], 34567))));

        let response = app(&tight_cfg(), Some(limiter))
            .oneshot(request)
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_security_headers(response.headers());
    }
}
