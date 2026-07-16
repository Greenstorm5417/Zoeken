//! Edge tests for cache-hit, resolution-failure, and fallback behaviors.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use zoeken_favicons::{
    CacheLookup, Favicon, FaviconCache, FaviconOutcome, FaviconResolver, FaviconService,
    InMemoryFaviconCache, ResolveError, ResolveFuture, SqliteFaviconCache, StaticResolver,
};

/// A small deterministic PNG-ish favicon tagged by a single byte value.
fn png(tag: u8) -> Favicon {
    Favicon::new(vec![tag; 16], "image/png")
}

struct CountingResolver {
    inner: StaticResolver,
    calls: AtomicUsize,
}

impl CountingResolver {
    fn new(inner: StaticResolver) -> Self {
        Self {
            inner,
            calls: AtomicUsize::new(0),
        }
    }

    fn calls(&self) -> usize {
        self.calls.load(Ordering::SeqCst)
    }
}

impl FaviconResolver for CountingResolver {
    fn name(&self) -> &str {
        self.inner.name()
    }

    fn resolve<'a>(&'a self, authority: &'a str) -> ResolveFuture<'a> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.inner.resolve(authority)
    }
}

#[tokio::test]
async fn cache_hit_returns_cached_without_resolving_in_memory() {
    let cache = InMemoryFaviconCache::new();
    cache.set("duckduckgo", "example.com", Some(&png(1)));

    let resolver = Arc::new(CountingResolver::new(StaticResolver::failing(
        "duckduckgo",
        "resolver must not be called on a cache hit",
    )));
    let counter = resolver.clone();
    let service = FaviconService::new(resolver, cache);

    let outcome = service.get_favicon("example.com").await;

    assert_eq!(outcome, FaviconOutcome::Serve(png(1)));
    assert_eq!(counter.calls(), 0, "resolver was consulted on a cache hit");
}

#[tokio::test]
async fn cache_hit_returns_cached_without_resolving_sqlite() {
    let cache = SqliteFaviconCache::in_memory().expect("open in-memory sqlite cache");
    cache.set("google", "cached.example", Some(&png(2)));

    let resolver = Arc::new(CountingResolver::new(StaticResolver::serving(
        "google",
        png(99),
    )));
    let counter = resolver.clone();
    let service = FaviconService::new(resolver, cache);

    let outcome = service.get_favicon("cached.example").await;

    assert_eq!(outcome, FaviconOutcome::Serve(png(2)));
    assert_eq!(counter.calls(), 0, "resolver was consulted on a cache hit");
}

#[tokio::test]
async fn cached_favicon_wins_over_failing_resolver_in_memory() {
    let cache = InMemoryFaviconCache::new();
    cache.set("duckduckgo", "example.net", Some(&png(3)));

    let resolver = Arc::new(CountingResolver::new(StaticResolver::failing(
        "duckduckgo",
        "boom",
    )));
    let counter = resolver.clone();
    let service = FaviconService::new(resolver, cache);

    let outcome = service.get_favicon("example.net").await;

    assert_eq!(outcome, FaviconOutcome::Serve(png(3)));
    assert_eq!(
        counter.calls(),
        0,
        "cache hit should preempt the failing resolver"
    );
}

#[tokio::test]
async fn cached_favicon_wins_over_failing_resolver_sqlite() {
    let cache = SqliteFaviconCache::in_memory().expect("open in-memory sqlite cache");
    cache.set("google", "example.net", Some(&png(4)));

    let resolver = Arc::new(StaticResolver::failing("google", "boom"));
    let service = FaviconService::new(resolver, cache);

    let outcome = service.get_favicon("example.net").await;

    assert_eq!(outcome, FaviconOutcome::Serve(png(4)));
}

#[tokio::test]
async fn resolution_failure_serves_favicon_that_appeared_after_miss() {
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
        favicon: png(5),
        gets: AtomicUsize::new(0),
    };
    let resolver = Arc::new(StaticResolver::failing("stub", "boom"));
    let service = FaviconService::new(resolver, cache);

    let outcome = service.get_favicon("appearing.example").await;
    assert_eq!(outcome, FaviconOutcome::Serve(png(5)));
}

#[tokio::test]
async fn unresolved_and_uncached_falls_back_and_does_not_cache_failure_in_memory() {
    let cache = InMemoryFaviconCache::new();
    let resolver = Arc::new(StaticResolver::failing("duckduckgo", "boom"));
    let service = FaviconService::new(resolver, cache);

    let outcome = service.get_favicon("missing.example").await;
    assert_eq!(outcome, FaviconOutcome::Fallback);

    assert_eq!(
        service.cache().get("duckduckgo", "missing.example"),
        CacheLookup::Absent
    );
}

#[tokio::test]
async fn unresolved_and_uncached_falls_back_and_does_not_cache_failure_sqlite() {
    let cache = SqliteFaviconCache::in_memory().expect("open in-memory sqlite cache");
    let resolver = Arc::new(StaticResolver::failing("google", "boom"));
    let service = FaviconService::new(resolver, cache);

    let outcome = service.get_favicon("missing.example").await;
    assert_eq!(outcome, FaviconOutcome::Fallback);

    assert_eq!(
        service.cache().get("google", "missing.example"),
        CacheLookup::Absent
    );
}

#[tokio::test]
async fn definitive_no_favicon_caches_known_missing_and_avoids_reresolve_in_memory() {
    let cache = InMemoryFaviconCache::new();
    let resolver = Arc::new(CountingResolver::new(StaticResolver::empty("duckduckgo")));
    let counter = resolver.clone();
    let service = FaviconService::new(resolver, cache);

    assert_eq!(
        service.get_favicon("none.example").await,
        FaviconOutcome::Fallback
    );
    assert_eq!(counter.calls(), 1);
    assert_eq!(
        service.cache().get("duckduckgo", "none.example"),
        CacheLookup::KnownMissing
    );

    assert_eq!(
        service.get_favicon("none.example").await,
        FaviconOutcome::Fallback
    );
    assert_eq!(
        counter.calls(),
        1,
        "known-missing marker should prevent re-resolving"
    );
}

#[tokio::test]
async fn definitive_no_favicon_caches_known_missing_and_avoids_reresolve_sqlite() {
    let cache = SqliteFaviconCache::in_memory().expect("open in-memory sqlite cache");
    let resolver = Arc::new(CountingResolver::new(StaticResolver::empty("google")));
    let counter = resolver.clone();
    let service = FaviconService::new(resolver, cache);

    assert_eq!(
        service.get_favicon("none.example").await,
        FaviconOutcome::Fallback
    );
    assert_eq!(counter.calls(), 1);
    assert_eq!(
        service.cache().get("google", "none.example"),
        CacheLookup::KnownMissing
    );

    assert_eq!(
        service.get_favicon("none.example").await,
        FaviconOutcome::Fallback
    );
    assert_eq!(
        counter.calls(),
        1,
        "known-missing marker should prevent re-resolving"
    );
}

#[tokio::test]
async fn fallback_outcome_exposes_no_favicon() {
    let cache = InMemoryFaviconCache::new();
    let resolver = Arc::new(StaticResolver::failing("duckduckgo", "boom"));
    let service = FaviconService::new(resolver, cache);

    let outcome = service.get_favicon("missing.example").await;
    assert!(outcome.is_fallback());
    assert_eq!(outcome.favicon(), None);
}

#[allow(dead_code)]
fn _assert_resolve_error_is_error(e: ResolveError) -> impl std::error::Error {
    e
}
