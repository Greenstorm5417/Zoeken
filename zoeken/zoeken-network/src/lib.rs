//! Outbound HTTP network pools with browser fingerprinting, request routing, and Tor checks.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use rand::Rng;
use wreq::header::{COOKIE, HeaderMap, HeaderValue, RETRY_AFTER, SERVER};
use wreq::redirect;
use wreq::{Client, Method, Proxy, Response};
use wreq_util::{Emulation, Profile};
use zoeken_settings::{BoolOrString, NetworkSettings, OutgoingSettings, Proxies, StringOrVec};
use zoeken_storage::{OriginPolicy, PermitDecision, Storage};

/// Tor routing check endpoint.
pub const TOR_CHECK_URL: &str = "https://check.torproject.org/api/ip";

/// Base delay for retry backoff.
const RETRY_BACKOFF_BASE: Duration = Duration::from_millis(100);

/// Upper bound on retry backoff.
const RETRY_BACKOFF_MAX: Duration = Duration::from_secs(2);

/// Global default network name.
pub const DEFAULT_NETWORK: &str = "__DEFAULT__";

#[derive(Debug, thiserror::Error)]
pub enum NetworkError {
    #[error("failed to build HTTP client for network '{name}': {source}")]
    ClientBuild {
        name: String,
        #[source]
        source: wreq::Error,
    },
    #[error("network '{name}' references unknown network '{target}'")]
    UnknownReference { name: String, target: String },
    #[error("transport error on network '{name}': {source}")]
    Transport {
        name: String,
        #[source]
        source: wreq::Error,
    },
    #[error("access denied on network '{name}' (HTTP {status})")]
    AccessDenied { name: String, status: u16 },
    #[error("cloudflare access denied on network '{name}' (HTTP {status})")]
    CloudflareAccessDenied { name: String, status: u16 },
    #[error("too many requests on network '{name}' (HTTP {status})")]
    TooManyRequests {
        name: String,
        status: u16,
        retry_after: Option<Duration>,
    },
    #[error("captcha challenge on network '{name}' (HTTP {status})")]
    Captcha { name: String, status: u16 },
    #[error("cloudflare captcha on network '{name}' (HTTP {status})")]
    CloudflareCaptcha { name: String, status: u16 },
    #[error("recaptcha captcha on network '{name}' (HTTP {status})")]
    RecaptchaCaptcha { name: String, status: u16 },
    #[error("HTTP error on network '{name}' (HTTP {status})")]
    HttpStatus { name: String, status: u16 },
    #[error("network '{name}' is configured for Tor but is not routing through Tor")]
    Tor { name: String },
    #[error("outbound request queue deadline expired for origin '{origin}'")]
    QueueExpired { origin: String },
    #[error("outbound coordination storage is unavailable")]
    CoordinationUnavailable,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmulationProfile {
    #[default]
    Random,
    Fixed(Profile),
}

impl EmulationProfile {
    #[must_use]
    pub fn new(profile: Profile) -> Self {
        Self::Fixed(profile)
    }

    #[must_use]
    pub fn chrome() -> Self {
        Self::Fixed(Profile::Chrome133)
    }

    #[must_use]
    pub fn firefox() -> Self {
        Self::Fixed(Profile::Firefox136)
    }

    #[must_use]
    pub fn safari() -> Self {
        Self::Fixed(Profile::Safari18)
    }

    #[must_use]
    pub fn resolve(self) -> Emulation {
        match self {
            Self::Random => Emulation::weighted_random(),
            Self::Fixed(profile) => Emulation::builder().profile(profile).build(),
        }
    }

    #[must_use]
    pub fn client_pool(self) -> Vec<Emulation> {
        match self {
            Self::Random => (0..RANDOM_PROFILE_POOL_SIZE)
                .map(|_| Emulation::weighted_random())
                .collect(),
            Self::Fixed(profile) => vec![Emulation::builder().profile(profile).build()],
        }
    }
}

const RANDOM_PROFILE_POOL_SIZE: usize = 8;

impl From<Profile> for EmulationProfile {
    fn from(profile: Profile) -> Self {
        Self::Fixed(profile)
    }
}

#[derive(Clone)]
pub struct NetworkConfig {
    pub timeout: Duration,
    pub retries: u32,
    pub retry_on_http_error: Vec<u16>,
    pub proxies: Vec<Proxy>,
    pub max_redirects: usize,
    pub verify: bool,
    pub headers: HeaderMap,
    pub local_addresses: Vec<IpAddr>,
    pub enable_http2: bool,
    pub pool_connections: usize,
    pub pool_maxsize: usize,
    pub keepalive_expiry: Duration,
    pub using_tor_proxy: bool,
    pub emulation: EmulationProfile,
}

impl Default for NetworkConfig {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(3),
            retries: 0,
            retry_on_http_error: Vec::new(),
            proxies: Vec::new(),
            max_redirects: 30,
            verify: true,
            headers: HeaderMap::new(),
            local_addresses: Vec::new(),
            enable_http2: true,
            pool_connections: 100,
            pool_maxsize: 10,
            keepalive_expiry: Duration::from_secs(5),
            using_tor_proxy: false,
            emulation: EmulationProfile::default(),
        }
    }
}

