// Property 19: security headers on every response.

use axum::Router;
use axum::body::Body;
use axum::extract::Path;
use axum::http::header::{
    CONTENT_SECURITY_POLICY, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderName, HeaderValue, Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{any as any_method, get};
use proptest::prelude::*;
use tower::ServiceExt;
use zoeken_server::middleware::{apply_middleware, security_headers};
use zoeken_settings::{DeploymentConfig, default_content_security_policy};

const BODY_LIMIT_BYTES: usize = 32;

const STATUS_CODES: &[u16] = &[
    200, 201, 204, 301, 400, 401, 403, 404, 405, 408, 409, 413, 415, 429, 500, 502, 503,
];

#[derive(Debug, Clone)]
enum Scenario {
    HandlerStatus(u16),
    UnmatchedPath(String),
    WrongMethod,
    OversizedBody,
}

fn scenario_strategy() -> impl Strategy<Value = Scenario> {
    prop_oneof![
        prop::sample::select(STATUS_CODES.to_vec()).prop_map(Scenario::HandlerStatus),
        prop::collection::vec("[a-z0-9_]{1,8}", 1..4)
            .prop_map(|segs| Scenario::UnmatchedPath(segs.join("/"))),
        Just(Scenario::WrongMethod),
        Just(Scenario::OversizedBody),
    ]
}

fn config_strategy() -> impl Strategy<Value = DeploymentConfig> {
    let csp = prop_oneof![
        Just(None),
        Just(Some(default_content_security_policy())),
        Just(Some("default-src 'none'".to_string())),
        Just(Some(
            "default-src 'self'; img-src 'self' https:".to_string()
        )),
    ];
    (any::<bool>(), csp).prop_map(|(hsts, content_security_policy)| DeploymentConfig {
        hsts,
        content_security_policy,
        max_request_body_bytes: BODY_LIMIT_BYTES,
        ..DeploymentConfig::default()
    })
}

async fn echo_status(Path(code): Path<u16>) -> Response {
    let status = StatusCode::from_u16(code).unwrap_or(StatusCode::OK);
    (status, "echo").into_response()
}

fn build_app(cfg: &DeploymentConfig) -> Router {
    let router = Router::new()
        .route("/ok", get(|| async { "ok" }))
        .route("/echo/{code}", any_method(echo_status));
    apply_middleware(router, cfg, None)
}

fn build_request(scenario: &Scenario) -> Request<Body> {
    match scenario {
        Scenario::HandlerStatus(code) => Request::builder()
            .method(Method::GET)
            .uri(format!("/echo/{code}"))
            .body(Body::empty())
            .unwrap(),
        Scenario::UnmatchedPath(path) => Request::builder()
            .method(Method::GET)
            .uri(format!("/{path}/definitely-not-a-route"))
            .body(Body::empty())
            .unwrap(),
        Scenario::WrongMethod => Request::builder()
            .method(Method::DELETE)
            .uri("/ok")
            .body(Body::empty())
            .unwrap(),
        Scenario::OversizedBody => {
            let oversized = vec![b'x'; BODY_LIMIT_BYTES * 4];
            Request::builder()
                .method(Method::POST)
                .uri("/echo/200")
                .header(axum::http::header::CONTENT_LENGTH, oversized.len())
                .body(Body::from(oversized))
                .unwrap()
        }
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Property 19: security headers on every response.
    #[test]
    fn security_headers_on_every_response(
        scenario in scenario_strategy(),
        cfg in config_strategy(),
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime");

        let response = rt.block_on(async {
            build_app(&cfg)
                .oneshot(build_request(&scenario))
                .await
                .expect("router is infallible")
        });

        let status = response.status();
        let headers = response.headers();

        for name in [
            &X_CONTENT_TYPE_OPTIONS,
            &X_FRAME_OPTIONS,
            &CONTENT_SECURITY_POLICY,
        ] {
            prop_assert!(
                headers.contains_key(name),
                "missing {name} on {status} response for scenario {scenario:?}",
            );
        }

        let expected: Vec<(HeaderName, HeaderValue)> = security_headers(&cfg);
        for (name, value) in &expected {
            let got = headers.get(name);
            prop_assert_eq!(
                got,
                Some(value),
                "header {} mismatch on {} response (scenario {:?}): expected {:?}, got {:?}",
                name,
                status,
                scenario,
                value,
                got,
            );
        }

        // When HSTS is disabled the Strict-Transport-Security header must be
        // absent (it is only emitted behind opt-in TLS termination, Req 16.4),
        // confirming the header set tracks the configuration exactly.
        if !cfg.hsts {
            prop_assert!(
                !headers.contains_key(&STRICT_TRANSPORT_SECURITY),
                "Strict-Transport-Security must be absent when HSTS is off (scenario {scenario:?})",
            );
        }

        // Sanity: the scenarios actually produce the intended status classes so
        // the property is exercised over the full range the design calls out.
        match &scenario {
            Scenario::UnmatchedPath(_) => prop_assert_eq!(status, StatusCode::NOT_FOUND),
            Scenario::WrongMethod => prop_assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED),
            Scenario::OversizedBody => prop_assert_eq!(status, StatusCode::PAYLOAD_TOO_LARGE),
            Scenario::HandlerStatus(code) => {
                prop_assert_eq!(status.as_u16(), *code);
            }
        }
    }
}
