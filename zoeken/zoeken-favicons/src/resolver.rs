//! Favicon resolver trait, static stub, and a simple HTTP `/favicon.ico` resolver.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use zoeken_network::NetworkManager;

use crate::safe_outbound::SafeOutboundTransport;
use crate::service::Favicon;

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
    Missing,
    Fail(String),
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
            outcome: StaticOutcome::Missing,
        }
    }

    /// A resolver that always fails.
    pub fn failing(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            outcome: StaticOutcome::Fail(reason.into()),
        }
    }
}

impl FaviconResolver for StaticResolver {
    fn name(&self) -> &str {
        &self.name
    }

    fn resolve<'a>(&'a self, _authority: &'a str) -> ResolveFuture<'a> {
        Box::pin(async move {
            match &self.outcome {
                StaticOutcome::Favicon(favicon) => Ok(Some(favicon.clone())),
                StaticOutcome::Missing => Ok(None),
                StaticOutcome::Fail(reason) => Err(ResolveError::Upstream(reason.clone())),
            }
        })
    }
}

/// Cap favicon payloads (typical icons are tiny; reject pathological bodies).
const MAX_FAVICON_BYTES: usize = 1024 * 1024;

/// Fetches `https://{authority}/favicon.ico` (shortest network path).
pub struct HttpFaviconResolver {
    provider: String,
    transport: SafeOutboundTransport,
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
            .timeout(Duration::from_secs(10))
            .build()
            .expect("build favicon HTTP client");
        Self {
            provider: provider.to_string(),
            transport: SafeOutboundTransport::Direct(client),
        }
    }

    /// Build a production resolver whose every hop uses shared origin
    /// coordination. Redirect targets are still SSRF-validated individually.
    #[must_use]
    pub fn for_provider_with_network(provider: &str, network: Arc<NetworkManager>) -> Self {
        Self {
            provider: provider.to_string(),
            transport: SafeOutboundTransport::Coordinated {
                network,
                network_name: "favicon",
                timeout: Duration::from_secs(10),
            },
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
            let fetched = self
                .transport
                .get(&url, MAX_FAVICON_BYTES)
                .await
                .map_err(ResolveError::Upstream)?;
            if fetched.status != 200 {
                return Ok(None);
            }
            if fetched.body.is_empty() {
                return Ok(None);
            }
            let mime = fetched
                .content_type
                .unwrap_or_else(|| "image/x-icon".to_string());
            Ok(Some(Favicon::new(fetched.body, mime)))
        })
    }
}