impl std::fmt::Debug for NetworkConfig {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // `wreq::Proxy` is not `Debug`; summarize it by count instead.
        f.debug_struct("NetworkConfig")
            .field("timeout", &self.timeout)
            .field("retries", &self.retries)
            .field("retry_on_http_error", &self.retry_on_http_error)
            .field(
                "proxies",
                &format_args!("[{} proxy(ies)]", self.proxies.len()),
            )
            .field("max_redirects", &self.max_redirects)
            .field("verify", &self.verify)
            .field(
                "header_names",
                &self
                    .headers
                    .keys()
                    .map(|name| name.as_str())
                    .collect::<Vec<_>>(),
            )
            .field("local_addresses", &self.local_addresses)
            .field("enable_http2", &self.enable_http2)
            .field("pool_connections", &self.pool_connections)
            .field("pool_maxsize", &self.pool_maxsize)
            .field("keepalive_expiry", &self.keepalive_expiry)
            .field("using_tor_proxy", &self.using_tor_proxy)
            .field("emulation", &self.emulation)
            .finish()
    }
}

impl NetworkConfig {
    #[must_use]
    pub fn from_outgoing(outgoing: &OutgoingSettings) -> Self {
        Self {
            timeout: duration_from_secs_f64(outgoing.request_timeout),
            retries: outgoing.retries,
            retry_on_http_error: Vec::new(),
            proxies: proxies_to_vec(outgoing.proxies.as_ref()),
            max_redirects: outgoing.max_redirects as usize,
            verify: verify_flag(outgoing.verify.as_ref()),
            headers: HeaderMap::new(),
            local_addresses: source_ips_to_addrs(outgoing.source_ips.as_ref()),
            enable_http2: outgoing.enable_http2,
            pool_connections: outgoing.pool_connections as usize,
            pool_maxsize: outgoing.pool_maxsize as usize,
            keepalive_expiry: duration_from_secs_f64(outgoing.keepalive_expiry),
            using_tor_proxy: outgoing.using_tor_proxy,
            emulation: EmulationProfile::default(),
        }
    }

    #[must_use]
    pub fn from_network_settings(outgoing: &OutgoingSettings, network: &NetworkSettings) -> Self {
        let mut cfg = Self::from_outgoing(outgoing);

        if let Some(timeout) = network.request_timeout {
            cfg.timeout = duration_from_secs_f64(timeout);
        }
        if let Some(enable_http2) = network.enable_http2 {
            cfg.enable_http2 = enable_http2;
        }
        if let Some(pool_connections) = network.pool_connections {
            cfg.pool_connections = pool_connections as usize;
        }
        if let Some(pool_maxsize) = network.pool_maxsize {
            cfg.pool_maxsize = pool_maxsize as usize;
        }
        if let Some(keepalive_expiry) = network.keepalive_expiry {
            cfg.keepalive_expiry = duration_from_secs_f64(keepalive_expiry);
        }
        if let Some(verify) = network.verify.as_ref() {
            cfg.verify = verify_flag(Some(verify));
        }
        if let Some(max_redirects) = network.max_redirects {
            cfg.max_redirects = max_redirects as usize;
        }
        if let Some(retries) = network.retries {
            cfg.retries = retries;
        }
        if let Some(retry_on_http_error) = network.retry_on_http_error.as_ref() {
            cfg.retry_on_http_error = retry_on_http_error.clone();
        }
        if network.proxies.is_some() {
            cfg.proxies = proxies_to_vec(network.proxies.as_ref());
        }
        if network.source_ips.is_some() {
            cfg.local_addresses = source_ips_to_addrs(network.source_ips.as_ref());
        }
        if let Some(using_tor_proxy) = network.using_tor_proxy {
            cfg.using_tor_proxy = using_tor_proxy;
        }

        cfg
    }
}

fn duration_from_secs_f64(secs: f64) -> Duration {
    if secs.is_finite() && secs > 0.0 {
        Duration::from_secs_f64(secs)
    } else {
        Duration::from_secs(0)
    }
}

fn verify_flag(verify: Option<&BoolOrString>) -> bool {
    match verify {
        Some(BoolOrString::Bool(value)) => *value,
        Some(BoolOrString::Str(_)) => true,
        None => true,
    }
}

fn source_ips_to_addrs(source_ips: Option<&StringOrVec>) -> Vec<IpAddr> {
    let mut out = Vec::new();
    let mut push = |raw: &str| {
        if let Ok(addr) = raw.trim().parse::<IpAddr>() {
            out.push(addr);
        }
    };
    match source_ips {
        Some(StringOrVec::One(value)) => push(value),
        Some(StringOrVec::Many(values)) => values.iter().for_each(|v| push(v)),
        None => {}
    }
    out
}

fn proxies_to_vec(proxies: Option<&Proxies>) -> Vec<Proxy> {
    let mut out = Vec::new();
    match proxies {
        Some(Proxies::Single(url)) => {
            if let Ok(proxy) = Proxy::all(url.as_str()) {
                out.push(proxy);
            }
        }
        Some(Proxies::Map(map)) => {
            for (scheme, urls) in map {
                let url_list = match urls {
                    StringOrVec::One(u) => vec![u.clone()],
                    StringOrVec::Many(us) => us.clone(),
                };
                for url in url_list {
                    let made = match scheme.trim_end_matches(':') {
                        "http" => Proxy::http(url.as_str()),
                        "https" => Proxy::https(url.as_str()),
                        _ => Proxy::all(url.as_str()),
                    };
                    if let Ok(proxy) = made {
                        out.push(proxy);
                    }
                }
            }
        }
        None => {}
    }
    out
}

