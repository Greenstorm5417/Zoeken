//! Pluggable query-suggestion backends: [`AutocompleteService`] dispatch,
//! network-backed providers, and [`StaticBackend`] for testing.

mod backends;

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use serde::Serialize;
use zoeken_network::{DEFAULT_NETWORK, FlightCache, NetworkManager, NetworkRequest};

use backends::{
    BaiduBackend, BingBackend, DbpediaBackend, MwmblBackend, NaverBackend, PrivacywallBackend,
    QuarkBackend, QwantBackend, Search360Backend, SeznamBackend, SogouBackend, SwisscowsBackend,
    YandexBackend,
};

/// The default per-backend timeout applied by [`AutocompleteService`]
/// (mirrors the reference `outgoing.request_timeout` default of 3 seconds).
pub const DEFAULT_AUTOCOMPLETE_TIMEOUT: Duration = Duration::from_secs(3);

/// DuckDuckGo's "all locales" region token (reference `traits.all_locale`).
const DUCKDUCKGO_ALL_LOCALE: &str = "wt-wt";

/// One autocomplete suggestion. Plain backends fill `text` only; rich backends
/// (Brave with `rich=true`) may also set `subtext` / `image`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Suggestion {
    pub text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub subtext: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub image: Option<String>,
}

impl Suggestion {
    #[must_use]
    pub fn text(text: impl Into<String>) -> Self {
        Self {
            text: text.into(),
            subtext: None,
            image: None,
        }
    }
}

impl From<String> for Suggestion {
    fn from(text: String) -> Self {
        Self::text(text)
    }
}

impl From<&str> for Suggestion {
    fn from(text: &str) -> Self {
        Self::text(text)
    }
}

/// Map plain suggestion strings into the shared DTO.
#[must_use]
pub fn suggestions_from_texts(texts: impl IntoIterator<Item = String>) -> Vec<Suggestion> {
    texts.into_iter().map(Suggestion::from).collect()
}

/// Backend error: request failed or response unparseable.
#[derive(Debug, thiserror::Error)]
pub enum BackendError {
    #[error("autocomplete backend request failed: {0}")]
    Request(String),
    #[error("autocomplete backend returned an unexpected response: {0}")]
    Response(String),
}

pub type SuggestFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Vec<Suggestion>, BackendError>> + Send + 'a>>;

/// A pluggable autocomplete backend producing suggestions for a partial query.
/// Trait objects are injected: network-backed in production, in-memory in tests.
pub trait AutocompleteBackend: Send + Sync {
    fn name(&self) -> &str;
    fn suggest<'a>(&'a self, query: &'a str, locale: &'a str) -> SuggestFuture<'a>;
}

/// The autocomplete dispatch point: holds a backend and timeout, returning
/// empty lists on error/timeout. Results are cached in memory for
/// [`CACHE_TTL`] so repeated prefixes (backspacing, retyping) skip the
/// upstream round-trip entirely. Entries are weighted as one each so
/// `cache_capacity` bounds entry count.
#[derive(Clone)]
pub struct AutocompleteService {
    backend: Option<Arc<dyn AutocompleteBackend>>,
    timeout: Duration,
    cache: Arc<FlightCache<String, Vec<Suggestion>>>,
    cache_ttl: Duration,
    hmac_key: Arc<[u8; 32]>,
}

