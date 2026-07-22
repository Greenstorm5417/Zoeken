//! Privacy-preserving, bounded in-process cache for outbound engine responses.

use std::sync::Arc;
use std::time::Duration;

use zoeken_engine_core::{EngineResponse, SearchQueryView};
use zoeken_network::{FlightCache, NetworkRequest};

fn response_weight(response: &EngineResponse) -> usize {
    response.body.len()
        + response.url.len()
        + response
            .headers
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum::<usize>()
}

/// Keys are opaque HMAC digests; raw queries, bodies, and responses never
/// enter persistent storage.
pub(crate) struct ResponseCache {
    cache: FlightCache<String, EngineResponse>,
    pub(crate) hmac_key: [u8; 32],
    html_ttl: Duration,
    structured_ttl: Duration,
}

impl ResponseCache {
    pub(crate) fn new(html_ttl: Duration, structured_ttl: Duration, max_bytes: usize) -> Self {
        Self {
            cache: FlightCache::new(max_bytes.max(1), response_weight),
            hmac_key: rand::random(),
            html_ttl,
            structured_ttl,
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<EngineResponse> {
        self.cache.get(&key.to_string())
    }

    pub(crate) fn put(&self, key: String, response: EngineResponse, structured: bool) {
        let ttl = if structured {
            self.structured_ttl
        } else {
            self.html_ttl
        };
        self.cache.put(key, response, ttl);
    }

    pub(crate) fn flight(&self, key: &str) -> Option<Arc<tokio::sync::Mutex<()>>> {
        self.cache.flight(&key.to_string())
    }

    pub(crate) fn finish_flight(&self, key: &str) {
        self.cache.finish_flight(&key.to_string());
    }
}

pub(crate) fn cache_key(
    secret: &[u8],
    engine: &str,
    request: &NetworkRequest,
    query: &SearchQueryView,
) -> String {
    use hmac::{KeyInit, Mac};

    let mut mac = <hmac::Hmac<sha2::Sha256> as KeyInit>::new_from_slice(secret)
        .expect("HMAC accepts keys of any size");
    for component in [
        engine.as_bytes(),
        request.method.as_str().as_bytes(),
        request.url.as_bytes(),
    ] {
        mac.update(component);
        mac.update(&[0]);
    }
    mac.update(request.body.as_deref().unwrap_or_default());
    mac.update(&[0]);
    mac.update(query.query.as_bytes());
    mac.update(&[0]);
    mac.update(query.locale.as_bytes());
    mac.update(&[0]);
    mac.update(&query.pageno.to_be_bytes());
    mac.update(format!("{:?}", query.safesearch).as_bytes());
    mac.update(format!("{:?}", query.time_range).as_bytes());
    hex::encode(mac.finalize().into_bytes())
}

fn response_header<'a>(response: &'a EngineResponse, name: &str) -> Option<&'a str> {
    response
        .headers
        .iter()
        .find(|(header, _)| header.eq_ignore_ascii_case(name))
        .map(|(_, value)| value.as_str())
}

pub(crate) fn response_is_structured(response: &EngineResponse) -> bool {
    response_header(response, "content-type").is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("json") || value.contains("xml")
    })
}

pub(crate) fn response_is_cacheable(response: &EngineResponse) -> bool {
    if response.status != 200 || response_header(response, "set-cookie").is_some() {
        return false;
    }
    if response_header(response, "cache-control").is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("private") || value.contains("no-store")
    }) {
        return false;
    }
    !response_header(response, "vary").is_some_and(|value| {
        let value = value.to_ascii_lowercase();
        value.contains("cookie") || value.contains("authorization") || value.trim() == "*"
    })
}
