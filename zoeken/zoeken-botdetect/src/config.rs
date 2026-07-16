//! Configuration for bot detection and rate limiting.

use std::net::IpAddr;
use std::str::FromStr;

use ipnet::IpNet;
use serde::Deserialize;

/// Error returned when `limiter.toml` cannot be parsed.
#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
    /// The TOML text was syntactically invalid.
    #[error("failed to parse limiter.toml: {0}")]
    Toml(#[from] toml::de::Error),
}

pub const DEFAULT_IPV4_PREFIX: u8 = 32;
pub const DEFAULT_IPV6_PREFIX: u8 = 48;

pub const RESERVED_PASS_NETS: &[&str] = &["167.235.158.251", "2a01:04f8:1c1c:8fc2::/64"];

/// Fully-resolved limiter configuration used by the detector.
#[derive(Debug, Clone)]
pub struct LimiterConfig {
    pub enabled: bool,
    pub ipv4_prefix: u8,
    pub ipv6_prefix: u8,
    pub trusted_proxies: Vec<IpNet>,
    pub filter_link_local: bool,
    pub link_token: bool,
    pub block_ip: Vec<IpNet>,
    pub pass_ip: Vec<IpNet>,
    pub pass_reserved_nets: bool,
    pub rate_limit: RateLimitConfig,
    pub heuristics: HeaderHeuristics,
}

impl Default for LimiterConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            ipv4_prefix: DEFAULT_IPV4_PREFIX,
            ipv6_prefix: DEFAULT_IPV6_PREFIX,
            trusted_proxies: Vec::new(),
            filter_link_local: false,
            link_token: false,
            block_ip: Vec::new(),
            pass_ip: Vec::new(),
            pass_reserved_nets: true,
            rate_limit: RateLimitConfig::default(),
            heuristics: HeaderHeuristics::default(),
        }
    }
}

/// Token-bucket parameters.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RateLimitConfig {
    pub capacity: f64,
    pub refill_per_second: f64,
    pub suspicious_capacity: f64,
    pub suspicious_refill_per_second: f64,
}

impl Default for RateLimitConfig {
    fn default() -> Self {
        Self {
            capacity: 15.0,
            refill_per_second: 150.0 / 600.0,
            suspicious_capacity: 2.0,
            suspicious_refill_per_second: 10.0 / 600.0,
        }
    }
}

impl RateLimitConfig {
    pub fn params(&self, suspicious: bool) -> (f64, f64) {
        if suspicious {
            (self.suspicious_capacity, self.suspicious_refill_per_second)
        } else {
            (self.capacity, self.refill_per_second)
        }
    }
}

/// Toggle set for request-header heuristics.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct HeaderHeuristics {
    pub accept: bool,
    pub accept_encoding: bool,
    pub accept_language: bool,
    pub connection: bool,
    pub sec_fetch: bool,
    pub user_agent: bool,
}

impl Default for HeaderHeuristics {
    fn default() -> Self {
        Self {
            accept: true,
            accept_encoding: true,
            accept_language: true,
            connection: true,
            sec_fetch: true,
            user_agent: true,
        }
    }
}

#[derive(Debug, Default, Deserialize)]
struct RawFile {
    #[serde(default)]
    botdetection: RawBotdetection,
}

#[derive(Debug, Default, Deserialize)]
struct RawBotdetection {
    ipv4_prefix: Option<u8>,
    ipv6_prefix: Option<u8>,
    #[serde(default)]
    trusted_proxies: Vec<String>,
    #[serde(default)]
    ip_limit: RawIpLimit,
    #[serde(default)]
    ip_lists: RawIpLists,
    #[serde(default)]
    rate_limit: RawRateLimit,
    #[serde(default)]
    header_heuristics: RawHeuristics,
}

