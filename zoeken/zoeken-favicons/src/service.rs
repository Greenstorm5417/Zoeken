//! Favicon service: orchestrates resolution and caching with injectable backends.

use std::sync::Arc;

use crate::cache::{CacheLookup, Favicon, FaviconCache};
use crate::resolver::FaviconResolver;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaviconOutcome {
    Serve(Favicon),
    Fallback,
}

impl FaviconOutcome {
    /// The favicon to serve, if any.
    pub fn favicon(&self) -> Option<&Favicon> {
        match self {
            FaviconOutcome::Serve(f) => Some(f),
            FaviconOutcome::Fallback => None,
        }
    }

    /// Whether this outcome is the fallback.
    pub fn is_fallback(&self) -> bool {
        matches!(self, FaviconOutcome::Fallback)
    }
}

/// Resolves, caches, and serves favicons with injectable backends.
pub struct FaviconService<C: FaviconCache> {
    resolver: Arc<dyn FaviconResolver>,
    cache: C,
}

impl<C: FaviconCache> FaviconService<C> {
    /// Build a service over the given `resolver` and `cache`.
    pub fn new(resolver: Arc<dyn FaviconResolver>, cache: C) -> Self {
        Self { resolver, cache }
    }

    /// The configured resolver's name (also the cache-key namespace).
    pub fn resolver_name(&self) -> &str {
        self.resolver.name()
    }

    /// Borrow the underlying cache (useful for inspection in tests).
    pub fn cache(&self) -> &C {
        &self.cache
    }

    /// Resolve favicon for authority using cache hits/misses and fallback on failure.
    pub async fn get_favicon(&self, authority: &str) -> FaviconOutcome {
        let resolver = self.resolver.name();

        match self.cache.get(resolver, authority) {
            CacheLookup::Hit(favicon) => FaviconOutcome::Serve(favicon), // 12.1 / 12.3
            CacheLookup::KnownMissing => FaviconOutcome::Fallback,
            CacheLookup::Absent => self.resolve_and_cache(resolver, authority).await,
        }
    }

    async fn resolve_and_cache(&self, resolver: &str, authority: &str) -> FaviconOutcome {
        match self.resolver.resolve(authority).await {
            Ok(Some(favicon)) => {
                self.cache.set(resolver, authority, Some(&favicon));
                FaviconOutcome::Serve(favicon)
            }
            Ok(None) => {
                self.cache.set(resolver, authority, None);
                FaviconOutcome::Fallback
            }
            Err(_) => match self.cache.get(resolver, authority) {
                CacheLookup::Hit(favicon) => FaviconOutcome::Serve(favicon),
                _ => FaviconOutcome::Fallback,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{FaviconCache, InMemoryFaviconCache, SqliteFaviconCache};
    use crate::resolver::{FaviconResolver, ResolveError, ResolveFuture, StaticResolver};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn png(tag: u8) -> Favicon {
        Favicon::new(vec![tag; 16], "image/png")
    }

    #[tokio::test]
    async fn cache_hit_returns_cached_favicon() {
        let cache = InMemoryFaviconCache::new();
        let resolver = Arc::new(StaticResolver::failing("stub", "should not be called"));
        cache.set("stub", "example.com", Some(&png(1)));

        let service = FaviconService::new(resolver, cache);
        let outcome = service.get_favicon("example.com").await;

        assert_eq!(outcome, FaviconOutcome::Serve(png(1)));
    }

    #[tokio::test]
    async fn cache_miss_resolves_then_stores() {
        let cache = InMemoryFaviconCache::new();
        let resolver = Arc::new(StaticResolver::serving("stub", png(7)));
        let service = FaviconService::new(resolver, cache);

        let outcome = service.get_favicon("example.org").await;
        assert_eq!(outcome, FaviconOutcome::Serve(png(7)));

        assert_eq!(
            service.cache().get("stub", "example.org"),
            CacheLookup::Hit(png(7))
        );
    }

    #[tokio::test]
    async fn resolution_failure_with_cache_returns_cached() {
        struct AppearingCache {
            favicon: Favicon,
            gets: AtomicUsize,
        }
        impl FaviconCache for AppearingCache {
            fn get(&self, _resolver: &str, _authority: &str) -> CacheLookup {
                if self.gets.fetch_add(1, Ordering::SeqCst) == 0 {
                    CacheLookup::Absent
                } else {
                    CacheLookup::Hit(self.favicon.clone())
                }
            }
            fn set(&self, _resolver: &str, _authority: &str, _favicon: Option<&Favicon>) -> bool {
                true
            }
        }

        let cache = AppearingCache {
            favicon: png(3),
            gets: AtomicUsize::new(0),
        };
        let resolver = Arc::new(StaticResolver::failing("stub", "boom"));
        let service = FaviconService::new(resolver, cache);

        let outcome = service.get_favicon("example.net").await;
        assert_eq!(outcome, FaviconOutcome::Serve(png(3)));
    }

    #[tokio::test]
    async fn unresolved_and_uncached_returns_fallback_and_does_not_cache_failure() {
        let cache = InMemoryFaviconCache::new();
        let resolver = Arc::new(StaticResolver::failing("stub", "boom"));
        let service = FaviconService::new(resolver, cache);

        let outcome = service.get_favicon("missing.example").await;
        assert_eq!(outcome, FaviconOutcome::Fallback);

        assert_eq!(
            service.cache().get("stub", "missing.example"),
            CacheLookup::Absent
        );
    }

    #[tokio::test]
    async fn definitive_no_favicon_caches_known_missing() {
        struct CountingEmpty {
            calls: AtomicUsize,
        }
        impl FaviconResolver for CountingEmpty {
            fn name(&self) -> &str {
                "counting"
            }
            fn resolve<'a>(&'a self, _authority: &'a str) -> ResolveFuture<'a> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok::<Option<Favicon>, ResolveError>(None) })
            }
        }

        let resolver = Arc::new(CountingEmpty {
            calls: AtomicUsize::new(0),
        });
        let calls = resolver.clone();
        let cache = InMemoryFaviconCache::new();
        let service = FaviconService::new(resolver, cache);

        assert_eq!(
            service.get_favicon("none.example").await,
            FaviconOutcome::Fallback
        );
        assert_eq!(
            service.get_favicon("none.example").await,
            FaviconOutcome::Fallback
        );
        assert_eq!(calls.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn sqlite_backed_miss_then_hit() {
        let cache = SqliteFaviconCache::in_memory().expect("open in-memory cache");
        let resolver = Arc::new(StaticResolver::serving("stub", png(9)));
        let service = FaviconService::new(resolver, cache);

        assert_eq!(
            service.get_favicon("sqlite.example").await,
            FaviconOutcome::Serve(png(9))
        );
        assert_eq!(
            service.cache().get("stub", "sqlite.example"),
            CacheLookup::Hit(png(9))
        );
    }
}
