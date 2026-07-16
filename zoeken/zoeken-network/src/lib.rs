//! Outbound HTTP network pools with browser fingerprinting, request routing, and Tor checks.

use std::collections::BTreeMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

use wreq::header::{COOKIE, HeaderMap, HeaderName, HeaderValue, SERVER};
use wreq::redirect;
use wreq::{Client, Method, Proxy, Response};
use wreq_util::{Emulation, EmulationOS, EmulationOption};
use zoeken_settings::{BoolOrString, NetworkSettings, OutgoingSettings, Proxies, StringOrVec};

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
    TooManyRequests { name: String, status: u16 },
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum EmulationProfile {
    #[default]
    Random,
    Fixed(Emulation),
}

impl EmulationProfile {
    #[must_use]
    pub fn new(emulation: Emulation) -> Self {
        Self::Fixed(emulation)
    }

    #[must_use]
    pub fn chrome() -> Self {
        Self::Fixed(Emulation::Chrome133)
    }

    #[must_use]
    pub fn firefox() -> Self {
        Self::Fixed(Emulation::Firefox136)
    }

    #[must_use]
    pub fn safari() -> Self {
        Self::Fixed(Emulation::Safari18)
    }

    #[must_use]
    pub fn resolve(self) -> EmulationOption {
        match self {
            Self::Random => weighted_random_emulation(),
            Self::Fixed(emulation) => EmulationOption::builder().emulation(emulation).build(),
        }
    }

    #[must_use]
    pub fn client_pool(self) -> Vec<EmulationOption> {
        match self {
            Self::Random => (0..RANDOM_PROFILE_POOL_SIZE)
                .map(|_| weighted_random_emulation())
                .collect(),
            Self::Fixed(emulation) => vec![EmulationOption::builder().emulation(emulation).build()],
        }
    }
}

const RANDOM_PROFILE_POOL_SIZE: usize = 8;

impl From<Emulation> for EmulationProfile {
    fn from(emulation: Emulation) -> Self {
        Self::Fixed(emulation)
    }
}

struct EmulationClass {
    weight: u32,
    platforms: &'static [EmulationOS],
    profiles: &'static [Emulation],
}

const EMULATION_CLASSES: &[EmulationClass] = &[
    EmulationClass {
        weight: 7141,
        platforms: &[EmulationOS::Windows, EmulationOS::MacOS, EmulationOS::Linux],
        profiles: &[
            Emulation::Chrome137,
            Emulation::Chrome136,
            Emulation::Chrome135,
            Emulation::Chrome134,
            Emulation::Chrome133,
            Emulation::Chrome132,
            Emulation::Chrome131,
        ],
    },
    EmulationClass {
        weight: 502,
        platforms: &[EmulationOS::Windows, EmulationOS::MacOS],
        profiles: &[
            Emulation::Edge134,
            Emulation::Edge131,
            Emulation::Edge127,
            Emulation::Edge122,
        ],
    },
    EmulationClass {
        weight: 173,
        platforms: &[EmulationOS::Windows, EmulationOS::MacOS, EmulationOS::Linux],
        profiles: &[
            Emulation::Opera119,
            Emulation::Opera118,
            Emulation::Opera117,
            Emulation::Opera116,
        ],
    },
];

fn chromium_header_order() -> Vec<HeaderName> {
    [
        "sec-ch-ua",
        "sec-ch-ua-mobile",
        "sec-ch-ua-platform",
        "upgrade-insecure-requests",
        "user-agent",
        "accept",
        "content-type",
        "content-length",
        "origin",
        "sec-fetch-site",
        "sec-fetch-mode",
        "sec-fetch-user",
        "sec-fetch-dest",
        "referer",
        "accept-encoding",
        "accept-language",
        "priority",
        "cookie",
    ]
    .into_iter()
    .map(HeaderName::from_static)
    .collect()
}

fn fast_random() -> u64 {
    use std::cell::Cell;
    use std::collections::hash_map::RandomState;
    use std::hash::{BuildHasher, Hasher};

    thread_local! {
        static KEY: RandomState = RandomState::new();
        static COUNTER: Cell<u64> = const { Cell::new(0) };
    }

    KEY.with(|key| {
        COUNTER.with(|ctr| {
            let n = ctr.get().wrapping_add(1);
            ctr.set(n);
            let mut h = key.build_hasher();
            h.write_u64(n);
            h.finish()
        })
    })
}

fn weighted_random_emulation() -> EmulationOption {
    let (r1, r2) = (fast_random(), fast_random());
    let total: u32 = EMULATION_CLASSES.iter().map(|c| c.weight).sum();
    let mut t = (r1 % total as u64) as u32;
    let class = EMULATION_CLASSES
        .iter()
        .find(|c| {
            t = t.checked_sub(c.weight).unwrap_or(u32::MAX);
            t == u32::MAX
        })
        .unwrap_or(&EMULATION_CLASSES[0]);
    let n = class.profiles.len();
    let idx = ((r1 >> 32) as usize % n).min((r2 >> 32) as usize % n);
    EmulationOption::builder()
        .emulation(class.profiles[idx])
        .emulation_os(class.platforms[(r2 as usize) % class.platforms.len()])
        .build()
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
            .field("headers", &self.headers)
            .field("local_addresses", &self.local_addresses)
            .field("enable_http2", &self.enable_http2)
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

async fn backoff_delay(attempt: u32) {
    tokio::time::sleep(backoff_duration(attempt)).await;
}

pub struct Network {
    config: NetworkConfig,
    clients: Vec<Client>,
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
        let cursor = self.next_rotation();
        let (client, proxy) = self.select(cursor);

        let max_attempts = self.config.retries.saturating_add(1);
        let mut attempt: u32 = 0;

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
            if let Some(timeout) = req.timeout {
                builder = builder.timeout(timeout);
            }
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
                    if !last_attempt && self.is_retryable_status(resp.status().as_u16()) {
                        backoff_delay(attempt).await;
                        continue;
                    }
                    return self.map_response(name, resp).await;
                }
                Err(source) => {
                    if !last_attempt {
                        backoff_delay(attempt).await;
                        continue;
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
    emulation: EmulationOption,
) -> Result<Client, NetworkError> {
    let redirect_policy = if config.max_redirects == 0 {
        redirect::Policy::none()
    } else {
        redirect::Policy::limited(config.max_redirects)
    };

    let mut builder = Client::builder()
        .emulation(emulation)
        .headers_order(chromium_header_order())
        .timeout(config.timeout)
        .redirect(redirect_policy)
        .cert_verification(config.verify);

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

#[derive(Debug)]
pub struct NetworkManager {
    default: Network,
    networks: BTreeMap<String, Network>,
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

        Ok(Self { default, networks })
    }

    #[must_use]
    pub fn get(&self, name: &str) -> &Network {
        self.networks.get(name).unwrap_or(&self.default)
    }

    pub async fn request(&self, net: &str, req: NetworkRequest) -> Result<Response, NetworkError> {
        self.get(net).request(net, req).await
    }

    pub async fn check_tor(&self, net: &str) -> Result<bool, NetworkError> {
        self.get(net).check_tor(net).await
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

#[cfg(test)]
mod tests {
    use super::*;
    use zoeken_settings::Settings;

    fn default_outgoing() -> OutgoingSettings {
        Settings::defaults().outgoing
    }

    #[test]
    fn emulation_profile_defaults_to_random() {
        assert_eq!(EmulationProfile::default(), EmulationProfile::Random);
        assert_eq!(
            EmulationProfile::chrome(),
            EmulationProfile::Fixed(Emulation::Chrome133)
        );
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
