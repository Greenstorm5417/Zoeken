//! Shared SSRF-safe outbound GET used by favicon resolution and `/image_proxy`.

use std::sync::Arc;
use std::time::Duration;

use futures_util::StreamExt;
use zoeken_network::{NetworkManager, NetworkRequest};

/// Browser-like `Accept` header for image fetches.
pub const IMAGE_ACCEPT: &str = "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";

/// Redirect hop budget for proxied image/favicon fetches.
pub const MAX_REDIRECT_HOPS: usize = 4;

/// Response body from a safe outbound GET (status, headers subset, capped body).
#[derive(Debug, Clone)]
pub struct SafeOutboundBody {
    pub status: u16,
    pub content_type: Option<String>,
    pub content_length: Option<u64>,
    pub body: Vec<u8>,
}

/// GET `url` with `client`, following up to `max_hops` redirects manually so
/// that every hop (not just the first URL) passes the SSRF policy in
/// [`crate::validate_proxy_url`] and a DNS resolve-time IP check via
/// [`crate::validate_resolved_url`]. Client-level redirect following is disabled
/// per request; a redirect without a usable `Location` is returned as-is.
pub async fn get_following_safe_redirects(
    client: &wreq::Client,
    url: &str,
    max_hops: usize,
) -> Result<wreq::Response, String> {
    let mut current = url.to_string();
    for _ in 0..=max_hops {
        crate::validate_resolved_url(&current)
            .await
            .map_err(|rejection| rejection.reason().to_string())?;
        let resp = client
            .get(&current)
            .redirect(wreq::redirect::Policy::none())
            .header(http::header::ACCEPT, IMAGE_ACCEPT)
            .send()
            .await
            .map_err(|e| e.to_string())?;
        if !resp.status().is_redirection() {
            return Ok(resp);
        }
        let Some(location) = resp
            .headers()
            .get(http::header::LOCATION)
            .and_then(|v| v.to_str().ok())
        else {
            return Ok(resp);
        };
        current = join_redirect(&current, location)?;
    }
    Err("too many redirects".to_string())
}

/// Same hop-by-hop SSRF redirect loop as [`get_following_safe_redirects`], but
/// each GET goes through a coordinated [`NetworkManager`] origin.
pub async fn get_following_safe_redirects_coordinated(
    network: &NetworkManager,
    network_name: &str,
    url: &str,
    max_hops: usize,
    timeout: Duration,
) -> Result<wreq::Response, String> {
    let mut current = url.to_string();
    for _ in 0..=max_hops {
        crate::validate_resolved_url(&current)
            .await
            .map_err(|rejection| rejection.reason().to_string())?;
        let mut headers = http::HeaderMap::new();
        headers.insert(
            http::header::ACCEPT,
            http::HeaderValue::from_static(IMAGE_ACCEPT),
        );
        let response = network
            .request(
                network_name,
                NetworkRequest::get(&current)
                    .with_headers(headers)
                    .with_max_redirects(0)
                    .with_timeout(timeout),
            )
            .await
            .map_err(|error| error.to_string())?;
        if !response.status().is_redirection() {
            return Ok(response);
        }
        let Some(location) = response
            .headers()
            .get(http::header::LOCATION)
            .and_then(|value| value.to_str().ok())
        else {
            return Ok(response);
        };
        current = join_redirect(&current, location)?;
    }
    Err("too many redirects".to_string())
}

/// SSRF-safe GET via a direct client, reading at most `max_bytes` of body.
pub async fn safe_outbound_get(
    client: &wreq::Client,
    url: &str,
    max_hops: usize,
    max_bytes: usize,
) -> Result<SafeOutboundBody, String> {
    let resp = get_following_safe_redirects(client, url, max_hops).await?;
    read_capped(resp, max_bytes).await
}

/// SSRF-safe GET via a coordinated network, reading at most `max_bytes` of body.
pub async fn safe_outbound_get_coordinated(
    network: &NetworkManager,
    network_name: &str,
    url: &str,
    max_hops: usize,
    timeout: Duration,
    max_bytes: usize,
) -> Result<SafeOutboundBody, String> {
    let resp =
        get_following_safe_redirects_coordinated(network, network_name, url, max_hops, timeout)
            .await?;
    read_capped(resp, max_bytes).await
}

/// Transport used by production favicon/image fetchers.
#[derive(Clone)]
pub enum SafeOutboundTransport {
    Direct(wreq::Client),
    Coordinated {
        network: Arc<NetworkManager>,
        network_name: &'static str,
        timeout: Duration,
    },
}

impl SafeOutboundTransport {
    pub async fn get(&self, url: &str, max_bytes: usize) -> Result<SafeOutboundBody, String> {
        match self {
            Self::Direct(client) => {
                safe_outbound_get(client, url, MAX_REDIRECT_HOPS, max_bytes).await
            }
            Self::Coordinated {
                network,
                network_name,
                timeout,
            } => {
                safe_outbound_get_coordinated(
                    network,
                    network_name,
                    url,
                    MAX_REDIRECT_HOPS,
                    *timeout,
                    max_bytes,
                )
                .await
            }
        }
    }
}

fn join_redirect(current: &str, location: &str) -> Result<String, String> {
    url::Url::parse(current)
        .ok()
        .and_then(|base| base.join(location).ok())
        .map(String::from)
        .ok_or_else(|| "invalid redirect location".to_string())
}

async fn read_capped(resp: wreq::Response, max_bytes: usize) -> Result<SafeOutboundBody, String> {
    let status = resp.status().as_u16();
    let content_type = resp
        .headers()
        .get(http::header::CONTENT_TYPE)
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string());
    let content_length = resp
        .headers()
        .get(http::header::CONTENT_LENGTH)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.trim().parse::<u64>().ok());

    if let Some(declared) = content_length
        && declared > max_bytes as u64
    {
        return Err("response exceeds size limit".to_string());
    }

    let mut body = Vec::new();
    let mut stream = resp.bytes_stream();
    while let Some(chunk) = stream.next().await {
        let chunk = chunk.map_err(|e| e.to_string())?;
        if body.len().saturating_add(chunk.len()) > max_bytes {
            return Err("response exceeds size limit".to_string());
        }
        body.extend_from_slice(&chunk);
    }

    Ok(SafeOutboundBody {
        status,
        content_type,
        content_length: content_length.or(Some(body.len() as u64)),
        body,
    })
}