impl AutocompleteService {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            backend: None,
            timeout: DEFAULT_AUTOCOMPLETE_TIMEOUT,
            cache: Arc::new(FlightCache::new(2048, |_: &Vec<Suggestion>| 1)),
            cache_ttl: Duration::from_secs(300),
            hmac_key: Arc::new(rand::random()),
        }
    }

    #[must_use]
    pub fn with_backend(backend: Arc<dyn AutocompleteBackend>) -> Self {
        Self {
            backend: Some(backend),
            timeout: DEFAULT_AUTOCOMPLETE_TIMEOUT,
            cache: Arc::new(FlightCache::new(2048, |_: &Vec<Suggestion>| 1)),
            cache_ttl: Duration::from_secs(300),
            hmac_key: Arc::new(rand::random()),
        }
    }

    /// Replace the per-call timeout; backend calls exceeding it yield empty lists.
    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = timeout;
        self
    }

    /// Configure the bounded, process-memory-only suggestion cache.
    #[must_use]
    pub fn with_cache(mut self, ttl: Duration, max_entries: usize) -> Self {
        self.cache_ttl = ttl;
        self.cache = Arc::new(FlightCache::new(
            max_entries.max(1),
            |_: &Vec<Suggestion>| 1,
        ));
        self
    }

    #[must_use]
    pub fn is_enabled(&self) -> bool {
        self.backend.is_some()
    }

    #[must_use]
    pub fn backend_name(&self) -> Option<&str> {
        self.backend.as_deref().map(AutocompleteBackend::name)
    }

    /// Return suggestions for the partial `query` in `locale`.
    /// Returns empty lists when no backend is configured, on error, or timeout.
    pub async fn suggest(&self, query: &str, locale: &str) -> Vec<Suggestion> {
        let Some(backend) = self.backend.as_ref() else {
            return Vec::new();
        };

        let key = autocomplete_key(&self.hmac_key[..], backend.name(), query, locale);
        if let Some(suggestions) = self.cache.get(&key) {
            metrics::counter!("autocomplete_cache_total", "outcome" => "hit").increment(1);
            return suggestions;
        }

        let Some(flight) = self.cache.flight(&key) else {
            return Vec::new();
        };
        let _guard = flight.lock().await;

        // The leader populates the cache while followers wait on the key lock.
        if let Some(suggestions) = self.cache.get(&key) {
            metrics::counter!("autocomplete_singleflight_total", "outcome" => "shared")
                .increment(1);
            return suggestions;
        }

        let suggestions =
            match tokio::time::timeout(self.timeout, backend.suggest(query, locale)).await {
                Ok(Ok(suggestions)) => suggestions,
                Ok(Err(_)) | Err(_) => {
                    self.cache.finish_flight(&key);
                    return Vec::new();
                }
            };

        self.cache
            .put(key.clone(), suggestions.clone(), self.cache_ttl);
        self.cache.finish_flight(&key);
        suggestions
    }
}

fn autocomplete_key(secret: &[u8], backend: &str, query: &str, locale: &str) -> String {
    use hmac::{KeyInit, Mac};

    let mut mac = <hmac::Hmac<sha2::Sha256> as KeyInit>::new_from_slice(secret)
        .expect("HMAC accepts keys of any size");
    for component in [backend, locale, query] {
        mac.update(component.as_bytes());
        mac.update(&[0]);
    }
    hex::encode(mac.finalize().into_bytes())
}

impl Default for AutocompleteService {
    fn default() -> Self {
        Self::disabled()
    }
}

impl std::fmt::Debug for AutocompleteService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AutocompleteService")
            .field("backend", &self.backend_name())
            .field("timeout", &self.timeout)
            .finish()
    }
}

/// The names of the real backends this crate can build (SearXNG `autocomplete.backends` parity).
pub const BACKEND_NAMES: &[&str] = &[
    "360search",
    "baidu",
    "bing",
    "brave",
    "dbpedia",
    "duckduckgo",
    "google",
    "mwmbl",
    "naver",
    "privacywall",
    "quark",
    "qwant",
    "seznam",
    "sogou",
    "startpage",
    "swisscows",
    "wikipedia",
    "yandex",
];

/// Build a real network-backed backend by name, or `None` for unknown names.
#[must_use]
pub fn backend_by_name(
    name: &str,
    network: Arc<NetworkManager>,
) -> Option<Arc<dyn AutocompleteBackend>> {
    match name {
        "360search" => Some(Arc::new(Search360Backend::new(network))),
        "baidu" => Some(Arc::new(BaiduBackend::new(network))),
        "bing" => Some(Arc::new(BingBackend::new(network))),
        "brave" => Some(Arc::new(BraveBackend::new(network))),
        "dbpedia" => Some(Arc::new(DbpediaBackend::new(network))),
        "duckduckgo" => Some(Arc::new(DuckDuckGoBackend::new(network))),
        "google" => Some(Arc::new(GoogleBackend::new(network))),
        "mwmbl" => Some(Arc::new(MwmblBackend::new(network))),
        "naver" => Some(Arc::new(NaverBackend::new(network))),
        "privacywall" => Some(Arc::new(PrivacywallBackend::new(network))),
        "quark" => Some(Arc::new(QuarkBackend::new(network))),
        "qwant" => Some(Arc::new(QwantBackend::new(network))),
        "seznam" => Some(Arc::new(SeznamBackend::new(network))),
        "sogou" => Some(Arc::new(SogouBackend::new(network))),
        "startpage" => Some(Arc::new(StartpageBackend::new(network))),
        "swisscows" => Some(Arc::new(SwisscowsBackend::new(network))),
        "wikipedia" => Some(Arc::new(WikipediaBackend::new(network))),
        "yandex" => Some(Arc::new(YandexBackend::new(network))),
        _ => None,
    }
}

