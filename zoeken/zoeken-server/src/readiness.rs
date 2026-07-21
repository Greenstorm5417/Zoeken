//! Readiness state and the `/readyz` probe.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use axum::extract::State;
use axum::http::{StatusCode, header};
use axum::response::{IntoResponse, Response};

#[derive(Clone, Debug)]
pub struct ReadinessState {
    serving: Arc<AtomicBool>,
    storage_healthy: Arc<AtomicBool>,
}

impl ReadinessState {
    pub fn new_not_ready() -> Self {
        ReadinessState {
            serving: Arc::new(AtomicBool::new(false)),
            storage_healthy: Arc::new(AtomicBool::new(true)),
        }
    }

    pub fn set_ready(&self) {
        self.serving.store(true, Ordering::SeqCst);
    }

    pub fn begin_draining(&self) {
        self.serving.store(false, Ordering::SeqCst);
    }

    pub fn set_storage_healthy(&self, healthy: bool) {
        self.storage_healthy.store(healthy, Ordering::SeqCst);
    }

    pub fn is_ready(&self) -> bool {
        self.serving.load(Ordering::SeqCst) && self.storage_healthy.load(Ordering::SeqCst)
    }
}

impl Default for ReadinessState {
    fn default() -> Self {
        Self::new_not_ready()
    }
}

/// `GET /readyz` readiness probe.
pub async fn readyz(State(readiness): State<ReadinessState>) -> Response {
    if readiness.is_ready() {
        (
            StatusCode::OK,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"status":"ready"}"#,
        )
            .into_response()
    } else {
        (
            StatusCode::SERVICE_UNAVAILABLE,
            [(header::CONTENT_TYPE, "application/json")],
            r#"{"status":"not_ready"}"#,
        )
            .into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use axum::Router;
    use axum::body::{Body, to_bytes};
    use axum::http::Request;
    use axum::routing::get;
    use tower::ServiceExt;

    #[test]
    fn new_state_is_not_ready() {
        let state = ReadinessState::new_not_ready();
        assert!(!state.is_ready());
    }

    #[test]
    fn set_ready_makes_state_ready() {
        let state = ReadinessState::new_not_ready();
        state.set_ready();
        assert!(state.is_ready());
    }

    #[test]
    fn begin_draining_makes_state_not_ready() {
        let state = ReadinessState::new_not_ready();
        state.set_ready();
        state.begin_draining();
        assert!(!state.is_ready());
    }

    #[test]
    fn clones_share_the_same_flag() {
        let state = ReadinessState::new_not_ready();
        let clone = state.clone();
        state.set_ready();
        assert!(clone.is_ready());
        clone.begin_draining();
        assert!(!state.is_ready());
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

    #[tokio::test]
    async fn readyz_reports_not_ready_before_startup() {
        let response = readyz_app(ReadinessState::new_not_ready())
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(body_text(response).await.contains("not_ready"));
    }

    #[tokio::test]
    async fn readyz_reports_ready_after_startup() {
        let readiness = ReadinessState::new_not_ready();
        readiness.set_ready();

        let response = readyz_app(readiness)
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert!(body_text(response).await.contains("ready"));
    }

    #[tokio::test]
    async fn readyz_reports_not_ready_while_draining() {
        let readiness = ReadinessState::new_not_ready();
        readiness.set_ready();
        readiness.begin_draining();

        let response = readyz_app(readiness)
            .oneshot(
                Request::builder()
                    .uri("/readyz")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        assert!(body_text(response).await.contains("not_ready"));
    }
}
