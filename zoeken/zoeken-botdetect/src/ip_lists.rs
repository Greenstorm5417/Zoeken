//! Pass-list and block-list checks, plus client-network grouping.

use std::net::IpAddr;

use ipnet::IpNet;

use crate::config::LimiterConfig;

/// Check if an IP is on the pass-list.
pub fn pass_ip(ip: IpAddr, cfg: &LimiterConfig) -> bool {
    if cfg.pass_reserved_nets && cfg.reserved_pass_nets().iter().any(|net| net.contains(&ip)) {
        return true;
    }
    cfg.pass_ip.iter().any(|net| net.contains(&ip))
}

/// Returns `true` when `ip` is on the block-list (a configured block condition).
pub fn block_ip(ip: IpAddr, cfg: &LimiterConfig) -> bool {
    cfg.block_ip.iter().any(|net| net.contains(&ip))
}

/// Group an IP into its client network using the configured prefix lengths.
pub fn client_network(ip: IpAddr, ipv4_prefix: u8, ipv6_prefix: u8) -> IpNet {
    match ip {
        IpAddr::V4(v4) => {
            let prefix = ipv4_prefix.min(32);
            IpNet::new(IpAddr::V4(v4), prefix)
                .map(|net| net.trunc())
                .unwrap_or_else(|_| IpNet::new(IpAddr::V4(v4), 32).unwrap())
        }
        IpAddr::V6(v6) => {
            let prefix = ipv6_prefix.min(128);
            IpNet::new(IpAddr::V6(v6), prefix)
                .map(|net| net.trunc())
                .unwrap_or_else(|_| IpNet::new(IpAddr::V6(v6), 128).unwrap())
        }
    }
}

/// Check if an IP is link-local.
pub fn is_link_local(ip: IpAddr) -> bool {
    match ip {
        IpAddr::V4(v4) => v4.is_link_local(),
        IpAddr::V6(v6) => (v6.segments()[0] & 0xffc0) == 0xfe80,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn cfg_with(pass: &[&str], block: &[&str]) -> LimiterConfig {
        LimiterConfig {
            pass_reserved_nets: false,
            pass_ip: pass.iter().map(|s| IpNet::from_str(s).unwrap()).collect(),
            block_ip: block.iter().map(|s| IpNet::from_str(s).unwrap()).collect(),
            ..LimiterConfig::default()
        }
    }

    #[test]
    fn pass_list_matches_cidr_membership() {
        let cfg = cfg_with(&["192.168.0.0/16"], &[]);
        assert!(pass_ip("192.168.1.5".parse().unwrap(), &cfg));
        assert!(!pass_ip("10.0.0.1".parse().unwrap(), &cfg));
    }

    #[test]
    fn block_list_matches_cidr_membership() {
        let cfg = cfg_with(&[], &["93.184.216.0/24"]);
        assert!(block_ip("93.184.216.34".parse().unwrap(), &cfg));
        assert!(!block_ip("93.184.217.34".parse().unwrap(), &cfg));
    }

    #[test]
    fn version_mismatch_never_matches() {
        let cfg = cfg_with(&["192.168.0.0/16"], &[]);
        assert!(!pass_ip("fe80::1".parse().unwrap(), &cfg));
    }

    #[test]
    fn reserved_pass_nets_pass_list_honours_toggle() {
        let mut cfg = LimiterConfig::default();
        let org_ip: IpAddr = "167.235.158.251".parse().unwrap();
        assert!(pass_ip(org_ip, &cfg));
        cfg.pass_reserved_nets = false;
        assert!(!pass_ip(org_ip, &cfg));
    }

    #[test]
    fn client_network_groups_by_prefix() {
        let net = client_network("192.168.1.55".parse().unwrap(), 24, 48);
        assert_eq!(net, IpNet::from_str("192.168.1.0/24").unwrap());

        let net6 = client_network("2a01:4f8:1c1c:8fc2::5".parse().unwrap(), 32, 48);
        assert_eq!(net6, IpNet::from_str("2a01:4f8:1c1c::/48").unwrap());
    }

    #[test]
    fn link_local_detection() {
        assert!(is_link_local("169.254.1.1".parse().unwrap()));
        assert!(is_link_local("fe80::1".parse().unwrap()));
        assert!(!is_link_local("8.8.8.8".parse().unwrap()));
        assert!(!is_link_local("2001:db8::1".parse().unwrap()));
    }
}
