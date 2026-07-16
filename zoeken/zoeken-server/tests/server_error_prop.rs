use axum::body::to_bytes;
use axum::http::{Method, StatusCode, header};
use proptest::prelude::*;
use zoeken_server::middleware::{ClientError, ErrorKind, INTERNAL_ERROR_BODY, error_response};

fn body_text(response: axum::response::Response) -> String {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build current-thread runtime");
    rt.block_on(async {
        let bytes = to_bytes(response.into_body(), usize::MAX)
            .await
            .expect("collect body");
        String::from_utf8(bytes.to_vec()).expect("body is valid UTF-8")
    })
}

fn method_strategy() -> impl Strategy<Value = Method> {
    prop_oneof![
        Just(Method::GET),
        Just(Method::POST),
        Just(Method::PUT),
        Just(Method::DELETE),
        Just(Method::PATCH),
        Just(Method::HEAD),
    ]
}

fn internal_fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("SearchOrchestratorError".to_string()),
        Just("zoeken_search::pipeline::AggregatorError".to_string()),
        Just("std::io::Error(Os { code: 111 })".to_string()),
        Just("PoisonError { .. }".to_string()),
        Just("/home/app/zoeken-search/src/lib.rs:42:17".to_string()),
        Just("/usr/local/cargo/registry/src/foo-1.2.3/src/mod.rs:8".to_string()),
        Just("thread 'tokio-runtime-worker' panicked at 'unwrap on None'".to_string()),
        Just("stack backtrace:\n   0: core::panicking::panic".to_string()),
        "[a-zA-Z_][a-zA-Z0-9_:]{0,40}".prop_map(|s| s),
        "[0-9]{1,6}".prop_map(|s| s),
    ]
}

fn cause_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec(internal_fragment(), 0..5).prop_map(|parts| parts.join(" "))
}

fn path_strategy() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("/search".to_string()),
        Just("/autocompleter".to_string()),
        Just("/".to_string()),
        "/[a-z0-9/_-]{0,30}".prop_map(|s| s),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn server_error_body_is_generic_and_never_leaks_cause(
        method in method_strategy(),
        path in path_strategy(),
        cause in cause_strategy(),
    ) {
        let response = error_response(ErrorKind::server(&method, &path, &cause));

        prop_assert_eq!(
            response.status(),
            StatusCode::INTERNAL_SERVER_ERROR,
            "server-side failure must be a 500 (Req 19.1)"
        );

        prop_assert_eq!(
            response
                .headers()
                .get(header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok()),
            Some("application/json"),
        );

        let body = body_text(response);

        prop_assert_eq!(
            body.as_str(),
            INTERNAL_ERROR_BODY,
            "body must be the fixed generic message regardless of cause"
        );

        if cause.len() >= 8 {
            prop_assert!(
                !body.contains(cause.as_str()),
                "body must not contain the full cause string"
            );
        }
        for token in cause.split_whitespace().filter(|t| t.len() >= 3) {
            prop_assert!(
                !body.contains(token),
                "body must not contain any internal fragment of the cause: {:?}",
                token
            );
        }
    }

    #[test]
    fn client_error_surfaces_status_and_message(detail in "[a-zA-Z0-9 _-]{0,40}") {
        let response = error_response(ErrorKind::client(ClientError::BadRequest(detail.clone())));

        let status = response.status();
        prop_assert!(status.is_client_error(), "client errors are 4xx, got {}", status);

        let body = body_text(response);
        prop_assert_ne!(body.as_str(), INTERNAL_ERROR_BODY);
        prop_assert!(
            body.contains(&detail),
            "client message should name the client's mistake"
        );
    }
}