/// Build an [`AutocompleteService`] for the configured backend name.
/// Unknown/empty names produce a disabled service; known names produce a backend service.
#[must_use]
pub fn service_for(name: Option<&str>, network: Arc<NetworkManager>) -> AutocompleteService {
    match name.filter(|n| !n.is_empty()) {
        Some(name) => match backend_by_name(name, network) {
            Some(backend) => AutocompleteService::with_backend(backend),
            None => AutocompleteService::disabled(),
        },
        None => AutocompleteService::disabled(),
    }
}

/// A real autocomplete backend querying DuckDuckGo's suggestion endpoint.
pub struct DuckDuckGoBackend {
    network: Arc<NetworkManager>,
    network_name: String,
}

impl DuckDuckGoBackend {
    /// Build a backend issuing requests through the default network.
    #[must_use]
    pub fn new(network: Arc<NetworkManager>) -> Self {
        Self {
            network,
            network_name: DEFAULT_NETWORK.to_string(),
        }
    }

    /// Build a backend issuing requests through the named network.
    #[must_use]
    pub fn with_network_name(
        network: Arc<NetworkManager>,
        network_name: impl Into<String>,
    ) -> Self {
        Self {
            network,
            network_name: network_name.into(),
        }
    }

    /// Build the request URL for query/locale.
    fn build_url(query: &str, locale: &str) -> String {
        let region = duckduckgo_region(locale);
        let q = url::form_urlencoded::byte_serialize(query.as_bytes()).collect::<String>();
        let kl = url::form_urlencoded::byte_serialize(region.as_bytes()).collect::<String>();
        format!("https://duckduckgo.com/ac/?type=list&q={q}&kl={kl}")
    }
}

impl AutocompleteBackend for DuckDuckGoBackend {
    fn name(&self) -> &str {
        "duckduckgo"
    }

    fn suggest<'a>(&'a self, query: &'a str, locale: &'a str) -> SuggestFuture<'a> {
        Box::pin(async move {
            let url = Self::build_url(query, locale);
            let resp = self
                .network
                .request(&self.network_name, NetworkRequest::get(url))
                .await
                .map_err(|e| BackendError::Request(e.to_string()))?;

            let text = resp
                .text()
                .await
                .map_err(|e| BackendError::Response(e.to_string()))?;

            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| BackendError::Response(e.to_string()))?;

            Ok(suggestions_from_texts(parse_duckduckgo_suggestions(&value)))
        })
    }
}

/// Map a locale to DuckDuckGo's kl region token.
fn duckduckgo_region(locale: &str) -> String {
    let trimmed = locale.trim();
    if trimmed.is_empty() {
        DUCKDUCKGO_ALL_LOCALE.to_string()
    } else {
        trimmed.to_string()
    }
}

/// Parse DuckDuckGo's type=list suggestion payload: extracts strings from the second element.
#[must_use]
pub fn parse_duckduckgo_suggestions(value: &serde_json::Value) -> Vec<String> {
    parse_opensearch_suggestions(value)
}