#[derive(Debug, Clone)]
pub struct NetworkRequest {
    pub method: Method,
    pub url: String,
    pub headers: HeaderMap,
    pub body: Option<Vec<u8>>,
    pub cookies: Vec<(String, String)>,
    pub timeout: Option<Duration>,
    pub raise_for_httperror: bool,
    pub max_redirects: Option<usize>,
    pub verify: Option<bool>,
}

impl NetworkRequest {
    #[must_use]
    pub fn new(method: Method, url: impl Into<String>) -> Self {
        Self {
            method,
            url: url.into(),
            headers: HeaderMap::new(),
            body: None,
            cookies: Vec::new(),
            timeout: None,
            raise_for_httperror: true,
            max_redirects: None,
            verify: None,
        }
    }

    #[must_use]
    pub fn get(url: impl Into<String>) -> Self {
        Self::new(Method::GET, url)
    }

    #[must_use]
    pub fn post(url: impl Into<String>) -> Self {
        Self::new(Method::POST, url)
    }

    #[must_use]
    pub fn with_headers(mut self, headers: HeaderMap) -> Self {
        self.headers = headers;
        self
    }

    #[must_use]
    pub fn with_body(mut self, body: impl Into<Vec<u8>>) -> Self {
        self.body = Some(body.into());
        self
    }

    #[must_use]
    pub fn cookie(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.cookies.push((name.into(), value.into()));
        self
    }

    #[must_use]
    pub fn with_cookies(mut self, cookies: Vec<(String, String)>) -> Self {
        self.cookies = cookies;
        self
    }

    #[must_use]
    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    #[must_use]
    pub fn with_raise_for_httperror(mut self, raise: bool) -> Self {
        self.raise_for_httperror = raise;
        self
    }

    #[must_use]
    pub fn with_max_redirects(mut self, max_redirects: usize) -> Self {
        self.max_redirects = Some(max_redirects);
        self
    }

    #[must_use]
    pub fn with_verify(mut self, verify: bool) -> Self {
        self.verify = Some(verify);
        self
    }

    fn cookie_header(&self) -> Option<HeaderValue> {
        if self.cookies.is_empty() {
            return None;
        }
        let joined = self
            .cookies
            .iter()
            .map(|(name, value)| format!("{name}={value}"))
            .collect::<Vec<_>>()
            .join("; ");
        HeaderValue::from_str(&joined).ok()
    }
}

fn is_cloudflare_challenge(status: u16, body: &str) -> bool {
    if matches!(status, 429 | 503) {
        if body.contains("__cf_chl_jschl_tk__=") {
            return true;
        }
        if body.contains("/cdn-cgi/challenge-platform/")
            && body.contains("orchestrate/jsch/v1")
            && body.contains("window._cf_chl_enter(")
        {
            return true;
        }
    }
    status == 403 && body.contains("__cf_chl_captcha_tk__=")
}

fn is_cloudflare_firewall(status: u16, body: &str) -> bool {
    status == 403 && body.contains("<span class=\"cf-error-code\">1020</span>")
}

fn backoff_duration(attempt: u32) -> Duration {
    let shift = attempt.saturating_sub(1).min(16);
    let scaled = RETRY_BACKOFF_BASE
        .checked_mul(1u32 << shift)
        .unwrap_or(RETRY_BACKOFF_MAX);
    scaled.min(RETRY_BACKOFF_MAX)
}

async fn backoff_delay(attempt: u32, remaining: Duration) -> bool {
    let ceiling = backoff_duration(attempt).min(remaining);
    if ceiling.is_zero() {
        return false;
    }
    let millis = ceiling.as_millis().min(u128::from(u64::MAX)) as u64;
    let jitter = rand::rng().random_range(0..=millis);
    tokio::time::sleep(Duration::from_millis(jitter)).await;
    true
}

fn parse_retry_after(value: &str) -> Option<Duration> {
    if let Ok(seconds) = value.trim().parse::<u64>() {
        return Some(Duration::from_secs(seconds));
    }
    let date = httpdate::parse_http_date(value).ok()?;
    date.duration_since(std::time::SystemTime::now()).ok()
}

pub struct Network {
    config: NetworkConfig,
    /// Built as `addresses × emulations`: index = addr * emulations_per_addr + emu.
    clients: Vec<Client>,
    emulations_per_addr: usize,
    rotation: AtomicUsize,
}

impl Network {
    pub fn build(name: &str, config: NetworkConfig) -> Result<Self, NetworkError> {
        let emulations = config.emulation.client_pool();
        let addresses: Vec<Option<IpAddr>> = if config.local_addresses.is_empty() {
            vec![None]
        } else {
            config
                .local_addresses
                .iter()
                .map(|addr| Some(*addr))
                .collect()
        };

        let mut clients = Vec::with_capacity(addresses.len() * emulations.len());
        for addr in &addresses {
            for emulation in &emulations {
                clients.push(build_client(name, &config, *addr, emulation.clone())?);
            }
        }

        Ok(Self {
            config,
            clients,
            emulations_per_addr: emulations.len().max(1),
            rotation: AtomicUsize::new(0),
        })
    }

    #[must_use]
    pub fn config(&self) -> &NetworkConfig {
        &self.config
    }

    #[must_use]
    pub fn client(&self) -> &Client {
        &self.clients[0]
    }

    #[must_use]
    pub fn clients(&self) -> &[Client] {
        &self.clients
    }

    fn next_rotation(&self) -> usize {
        self.rotation.fetch_add(1, Ordering::Relaxed)
    }

