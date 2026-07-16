// Property test for trusted-proxy client-IP derivation.
// Compares the production function with an independent reference model.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use ipnet::IpNet;
use proptest::prelude::*;
use zoeken_botdetect::client_ip::derive_client_ip;

/// Reference IPv4-mapped normalisation.
fn ref_normalize(ip: IpAddr) -> IpAddr {
    match ip {
        IpAddr::V6(v6) => {
            let o = v6.octets();
            let is_mapped = o[0..10].iter().all(|&b| b == 0) && o[10] == 0xff && o[11] == 0xff;
            if is_mapped {
                IpAddr::V4(Ipv4Addr::new(o[12], o[13], o[14], o[15]))
            } else {
                IpAddr::V6(v6)
            }
        }
        other => other,
    }
}

fn peer_trusted(peer: Option<IpAddr>, trusted: &[IpNet]) -> bool {
    match peer {
        Some(ip) => {
            let ip = ref_normalize(ip);
            !trusted.is_empty() && trusted.iter().any(|net| net.contains(&ip))
        }
        None => false,
    }
}

/// Independent precedence: forwarded headers only when TCP peer is trusted.
fn ref_derive(
    peer: Option<IpAddr>,
    xff: &[IpAddr],
    x_real_ip: Option<IpAddr>,
    trusted: &[IpNet],
) -> Option<IpAddr> {
    let peer = peer.map(ref_normalize);
    if peer_trusted(peer, trusted) {
        if !xff.is_empty() {
            let mut chosen: Option<IpAddr> = None;
            let mut i = xff.len();
            while i > 0 {
                i -= 1;
                let addr = xff[i];
                let is_trusted = trusted.iter().any(|net| net.contains(&addr));
                if !is_trusted {
                    chosen = Some(addr);
                    break;
                }
            }
            let selected = chosen.unwrap_or(xff[0]);
            return Some(ref_normalize(selected));
        }
        if let Some(ip) = x_real_ip {
            return Some(ref_normalize(ip));
        }
    }
    peer
}

fn ipv4() -> impl Strategy<Value = IpAddr> {
    any::<[u8; 4]>().prop_map(|o| IpAddr::V4(Ipv4Addr::new(o[0], o[1], o[2], o[3])))
}

fn ipv6() -> impl Strategy<Value = IpAddr> {
    prop_oneof![
        any::<[u16; 8]>().prop_map(|s| IpAddr::V6(Ipv6Addr::new(
            s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]
        ))),
        any::<[u8; 4]>().prop_map(|o| {
            let v4 = Ipv4Addr::new(o[0], o[1], o[2], o[3]);
            IpAddr::V6(v4.to_ipv6_mapped())
        }),
    ]
}

fn any_ip() -> impl Strategy<Value = IpAddr> {
    prop_oneof![ipv4(), ipv6()]
}

fn trusted_net() -> impl Strategy<Value = IpNet> {
    prop_oneof![
        (any::<[u8; 4]>(), 0u8..=32u8).prop_map(|(o, len)| {
            let net = ipnet::Ipv4Net::new(Ipv4Addr::new(o[0], o[1], o[2], o[3]), len).unwrap();
            IpNet::V4(net.trunc())
        }),
        (any::<[u16; 8]>(), 0u8..=128u8).prop_map(|(s, len)| {
            let addr = Ipv6Addr::new(s[0], s[1], s[2], s[3], s[4], s[5], s[6], s[7]);
            let net = ipnet::Ipv6Net::new(addr, len).unwrap();
            IpNet::V6(net.trunc())
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn derive_client_ip_matches_reference_model(
        xff in prop::collection::vec(any_ip(), 0..6),
        trusted in prop::collection::vec(trusted_net(), 0..4),
        x_real_ip in proptest::option::of(any_ip()),
        peer in proptest::option::of(any_ip()),
    ) {
        let got = derive_client_ip(peer, &xff, x_real_ip, &trusted);
        let expected = ref_derive(peer, &xff, x_real_ip, &trusted);
        prop_assert_eq!(got, expected);
    }

    #[test]
    fn no_trusted_proxies_ignores_forwarded_headers(
        xff in prop::collection::vec(any_ip(), 1..6),
        x_real_ip in proptest::option::of(any_ip()),
        peer in proptest::option::of(any_ip()),
    ) {
        let got = derive_client_ip(peer, &xff, x_real_ip, &[]);
        let expected = peer.map(ref_normalize);
        prop_assert_eq!(got, expected);
    }

    #[test]
    fn untrusted_peer_ignores_spoofed_headers(
        xff in prop::collection::vec(any_ip(), 1..4),
        x_real_ip in proptest::option::of(any_ip()),
    ) {
        let peer: IpAddr = "192.0.2.50".parse().unwrap();
        let trusted = vec!["10.0.0.0/8".parse().unwrap()];
        let got = derive_client_ip(Some(peer), &xff, x_real_ip, &trusted);
        prop_assert_eq!(got, Some(peer));
    }

    #[test]
    fn all_trusted_chain_selects_leftmost(
        octets in prop::collection::vec(any::<[u8; 3]>(), 1..6),
    ) {
        let trusted = vec!["10.0.0.0/8".parse::<IpNet>().unwrap()];
        let xff: Vec<IpAddr> = octets
            .iter()
            .map(|o| IpAddr::V4(Ipv4Addr::new(10, o[0], o[1], o[2])))
            .collect();
        let peer = xff[xff.len() - 1];
        let got = derive_client_ip(Some(peer), &xff, None, &trusted);
        prop_assert_eq!(got, Some(xff[0]));
    }
}
