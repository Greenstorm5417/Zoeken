//! Privacy-preserving, bounded in-process caches for outbound engine responses.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use zoeken_engine_core::{EngineResponse, SearchQueryView};
use zoeken_network::NetworkRequest;

pub(crate) struct CachedResponse {
    at: Instant,
    ttl: Duration,
    bytes: usize,
    response: EngineResponse,
}

/// Keys are opaque HMAC digests; raw queries, bodies, and responses never
/// enter persistent storage.
pub(crate) struct ResponseCache {
    pub(crate) entries: Mutex<HashMap<String, CachedResponse>>,
    flights: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
    pub(crate) total_bytes: Mutex<usize>,
    pub(crate) hmac_key: [u8; 32],
    html_ttl: Duration,
    structured_ttl: Duration,
    max_bytes: usize,
}

impl ResponseCache {
    pub(crate) fn new(html_ttl: Duration, structured_ttl: Duration, max_bytes: usize) -> Self {
        Self {
            entries: Mutex::new(HashMap::new()),
            flights: Mutex::new(HashMap::new()),
            total_bytes: Mutex::new(0),
            hmac_key: rand::random(),
            html_ttl,
            structured_ttl,
            max_bytes: max_bytes.max(1),
        }
    }

    pub(crate) fn get(&self, key: &str) -> Option<EngineResponse> {
        let mut entries = self.entries.lock().ok()?;
        let entry = entries.get(key)?;
        if entry.at.elapsed() < entry.ttl {
            return Some(entry.response.clone());
        }
        let expired = entries.remove(key)?;
        if let Ok(mut total) = self.total_bytes.lock() {
            *total = total.saturating_sub(expired.bytes);
        }
        None
    }

    pub(crate) fn put(&self, key: String, response: EngineResponse, structured: bool) {
        let bytes = response.body.len()
            + response.url.len()
            + response
                .headers
                .iter()
                .map(|(name, value)| name.len() + value.len())
                .sum::<usize>();
        if bytes > self.max_bytes {
            return;
        }
        let Ok(mut entries) = self.entries.lock() else {
            return;
        };
        let Ok(mut total) = self.total_bytes.lock() else {
            return;
        };
        entries.retain(|_, entry| {
            let keep = entry.at.elapsed() < entry.ttl;
            if !keep {
                *total = total.saturating_sub(entry.bytes);
            }
            keep
        });
        while total.saturating_add(bytes) > self.max_bytes {
            let Some(oldest) = entries
                .iter()
                .min_by_key(|(_, entry)| entry.at)
                .map(|(key, _)| key.clone())
            else {
                break;
            };
            if let Some(removed) = entries.remove(&oldest) {
                *total = total.saturating_sub(removed.bytes);
            }
        }
        if let Some(previous) = entries.insert(
            key,
            CachedResponse {
                at: Instant::now(),
                ttl: if structured {
                    self.structured_ttl
                } else {
                    self.html_ttl
                },
                bytes,
                response,
            },
        ) {
            *total = total.saturating_sub(previous.bytes);
        }
        *total = total.saturating_add(bytes);
    }

    pub(crate) fn flight(&self, key: &str) -> Option<Arc<tokio::sync::Mutex<()>>> {
        let mut flights = self.flights.lock().ok()?;
        Some(
            flights
                .entry(key.to_string())
                .or_insert_with(|| Arc::new(tokio::sync::Mutex::new(())))
                .clone(),
        )
    }

    pub(crate) fn finish_flight(&self, key: &str) {
        if let Ok(mut flights) = self.flights.lock() {
            flights.remove(key);
        }
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
