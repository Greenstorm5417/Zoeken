use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode};
use axum::response::Response;
use axum::routing::get;
use proptest::prelude::*;
use tower::ServiceExt;
use zoeken_server::readiness::{ReadinessState, readyz};

#[derive(Debug, Clone, Copy)]
enum Transition {
    SetReady,
    BeginDraining,
    InitFailure,
}

fn transition_strategy() -> impl Strategy<Value = Transition> {
    prop_oneof![
        Just(Transition::SetReady),
        Just(Transition::BeginDraining),
        Just(Transition::InitFailure),
    ]
}

fn readyz_app(readiness: ReadinessState) -> Router {
    Router::new()
        .route("/readyz", get(readyz))
        .with_state(readiness)
}

async fn body_text(response: Response) -> String {
    let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    String::from_utf8(body.to_vec()).unwrap()
}

async fn probe(readiness: ReadinessState) -> (StatusCode, String) {
    let response = readyz_app(readiness)
        .oneshot(
            Request::builder()
                .uri("/readyz")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    let status = response.status();
    (status, body_text(response).await)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn readiness_reflects_lifecycle_state(
        transitions in proptest::collection::vec(transition_strategy(), 0..64),
    ) {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let state = ReadinessState::new_not_ready();

        let mut expected_ready = false;

        prop_assert_eq!(state.is_ready(), expected_ready);
        let (status, text) = rt.block_on(probe(state.clone()));
        let expected_status = if expected_ready {
            StatusCode::OK
        } else {
            StatusCode::SERVICE_UNAVAILABLE
        };
        prop_assert_eq!(status, expected_status);
        if expected_ready {
            prop_assert!(text.contains("ready") && !text.contains("not_ready"));
        } else {
            prop_assert!(text.contains("not_ready"));
        }

        for (i, transition) in transitions.iter().enumerate() {
            match transition {
                Transition::SetReady => {
                    state.set_ready();
                    expected_ready = true;
                }
                Transition::BeginDraining => {
                    state.begin_draining();
                    expected_ready = false;
                }
                Transition::InitFailure => {
                }
            }

            prop_assert_eq!(
                state.is_ready(),
                expected_ready,
                "after transition #{} ({:?}): is_ready mismatch",
                i,
                transition,
            );

            let (status, text) = rt.block_on(probe(state.clone()));
            let expected_status = if expected_ready {
                StatusCode::OK
            } else {
                StatusCode::SERVICE_UNAVAILABLE
            };
            prop_assert_eq!(
                status,
                expected_status,
                "after transition #{} ({:?}): /readyz status mismatch (expected_ready={})",
                i,
                transition,
                expected_ready,
            );
            if expected_ready {
                prop_assert!(
                    text.contains("ready") && !text.contains("not_ready"),
                    "after transition #{} ({:?}): ready body mismatch: {}",
                    i,
                    transition,
                    text,
                );
            } else {
                prop_assert!(
                    text.contains("not_ready"),
                    "after transition #{} ({:?}): not-ready body mismatch: {}",
                    i,
                    transition,
                    text,
                );
            }
        }
    }
}