    /// Pick a client for a request: source addresses keep rotating round-robin
    /// (`cursor`), but the emulation profile is stable per upstream host, so a
    /// given upstream always sees the same browser identity (TLS + header
    /// profile). Flapping between profiles per request from one IP is itself a
    /// detectable fingerprint.
    fn client_for(&self, cursor: usize, url: &str) -> &Client {
        use std::hash::{Hash, Hasher};
        let addr_groups = (self.clients.len() / self.emulations_per_addr).max(1);
        let addr_idx = cursor % addr_groups;
        let emu_idx = match url::Url::parse(url)
            .ok()
            .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        {
            Some(host) => {
                let mut hasher = std::collections::hash_map::DefaultHasher::new();
                host.hash(&mut hasher);
                (hasher.finish() as usize) % self.emulations_per_addr
            }
            None => 0,
        };
        &self.clients[addr_idx * self.emulations_per_addr + emu_idx]
    }

    fn select(&self, cursor: usize) -> (&Client, Option<&Proxy>) {
        let client = &self.clients[cursor % self.clients.len()];
        let proxy = if self.config.proxies.is_empty() {
            None
        } else {
            Some(&self.config.proxies[cursor % self.config.proxies.len()])
        };
        (client, proxy)
    }

    pub async fn request(&self, name: &str, req: NetworkRequest) -> Result<Response, NetworkError> {
        // Browser identity, source address and proxy are all stable for an
        // origin. Rotating any one of them per request creates a conflicting
        // fingerprint and prevents origin-scoped quotas from being meaningful.
        let cursor = stable_origin_hash(&req.url).unwrap_or_else(|| self.next_rotation());
        let client = self.client_for(cursor, &req.url);
        let proxy = if self.config.proxies.is_empty() {
            None
        } else {
            Some(&self.config.proxies[cursor % self.config.proxies.len()])
        };

        let max_attempts = self.config.retries.saturating_add(1);
        let mut attempt: u32 = 0;
        let request_budget = req.timeout.unwrap_or(self.config.timeout);
        let deadline = tokio::time::Instant::now() + request_budget;

        loop {
            attempt += 1;

            let mut builder = client.request(req.method.clone(), req.url.as_str());
            if let Some(max_redirects) = req.max_redirects {
                builder = if max_redirects == 0 {
                    builder.redirect(redirect::Policy::none())
                } else {
                    builder.redirect(redirect::Policy::limited(max_redirects))
                };
            }
            if !req.headers.is_empty() {
                builder = builder.headers(req.headers.clone());
            }
            if let Some(value) = req.cookie_header() {
                builder = builder.header(COOKIE, value);
            }
            if let Some(proxy) = proxy {
                builder = builder.proxy(proxy.clone());
            }
            let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
            builder = builder.timeout(remaining);
            if let Some(body) = req.body.clone() {
                builder = builder.body(body);
            }

            let outcome = builder.send().await;
            let last_attempt = attempt >= max_attempts;

            match outcome {
                Ok(resp) => {
                    if !req.raise_for_httperror {
                        return Ok(resp);
                    }
                    if !last_attempt
                        && !matches!(resp.status().as_u16(), 403 | 429)
                        && self.is_retryable_status(resp.status().as_u16())
                    {
                        let remaining =
                            deadline.saturating_duration_since(tokio::time::Instant::now());
                        if backoff_delay(attempt, remaining).await {
                            continue;
                        }
                    }
                    return self.map_response(name, resp).await;
                }
                Err(source) => {
                    if !last_attempt {
                        let remaining =
                            deadline.saturating_duration_since(tokio::time::Instant::now());
                        if backoff_delay(attempt, remaining).await {
                            continue;
                        }
                    }
                    return Err(NetworkError::Transport {
                        name: name.to_string(),
                        source,
                    });
                }
            }
        }
    }

    fn is_retryable_status(&self, status: u16) -> bool {
        self.config.retry_on_http_error.contains(&status)
    }

    async fn map_response(&self, name: &str, resp: Response) -> Result<Response, NetworkError> {
        let status = resp.status().as_u16();
        if status < 400 {
            return Ok(resp);
        }

        metrics::counter!("outbound_http_status_total", "category" => format!("{}xx", status / 100))
            .increment(1);
        let retry_after = resp
            .headers()
            .get(RETRY_AFTER)
            .and_then(|value| value.to_str().ok())
            .and_then(parse_retry_after);
        if let Some(delay) = retry_after {
            metrics::histogram!("outbound_retry_after_seconds").record(delay.as_secs_f64());
        }

        let server_is_cloudflare = resp
            .headers()
            .get(SERVER)
            .and_then(|v| v.to_str().ok())
            .is_some_and(|s| s.starts_with("cloudflare"));

        let body = if matches!(status, 403 | 429 | 503) {
            resp.text().await.unwrap_or_default()
        } else {
            String::new()
        };

        if server_is_cloudflare {
            if is_cloudflare_challenge(status, &body) {
                return Err(NetworkError::CloudflareCaptcha {
                    name: name.to_string(),
                    status,
                });
            }
            if is_cloudflare_firewall(status, &body) {
                return Err(NetworkError::CloudflareAccessDenied {
                    name: name.to_string(),
                    status,
                });
            }
        }
        if status == 503 && body.contains("\"https://www.google.com/recaptcha/") {
            return Err(NetworkError::RecaptchaCaptcha {
                name: name.to_string(),
                status,
            });
        }

        match status {
            401..=403 => Err(NetworkError::AccessDenied {
                name: name.to_string(),
                status,
            }),
            429 | 503 => Err(NetworkError::TooManyRequests {
                name: name.to_string(),
                status,
                retry_after,
            }),
            _ => Err(NetworkError::HttpStatus {
                name: name.to_string(),
                status,
            }),
        }
    }

