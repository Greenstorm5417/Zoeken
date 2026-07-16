//! Client-IP derivation with support for trusted proxy chains.

use std::net::IpAddr;

use ipnet::IpNet;

/// Normalise an IPv4-mapped IPv6 address.
pub fn normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => match v6.to_ipv4_mapped() {
            Some(v4) => IpAddr::V4(v4),
            None => IpAddr::V6(v6),
        },
        other => other,
    }
}

/// Parse an `X-Forwarded-For` header value.
pub fn parse_forwarded_for(header_value: &str) -> Vec<IpAddr> {
    let mut out = Vec::new();
    for part in header_value.split(',') {
        let token = part.trim();
        if token.is_empty() {
            continue;
        }
        match token.parse::<IpAddr>() {
            Ok(ip) => out.push(normalize(ip)),
            Err(_) => return Vec::new(),
        }
    }
    out
}

/// Select the real client address from a forwarded chain.
pub fn trusted_remote_addr(
    x_forwarded_for: &[IpAddr],
    trusted_proxies: &[IpNet],
) -> Option<IpAddr> {
    if x_forwarded_for.is_empty() {
        return None;
    }
    for &addr in x_forwarded_for.iter().rev() {
        let trusted = trusted_proxies.iter().any(|net| net.contains(&addr));
        if !trusted {
            return Some(addr);
        }
    }
    Some(x_forwarded_for[0])
}

fn peer_is_trusted(peer: Option<IpAddr>, trusted_proxies: &[IpNet]) -> bool {
    match peer {
        Some(ip) => {
            let ip = normalize(ip);
            !trusted_proxies.is_empty() && trusted_proxies.iter().any(|net| net.contains(&ip))
        }
        None => false,
    }
}

/// Derive the client IP from forwarded headers or the peer address.
///
/// `X-Forwarded-For` / `X-Real-IP` are honored **only** when the TCP peer is in
/// `trusted_proxies`. Otherwise the peer address is used (spoofed headers ignored).
pub fn derive_client_ip(
    peer: Option<IpAddr>,
    x_forwarded_for: &[IpAddr],
    x_real_ip: Option<IpAddr>,
    trusted_proxies: &[IpNet],
) -> Option<IpAddr> {
    let peer = peer.map(normalize);
    if peer_is_trusted(peer, trusted_proxies) {
        if !x_forwarded_for.is_empty()
            && let Some(ip) = trusted_remote_addr(x_forwarded_for, trusted_proxies)
        {
            return Some(normalize(ip));
        }
        if let Some(ip) = x_real_ip {
            return Some(normalize(ip));
        }
    }
    peer
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn nets(entries: &[&str]) -> Vec<IpNet> {
        entries
            .iter()
            .map(|s| IpNet::from_str(s).unwrap())
            .collect()
    }

    fn ips(entries: &[&str]) -> Vec<IpAddr> {
        entries.iter().map(|s| s.parse().unwrap()).collect()
    }

    #[test]
    fn picks_first_untrusted_from_right_when_peer_trusted() {
        let xff = ips(&["203.0.113.9", "198.51.100.7", "10.0.0.1"]);
        let trusted = nets(&["10.0.0.0/8"]);
        let peer = "10.0.0.1".parse().unwrap();
        let ip = derive_client_ip(Some(peer), &xff, None, &trusted).unwrap();
        assert_eq!(ip, "198.51.100.7".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn walks_through_multiple_trusted_hops() {
        let xff = ips(&["203.0.113.9", "10.0.0.2", "10.0.0.1"]);
        let trusted = nets(&["10.0.0.0/8"]);
        let peer = "10.0.0.1".parse().unwrap();
        let ip = derive_client_ip(Some(peer), &xff, None, &trusted).unwrap();
        assert_eq!(ip, "203.0.113.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn all_trusted_falls_back_to_leftmost() {
        let xff = ips(&["10.0.0.3", "10.0.0.2", "10.0.0.1"]);
        let trusted = nets(&["10.0.0.0/8"]);
        let peer = "10.0.0.1".parse().unwrap();
        let ip = derive_client_ip(Some(peer), &xff, None, &trusted).unwrap();
        assert_eq!(ip, "10.0.0.3".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn spoofed_headers_ignored_when_peer_untrusted() {
        let xff = ips(&["203.0.113.9"]);
        let real: IpAddr = "198.51.100.1".parse().unwrap();
        let peer: IpAddr = "192.0.2.5".parse().unwrap();
        let trusted = nets(&["10.0.0.0/8", "127.0.0.0/8"]);
        let ip = derive_client_ip(Some(peer), &xff, Some(real), &trusted).unwrap();
        assert_eq!(ip, peer);
    }

    #[test]
    fn forwarded_for_ignored_without_trusted_proxies() {
        let xff = ips(&["203.0.113.9"]);
        let real: IpAddr = "198.51.100.1".parse().unwrap();
        let peer: IpAddr = "192.0.2.5".parse().unwrap();
        let ip = derive_client_ip(Some(peer), &xff, Some(real), &[]).unwrap();
        assert_eq!(ip, peer);
    }

    #[test]
    fn trusted_peer_uses_x_real_ip_without_xff() {
        let real: IpAddr = "198.51.100.1".parse().unwrap();
        let peer: IpAddr = "10.0.0.1".parse().unwrap();
        let trusted = nets(&["10.0.0.0/8"]);
        assert_eq!(
            derive_client_ip(Some(peer), &[], Some(real), &trusted).unwrap(),
            real
        );
    }

    #[test]
    fn falls_back_to_peer() {
        let peer: IpAddr = "192.0.2.5".parse().unwrap();
        assert_eq!(derive_client_ip(Some(peer), &[], None, &[]).unwrap(), peer);
    }

    #[test]
    fn no_source_yields_none() {
        assert!(derive_client_ip(None, &[], None, &[]).is_none());
    }

    #[test]
    fn ipv4_mapped_is_normalized() {
        let mapped: IpAddr = "::ffff:203.0.113.9".parse().unwrap();
        assert_eq!(normalize(mapped), "203.0.113.9".parse::<IpAddr>().unwrap());
    }

    #[test]
    fn malformed_forwarded_for_is_discarded() {
        assert!(parse_forwarded_for("not-an-ip, 203.0.113.9").is_empty());
        assert_eq!(
            parse_forwarded_for("203.0.113.9, 10.0.0.1"),
            ips(&["203.0.113.9", "10.0.0.1"])
        );
    }
}
