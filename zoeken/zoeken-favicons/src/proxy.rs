//! Image proxy content policy: decides if an image may be served based on size and type.
//! Pure function for direct property testing and enforcement.

use std::net::IpAddr;

pub const DEFAULT_MAX_IMAGE_BYTES: u64 = 5 * 1024 * 1024;

/// Why a candidate image URL must not be fetched by the proxy.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProxyUrlRejection {
    InvalidUrl,
    DisallowedScheme,
    DisallowedHost,
}

impl ProxyUrlRejection {
    pub fn reason(self) -> &'static str {
        match self {
            ProxyUrlRejection::InvalidUrl => "invalid url",
            ProxyUrlRejection::DisallowedScheme => "disallowed url scheme",
            ProxyUrlRejection::DisallowedHost => "disallowed url host",
        }
    }
}

/// Reject non-http(s) URLs and obviously internal/metadata targets (SSRF guard).
pub fn validate_proxy_url(raw: &str) -> Result<(), ProxyUrlRejection> {
    let parsed = url::Url::parse(raw).map_err(|_| ProxyUrlRejection::InvalidUrl)?;
    match parsed.scheme() {
        "http" | "https" => {}
        _ => return Err(ProxyUrlRejection::DisallowedScheme),
    }
    let Some(host) = parsed.host_str() else {
        return Err(ProxyUrlRejection::InvalidUrl);
    };
    if is_blocked_host(host) {
        return Err(ProxyUrlRejection::DisallowedHost);
    }
    Ok(())
}

/// Reject favicon authorities that would resolve to loopback/private/metadata hosts.
pub fn validate_proxy_authority(authority: &str) -> Result<(), ProxyUrlRejection> {
    let authority = authority.trim();
    if authority.is_empty() || authority.contains('/') {
        return Err(ProxyUrlRejection::InvalidUrl);
    }
    // Parse host[:port] via the URL crate so `127.0.0.1:80` / `[::1]:443` work.
    let dummy = format!("http://{authority}/");
    let parsed = url::Url::parse(&dummy).map_err(|_| ProxyUrlRejection::InvalidUrl)?;
    let Some(host) = parsed.host_str() else {
        return Err(ProxyUrlRejection::InvalidUrl);
    };
    if is_blocked_host(host) {
        return Err(ProxyUrlRejection::DisallowedHost);
    }
    Ok(())
}

fn is_blocked_host(host: &str) -> bool {
    let lower = host
        .trim_end_matches('.')
        .trim_matches(['[', ']'])
        .to_ascii_lowercase();
    if lower.is_empty()
        || lower == "localhost"
        || lower.ends_with(".localhost")
        || lower == "metadata.google.internal"
        || lower.ends_with(".internal")
        || lower.ends_with(".local")
    {
        return true;
    }
    if let Ok(ip) = lower.parse::<IpAddr>() {
        return is_blocked_ip(ip);
    }
    false
}

