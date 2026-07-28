//! Favicon service: resolve + cache. Memory HashMap for tests/no-storage;
//! `zoeken-storage` for production.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};

use tokio::sync::Mutex;
use zoeken_storage::{FaviconData, FaviconLookup, FaviconPolicy, Storage};

use crate::resolver::FaviconResolver;

/// Resolved favicon bytes + MIME type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Favicon {
    pub data: Vec<u8>,
    pub mime: String,
}

impl Favicon {
    pub fn new(data: impl Into<Vec<u8>>, mime: impl Into<String>) -> Self {
        Self {
            data: data.into(),
            mime: mime.into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaviconOutcome {
    Serve(Favicon),
    Fallback,
}

impl FaviconOutcome {
    pub fn favicon(&self) -> Option<&Favicon> {
        match self {
            FaviconOutcome::Serve(f) => Some(f),
            FaviconOutcome::Fallback => None,
        }
    }

    pub fn is_fallback(&self) -> bool {
        matches!(self, FaviconOutcome::Fallback)
    }
}

enum CacheBackend {
    Memory(StdMutex<HashMap<(String, String), Option<Favicon>>>),
    Storage {
        storage: Arc<dyn Storage>,
        policy: FaviconPolicy,
    },
}

/// Resolves and caches favicons. Production uses storage; tests/no-coordinator
/// use an in-process HashMap.
pub struct FaviconService {
    resolver: Arc<dyn FaviconResolver>,
    cache: CacheBackend,
    in_flight: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

impl FaviconService {
    /// In-memory cache (unit tests / AppState without a storage coordinator).
    pub fn memory(resolver: Arc<dyn FaviconResolver>) -> Self {
        Self {
            resolver,
            cache: CacheBackend::Memory(StdMutex::new(HashMap::new())),
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    /// Persistent cache backed by unified storage.
    pub fn storage(
        resolver: Arc<dyn FaviconResolver>,
        storage: Arc<dyn Storage>,
        policy: FaviconPolicy,
    ) -> Self {
        Self {
            resolver,
            cache: CacheBackend::Storage { storage, policy },
            in_flight: Mutex::new(HashMap::new()),
        }
    }

    pub fn resolver_name(&self) -> &str {
        self.resolver.name()
    }

    /// Pre-seed the in-memory cache (tests only; no-op for storage backend).
    pub fn seed(&self, authority: &str, favicon: Option<&Favicon>) {
        let CacheBackend::Memory(map) = &self.cache else {
            return;
        };
        let Ok(mut map) = map.lock() else {
            return;
        };
        map.insert(
            (self.resolver.name().to_string(), authority.to_string()),
            favicon.cloned(),
        );
    }

    pub async fn get_favicon(&self, authority: &str) -> FaviconOutcome {
        match &self.cache {
            CacheBackend::Memory(_) => self.get_memory(authority).await,
            CacheBackend::Storage { .. } => self.get_storage(authority).await,
        }
    }

    async fn get_memory(&self, authority: &str) -> FaviconOutcome {
        let resolver = self.resolver.name();
        match self.memory_get(resolver, authority) {
            Some(Some(favicon)) => FaviconOutcome::Serve(favicon),
            Some(None) => FaviconOutcome::Fallback,
            None => match self.resolver.resolve(authority).await {
                Ok(Some(favicon)) => {
                    self.memory_set(resolver, authority, Some(&favicon));
                    FaviconOutcome::Serve(favicon)
                }
                Ok(None) => {
                    self.memory_set(resolver, authority, None);
                    FaviconOutcome::Fallback
                }
                Err(_) => FaviconOutcome::Fallback,
            },
        }
    }

    fn memory_get(&self, resolver: &str, authority: &str) -> Option<Option<Favicon>> {
        let CacheBackend::Memory(map) = &self.cache else {
            return None;
        };
        let Ok(map) = map.lock() else {
            return None;
        };
        map.get(&(resolver.to_string(), authority.to_string()))
            .cloned()
    }

    fn memory_set(&self, resolver: &str, authority: &str, favicon: Option<&Favicon>) {
        let CacheBackend::Memory(map) = &self.cache else {
            return;
        };
        let Ok(mut map) = map.lock() else {
            return;
        };
        map.insert(
            (resolver.to_string(), authority.to_string()),
            favicon.cloned(),
        );
    }

    async fn lookup_storage(&self, authority: &str) -> Result<FaviconLookup, ()> {
        let CacheBackend::Storage { storage, .. } = &self.cache else {
            return Err(());
        };
        storage
            .favicon_get(self.resolver.name(), authority)
            .await
            .map_err(|_| ())
    }

    fn storage_outcome(lookup: FaviconLookup) -> Option<FaviconOutcome> {
        match lookup {
            FaviconLookup::Hit(favicon) => Some(FaviconOutcome::Serve(Favicon {
                data: favicon.data,
                mime: favicon.mime,
            })),
            FaviconLookup::KnownMissing => Some(FaviconOutcome::Fallback),
            FaviconLookup::Absent => None,
        }
    }

    async fn get_storage(&self, authority: &str) -> FaviconOutcome {
        let Ok(lookup) = self.lookup_storage(authority).await else {
            metrics::counter!("storage_operations_total", "operation" => "favicon_get", "outcome" => "error")
                .increment(1);
            return FaviconOutcome::Fallback;
        };
        if let Some(outcome) = Self::storage_outcome(lookup) {
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

        let Ok(lookup) = self.lookup_storage(authority).await else {
            self.in_flight.lock().await.remove(authority);
            return FaviconOutcome::Fallback;
        };
        if let Some(outcome) = Self::storage_outcome(lookup) {
            metrics::counter!("favicon_singleflight_total", "outcome" => "shared").increment(1);
            return outcome;
        }

        let CacheBackend::Storage { storage, policy } = &self.cache else {
            return FaviconOutcome::Fallback;
        };

        let outcome = match self.resolver.resolve(authority).await {
            Ok(Some(favicon)) => {
                let stored = FaviconData {
                    data: favicon.data.clone(),
                    mime: favicon.mime.clone(),
                };
                if storage
                    .favicon_put(self.resolver.name(), authority, Some(&stored), policy)
                    .await
                    .is_err()
                {
                    FaviconOutcome::Fallback
                } else {
                    FaviconOutcome::Serve(favicon)
                }
            }
            Ok(None) => {
                if storage
                    .favicon_put(self.resolver.name(), authority, None, policy)
                    .await
                    .is_err()
                {
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
    use crate::resolver::{FaviconResolver, ResolveError, ResolveFuture, StaticResolver};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    fn png(tag: u8) -> Favicon {
        Favicon::new(vec![tag; 16], "image/png")
    }

    fn policy() -> FaviconPolicy {
        FaviconPolicy {
            positive_ttl: Duration::from_secs(60),
            negative_ttl: Duration::from_secs(10),
            max_blob_bytes: 1024,
            max_total_bytes: 4096,
        }
    }

    #[tokio::test]
    async fn cache_hit_returns_cached_favicon() {
        let resolver = Arc::new(StaticResolver::failing("stub", "should not be called"));
        let service = FaviconService::memory(resolver);
        service.seed("example.com", Some(&png(1)));

        let outcome = service.get_favicon("example.com").await;
        assert_eq!(outcome, FaviconOutcome::Serve(png(1)));
    }

    #[tokio::test]
    async fn cache_miss_resolves_then_stores() {
        struct CountingServe {
            calls: AtomicUsize,
        }
        impl FaviconResolver for CountingServe {
            fn name(&self) -> &str {
                "stub"
            }
            fn resolve<'a>(&'a self, _authority: &'a str) -> ResolveFuture<'a> {
                self.calls.fetch_add(1, Ordering::SeqCst);
                Box::pin(async { Ok(Some(png(7))) })
            }
        }

        let resolver = Arc::new(CountingServe {
            calls: AtomicUsize::new(0),
        });
        let calls = resolver.clone();
        let service = FaviconService::memory(resolver);

        assert_eq!(
            service.get_favicon("example.org").await,
            FaviconOutcome::Serve(png(7))
        );
        assert_eq!(
            service.get_favicon("example.org").await,
            FaviconOutcome::Serve(png(7))
        );
        assert_eq!(calls.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn unresolved_and_uncached_returns_fallback() {
        let resolver = Arc::new(StaticResolver::failing("stub", "boom"));
        let service = FaviconService::memory(resolver);
        assert_eq!(
            service.get_favicon("missing.example").await,
            FaviconOutcome::Fallback
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
        let service = FaviconService::memory(resolver);

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
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    Ok(Some(png(9)))
                })
            }
        }

        let resolver = Arc::new(SlowResolver {
            calls: AtomicUsize::new(0),
        });
        let storage: Arc<dyn Storage> =
            Arc::new(zoeken_storage::SqliteStorage::in_memory().await.unwrap());
        let service = FaviconService::storage(resolver.clone(), storage, policy());
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
