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

/// Fetches `https://{authority}/favicon.ico` (shortest network path).
#[derive(Debug)]
pub struct HttpFaviconResolver {
    provider: String,
}

impl HttpFaviconResolver {
    #[must_use]
    pub fn new() -> Self {
        Self {
            provider: "http".to_string(),
        }
    }

    #[must_use]
    pub fn for_provider(provider: &str) -> Self {
        Self {
            provider: provider.to_string(),
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
            let client = wreq::Client::builder()
                .redirect(wreq::redirect::Policy::none())
                .build()
                .map_err(|e| ResolveError::Upstream(e.to_string()))?;
            let resp = client
                .get(&url)
                .send()
                .await
                .map_err(|e| ResolveError::Upstream(e.to_string()))?;
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
