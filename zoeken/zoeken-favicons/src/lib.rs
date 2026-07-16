//! zoeken-favicons: favicon resolution, caching, proxying, and the image-proxy
//! content policy.
//!
//! ## Overview
//!
//! * A [`FaviconResolver`] is an injectable backend that fetches a favicon for
//!   a hostname (authority) from an external source. It is a trait so tests can
//!   stub it without any real network I/O.
//! * A [`FaviconCache`] stores resolved favicons keyed by `(resolver, authority)`.
//!   The default [`SqliteFaviconCache`] persists BLOBs in a SQLite database
//!   (mirroring `cache.py`). A distinguished *known-missing* marker records that
//!   a resolver previously found no favicon, so it is not re-resolved.
//! * A [`FaviconService`] ties a resolver and a cache together, implementing the
//!   cache-hit (12.1), miss-resolve-store (12.2), resolution-failure-with-cache
//!   (12.3), and unresolved-fallback (12.4) behaviors.
//! * [`image_proxy_decision`] is a pure function implementing the `/image_proxy`
//!   content-type and size policy (14.7).

mod cache;
mod hmac;
mod proxy;
mod resolver;
mod service;

pub use cache::{CacheLookup, Favicon, FaviconCache, InMemoryFaviconCache, SqliteFaviconCache};
pub use hmac::{is_hmac_of, new_hmac};
pub use proxy::{
    DEFAULT_MAX_IMAGE_BYTES, ImageProxyDecision, ImageProxyPolicy, ImageProxyRejection,
    ProxyUrlRejection, image_proxy_decision, validate_proxy_authority, validate_proxy_url,
};
pub use resolver::{
    FaviconResolver, HttpFaviconResolver, ResolveError, ResolveFuture, StaticResolver,
};
pub use service::{FaviconOutcome, FaviconService};