/// OpenSearch-style `[query, [suggestions...]]` payload (DDG / Startpage / Wikipedia).
#[must_use]
pub fn parse_opensearch_suggestions(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Brave `?rich=true` payload: second element is objects with `q`/`name`/`desc`/`img`,
/// or plain strings when rich data is absent.
#[must_use]
pub fn parse_brave_suggestions(value: &serde_json::Value) -> Vec<Suggestion> {
    let Some(items) = value
        .as_array()
        .and_then(|arr| arr.get(1))
        .and_then(serde_json::Value::as_array)
    else {
        return Vec::new();
    };

    items
        .iter()
        .filter_map(|item| {
            if let Some(text) = item.as_str() {
                return Some(Suggestion::text(text));
            }
            let obj = item.as_object()?;
            let q = obj.get("q").and_then(serde_json::Value::as_str)?;
            // Prefer entity display name for the inserted query when present.
            let text = obj
                .get("name")
                .and_then(serde_json::Value::as_str)
                .filter(|s| !s.is_empty())
                .unwrap_or(q);
            if text.is_empty() {
                return None;
            }
            let subtext = obj
                .get("desc")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            let image = obj
                .get("img")
                .and_then(serde_json::Value::as_str)
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string);
            Some(Suggestion {
                text: text.to_string(),
                subtext,
                image,
            })
        })
        .collect()
}

/// Google `gws-wiz` complete payload: `[[[html, ...], ...], ...]` — strip tags from first cell.
#[must_use]
pub fn parse_google_suggestions(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .and_then(|arr| arr.first())
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| {
                    item.as_array()
                        .and_then(|row| row.first())
                        .and_then(serde_json::Value::as_str)
                        .map(strip_simple_html)
                })
                .filter(|s| !s.is_empty())
                .collect()
        })
        .unwrap_or_default()
}

fn strip_simple_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for ch in input.chars() {
        match ch {
            '<' => in_tag = true,
            '>' => in_tag = false,
            _ if !in_tag => out.push(ch),
            _ => {}
        }
    }
    out
}

pub(crate) fn encode_query(query: &str) -> String {
    url::form_urlencoded::byte_serialize(query.as_bytes()).collect()
}

/// Google autocomplete (`/complete/search?client=gws-wiz`).
pub struct GoogleBackend {
    network: Arc<NetworkManager>,
    network_name: String,
}

impl GoogleBackend {
    #[must_use]
    pub fn new(network: Arc<NetworkManager>) -> Self {
        Self {
            network,
            network_name: DEFAULT_NETWORK.to_string(),
        }
    }

    fn build_url(query: &str, locale: &str) -> String {
        let hl = locale
            .split(['-', '_'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("en");
        let q = encode_query(query);
        let hl = encode_query(hl);
        format!("https://www.google.com/complete/search?client=gws-wiz&q={q}&hl={hl}")
    }
}

impl AutocompleteBackend for GoogleBackend {
    fn name(&self) -> &str {
        "google"
    }

    fn suggest<'a>(&'a self, query: &'a str, locale: &'a str) -> SuggestFuture<'a> {
        Box::pin(async move {
            let url = Self::build_url(query, locale);
            let resp = self
                .network
                .request(&self.network_name, NetworkRequest::get(url))
                .await
                .map_err(|e| BackendError::Request(e.to_string()))?;
            let text = resp
                .text()
                .await
                .map_err(|e| BackendError::Response(e.to_string()))?;
            // Google wraps JSON in `)]}'` or similar; take from first `[` to last `]`.
            let start = text.find('[').ok_or_else(|| {
                BackendError::Response("google autocomplete: no JSON array".into())
            })?;
            let end = text.rfind(']').ok_or_else(|| {
                BackendError::Response("google autocomplete: truncated JSON".into())
            })?;
            let value: serde_json::Value = serde_json::from_str(&text[start..=end])
                .map_err(|e| BackendError::Response(e.to_string()))?;
            Ok(suggestions_from_texts(parse_google_suggestions(&value)))
        })
    }
}

/// Wikipedia MediaWiki opensearch autocomplete.
pub struct WikipediaBackend {
    network: Arc<NetworkManager>,
    network_name: String,
}

impl WikipediaBackend {
    #[must_use]
    pub fn new(network: Arc<NetworkManager>) -> Self {
        Self {
            network,
            network_name: DEFAULT_NETWORK.to_string(),
        }
    }

    fn build_url(query: &str, locale: &str) -> String {
        let lang = locale
            .split(['-', '_'])
            .next()
            .filter(|s| !s.is_empty())
            .unwrap_or("en");
        let q = encode_query(query);
        format!(
            "https://{lang}.wikipedia.org/w/api.php?action=opensearch&format=json&formatversion=2&search={q}&namespace=0&limit=10"
        )
    }
}

