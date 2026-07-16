use std::net::{Ipv4Addr, SocketAddr};
use std::time::{Duration, Instant};

use axum::Router;
use axum::routing::get;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream;
use zoeken_server::readiness::ReadinessState;
use zoeken_server::serve::{ServeError, bind_listener, serve_with_shutdown};

fn loopback(port: u16) -> SocketAddr {
    SocketAddr::from((Ipv4Addr::LOCALHOST, port))
}

async fn read_response(mut stream: TcpStream, timeout: Duration) -> String {
    let mut buf = Vec::new();
    let _ = tokio::time::timeout(timeout, stream.read_to_end(&mut buf)).await;
    String::from_utf8_lossy(&buf).into_owned()
}

#[tokio::test]
async fn in_flight_request_drains_within_grace_period() {
    let listener = bind_listener(loopback(0)).await.expect("bind ephemeral");
    let addr = listener.local_addr().unwrap();

    let router = Router::new().route(
        "/drain",
        get(|| async {
            tokio::time::sleep(Duration::from_millis(200)).await;
            "drained-ok"
        }),
    );

    let grace = Duration::from_secs(10);

    let readiness = ReadinessState::new_not_ready();
    readiness.set_ready();
    let flip = readiness.clone();
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    let serve_task = tokio::spawn(async move {
        serve_with_shutdown(listener, router, grace, async move {
            let _ = rx.await;
            flip.begin_draining();
        })
        .await
    });

    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream
        .write_all(b"GET /drain HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("write request");

    tokio::time::sleep(Duration::from_millis(50)).await;
    let started = Instant::now();
    tx.send(()).expect("trigger shutdown");

    let response = read_response(stream, Duration::from_secs(5)).await;
    assert!(
        response.contains("200"),
        "the in-flight request should complete with a 200 response: {response:?}"
    );
    assert!(
        response.contains("drained-ok"),
        "the drained request should receive its full body: {response:?}"
    );

    let result = tokio::time::timeout(Duration::from_secs(5), serve_task)
        .await
        .expect("serve should return after the drain, not wait out the full grace")
        .expect("serve task panicked");
    assert!(result.is_ok(), "serve returned an error: {result:?}");

    let elapsed = started.elapsed();
    assert!(
        elapsed < grace,
        "serve should return when the drain completes, well within grace; took {elapsed:?}"
    );

    assert!(!readiness.is_ready(), "readiness should be draining");
}

#[tokio::test]
async fn remaining_connections_are_cut_after_grace_period() {
    let listener = bind_listener(loopback(0)).await.expect("bind ephemeral");
    let addr = listener.local_addr().unwrap();

    let router = Router::new().route(
        "/slow",
        get(|| async {
            tokio::time::sleep(Duration::from_secs(30)).await;
            "should-never-be-sent"
        }),
    );

    let grace = Duration::from_millis(300);
    let (tx, rx) = tokio::sync::oneshot::channel::<()>();

    let serve_task = tokio::spawn(async move {
        serve_with_shutdown(listener, router, grace, async move {
            let _ = rx.await;
        })
        .await
    });

    let mut stream = TcpStream::connect(addr).await.expect("connect");
    stream
        .write_all(b"GET /slow HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n")
        .await
        .expect("write request");

    tokio::time::sleep(Duration::from_millis(100)).await;
    let started = Instant::now();
    tx.send(()).expect("trigger shutdown");

    let result = tokio::time::timeout(Duration::from_secs(5), serve_task)
        .await
        .expect("serve should force-close after ~grace, not wait for the 30s handler")
        .expect("serve task panicked");
    assert!(result.is_ok(), "serve returned an error: {result:?}");

    let elapsed = started.elapsed();
    assert!(
        elapsed < Duration::from_secs(5),
        "serve should return shortly after the grace period; took {elapsed:?}"
    );

    let response = read_response(stream, Duration::from_secs(2)).await;
    assert!(
        !response.contains("should-never-be-sent"),
        "the force-closed request must not have completed its response: {response:?}"
    );
}

#[tokio::test]
async fn unbindable_address_reports_address_port_and_cause() {
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
            assert_eq!(
                *reported, addr,
                "the error should carry the offending address"
            );
            assert_eq!(
                source.kind(),
                std::io::ErrorKind::AddrInUse,
                "the underlying cause should be the OS AddrInUse error"
            );
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
    let source = std::error::Error::source(&error).expect("Bind error exposes its cause");
    assert!(
        message.contains(&source.to_string()) || !source.to_string().is_empty(),
        "message should surface the underlying cause: {message}"
    );
}
