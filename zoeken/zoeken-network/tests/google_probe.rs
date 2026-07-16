use wreq::header::{HeaderMap, HeaderName, HeaderValue};
use zoeken_network::{NetworkManager, NetworkRequest};
use zoeken_settings::OutgoingSettings;

#[tokio::test]
#[ignore = "makes a real outbound request to google"]
async fn probe_google_layout() {
    let networks = NetworkManager::from_settings(&OutgoingSettings::default()).expect("networks");

    let mut headers = HeaderMap::new();
    headers.insert(
        HeaderName::from_static("accept"),
        HeaderValue::from_static("*/*"),
    );

    let url = "https://www.google.com/search?q=rust&hl=en&lr=&cr=&ie=utf8&oe=utf8&filter=0&start=0";
    let req = NetworkRequest::get(url)
        .with_headers(headers)
        .with_cookies(vec![("CONSENT".to_string(), "YES+".to_string())]);

    let resp = networks.request("default", req).await.expect("request");
    let status = resp.status().as_u16();
    let body = resp.text().await.unwrap_or_default();

    let markers = [
        "data-ved",
        "ilUpNd H66NU aSRlid",
        "consent.google.com",
        "sorry/index",
        "captcha",
        "id=\"search\"",
        "id=\"rso\"",
        "class=\"g ",
        "jscontroller",
        "/url?q=",
    ];
    println!(
        "\n=== google probe: HTTP {status}, body {} bytes ===",
        body.len()
    );
    for m in markers {
        println!("{:<24} count={}", m, body.matches(m).count());
    }
    println!(
        "--- first 600 chars ---\n{}",
        body.chars().take(600).collect::<String>()
    );
}