    pub async fn check_tor(&self, name: &str) -> Result<bool, NetworkError> {
        if !self.config.using_tor_proxy {
            return Ok(false);
        }

        let cursor = self.next_rotation();
        let (client, proxy) = self.select(cursor);

        let mut builder = client.get(TOR_CHECK_URL).timeout(Duration::from_secs(60));
        if let Some(proxy) = proxy {
            builder = builder.proxy(proxy.clone());
        }

        let resp = builder
            .send()
            .await
            .map_err(|source| NetworkError::Transport {
                name: name.to_string(),
                source,
            })?;
        let payload: serde_json::Value =
            resp.json()
                .await
                .map_err(|source| NetworkError::Transport {
                    name: name.to_string(),
                    source,
                })?;

        Ok(payload
            .get("IsTor")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false))
    }

    pub async fn ensure_tor_routing(&self, name: &str) -> Result<(), NetworkError> {
        if !self.config.using_tor_proxy {
            return Ok(());
        }
        if self.check_tor(name).await? {
            Ok(())
        } else {
            Err(NetworkError::Tor {
                name: name.to_string(),
            })
        }
    }
}

fn stable_origin_hash(raw_url: &str) -> Option<usize> {
    use std::hash::{Hash, Hasher};

    let origin = url::Url::parse(raw_url)
        .ok()?
        .origin()
        .ascii_serialization();
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    origin.hash(&mut hasher);
    Some(hasher.finish() as usize)
}

impl std::fmt::Debug for Network {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Network")
            .field("config", &self.config)
            .field(
                "clients",
                &format_args!("[{} client(s)]", self.clients.len()),
            )
            .finish()
    }
}

fn build_client(
    name: &str,
    config: &NetworkConfig,
    local_address: Option<IpAddr>,
    emulation: Emulation,
) -> Result<Client, NetworkError> {
    let redirect_policy = if config.max_redirects == 0 {
        redirect::Policy::none()
    } else {
        redirect::Policy::limited(config.max_redirects)
    };

    let mut builder = Client::builder()
        .emulation(emulation)
        .timeout(config.timeout)
        .redirect(redirect_policy)
        .pool_max_size(config.pool_connections)
        .pool_max_idle_per_host(config.pool_maxsize)
        .pool_idle_timeout(config.keepalive_expiry)
        .tls_cert_verification(config.verify);

    if !config.enable_http2 {
        builder = builder.http1_only();
    }

    if let Some(addr) = local_address {
        builder = builder.local_address(Some(addr));
    }

    if !config.headers.is_empty() {
        builder = builder.default_headers(config.headers.clone());
    }

    builder.build().map_err(|source| NetworkError::ClientBuild {
        name: name.to_string(),
        source,
    })
}

pub struct NetworkManager {
    default: Network,
    networks: BTreeMap<String, Network>,
    coordinator: Option<Arc<dyn Storage>>,
    origin_policy: OriginPolicy,
}

impl std::fmt::Debug for NetworkManager {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("NetworkManager")
            .field("networks", &self.networks.keys().collect::<Vec<_>>())
            .field("coordinated", &self.coordinator.is_some())
            .field("origin_policy", &self.origin_policy)
            .finish_non_exhaustive()
    }
}

impl NetworkManager {
    pub fn from_settings(outgoing: &OutgoingSettings) -> Result<Self, NetworkError> {
        let default_cfg = NetworkConfig::from_outgoing(outgoing);
        let default = Network::build(DEFAULT_NETWORK, default_cfg.clone())?;

        let mut networks = BTreeMap::new();

        let mut ipv4_cfg = default_cfg.clone();
        ipv4_cfg.local_addresses = vec![IpAddr::V4(Ipv4Addr::UNSPECIFIED)];
        networks.insert("ipv4".to_string(), Network::build("ipv4", ipv4_cfg)?);

        let mut ipv6_cfg = default_cfg.clone();
        ipv6_cfg.local_addresses = vec![IpAddr::V6(Ipv6Addr::UNSPECIFIED)];
        networks.insert("ipv6".to_string(), Network::build("ipv6", ipv6_cfg)?);

        let mut references: Vec<(String, String)> = Vec::new();
        for (name, settings) in &outgoing.networks {
            if let Some(target) = &settings.network {
                references.push((name.clone(), target.clone()));
                continue;
            }
            let cfg = NetworkConfig::from_network_settings(outgoing, settings);
            networks.insert(name.clone(), Network::build(name, cfg)?);
        }

        if !networks.contains_key("image_proxy") {
            let mut image_proxy_cfg = default_cfg.clone();
            image_proxy_cfg.enable_http2 = false;
            networks.insert(
                "image_proxy".to_string(),
                Network::build("image_proxy", image_proxy_cfg)?,
            );
        }

        for (name, target) in references {
            let cfg = if target == DEFAULT_NETWORK {
                default.config().clone()
            } else if let Some(referenced) = networks.get(&target) {
                referenced.config().clone()
            } else {
                return Err(NetworkError::UnknownReference { name, target });
            };
            networks.insert(name.clone(), Network::build(&name, cfg)?);
        }

        let limits = &outgoing.origin_limits;
        Ok(Self {
            default,
            networks,
            coordinator: None,
            origin_policy: OriginPolicy {
                requests_per_second: limits.requests_per_second,
                burst: limits.burst,
                max_concurrent: limits.max_concurrent,
                lease_duration: Duration::from_secs(limits.lease_seconds),
            },
        })
    }

