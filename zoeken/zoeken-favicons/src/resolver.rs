//! Favicon resolver trait, static stub, and a simple HTTP `/favicon.ico` resolver.

use std::future::Future;
use std::pin::Pin;

use futures_util::StreamExt;

use crate::cache::Favicon;

#[derive(Debug, thiserror::Error)]
pub enum ResolveError {
    #[error("favicon resolution failed: {0}")]
    Upstream(String),
}

pub type ResolveFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<Favicon>, ResolveError>> + Send + 'a>>;

/// Backend that fetches favicons for an authority.
pub trait FaviconResolver: Send + Sync {
    /// Resolver name (used as cache key namespace).
    fn name(&self) -> &str;

    fn resolve<'a>(&'a self, authority: &'a str) -> ResolveFuture<'a>;
}

/// Stub resolver for testing: returns fixed outcomes without network I/O.
#[derive(Debug, Clone)]
pub struct StaticResolver {
    name: String,
    outcome: StaticOutcome,
}

#[derive(Debug, Clone)]
enum StaticOutcome {
    Favicon(Favicon),
    None,
    Error(String),
}

impl StaticResolver {
    /// A resolver that always resolves to `favicon`.
    pub fn serving(name: impl Into<String>, favicon: Favicon) -> Self {
        Self {
            name: name.into(),
            outcome: StaticOutcome::Favicon(favicon),
        }
    }

    /// A resolver that always resolves to *no favicon* (a definitive negative).
    pub fn empty(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            outcome: StaticOutcome::None,
        }
    }

    /// A resolver whose every attempt fails.
    pub fn failing(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            outcome: StaticOutcome::Error(message.into()),
        }
    }
}

impl FaviconResolver for StaticResolver {
    fn name(&self) -> &str {
        &self.name
    }

    fn resolve<'a>(&'a self, _authority: &'a str) -> ResolveFuture<'a> {
        let outcome = self.outcome.clone();
        Box::pin(async move {
            match outcome {
                StaticOutcome::Favicon(f) => Ok(Some(f)),
                StaticOutcome::None => Ok(None),
                StaticOutcome::Error(m) => Err(ResolveError::Upstream(m)),
            }
        })
    }
}

/// Browser-like `Accept` header for image fetches.
pub const IMAGE_ACCEPT: &str = "image/avif,image/webp,image/apng,image/svg+xml,image/*,*/*;q=0.8";

/// Redirect hop budget for proxied image/favicon fetches.
pub const MAX_REDIRECT_HOPS: usize = 4;

/// GET `url` with `client`, following up to `max_hops` redirects manually so
/// that every hop (not just the first URL) passes the SSRF policy in
/// [`crate::validate_proxy_url`]. Client-level redirect following is disabled
/// per request; a redirect without a usable `Location` is returned as-is.
pub async fn get_following_safe_redirects(
    client: &wreq::Client,
    url: &str,
    max_hops: usize,
) -> Result<wreq::Response, String> {
    let mut current = url.to_string();
    for _ in 0..=max_hops {
        crate::validate_proxy_url(&current).map_err(|rejection| rejection.reason().to_string())?;
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
        current = url::Url::parse(&current)
            .ok()
            .and_then(|base| base.join(location).ok())
            .map(String::from)
            .ok_or_else(|| "invalid redirect location".to_string())?;
    }
    Err("too many redirects".to_string())
}

/// Fetches `https://{authority}/favicon.ico` (shortest network path).
pub struct HttpFaviconResolver {
    provider: String,
    client: wreq::Client,
}

impl std::fmt::Debug for HttpFaviconResolver {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HttpFaviconResolver")
            .field("provider", &self.provider)
            .finish_non_exhaustive()
    }
}

impl HttpFaviconResolver {
    #[must_use]
    pub fn new() -> Self {
        Self::for_provider("http")
    }

    #[must_use]
    pub fn for_provider(provider: &str) -> Self {
        // One shared pooled client per resolver: rebuilding a TLS client for
        // every favicon fetch dominated resolution latency.
        let client = wreq::Client::builder()
            .redirect(wreq::redirect::Policy::none())
            .timeout(std::time::Duration::from_secs(10))
            .build()
            .expect("build favicon HTTP client");
        Self {
            provider: provider.to_string(),
            client,
        }
    }

    fn url(&self, authority: &str) -> String {
        match self.provider.as_str() {
            "duckduckgo" => format!("https://icons.duckduckgo.com/ip3/{authority}.ico"),
            "google" => {
                let query = url::form_urlencoded::Serializer::new(String::new())
                    .append_pair("domain", authority)
                    .append_pair("sz", "32")
                    .finish();
                format!("https://www.google.com/s2/favicons?{query}")
            }
            "yandex" => format!("https://favicon.yandex.net/favicon/{authority}"),
            "allesedv" => format!("https://f1.allesedv.com/32/{authority}"),
            _ => format!("https://{authority}/favicon.ico"),
        }
    }
}

impl Default for HttpFaviconResolver {
    fn default() -> Self {
        Self::new()
    }
}

impl FaviconResolver for HttpFaviconResolver {
    fn name(&self) -> &str {
        &self.provider
    }

    fn resolve<'a>(&'a self, authority: &'a str) -> ResolveFuture<'a> {
        Box::pin(async move {
            if authority.is_empty() || authority.contains('/') {
                return Err(ResolveError::Upstream("invalid authority".into()));
            }
            if crate::validate_proxy_authority(authority).is_err() {
                return Err(ResolveError::Upstream("disallowed authority".into()));
            }
            let url = self.url(authority);
            let resp = get_following_safe_redirects(&self.client, &url, MAX_REDIRECT_HOPS)
                .await
                .map_err(ResolveError::Upstream)?;
            if resp.status().as_u16() != 200 {
                return Ok(None);
            }
            let mime = resp
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .unwrap_or("image/x-icon")
                .to_string();
            // Cap favicon payloads (typical icons are tiny; reject pathological bodies).
            const MAX_FAVICON_BYTES: usize = 1024 * 1024;
            let mut data = Vec::new();
            let mut stream = resp.bytes_stream();
            while let Some(chunk) = stream.next().await {
                let chunk = chunk.map_err(|e| ResolveError::Upstream(e.to_string()))?;
                if data.len().saturating_add(chunk.len()) > MAX_FAVICON_BYTES {
                    return Err(ResolveError::Upstream("favicon exceeds size limit".into()));
                }
                data.extend_from_slice(&chunk);
            }
            if data.is_empty() {
                return Ok(None);
            }
            Ok(Some(Favicon::new(data, mime)))
        })
    }
}
