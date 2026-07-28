//! zoeken-favicons: favicon resolution, caching, proxying, and the image-proxy
//! content policy.
//!
//! ## Overview
//!
//! * A [`FaviconResolver`] fetches a favicon for a hostname. Injectable so tests
//!   can stub it without network I/O.
//! * [`FaviconService`] resolves and caches favicons — HashMap for tests /
//!   no-storage boots, unified storage for production. Known-missing markers
//!   prevent repeated misses.
//! * [`image_proxy_decision`] is a pure function implementing the `/image_proxy`
//!   content-type and size policy (14.7).
//! * [`safe_outbound_get`] / [`SafeOutboundTransport`] are the shared SSRF-safe
//!   GET helpers used by favicon resolution and the image proxy.

mod hmac;
mod proxy;
mod resolver;
mod safe_outbound;
mod service;

pub use hmac::{is_hmac_of, new_hmac};
pub use proxy::{
    DEFAULT_MAX_IMAGE_BYTES, ImageProxyDecision, ImageProxyPolicy, ImageProxyRejection,
    ProxyUrlRejection, image_proxy_decision, is_blocked_ip, validate_proxy_authority,
    validate_proxy_url, validate_resolved_url,
};
pub use resolver::{
    FaviconResolver, HttpFaviconResolver, ResolveError, ResolveFuture, StaticResolver,
};
pub use safe_outbound::{
    IMAGE_ACCEPT, MAX_REDIRECT_HOPS, SafeOutboundBody, SafeOutboundTransport,
    get_following_safe_redirects, get_following_safe_redirects_coordinated, safe_outbound_get,
    safe_outbound_get_coordinated,
};
pub use service::{Favicon, FaviconOutcome, FaviconService};