    /// Require storage-backed origin quotas for every request made by this manager.
    #[must_use]
    pub fn with_coordinator(mut self, storage: Arc<dyn Storage>) -> Self {
        self.coordinator = Some(storage);
        self
    }

    #[must_use]
    pub fn coordinator(&self) -> Option<Arc<dyn Storage>> {
        self.coordinator.clone()
    }

    #[must_use]
    pub fn get(&self, name: &str) -> &Network {
        self.networks.get(name).unwrap_or(&self.default)
    }

    pub async fn request(&self, net: &str, req: NetworkRequest) -> Result<Response, NetworkError> {
        let Some(storage) = &self.coordinator else {
            return self.get(net).request(net, req).await;
        };

        let origin = normalized_origin(&req.url).ok_or(NetworkError::CoordinationUnavailable)?;
        let wait_limit = req.timeout.unwrap_or(self.get(net).config.timeout);
        let deadline = tokio::time::Instant::now() + wait_limit;
        let started = std::time::Instant::now();

        let lease = loop {
            let permit = match storage.acquire_origin(&origin, &self.origin_policy).await {
                Ok(permit) => permit,
                Err(_) => {
                    metrics::counter!("outbound_permits_total", "outcome" => "storage_error")
                        .increment(1);
                    return Err(NetworkError::CoordinationUnavailable);
                }
            };
            match (permit.decision, permit.lease) {
                (PermitDecision::Granted, Some(lease)) => {
                    metrics::counter!("outbound_permits_total", "outcome" => "granted")
                        .increment(1);
                    break lease;
                }
                (
                    decision @ (PermitDecision::RateLimited | PermitDecision::ConcurrencyLimited),
                    _,
                ) => {
                    let outcome = match decision {
                        PermitDecision::RateLimited => "rate_wait",
                        PermitDecision::ConcurrencyLimited => "concurrency_wait",
                        PermitDecision::Granted => unreachable!(),
                    };
                    metrics::counter!("outbound_permits_total", "outcome" => outcome).increment(1);
                    let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
                    if remaining.is_zero() {
                        metrics::counter!("outbound_permits_total", "outcome" => "expired")
                            .increment(1);
                        return Err(NetworkError::QueueExpired { origin });
                    }
                    tokio::time::sleep(permit.retry_after.min(remaining)).await;
                }
                _ => return Err(NetworkError::CoordinationUnavailable),
            }
        };

        metrics::histogram!("outbound_permit_wait_seconds").record(started.elapsed().as_secs_f64());
        let request = self.get(net).request(net, req);
        tokio::pin!(request);
        let renewal_every = (self.origin_policy.lease_duration / 2).max(Duration::from_millis(100));
        let mut renewal = tokio::time::interval(renewal_every);
        renewal.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Delay);
        renewal.tick().await;
        let result = loop {
            tokio::select! {
                result = &mut request => break result,
                _ = renewal.tick() => {
                    match storage
                        .renew_origin(&lease, self.origin_policy.lease_duration)
                        .await
                    {
                        Ok(true) => {}
                        Ok(false) | Err(_) => {
                            metrics::counter!("outbound_permits_total", "outcome" => "renewal_failed")
                                .increment(1);
                            let _ = storage.release_origin(&lease).await;
                            return Err(NetworkError::CoordinationUnavailable);
                        }
                    }
                }
            }
        };
        if let Err(NetworkError::TooManyRequests {
            retry_after: Some(delay),
            ..
        }) = &result
            && storage.defer_origin(&origin, *delay).await.is_err()
        {
            let _ = storage.release_origin(&lease).await;
            return Err(NetworkError::CoordinationUnavailable);
        }
        if storage.release_origin(&lease).await.is_err() {
            return Err(NetworkError::CoordinationUnavailable);
        }
        result
    }

    pub async fn check_tor(&self, net: &str) -> Result<bool, NetworkError> {
        if !self.get(net).config.using_tor_proxy {
            return Ok(false);
        }
        let response = self
            .request(
                net,
                NetworkRequest::get(TOR_CHECK_URL).with_timeout(Duration::from_secs(60)),
            )
            .await?;
        let payload: serde_json::Value =
            response
                .json()
                .await
                .map_err(|source| NetworkError::Transport {
                    name: net.to_string(),
                    source,
                })?;
        Ok(payload
            .get("IsTor")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false))
    }

    #[must_use]
    pub fn default_network(&self) -> &Network {
        &self.default
    }

    #[must_use]
    pub fn contains(&self, name: &str) -> bool {
        self.networks.contains_key(name)
    }

    pub fn names(&self) -> impl Iterator<Item = &str> {
        self.networks.keys().map(String::as_str)
    }
}

