//! Listener binding, shutdown, and graceful serving.

use std::future::{Future, IntoFuture};
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use axum::Router;
use thiserror::Error;
use tokio::net::TcpListener;
use tokio::sync::Notify;

use crate::readiness::ReadinessState;
use zoeken_settings::DeploymentConfig;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ServeConfig {
    pub bind: SocketAddr,
    pub shutdown_grace: Duration,
    pub body_limit: usize,
    pub request_timeout: Duration,
}

impl ServeConfig {
    #[must_use]
    pub fn from_deployment(bind: SocketAddr, deployment: &DeploymentConfig) -> Self {
        ServeConfig {
            bind,
            shutdown_grace: Duration::from_secs(deployment.shutdown_grace_seconds),
            body_limit: deployment.effective_max_request_body_bytes(),
            request_timeout: Duration::from_secs(deployment.effective_request_timeout_seconds()),
        }
    }
}

#[derive(Debug, Error)]
pub enum ServeError {
    #[error("failed to bind address {} port {}: {source}", .addr.ip(), .addr.port())]
    Bind {
        addr: SocketAddr,
        #[source]
        source: std::io::Error,
    },

    #[error("server I/O error: {0}")]
    Io(#[source] std::io::Error),
}

/// Bind a TCP listener to `addr`.
pub async fn bind_listener(addr: SocketAddr) -> Result<TcpListener, ServeError> {
    TcpListener::bind(addr)
        .await
        .map_err(|source| ServeError::Bind { addr, source })
}

/// Resolve on the first termination signal and mark readiness draining.
pub async fn shutdown_signal(readiness: ReadinessState) {
    let ctrl_c = async {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to install Ctrl+C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }

    readiness.begin_draining();
}

/// Serve `router` until shutdown, then bound the drain by `cfg.shutdown_grace`.
pub async fn serve(
    listener: TcpListener,
    router: Router,
    cfg: &ServeConfig,
    readiness: ReadinessState,
) -> Result<(), ServeError> {
    serve_with_shutdown(
        listener,
        router,
        cfg.shutdown_grace,
        shutdown_signal(readiness),
    )
    .await
}

/// Test seam for graceful serving with a controllable shutdown future.
#[doc(hidden)]
pub async fn serve_with_shutdown<F>(
    listener: TcpListener,
    router: Router,
    grace: Duration,
    shutdown: F,
) -> Result<(), ServeError>
where
    F: Future<Output = ()> + Send + 'static,
{
    let drain_started = Arc::new(Notify::new());
    let signal = {
        let drain_started = Arc::clone(&drain_started);
        async move {
            shutdown.await;
            drain_started.notify_one();
        }
    };

    let make_service = router.into_make_service_with_connect_info::<SocketAddr>();
    let server = axum::serve(listener, make_service)
        .with_graceful_shutdown(signal)
        .into_future();
    tokio::pin!(server);

    tokio::select! {
        result = &mut server => return result.map_err(ServeError::Io),
        _ = drain_started.notified() => {}
    }

    tokio::select! {
        result = &mut server => result.map_err(ServeError::Io),
        _ = tokio::time::sleep(grace) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::net::{Ipv4Addr, SocketAddr};
    use std::time::Instant;

    use axum::Router;
    use axum::routing::get;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpStream;

    fn loopback(port: u16) -> SocketAddr {
        SocketAddr::from((Ipv4Addr::LOCALHOST, port))
    }

    #[tokio::test]
    async fn bind_listener_binds_an_ephemeral_port() {
        let listener = bind_listener(loopback(0)).await.expect("bind ephemeral");
        let local = listener.local_addr().unwrap();
        assert!(local.ip().is_loopback());
        assert_ne!(local.port(), 0, "an ephemeral port should be assigned");
    }

    /// A bind failure reports the address, the port, and the underlying cause
    /// (Req 10.4). Binding a port already held by another listener yields
    /// `AddrInUse`.
    #[tokio::test]
    async fn bind_listener_reports_address_port_and_cause_on_failure() {
        let held = bind_listener(loopback(0)).await.expect("hold a port");
        let addr = held.local_addr().unwrap();

        let error = bind_listener(addr)
            .await
            .expect_err("binding an in-use address must fail");

        match &error {
            ServeError::Bind {
                addr: reported,
                source,
            } => {
                assert_eq!(*reported, addr);
                // The cause is the OS error kind (AddrInUse on all platforms).
                assert_eq!(source.kind(), std::io::ErrorKind::AddrInUse);
            }
            other => panic!("expected a Bind error, got {other:?}"),
        }

        let message = error.to_string();
        assert!(
            message.contains(&addr.ip().to_string()),
            "message should name the address: {message}"
        );
        assert!(
            message.contains(&addr.port().to_string()),
            "message should name the port: {message}"
        );
    }

    /// `ServeConfig::from_deployment` maps the second-granularity settings onto
    /// durations and copies the bounded resource limits.
    #[test]
    fn serve_config_from_deployment_maps_fields() {
        let deployment = DeploymentConfig::default();
        let bind = loopback(8888);
        let cfg = ServeConfig::from_deployment(bind, &deployment);

        assert_eq!(cfg.bind, bind);
        assert_eq!(
            cfg.shutdown_grace,
            Duration::from_secs(deployment.shutdown_grace_seconds)
        );
        assert_eq!(cfg.body_limit, deployment.max_request_body_bytes);
        assert_eq!(
            cfg.request_timeout,
            Duration::from_secs(deployment.request_timeout_seconds)
        );
    }

    /// With no traffic, `serve` returns promptly once the shutdown trigger fires
    /// and the readiness state has been flipped to draining (Reqs 14.1, 14.4).
    #[tokio::test]
    async fn serve_returns_after_shutdown_with_no_traffic() {
        let listener = bind_listener(loopback(0)).await.unwrap();
        let router = Router::new().route("/", get(|| async { "ok" }));

        let readiness = ReadinessState::new_not_ready();
        readiness.set_ready();
        let flip = readiness.clone();
        let (tx, rx) = tokio::sync::oneshot::channel::<()>();

        let serve_task = tokio::spawn(async move {
            serve_with_shutdown(listener, router, Duration::from_secs(30), async move {
                let _ = rx.await;
                flip.begin_draining();
            })
            .await
        });

        // Give the server a moment to start accepting, then trigger shutdown.
        tokio::time::sleep(Duration::from_millis(50)).await;
        tx.send(()).unwrap();

        let result = tokio::time::timeout(Duration::from_secs(5), serve_task)
            .await
            .expect("serve should return promptly with no in-flight traffic")
            .expect("serve task panicked");
        assert!(result.is_ok(), "serve returned an error: {result:?}");
        assert!(
            !readiness.is_ready(),
            "readiness should be draining (Req 14.4)"
        );
    }

    /// A request still in flight past the grace period is force-closed and
    /// `serve` returns bounded by the grace period rather than the handler's
    /// (much longer) work (Reqs 14.2, 14.3).
    #[tokio::test]
    async fn serve_force_closes_in_flight_requests_after_grace() {
        let listener = bind_listener(loopback(0)).await.unwrap();
        let addr = listener.local_addr().unwrap();
        // A handler that sleeps far longer than the grace period.
        let router = Router::new().route(
            "/slow",
            get(|| async {
                tokio::time::sleep(Duration::from_secs(30)).await;
                "done"
            }),
        );

        let (tx, rx) = tokio::sync::oneshot::channel::<()>();
        let grace = Duration::from_millis(300);
        let serve_task = tokio::spawn(async move {
            serve_with_shutdown(listener, router, grace, async move {
                let _ = rx.await;
            })
            .await
        });

        // Open a connection and send a request that the handler will hold open.
        let mut stream = TcpStream::connect(addr).await.unwrap();
        stream
            .write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
            .await
            .unwrap();

        // Let the request reach the handler, then trigger shutdown.
        tokio::time::sleep(Duration::from_millis(100)).await;
        let started = Instant::now();
        tx.send(()).unwrap();

        let result = tokio::time::timeout(Duration::from_secs(5), serve_task)
            .await
            .expect("serve should return within the grace bound, not the handler's 30s")
            .expect("serve task panicked");
        assert!(result.is_ok(), "serve returned an error: {result:?}");

        let elapsed = started.elapsed();
        assert!(
            elapsed < Duration::from_secs(5),
            "serve should force-close after ~grace, took {elapsed:?}"
        );

        // The in-flight connection is force-closed without a completed response:
        // the read finishes without yielding a full `HTTP/1.1 200` body.
        let mut buf = Vec::new();
        let read = tokio::time::timeout(Duration::from_secs(2), stream.read_to_end(&mut buf)).await;
        if let Ok(Ok(_)) = read {
            let body = String::from_utf8_lossy(&buf);
            assert!(
                !body.contains("done"),
                "the slow handler must not have completed its response: {body}"
            );
        }
    }
}