#[derive(Debug, Default, Deserialize)]
struct RawIpLimit {
    filter_link_local: Option<bool>,
    link_token: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RawIpLists {
    #[serde(default)]
    block_ip: Vec<String>,
    #[serde(default)]
    pass_ip: Vec<String>,
    pass_reserved_nets: Option<bool>,
}

#[derive(Debug, Default, Deserialize)]
struct RawRateLimit {
    capacity: Option<f64>,
    refill_per_second: Option<f64>,
    suspicious_capacity: Option<f64>,
    suspicious_refill_per_second: Option<f64>,
}

#[derive(Debug, Default, Deserialize)]
struct RawHeuristics {
    accept: Option<bool>,
    accept_encoding: Option<bool>,
    accept_language: Option<bool>,
    connection: Option<bool>,
    sec_fetch: Option<bool>,
    user_agent: Option<bool>,
}

/// Parse a bare address or CIDR into a network.
pub fn parse_ip_or_net(s: &str) -> Option<IpNet> {
    let s = s.trim();
    if let Ok(net) = IpNet::from_str(s) {
        return Some(net);
    }
    match IpAddr::from_str(s) {
        Ok(IpAddr::V4(v4)) => IpNet::new(IpAddr::V4(v4), 32).ok(),
        Ok(IpAddr::V6(v6)) => IpNet::new(IpAddr::V6(v6), 128).ok(),
        Err(_) => None,
    }
}

fn parse_net_list(entries: &[String]) -> Vec<IpNet> {
    entries
        .iter()
        .filter_map(|e| {
            let net = parse_ip_or_net(e);
            if net.is_none() {
                tracing::warn!(entry = %e, "ignoring invalid IP/network in limiter.toml");
            }
            net
        })
        .collect()
}

impl LimiterConfig {
    /// Parse `limiter.toml` text.
    pub fn from_toml_str(toml_text: &str) -> Result<Self, ConfigError> {
        let raw: RawFile = toml::from_str(toml_text)?;
        let bd = raw.botdetection;

        let rate_default = RateLimitConfig::default();
        let heur_default = HeaderHeuristics::default();

        Ok(Self {
            enabled: true,
            ipv4_prefix: bd.ipv4_prefix.unwrap_or(DEFAULT_IPV4_PREFIX),
            ipv6_prefix: bd.ipv6_prefix.unwrap_or(DEFAULT_IPV6_PREFIX),
            trusted_proxies: parse_net_list(&bd.trusted_proxies),
            filter_link_local: bd.ip_limit.filter_link_local.unwrap_or(false),
            link_token: bd.ip_limit.link_token.unwrap_or(false),
            block_ip: parse_net_list(&bd.ip_lists.block_ip),
            pass_ip: parse_net_list(&bd.ip_lists.pass_ip),
            pass_reserved_nets: bd.ip_lists.pass_reserved_nets.unwrap_or(true),
            rate_limit: RateLimitConfig {
                capacity: bd.rate_limit.capacity.unwrap_or(rate_default.capacity),
                refill_per_second: bd
                    .rate_limit
                    .refill_per_second
                    .unwrap_or(rate_default.refill_per_second),
                suspicious_capacity: bd
                    .rate_limit
                    .suspicious_capacity
                    .unwrap_or(rate_default.suspicious_capacity),
                suspicious_refill_per_second: bd
                    .rate_limit
                    .suspicious_refill_per_second
                    .unwrap_or(rate_default.suspicious_refill_per_second),
            },
            heuristics: HeaderHeuristics {
                accept: bd.header_heuristics.accept.unwrap_or(heur_default.accept),
                accept_encoding: bd
                    .header_heuristics
                    .accept_encoding
                    .unwrap_or(heur_default.accept_encoding),
                accept_language: bd
                    .header_heuristics
                    .accept_language
                    .unwrap_or(heur_default.accept_language),
                connection: bd
                    .header_heuristics
                    .connection
                    .unwrap_or(heur_default.connection),
                sec_fetch: bd
                    .header_heuristics
                    .sec_fetch
                    .unwrap_or(heur_default.sec_fetch),
                user_agent: bd
                    .header_heuristics
                    .user_agent
                    .unwrap_or(heur_default.user_agent),
            },
        })
    }

    pub fn with_enabled(mut self, enabled: bool) -> Self {
        self.enabled = enabled;
        self
    }

    pub fn reserved_pass_nets(&self) -> Vec<IpNet> {
        if !self.pass_reserved_nets {
            return Vec::new();
        }
        RESERVED_PASS_NETS
            .iter()
            .filter_map(|s| parse_ip_or_net(s))
            .collect()
    }
}