impl AutocompleteBackend for WikipediaBackend {
    fn name(&self) -> &str {
        "wikipedia"
    }

    fn suggest<'a>(&'a self, query: &'a str, locale: &'a str) -> SuggestFuture<'a> {
        Box::pin(async move {
            let url = Self::build_url(query, locale);
            let resp = self
                .network
                .request(&self.network_name, NetworkRequest::get(url))
                .await
                .map_err(|e| BackendError::Request(e.to_string()))?;
            let text = resp
                .text()
                .await
                .map_err(|e| BackendError::Response(e.to_string()))?;
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| BackendError::Response(e.to_string()))?;
            Ok(suggestions_from_texts(parse_opensearch_suggestions(&value)))
        })
    }
}

/// Brave Search suggest API (public `rich=true` entity enrichments).
pub struct BraveBackend {
    network: Arc<NetworkManager>,
    network_name: String,
}

impl BraveBackend {
    #[must_use]
    pub fn new(network: Arc<NetworkManager>) -> Self {
        Self {
            network,
            network_name: DEFAULT_NETWORK.to_string(),
        }
    }

    fn build_url(query: &str) -> String {
        format!(
            "https://search.brave.com/api/suggest?q={}&rich=true",
            encode_query(query)
        )
    }
}

impl AutocompleteBackend for BraveBackend {
    fn name(&self) -> &str {
        "brave"
    }

    fn suggest<'a>(&'a self, query: &'a str, _locale: &'a str) -> SuggestFuture<'a> {
        Box::pin(async move {
            let url = Self::build_url(query);
            let resp = self
                .network
                .request(&self.network_name, NetworkRequest::get(url))
                .await
                .map_err(|e| BackendError::Request(e.to_string()))?;
            let text = resp
                .text()
                .await
                .map_err(|e| BackendError::Response(e.to_string()))?;
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| BackendError::Response(e.to_string()))?;
            Ok(parse_brave_suggestions(&value))
        })
    }
}

/// Startpage Firefox-extension suggestions endpoint.
pub struct StartpageBackend {
    network: Arc<NetworkManager>,
    network_name: String,
}

impl StartpageBackend {
    #[must_use]
    pub fn new(network: Arc<NetworkManager>) -> Self {
        Self {
            network,
            network_name: DEFAULT_NETWORK.to_string(),
        }
    }

    fn lui_for_locale(locale: &str) -> &'static str {
        match locale.split(['-', '_']).next().unwrap_or("en") {
            "da" => "dansk",
            "de" => "deutsch",
            "es" => "espanol",
            "fr" => "francais",
            "nb" => "norsk",
            "nl" => "nederlands",
            "pl" => "polski",
            "pt" => "portugues",
            "sv" => "svenska",
            _ => "english",
        }
    }

    fn build_url(query: &str, locale: &str) -> String {
        let lui = Self::lui_for_locale(locale);
        format!(
            "https://www.startpage.com/suggestions?q={}&format=opensearch&segment=startpage.defaultffx&lui={}",
            encode_query(query),
            encode_query(lui)
        )
    }
}

impl AutocompleteBackend for StartpageBackend {
    fn name(&self) -> &str {
        "startpage"
    }

    fn suggest<'a>(&'a self, query: &'a str, locale: &'a str) -> SuggestFuture<'a> {
        Box::pin(async move {
            let url = Self::build_url(query, locale);
            let mut headers = http::HeaderMap::new();
            headers.insert(
                http::header::USER_AGENT,
                http::HeaderValue::from_static(
                    "Mozilla/5.0 (X11; Linux x86_64; rv:120.0) Gecko/20100101 Firefox/120.0",
                ),
            );
            let req = NetworkRequest::get(url).with_headers(headers);
            let resp = self
                .network
                .request(&self.network_name, req)
                .await
                .map_err(|e| BackendError::Request(e.to_string()))?;
            let text = resp
                .text()
                .await
                .map_err(|e| BackendError::Response(e.to_string()))?;
            let value: serde_json::Value =
                serde_json::from_str(&text).map_err(|e| BackendError::Response(e.to_string()))?;
            Ok(suggestions_from_texts(parse_opensearch_suggestions(&value)))
        })
    }
}

