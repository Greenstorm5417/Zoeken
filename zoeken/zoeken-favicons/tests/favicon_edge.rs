//! Edge tests for cache-hit, resolution-failure, and fallback behaviors.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use zoeken_favicons::{
    Favicon, FaviconOutcome, FaviconResolver, FaviconService, ResolveError, ResolveFuture,
    StaticResolver,
};

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
    let resolver = Arc::new(CountingResolver::new(StaticResolver::failing(
        "duckduckgo",
        "resolver must not be called on a cache hit",
    )));
    let counter = resolver.clone();
    let service = FaviconService::memory(resolver);
    service.seed("example.com", Some(&png(1)));

    let outcome = service.get_favicon("example.com").await;

    assert_eq!(outcome, FaviconOutcome::Serve(png(1)));
    assert_eq!(counter.calls(), 0, "resolver was consulted on a cache hit");
}

#[tokio::test]
async fn cached_favicon_wins_over_failing_resolver_in_memory() {
    let resolver = Arc::new(CountingResolver::new(StaticResolver::failing(
        "duckduckgo",
        "boom",
    )));
    let counter = resolver.clone();
    let service = FaviconService::memory(resolver);
    service.seed("example.net", Some(&png(3)));

    let outcome = service.get_favicon("example.net").await;

    assert_eq!(outcome, FaviconOutcome::Serve(png(3)));
    assert_eq!(
        counter.calls(),
        0,
        "cache hit should preempt the failing resolver"
    );
}

#[tokio::test]
async fn unresolved_and_uncached_falls_back_in_memory() {
    let resolver = Arc::new(StaticResolver::failing("duckduckgo", "boom"));
    let service = FaviconService::memory(resolver);

    let outcome = service.get_favicon("missing.example").await;
    assert_eq!(outcome, FaviconOutcome::Fallback);
}

#[tokio::test]
async fn definitive_no_favicon_caches_known_missing_and_avoids_reresolve_in_memory() {
    let resolver = Arc::new(CountingResolver::new(StaticResolver::empty("duckduckgo")));
    let counter = resolver.clone();
    let service = FaviconService::memory(resolver);

    assert_eq!(
        service.get_favicon("none.example").await,
        FaviconOutcome::Fallback
    );
    assert_eq!(counter.calls(), 1);

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
    let resolver = Arc::new(StaticResolver::failing("duckduckgo", "boom"));
    let service = FaviconService::memory(resolver);

    let outcome = service.get_favicon("missing.example").await;
    assert!(outcome.is_fallback());
    assert_eq!(outcome.favicon(), None);
}

#[allow(dead_code)]
fn _assert_resolve_error_is_error(e: ResolveError) -> impl std::error::Error {
    e
}
