use std::net::SocketAddr;

use proptest::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::runtime::Runtime;
use zoeken_network::{Network, NetworkConfig, NetworkError, NetworkRequest};

const SPECIAL: [u16; 5] = [401, 402, 403, 429, 503];

fn is_access_ratelimit_or_captcha(outcome: &Result<(), NetworkError>) -> bool {
    matches!(
        outcome,
        Err(NetworkError::AccessDenied { .. })
            | Err(NetworkError::CloudflareAccessDenied { .. })
            | Err(NetworkError::TooManyRequests { .. })
            | Err(NetworkError::Captcha { .. })
            | Err(NetworkError::CloudflareCaptcha { .. })
            | Err(NetworkError::RecaptchaCaptcha { .. })
    )
}

async fn serve_one_status(listener: TcpListener, status: u16) {
    if let Ok((mut sock, _)) = listener.accept().await {
        let mut buf = [0u8; 2048];
        let _ = sock.read(&mut buf).await;

        let response =
            format!("HTTP/1.1 {status} S\r\nContent-Length: 0\r\nConnection: close\r\n\r\n");
        let _ = sock.write_all(response.as_bytes()).await;
        let _ = sock.flush().await;
    }
}

async fn request_status(status: u16) -> Result<(), NetworkError> {
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .expect("bind ephemeral port");
    let addr: SocketAddr = listener.local_addr().expect("local addr");

    let server = tokio::spawn(serve_one_status(listener, status));

    let network = Network::build("test", NetworkConfig::default()).expect("build network");
    let req = NetworkRequest::get(format!("http://{addr}/"));
    let outcome = network.request("test", req).await;

    server.abort();

    outcome.map(|_response| ())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(128))]

    #[test]
    #[ignore = "hangs intermittently on Windows during local HTTP status mapping checks"]
    fn status_mapping_matches_special_set(
        status in prop_oneof![
            prop::sample::select(SPECIAL.to_vec()),
            200u16..=599u16,
        ],
    ) {
        thread_local! {
            static RT: Runtime = Runtime::new().expect("tokio runtime");
        }

        let outcome = RT.with(|rt| rt.block_on(request_status(status)));

        let mapped = is_access_ratelimit_or_captcha(&outcome);
        let expected = SPECIAL.contains(&status);

        prop_assert_eq!(
            mapped,
            expected,
            "status {} mapped-to-special={} but expected {} (outcome: {:?})",
            status,
            mapped,
            expected,
            outcome.as_ref().err()
        );
    }
}
