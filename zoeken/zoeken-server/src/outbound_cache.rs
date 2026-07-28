//! Privacy-preserving, bounded in-process cache for outbound engine responses.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use moka::Expiry;
use moka::future::Cache;
use zoeken_engine_core::{EngineResponse, SearchQueryView};
use zoeken_network::NetworkRequest;

fn response_weight(response: &EngineResponse) -> u32 {
    let bytes = response.body.len()
        + response.url.len()
        + response
            .headers
            .iter()
            .map(|(name, value)| name.len() + value.len())
            .sum::<usize>();
    u32::try_from(bytes).unwrap_or(u32::MAX).max(1)
}

#[derive(Clone)]
struct CachedResponse {
    response: EngineResponse,
    structured: bool,
}

struct ResponseExpiry {
    html_ttl: Duration,
    structured_ttl: Duration,
}

impl Expiry<String, CachedResponse> for ResponseExpiry {
    fn expire_after_create(
        &self,
        _key: &String,
        value: &CachedResponse,
        _now: Instant,
    ) -> Option<Duration> {
        Some(if value.structured {
            self.structured_ttl
        } else {
            self.html_ttl
        })
    }
}

/// Load outcome for [`ResponseCache::get_or_load`]: successes that must not be retained
/// travel as [`LoadError::Uncached`] so moka skips storage while callers still get the body.
#[derive(Clone)]
enum LoadError<E> {
    Uncached {
        response: EngineResponse,
        http_duration: Duration,
    },
    Failed {
        error: E,
        http_duration: Option<Duration>,
    },
}

/// Keys are opaque HMAC digests; raw queries, bodies, and responses never
/// enter persistent storage.
pub(crate) struct ResponseCache {
    cache: Cache<String, CachedResponse>,
    pub(crate) hmac_key: [u8; 32],
}

impl ResponseCache {
    pub(crate) fn new(html_ttl: Duration, structured_ttl: Duration, max_bytes: usize) -> Self {
        let expiry = ResponseExpiry {
            html_ttl,
            structured_ttl,
        };
        Self {
            cache: Cache::builder()
                .max_capacity(max_bytes.max(1) as u64)
                .weigher(|_key, value: &CachedResponse| response_weight(&value.response))
                .expire_after(expiry)
                .build(),
            hmac_key: rand::random(),
        }
    }

    pub(crate) async fn get(&self, key: &str) -> Option<EngineResponse> {
        self.cache.get(key).await.map(|cached| cached.response)
    }

    /// Singleflight load. Cacheable successes are stored; uncacheable successes and
    /// failures are not. `http_duration` is set for the producer (and uncached/error
    /// paths); pure cache hits / shared waiters see `None`.
    pub(crate) async fn get_or_load<F, E>(
        &self,
        key: String,
        init: F,
    ) -> Result<(EngineResponse, Option<Duration>), (E, Option<Duration>)>
    where
        F: std::future::Future<
                Output = Result<(EngineResponse, bool, Duration), (E, Option<Duration>)>,
            >,
        E: Clone + Send + Sync + 'static,
    {
        let produced = Arc::new(Mutex::new(None));
        let produced_for_init = Arc::clone(&produced);
        match self
            .cache
            .try_get_with(key, async move {
                match init.await {
                    Ok((response, true, http_duration)) => {
                        if let Ok(mut guard) = produced_for_init.lock() {
                            *guard = Some(http_duration);
                        }
                        let structured = response_is_structured(&response);
                        Ok(CachedResponse {
                            response,
                            structured,
                        })
                    }
                    Ok((response, false, http_duration)) => Err(LoadError::Uncached {
                        response,
                        http_duration,
                    }),
                    Err((error, http_duration)) => Err(LoadError::Failed {
                        error,
                        http_duration,
                    }),
                }
            })
            .await
        {
            Ok(cached) => {
                let http_duration = produced.lock().ok().and_then(|mut guard| guard.take());
                Ok((cached.response, http_duration))
            }
            Err(error) => match Arc::unwrap_or_clone(error) {
                LoadError::Uncached {
                    response,
                    http_duration,
                } => Ok((response, Some(http_duration))),
                LoadError::Failed {
                    error,
                    http_duration,
                } => Err((error, http_duration)),
            },
        }
    }
}

pub(crate) fn cache_key(
    secret: &[u8],
    engine: &str,
    request: &NetworkRequest,
    query: &SearchQueryView,
) -> String {
    let mut value = Vec::new();
    for component in [
        engine.as_bytes(),
        request.method.as_str().as_bytes(),
        request.url.as_bytes(),
    ] {
        value.extend_from_slice(component);
        value.push(0);
    }
    value.extend_from_slice(request.body.as_deref().unwrap_or_default());
    value.push(0);
    value.extend_from_slice(query.query.as_bytes());
    value.push(0);
    value.extend_from_slice(query.locale.as_bytes());
    value.push(0);
    value.extend_from_slice(&query.pageno.to_be_bytes());
    value.extend_from_slice(format!("{:?}", query.safesearch).as_bytes());
    value.extend_from_slice(format!("{:?}", query.time_range).as_bytes());
    zoeken_favicons::new_hmac(secret, &value)
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
