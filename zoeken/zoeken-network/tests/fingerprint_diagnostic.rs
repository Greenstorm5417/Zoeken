use zoeken_network::{NetworkManager, NetworkRequest};
use zoeken_settings::OutgoingSettings;

#[tokio::test]
#[ignore = "makes a real outbound request to a fingerprinting service"]
async fn print_ddg_style_headers() {
    use wreq::header::{HeaderMap, HeaderName, HeaderValue};

    let networks = NetworkManager::from_settings(&OutgoingSettings::default()).expect("networks");

    let mut headers = HeaderMap::new();
    for (name, value) in [
        ("Sec-Fetch-Dest", "document"),
        ("Sec-Fetch-Mode", "navigate"),
        ("Sec-Fetch-Site", "same-origin"),
        ("Sec-Fetch-User", "?1"),
        ("Content-Type", "application/x-www-form-urlencoded"),
        ("Referer", "https://html.duckduckgo.com/"),
    ] {
        headers.insert(
            HeaderName::from_bytes(name.as_bytes()).unwrap(),
            HeaderValue::from_str(value).unwrap(),
        );
    }

    let req = NetworkRequest::post("https://tls.peet.ws/api/all").with_headers(headers);
    match networks.request("default", req).await {
        Ok(resp) => {
            let status = resp.status().as_u16();
            let body = resp.text().await.unwrap_or_default();
            println!("\n===== DDG-style POST (HTTP {status}) =====\n{body}\n");
        }
        Err(err) => println!("\n===== DDG-style POST ERROR: {err} =====\n"),
    }
}

#[tokio::test]
#[ignore = "makes a real outbound request to a fingerprinting service"]
async fn print_our_fingerprint() {
    let networks = NetworkManager::from_settings(&OutgoingSettings::default()).expect("networks");

    for attempt in 1..=3 {
        let req = NetworkRequest::get("https://tls.peet.ws/api/all");
        match networks.request("default", req).await {
            Ok(resp) => {
                let status = resp.status().as_u16();
                let body = resp.text().await.unwrap_or_default();
                println!("\n===== attempt {attempt} (HTTP {status}) =====\n{body}\n");
            }
            Err(err) => println!("\n===== attempt {attempt} ERROR: {err} =====\n"),
        }
    }
}
