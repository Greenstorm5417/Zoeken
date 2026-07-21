//! Favicon service: orchestrates resolution and caching with injectable backends.

use std::collections::HashMap;
use std::sync::Arc;

use crate::cache::{CacheLookup, Favicon, FaviconCache};
use crate::resolver::FaviconResolver;
use async_trait::async_trait;
use tokio::sync::Mutex;
use zoeken_storage::{FaviconData, FaviconLookup, FaviconPolicy, Storage};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaviconOutcome {
    Serve(Favicon),
    Fallback,
}

#[async_trait]
pub trait FaviconProvider: Send + Sync {
    async fn get_favicon(&self, authority: &str) -> FaviconOutcome;
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

        match self.cache.get(resolver, authority).await {
            CacheLookup::Hit(favicon) => FaviconOutcome::Serve(favicon), // 12.1 / 12.3
            CacheLookup::KnownMissing => FaviconOutcome::Fallback,
            CacheLookup::Absent => self.resolve_and_cache(resolver, authority).await,
        }
    }

    async fn resolve_and_cache(&self, resolver: &str, authority: &str) -> FaviconOutcome {
        match self.resolver.resolve(authority).await {
            Ok(Some(favicon)) => {
                self.cache.set(resolver, authority, Some(&favicon)).await;
                FaviconOutcome::Serve(favicon)
            }
            Ok(None) => {
                self.cache.set(resolver, authority, None).await;
                FaviconOutcome::Fallback
            }
            Err(_) => match self.cache.get(resolver, authority).await {
                CacheLookup::Hit(favicon) => FaviconOutcome::Serve(favicon),
                _ => FaviconOutcome::Fallback,
            },
        }
    }
}

#[async_trait]
impl<C: FaviconCache> FaviconProvider for FaviconService<C> {
    async fn get_favicon(&self, authority: &str) -> FaviconOutcome {
        FaviconService::get_favicon(self, authority).await
    }
}