fn normalized_origin(raw_url: &str) -> Option<String> {
    let url = url::Url::parse(raw_url).ok()?;
    match url.scheme() {
        "http" | "https" => Some(url.origin().ascii_serialization()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use zoeken_settings::Settings;
    use zoeken_storage::{
        EngineHealthSnapshot, EngineHealthUpdate, FaviconData, FaviconLookup, FaviconPolicy,
        OriginLease, PermitResult, StorageError,
    };

    struct UnavailableStorage;

    #[async_trait]
    impl Storage for UnavailableStorage {
        async fn healthcheck(&self) -> Result<(), StorageError> {
            Err(StorageError::InvalidConnectionConfig)
        }
        async fn acquire_origin(
            &self,
            _origin: &str,
            _policy: &OriginPolicy,
        ) -> Result<PermitResult, StorageError> {
            Err(StorageError::InvalidConnectionConfig)
        }
        async fn release_origin(&self, _lease: &OriginLease) -> Result<(), StorageError> {
            Err(StorageError::InvalidConnectionConfig)
        }
        async fn renew_origin(
            &self,
            _lease: &OriginLease,
            _lease_duration: Duration,
        ) -> Result<bool, StorageError> {
            Err(StorageError::InvalidConnectionConfig)
        }
        async fn defer_origin(&self, _origin: &str, _delay: Duration) -> Result<(), StorageError> {
            Err(StorageError::InvalidConnectionConfig)
        }
        async fn favicon_get(
            &self,
            _resolver: &str,
            _authority: &str,
        ) -> Result<FaviconLookup, StorageError> {
            Err(StorageError::InvalidConnectionConfig)
        }
        async fn favicon_put(
            &self,
            _resolver: &str,
            _authority: &str,
            _value: Option<&FaviconData>,
            _policy: &FaviconPolicy,
        ) -> Result<bool, StorageError> {
            Err(StorageError::InvalidConnectionConfig)
        }
        async fn record_engine_health(
            &self,
            _update: &EngineHealthUpdate,
        ) -> Result<(), StorageError> {
            Err(StorageError::InvalidConnectionConfig)
        }
        async fn latest_engine_health(
            &self,
            _engine: &str,
        ) -> Result<Option<EngineHealthSnapshot>, StorageError> {
            Err(StorageError::InvalidConnectionConfig)
        }
        async fn maintenance(&self, _favicon_max_total_bytes: usize) -> Result<(), StorageError> {
            Err(StorageError::InvalidConnectionConfig)
        }
    }

    fn default_outgoing() -> OutgoingSettings {
        Settings::defaults().outgoing
    }

    #[test]
    fn emulation_profile_defaults_to_random() {
        assert_eq!(EmulationProfile::default(), EmulationProfile::Random);
        assert_eq!(
            EmulationProfile::chrome(),
            EmulationProfile::Fixed(Profile::Chrome133)
        );
    }

    #[test]
    fn retry_after_accepts_seconds_and_http_dates() {
        assert_eq!(parse_retry_after("120"), Some(Duration::from_secs(120)));
        let future = std::time::SystemTime::now() + Duration::from_secs(60);
        let parsed = parse_retry_after(&httpdate::fmt_http_date(future)).unwrap();
        assert!(parsed >= Duration::from_secs(58));
        assert!(parsed <= Duration::from_secs(60));
        assert_eq!(parse_retry_after("not-a-date"), None);
    }

    #[tokio::test]
    async fn storage_outage_fails_closed_before_network_access() {
        let manager = NetworkManager::from_settings(&default_outgoing())
            .unwrap()
            .with_coordinator(Arc::new(UnavailableStorage));
        let result = manager
            .request(
                DEFAULT_NETWORK,
                NetworkRequest::get("https://example.com/private"),
            )
            .await;
        assert!(matches!(result, Err(NetworkError::CoordinationUnavailable)));
    }

    #[test]
    fn config_from_outgoing_maps_defaults() {
        let outgoing = default_outgoing();
        let cfg = NetworkConfig::from_outgoing(&outgoing);
        assert_eq!(cfg.timeout, Duration::from_secs_f64(3.0));
        assert_eq!(cfg.max_redirects, 30);
        assert_eq!(cfg.retries, 0);
        assert!(cfg.verify);
        assert!(cfg.enable_http2);
        assert!(cfg.proxies.is_empty());
        assert!(cfg.local_addresses.is_empty());
        assert!(cfg.retry_on_http_error.is_empty());
    }

    #[test]
    fn named_network_overlays_only_specified_fields() {
        let outgoing = default_outgoing();
        let ns = NetworkSettings {
            retries: Some(3),
            enable_http2: Some(false),
            retry_on_http_error: Some(vec![429, 503]),
            ..Default::default()
        };
        let cfg = NetworkConfig::from_network_settings(&outgoing, &ns);
        assert_eq!(cfg.retries, 3);
        assert!(!cfg.enable_http2);
        assert_eq!(cfg.retry_on_http_error, vec![429, 503]);
        assert_eq!(cfg.max_redirects, 30);
        assert_eq!(cfg.timeout, Duration::from_secs_f64(3.0));
    }

    #[test]
    fn source_ips_parsing_skips_non_ip_entries() {
        let many = StringOrVec::Many(vec![
            "127.0.0.1".to_string(),
            "::1".to_string(),
            "10.0.0.0/24".to_string(), // CIDR skipped at this stage
            "not-an-ip".to_string(),
        ]);
        let addrs = source_ips_to_addrs(Some(&many));
        assert_eq!(addrs.len(), 2);
        assert!(addrs.contains(&"127.0.0.1".parse::<IpAddr>().unwrap()));
        assert!(addrs.contains(&"::1".parse::<IpAddr>().unwrap()));
    }

    #[test]
    fn verify_flag_treats_ca_path_as_enabled() {
        assert!(verify_flag(None));
        assert!(verify_flag(Some(&BoolOrString::Bool(true))));
        assert!(!verify_flag(Some(&BoolOrString::Bool(false))));
        assert!(verify_flag(Some(&BoolOrString::Str(
            "/etc/ca.pem".to_string()
        ))));
    }

    #[test]
    fn manager_get_falls_back_to_default_for_unknown_name() {
        let outgoing = default_outgoing();
        let manager = NetworkManager::from_settings(&outgoing).expect("build manager");
        let fallback = manager.get("does-not-exist");
        assert!(std::ptr::eq(fallback, manager.default_network()));
        assert!(manager.contains("ipv4"));
        assert!(manager.contains("ipv6"));
        assert!(manager.contains("image_proxy"));
    }

    #[test]
    fn image_proxy_network_disables_http2() {
        let outgoing = default_outgoing();
        let manager = NetworkManager::from_settings(&outgoing).expect("build manager");
        assert!(!manager.get("image_proxy").config().enable_http2);
    }

    fn network_with(local_addresses: Vec<IpAddr>, proxies: Vec<Proxy>) -> Network {
        let cfg = NetworkConfig {
            local_addresses,
            proxies,
            emulation: EmulationProfile::chrome(),
            ..Default::default()
        };
        Network::build("test", cfg).expect("build network")
    }

    #[test]
    fn rotation_cycles_clients_in_order() {
        let addrs = vec![
            "127.0.0.1".parse().unwrap(),
            "127.0.0.2".parse().unwrap(),
            "127.0.0.3".parse().unwrap(),
        ];
        let net = network_with(addrs, Vec::new());
        let clients = net.clients();
        assert_eq!(clients.len(), 3);

        for round in 0..2 {
            for (expected, expected_client) in clients.iter().enumerate() {
                let cursor = net.next_rotation();
                let (client, proxy) = net.select(cursor);
                assert!(
                    std::ptr::eq(client, expected_client),
                    "round {round}: expected client index {expected}"
                );
                assert!(proxy.is_none(), "no proxies configured");
            }
        }
    }

    #[test]
    fn rotation_cycles_proxies_in_order() {
        let proxies = vec![
            Proxy::all("http://127.0.0.1:1").unwrap(),
            Proxy::all("http://127.0.0.1:2").unwrap(),
        ];
        let net = network_with(Vec::new(), proxies);
        assert_eq!(net.clients().len(), 1);
        for _ in 0..2 {
            for _ in 0..2 {
                let cursor = net.next_rotation();
                let (_client, proxy) = net.select(cursor);
                assert!(proxy.is_some());
            }
        }
        assert_eq!(net.rotation.load(Ordering::Relaxed), 4);
    }

    #[test]
    fn single_client_no_proxy_selects_index_zero() {
        let net = network_with(Vec::new(), Vec::new());
        for _ in 0..3 {
            let cursor = net.next_rotation();
            let (client, proxy) = net.select(cursor);
            assert!(std::ptr::eq(client, &net.clients()[0]));
            assert!(proxy.is_none());
        }
    }

    #[test]
    fn cookie_header_joins_pairs() {
        let req = NetworkRequest::get("https://example.invalid/")
            .cookie("a", "1")
            .cookie("b", "2");
        let value = req.cookie_header().expect("cookie header");
        assert_eq!(value.to_str().unwrap(), "a=1; b=2");
    }

    #[test]
    fn cookie_header_none_when_empty() {
        let req = NetworkRequest::get("https://example.invalid/");
        assert!(req.cookie_header().is_none());
    }

    #[test]
    fn cloudflare_challenge_detection_matches_reference() {
        assert!(is_cloudflare_challenge(
            503,
            "...__cf_chl_jschl_tk__=abc..."
        ));
        assert!(is_cloudflare_challenge(
            429,
            "...__cf_chl_jschl_tk__=abc..."
        ));
        let managed = "/cdn-cgi/challenge-platform/ orchestrate/jsch/v1 window._cf_chl_enter(";
        assert!(is_cloudflare_challenge(503, managed));
        assert!(!is_cloudflare_challenge(
            503,
            "/cdn-cgi/challenge-platform/ only"
        ));
        assert!(is_cloudflare_challenge(403, "x __cf_chl_captcha_tk__=zzz"));
        assert!(!is_cloudflare_challenge(200, "__cf_chl_jschl_tk__="));
    }

    #[test]
    fn cloudflare_firewall_detection_matches_reference() {
        assert!(is_cloudflare_firewall(
            403,
            "<span class=\"cf-error-code\">1020</span>"
        ));
        assert!(!is_cloudflare_firewall(403, "no marker"));
        assert!(!is_cloudflare_firewall(
            503,
            "<span class=\"cf-error-code\">1020</span>"
        ));
    }

    #[test]
    fn backoff_is_exponential_and_capped() {
        assert_eq!(backoff_duration(1), Duration::from_millis(100));
        assert_eq!(backoff_duration(2), Duration::from_millis(200));
        assert_eq!(backoff_duration(3), Duration::from_millis(400));
        assert_eq!(backoff_duration(99), RETRY_BACKOFF_MAX);
    }

    #[test]
    fn retryable_status_follows_config() {
        let cfg = NetworkConfig {
            retry_on_http_error: vec![429, 503],
            ..Default::default()
        };
        let net = Network::build("test", cfg).expect("build network");
        assert!(net.is_retryable_status(429));
        assert!(net.is_retryable_status(503));
        assert!(!net.is_retryable_status(200));
        assert!(!net.is_retryable_status(403));
    }

    #[tokio::test]
    async fn check_tor_skips_when_not_configured() {
        let net = network_with(Vec::new(), Vec::new());
        assert!(!net.check_tor("test").await.unwrap());
        net.ensure_tor_routing("test").await.unwrap();
    }
}