fn is_blocked_ip(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => {
            v4.is_loopback()
                || v4.is_private()
                || v4.is_link_local()
                || v4.is_unspecified()
                || v4.is_broadcast()
                || matches!(v4.octets(), [100, 64..=127, ..]) // CGNAT
                || matches!(v4.octets(), [0, ..])
        }
        IpAddr::V6(v6) => {
            if let Some(v4) = v6.to_ipv4_mapped() {
                return is_blocked_ip(IpAddr::V4(v4));
            }
            v6.is_loopback()
                || v6.is_unique_local()
                || v6.is_unicast_link_local()
                || v6.is_unspecified()
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImageProxyPolicy {
    pub allowed_content_type_prefixes: Vec<String>,
    pub max_bytes: u64,
}

impl Default for ImageProxyPolicy {
    fn default() -> Self {
        Self {
            allowed_content_type_prefixes: vec![
                "image/".to_string(),
                "binary/octet-stream".to_string(),
            ],
            max_bytes: DEFAULT_MAX_IMAGE_BYTES,
        }
    }
}

impl ImageProxyPolicy {
    /// Build a policy with explicit allowed prefixes and size limit.
    pub fn new(allowed_content_type_prefixes: Vec<String>, max_bytes: u64) -> Self {
        Self {
            allowed_content_type_prefixes,
            max_bytes,
        }
    }

    fn content_type_allowed(&self, content_type: &str) -> bool {
        let normalized = content_type
            .split(';')
            .next()
            .unwrap_or("")
            .trim()
            .to_ascii_lowercase();
        if normalized.is_empty() {
            return false;
        }
        self.allowed_content_type_prefixes
            .iter()
            .any(|prefix| normalized.starts_with(&prefix.to_ascii_lowercase()))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProxyRejection {
    MissingContentType,
    DisallowedContentType,
    TooLarge,
}

impl ImageProxyRejection {
    /// A short human-readable reason (used in the HTTP error body).
    pub fn reason(self) -> &'static str {
        match self {
            ImageProxyRejection::MissingContentType => "missing content type",
            ImageProxyRejection::DisallowedContentType => "disallowed content type",
            ImageProxyRejection::TooLarge => "image exceeds maximum size",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ImageProxyDecision {
    Serve,
    Reject(ImageProxyRejection),
}

impl ImageProxyDecision {
    /// Whether the decision is to serve the image.
    pub fn is_serve(self) -> bool {
        matches!(self, ImageProxyDecision::Serve)
    }
}

/// Decide whether an image may be proxied: content type allowed and size within limit.
pub fn image_proxy_decision(
    content_type: Option<&str>,
    size: Option<u64>,
    policy: &ImageProxyPolicy,
) -> ImageProxyDecision {
    if let Some(bytes) = size
        && bytes > policy.max_bytes
    {
        return ImageProxyDecision::Reject(ImageProxyRejection::TooLarge);
    }

    match content_type {
        None => ImageProxyDecision::Reject(ImageProxyRejection::MissingContentType),
        Some(ct) if policy.content_type_allowed(ct) => ImageProxyDecision::Serve,
        Some(_) => ImageProxyDecision::Reject(ImageProxyRejection::DisallowedContentType),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn serves_image_within_limits() {
        let policy = ImageProxyPolicy::default();
        assert_eq!(
            image_proxy_decision(Some("image/png"), Some(1024), &policy),
            ImageProxyDecision::Serve
        );
    }

    #[test]
    fn serves_binary_octet_stream() {
        let policy = ImageProxyPolicy::default();
        assert_eq!(
            image_proxy_decision(Some("binary/octet-stream"), Some(1024), &policy),
            ImageProxyDecision::Serve
        );
    }

    #[test]
    fn content_type_parameters_are_ignored() {
        let policy = ImageProxyPolicy::default();
        assert_eq!(
            image_proxy_decision(Some("image/jpeg; charset=binary"), Some(10), &policy),
            ImageProxyDecision::Serve
        );
    }

    #[test]
    fn rejects_disallowed_content_type() {
        let policy = ImageProxyPolicy::default();
        assert_eq!(
            image_proxy_decision(Some("text/html"), Some(10), &policy),
            ImageProxyDecision::Reject(ImageProxyRejection::DisallowedContentType)
        );
    }

    #[test]
    fn rejects_missing_content_type() {
        let policy = ImageProxyPolicy::default();
        assert_eq!(
            image_proxy_decision(None, Some(10), &policy),
            ImageProxyDecision::Reject(ImageProxyRejection::MissingContentType)
        );
    }

    #[test]
    fn rejects_oversized_image() {
        let policy = ImageProxyPolicy::default();
        assert_eq!(
            image_proxy_decision(
                Some("image/png"),
                Some(DEFAULT_MAX_IMAGE_BYTES + 1),
                &policy
            ),
            ImageProxyDecision::Reject(ImageProxyRejection::TooLarge)
        );
    }

    #[test]
    fn oversized_takes_precedence_over_content_type() {
        let policy = ImageProxyPolicy::default();
        assert_eq!(
            image_proxy_decision(
                Some("text/html"),
                Some(DEFAULT_MAX_IMAGE_BYTES + 1),
                &policy
            ),
            ImageProxyDecision::Reject(ImageProxyRejection::TooLarge)
        );
    }

    #[test]
    fn unknown_size_with_allowed_type_is_served() {
        let policy = ImageProxyPolicy::default();
        assert_eq!(
            image_proxy_decision(Some("image/gif"), None, &policy),
            ImageProxyDecision::Serve
        );
    }

    #[test]
    fn size_exactly_at_limit_is_served() {
        let policy = ImageProxyPolicy::default();
        assert_eq!(
            image_proxy_decision(Some("image/png"), Some(DEFAULT_MAX_IMAGE_BYTES), &policy),
            ImageProxyDecision::Serve
        );
    }

    #[test]
    fn accepts_public_https_url() {
        assert!(validate_proxy_url("https://cdn.example.com/a.png").is_ok());
    }

    #[test]
    fn rejects_file_scheme() {
        assert_eq!(
            validate_proxy_url("file:///etc/passwd"),
            Err(ProxyUrlRejection::DisallowedScheme)
        );
    }

    #[test]
    fn rejects_localhost_and_private_ips() {
        for url in [
            "http://localhost/a.png",
            "http://127.0.0.1/a.png",
            "http://10.0.0.1/a.png",
            "http://192.168.1.1/a.png",
            "http://172.16.0.1/a.png",
            "http://169.254.169.254/latest/meta-data",
            "http://[::1]/img",
            "http://[fc00::1]/img",
        ] {
            assert_eq!(
                validate_proxy_url(url),
                Err(ProxyUrlRejection::DisallowedHost),
                "{url}"
            );
        }
    }

    #[test]
    fn rejects_blocked_favicon_authorities() {
        for authority in [
            "localhost",
            "127.0.0.1",
            "127.0.0.1:80",
            "localhost:443",
            "[::1]",
            "[::1]:443",
            "10.1.2.3",
            "10.1.2.3:8080",
            "169.254.169.254",
        ] {
            assert_eq!(
                validate_proxy_authority(authority),
                Err(ProxyUrlRejection::DisallowedHost),
                "{authority}"
            );
        }
        assert!(validate_proxy_authority("example.com").is_ok());
        assert!(validate_proxy_authority("example.com:443").is_ok());
    }
}