/// An in-memory backend that always returns a fixed list of suggestions.
pub struct StaticBackend {
    name: String,
    suggestions: Vec<Suggestion>,
}

impl StaticBackend {
    /// A backend named `name` that returns `suggestions` for every query.
    #[must_use]
    pub fn new(name: impl Into<String>, suggestions: Vec<String>) -> Self {
        Self {
            name: name.into(),
            suggestions: suggestions_from_texts(suggestions),
        }
    }

    /// A backend that returns rich suggestions as-is.
    #[must_use]
    pub fn with_suggestions(name: impl Into<String>, suggestions: Vec<Suggestion>) -> Self {
        Self {
            name: name.into(),
            suggestions,
        }
    }
}

impl AutocompleteBackend for StaticBackend {
    fn name(&self) -> &str {
        &self.name
    }

    fn suggest<'a>(&'a self, _query: &'a str, _locale: &'a str) -> SuggestFuture<'a> {
        let suggestions = self.suggestions.clone();
        Box::pin(async move { Ok(suggestions) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};

    struct FailingBackend;

    impl AutocompleteBackend for FailingBackend {
        fn name(&self) -> &str {
            "failing"
        }
        fn suggest<'a>(&'a self, _query: &'a str, _locale: &'a str) -> SuggestFuture<'a> {
            Box::pin(async { Err(BackendError::Request("boom".to_string())) })
        }
    }

    struct SlowBackend;

    impl AutocompleteBackend for SlowBackend {
        fn name(&self) -> &str {
            "slow"
        }
        fn suggest<'a>(&'a self, _query: &'a str, _locale: &'a str) -> SuggestFuture<'a> {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(60)).await;
                Ok(vec![Suggestion::text("never")])
            })
        }
    }

    struct CountingBackend(AtomicUsize);

    impl AutocompleteBackend for CountingBackend {
        fn name(&self) -> &str {
            "counting"
        }

        fn suggest<'a>(&'a self, _query: &'a str, _locale: &'a str) -> SuggestFuture<'a> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(20)).await;
                Ok(vec![Suggestion::text("shared")])
            })
        }
    }

    #[tokio::test]
    async fn configured_backend_returns_its_suggestions() {
        let backend = Arc::new(StaticBackend::new(
            "stub",
            vec!["rust".to_string(), "rustlang".to_string()],
        ));
        let service = AutocompleteService::with_backend(backend);

        let suggestions = service.suggest("rus", "en-US").await;

        assert_eq!(
            suggestions,
            vec![Suggestion::text("rust"), Suggestion::text("rustlang")]
        );
        assert!(service.is_enabled());
        assert_eq!(service.backend_name(), Some("stub"));
    }

    #[tokio::test]
    async fn no_backend_returns_empty_list() {
        let service = AutocompleteService::disabled();

        let suggestions = service.suggest("rus", "en-US").await;

        assert!(suggestions.is_empty());
        assert!(!service.is_enabled());
        assert_eq!(service.backend_name(), None);
    }

    #[tokio::test]
    async fn backend_error_returns_empty_list() {
        let service = AutocompleteService::with_backend(Arc::new(FailingBackend));

        let suggestions = service.suggest("rus", "en-US").await;

        assert!(suggestions.is_empty());
    }

    #[tokio::test]
    async fn backend_timeout_returns_empty_list() {
        let service = AutocompleteService::with_backend(Arc::new(SlowBackend))
            .with_timeout(Duration::from_millis(20));

        let suggestions = service.suggest("rus", "en-US").await;

        assert!(suggestions.is_empty());
    }

    #[tokio::test]
    async fn identical_concurrent_requests_are_singleflighted() {
        let backend = Arc::new(CountingBackend(AtomicUsize::new(0)));
        let service = AutocompleteService::with_backend(backend.clone());
        let (a, b, c) = tokio::join!(
            service.suggest("rus", "en-US"),
            service.suggest("rus", "en-US"),
            service.suggest("rus", "en-US")
        );
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(backend.0.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn parse_duckduckgo_extracts_second_element_strings() {
        let value = serde_json::json!(["rus", ["rust", "russia", "rust lang"]]);
        assert_eq!(
            parse_duckduckgo_suggestions(&value),
            vec![
                "rust".to_string(),
                "russia".to_string(),
                "rust lang".to_string()
            ]
        );
    }

    #[test]
    fn parse_duckduckgo_handles_unexpected_shapes() {
        assert!(parse_duckduckgo_suggestions(&serde_json::json!(["rus"])).is_empty());
        assert!(parse_duckduckgo_suggestions(&serde_json::json!({"q": "rus"})).is_empty());
        assert_eq!(
            parse_duckduckgo_suggestions(&serde_json::json!(["rus", ["ok", 5, null, "two"]])),
            vec!["ok".to_string(), "two".to_string()]
        );
    }

    #[test]
    fn duckduckgo_url_encodes_query_and_defaults_region() {
        let url = DuckDuckGoBackend::build_url("rust lang", "");
        assert!(url.contains("type=list"));
        assert!(url.contains("q=rust+lang"));
        assert!(url.contains("kl=wt-wt"));
    }

    #[test]
    fn duckduckgo_region_passes_locale_through() {
        assert_eq!(duckduckgo_region("us-en"), "us-en");
        assert_eq!(duckduckgo_region("  "), DUCKDUCKGO_ALL_LOCALE);
    }

    #[test]
    fn backend_registry_knows_all_searxng_backends() {
        assert_eq!(BACKEND_NAMES.len(), 18);
        for name in BACKEND_NAMES {
            assert!(BACKEND_NAMES.contains(name), "{name}");
        }
        assert!(!BACKEND_NAMES.contains(&"not-a-backend"));
    }

    #[test]
    fn google_url_and_parse() {
        let url = GoogleBackend::build_url("rust lang", "en-US");
        assert!(url.contains("complete/search"));
        assert!(url.contains("q=rust+lang"));
        assert!(url.contains("hl=en"));
        let value = serde_json::json!([[["rust", 0], ["<b>rust</b> lang", 0]], null]);
        assert_eq!(
            parse_google_suggestions(&value),
            vec!["rust".to_string(), "rust lang".to_string()]
        );
    }

    #[test]
    fn wikipedia_url_and_parse() {
        let url = WikipediaBackend::build_url("rust", "de-DE");
        assert!(url.contains("de.wikipedia.org"));
        assert!(url.contains("action=opensearch"));
        let value = serde_json::json!(["rust", ["Rust", "Rust (Programmiersprache)"]]);
        assert_eq!(
            parse_opensearch_suggestions(&value),
            vec!["Rust".to_string(), "Rust (Programmiersprache)".to_string()]
        );
    }

    #[test]
    fn brave_url_and_rich_parse() {
        let url = BraveBackend::build_url("einstein");
        assert!(url.contains("search.brave.com/api/suggest"));
        assert!(url.contains("q=einstein"));
        assert!(url.contains("rich=true"));
        let value = serde_json::json!([
            "einstein",
            [
                {
                    "is_entity": true,
                    "q": "einstein",
                    "name": "Albert Einstein",
                    "desc": "German-born theoretical physicist",
                    "img": "https://imgs.search.brave.com/einstein.jpg"
                },
                {"is_entity": false, "q": "einstein iq"},
                "plain string fallback"
            ]
        ]);
        assert_eq!(
            parse_brave_suggestions(&value),
            vec![
                Suggestion {
                    text: "Albert Einstein".to_string(),
                    subtext: Some("German-born theoretical physicist".to_string()),
                    image: Some("https://imgs.search.brave.com/einstein.jpg".to_string()),
                },
                Suggestion::text("einstein iq"),
                Suggestion::text("plain string fallback"),
            ]
        );
    }

    #[test]
    fn brave_plain_opensearch_still_parses() {
        let value = serde_json::json!(["rust", ["rust lang", "rustc"]]);
        assert_eq!(
            parse_brave_suggestions(&value),
            vec![Suggestion::text("rust lang"), Suggestion::text("rustc")]
        );
    }

    #[test]
    fn startpage_url_and_parse() {
        let url = StartpageBackend::build_url("rust", "de");
        assert!(url.contains("startpage.com/suggestions"));
        assert!(url.contains("lui=deutsch"));
        let value = serde_json::json!(["rust", ["rust programming"]]);
        assert_eq!(
            parse_opensearch_suggestions(&value),
            vec!["rust programming".to_string()]
        );
    }
}
