//! Property test: favicon miss resolves then caches.

use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use proptest::prelude::*;

use zoeken_favicons::{
    CacheLookup, Favicon, FaviconCache, FaviconOutcome, FaviconResolver, FaviconService,
    InMemoryFaviconCache, ResolveFuture, StaticResolver,
};

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

fn resolver_name_strategy() -> impl Strategy<Value = String> {
    prop::sample::select(&["duckduckgo", "google", "yandex", "allesedv"][..])
        .prop_map(str::to_string)
}

fn authority_strategy() -> impl Strategy<Value = String> {
    prop::string::string_regex("[\\PC]{0,40}").expect("valid regex")
}

fn favicon_strategy() -> impl Strategy<Value = Favicon> {
    (
        prop::collection::vec(any::<u8>(), 0..64),
        prop::sample::select(&["image/png", "image/x-icon", "image/svg+xml"][..]),
    )
        .prop_map(|(data, mime)| Favicon::new(data, mime))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn prop_favicon_miss_resolves_then_caches(
        resolver_name in resolver_name_strategy(),
        authority in authority_strategy(),
        favicon in favicon_strategy(),
    ) {
        let cache = InMemoryFaviconCache::new();
        let resolver = Arc::new(CountingResolver::new(StaticResolver::serving(
            resolver_name.clone(),
            favicon.clone(),
        )));
        let counter = resolver.clone();
        let service = FaviconService::new(resolver, cache);

        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("build current-thread runtime");

        runtime.block_on(async {
            prop_assert_eq!(
                service.cache().get(&resolver_name, &authority).await,
                CacheLookup::Absent
            );
            let first = service.get_favicon(&authority).await;
            prop_assert_eq!(first, FaviconOutcome::Serve(favicon.clone()));
            prop_assert_eq!(counter.calls(), 1, "resolver invoked once on miss");

            prop_assert_eq!(
                service.cache().get(&resolver_name, &authority).await,
                CacheLookup::Hit(favicon.clone())
            );

            let second = service.get_favicon(&authority).await;
            prop_assert_eq!(second, FaviconOutcome::Serve(favicon.clone()));
            prop_assert_eq!(counter.calls(), 1, "second lookup served from cache, no re-resolve");

            Ok(())
        })?;
    }
}