/// Backend-neutral persistent favicon service used by production.
/// Per-key mutexes collapse simultaneous misses without storing request data.
pub struct StorageFaviconService {
    resolver: Arc<dyn FaviconResolver>,
    storage: Arc<dyn Storage>,
    policy: FaviconPolicy,
    in_flight: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl StorageFaviconService {
    pub fn new(
        resolver: Arc<dyn FaviconResolver>,
        storage: Arc<dyn Storage>,
        policy: FaviconPolicy,
    ) -> Self {
        Self {
            resolver,
            storage,
            policy,
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    async fn lookup(&self, authority: &str) -> Result<FaviconLookup, ()> {
        self.storage
            .favicon_get(self.resolver.name(), authority)
            .await
            .map_err(|_| ())
    }

    fn outcome(lookup: FaviconLookup) -> Option<FaviconOutcome> {
        match lookup {
            FaviconLookup::Hit(favicon) => Some(FaviconOutcome::Serve(Favicon {
                data: favicon.data,
                mime: favicon.mime,
            })),
            FaviconLookup::KnownMissing => Some(FaviconOutcome::Fallback),
            FaviconLookup::Absent => None,
        }
    }
}

#[async_trait]
impl FaviconProvider for StorageFaviconService {
    async fn get_favicon(&self, authority: &str) -> FaviconOutcome {
        let Ok(lookup) = self.lookup(authority).await else {
            metrics::counter!("storage_operations_total", "operation" => "favicon_get", "outcome" => "error")
                .increment(1);
            return FaviconOutcome::Fallback;
        };
        if let Some(outcome) = Self::outcome(lookup) {
            metrics::counter!("favicon_cache_total", "outcome" => "hit").increment(1);
            return outcome;
        }

        let key_lock = {
            let mut in_flight = self.in_flight.lock().await;
            Arc::clone(
                in_flight
                    .entry(authority.to_string())
                    .or_insert_with(|| Arc::new(Mutex::new(()))),
            )
        };
        let _guard = key_lock.lock().await;

        let Ok(lookup) = self.lookup(authority).await else {
            self.in_flight.lock().await.remove(authority);
            return FaviconOutcome::Fallback;
        };
        if let Some(outcome) = Self::outcome(lookup) {
            metrics::counter!("favicon_singleflight_total", "outcome" => "shared").increment(1);
            return outcome;
        }

        let outcome = match self.resolver.resolve(authority).await {
            Ok(Some(favicon)) => {
                let stored = FaviconData {
                    data: favicon.data.clone(),
                    mime: favicon.mime.clone(),
                };
                if self
                    .storage
                    .favicon_put(self.resolver.name(), authority, Some(&stored), &self.policy)
                    .await
                    .is_err()
                {
                    FaviconOutcome::Fallback
                } else {
                    FaviconOutcome::Serve(favicon)
                }
            }
            Ok(None) => {
                let stored = self
                    .storage
                    .favicon_put(self.resolver.name(), authority, None, &self.policy)
                    .await;
                if stored.is_err() {
                    metrics::counter!("storage_operations_total", "operation" => "favicon_put", "outcome" => "error")
                        .increment(1);
                }
                FaviconOutcome::Fallback
            }
            Err(_) => FaviconOutcome::Fallback,
        };

        self.in_flight.lock().await.remove(authority);
        outcome
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cache::{FaviconCache, InMemoryFaviconCache};
    use crate::resolver::{FaviconResolver, ResolveError, ResolveFuture, StaticResolver};
    use std::sync::atomic::{AtomicUsize, Ordering};

    fn png(tag: u8) -> Favicon {
        Favicon::new(vec![tag; 16], "image/png")
    }

    #[tokio::test]
    async fn cache_hit_returns_cached_favicon() {
        let cache = InMemoryFaviconCache::new();
        let resolver = Arc::new(StaticResolver::failing("stub", "should not be called"));
        cache.set("stub", "example.com", Some(&png(1))).await;

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
            service.cache().get("stub", "example.org").await,
            CacheLookup::Hit(png(7))
        );
    }

    #[tokio::test]
    async fn resolution_failure_with_cache_returns_cached() {
        struct AppearingCache {
            favicon: Favicon,
            gets: AtomicUsize,
        }
        #[async_trait]
        impl FaviconCache for AppearingCache {
            async fn get(&self, _resolver: &str, _authority: &str) -> CacheLookup {
                if self.gets.fetch_add(1, Ordering::SeqCst) == 0 {
                    CacheLookup::Absent
                } else {
                    CacheLookup::Hit(self.favicon.clone())
                }
            }
            async fn set(
                &self,
                _resolver: &str,
                _authority: &str,
                _favicon: Option<&Favicon>,
            ) -> bool {
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
            service.cache().get("stub", "missing.example").await,
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
    async fn persistent_cache_misses_are_singleflighted() {
        struct SlowResolver {
            calls: AtomicUsize,
        }
        impl FaviconResolver for SlowResolver {
            fn name(&self) -> &str {
                "slow"
            }
            fn resolve<'a>(&'a self, _authority: &'a str) -> ResolveFuture<'a> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async {
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                    Ok(Some(png(9)))
                })
            }
        }

        let resolver = Arc::new(SlowResolver {
            calls: AtomicUsize::new(0),
        });
        let storage: Arc<dyn Storage> =
            Arc::new(zoeken_storage::SqliteStorage::in_memory().await.unwrap());
        let service = StorageFaviconService::new(
            resolver.clone(),
            storage,
            FaviconPolicy {
                positive_ttl: std::time::Duration::from_secs(60),
                negative_ttl: std::time::Duration::from_secs(10),
                max_blob_bytes: 1024,
                max_total_bytes: 4096,
            },
        );
        let (a, b, c) = tokio::join!(
            service.get_favicon("example.com"),
            service.get_favicon("example.com"),
            service.get_favicon("example.com")
        );
        assert_eq!(a, FaviconOutcome::Serve(png(9)));
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(resolver.calls.load(Ordering::SeqCst), 1);
    }
}
