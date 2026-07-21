use std::net::{IpAddr, Ipv4Addr};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use proptest::prelude::*;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use zoeken_network::{EmulationProfile, Network, NetworkConfig, NetworkRequest};

async fn handle_conn(mut stream: TcpStream) {
    let mut buf = Vec::new();
    let mut tmp = [0u8; 1024];
    loop {
        match stream.read(&mut tmp).await {
            Ok(0) => break,
            Ok(read) => {
                buf.extend_from_slice(&tmp[..read]);
                if buf.windows(4).any(|w| w == b"\r\n\r\n") {
                    break;
                }
            }
            Err(_) => break,
        }
    }
    let resp = b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok";
    let _ = stream.write_all(resp).await;
    let _ = stream.flush().await;
    let _ = stream.shutdown().await;
}

async fn observe_origin_mapping(addrs: &[IpAddr]) -> Vec<IpAddr> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0))
        .await
        .expect("bind loopback listener");
    let local = listener.local_addr().expect("listener addr");

    let recorded: Arc<Mutex<Vec<IpAddr>>> = Arc::new(Mutex::new(Vec::new()));
    let recorded_srv = Arc::clone(&recorded);
    let server = tokio::spawn(async move {
        while let Ok((stream, peer)) = listener.accept().await {
            recorded_srv.lock().unwrap().push(peer.ip());
            tokio::spawn(handle_conn(stream));
        }
    });

    let cfg = NetworkConfig {
        local_addresses: addrs.to_vec(),
        enable_http2: false,
        timeout: Duration::from_secs(5),
        emulation: EmulationProfile::chrome(),
        ..Default::default()
    };
    let net = Network::build("test", cfg).expect("build network");

    let url = format!("http://{local}/");
    for _ in 0..addrs.len() {
        net.request("test", NetworkRequest::get(url.clone()))
            .await
            .expect("request should succeed against loopback server");
    }

    server.abort();
    recorded.lock().unwrap().clone()
}

fn distinct_source_addrs() -> impl Strategy<Value = Vec<IpAddr>> {
    proptest::collection::hash_set(2u8..=254u8, 1..=6).prop_map(|octets| {
        octets
            .into_iter()
            .map(|o| IpAddr::V4(Ipv4Addr::new(127, 0, 0, o)))
            .collect::<Vec<_>>()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn source_address_is_stable_for_one_origin(addrs in distinct_source_addrs()) {
        let rt = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(1)
            .enable_all()
            .build()
            .expect("build tokio runtime");
        let observed = rt.block_on(observe_origin_mapping(&addrs));

        prop_assert_eq!(observed.len(), addrs.len());
        prop_assert!(addrs.contains(&observed[0]));
        prop_assert!(observed.iter().all(|address| address == &observed[0]));
    }
}
