use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use axum::http::{HeaderMap, HeaderValue};
use ipnet::IpNet;
use proptest::prelude::*;
use zoeken_server::middleware::request_client_ip;

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

fn peer_trusted(peer: IpAddr, trusted: &[IpNet]) -> bool {
    !trusted.is_empty() && trusted.iter().any(|net| net.contains(&peer))
}

fn ref_client_ip(peer: SocketAddr, xff: &[IpAddr], trusted: &[IpNet]) -> IpAddr {
    let peer_ip = ref_normalize(peer.ip());
    if peer_trusted(peer_ip, trusted) && !xff.is_empty() {
        let normalized: Vec<IpAddr> = xff.iter().map(|&a| ref_normalize(a)).collect();
        let mut i = normalized.len();
        while i > 0 {
            i -= 1;
            let addr = normalized[i];
            let is_trusted = trusted.iter().any(|net| net.contains(&addr));
            if !is_trusted {
                return addr;
            }
        }
        return normalized[0];
    }
    peer_ip
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

fn peer_addr() -> impl Strategy<Value = SocketAddr> {
    (any_ip(), any::<u16>()).prop_map(|(ip, port)| SocketAddr::new(ip, port))
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

fn headers_with_xff(xff: &[IpAddr]) -> HeaderMap {
    let mut headers = HeaderMap::new();
    if !xff.is_empty() {
        let value = xff
            .iter()
            .map(|ip| ip.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        headers.insert(
            "x-forwarded-for",
            HeaderValue::from_str(&value).expect("formatted IP list is a valid header value"),
        );
    }
    headers
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(400))]

    #[test]
    fn request_client_ip_matches_reference_model(
        peer in peer_addr(),
        xff in prop::collection::vec(any_ip(), 0..6),
        trusted in prop::collection::vec(trusted_net(), 0..4),
    ) {
        let headers = headers_with_xff(&xff);
        let got = request_client_ip(Some(peer), &headers, &trusted);
        let expected = ref_client_ip(peer, &xff, &trusted);
        prop_assert_eq!(got, Some(expected));
    }

    #[test]
    fn no_trusted_proxies_uses_direct_peer(
        peer in peer_addr(),
        xff in prop::collection::vec(any_ip(), 1..6),
    ) {
        let headers = headers_with_xff(&xff);
        let got = request_client_ip(Some(peer), &headers, &[]);
        prop_assert_eq!(got, Some(ref_normalize(peer.ip())));
    }

    #[test]
    fn trusted_proxies_select_nearest_untrusted_hop(
        prefix in prop::collection::vec(any_ip(), 0..3),
        nearest_octet in any::<u8>(),
        trusted_suffix in prop::collection::vec(any::<[u8; 3]>(), 0..4),
    ) {
        let trusted = vec![IpNet::V4(
            ipnet::Ipv4Net::new(Ipv4Addr::new(10, 0, 0, 0), 8).unwrap(),
        )];

        let nearest_untrusted = IpAddr::V4(Ipv4Addr::new(203, 0, 113, nearest_octet));

        let mut xff = prefix;
        xff.push(nearest_untrusted);
        for o in &trusted_suffix {
            xff.push(IpAddr::V4(Ipv4Addr::new(10, o[0], o[1], o[2])));
        }

        let headers = headers_with_xff(&xff);
        let got = request_client_ip(Some("10.0.0.1:443".parse().unwrap()), &headers, &trusted);
        prop_assert_eq!(got, Some(nearest_untrusted));
    }

    #[test]
    fn untrusted_peer_ignores_spoofed_xff(
        peer in peer_addr(),
        spoofed in any_ip(),
    ) {
        let trusted = vec![IpNet::V4(
            ipnet::Ipv4Net::new(Ipv4Addr::new(10, 0, 0, 0), 8).unwrap(),
        )];
        prop_assume!(!trusted.iter().any(|net| net.contains(&ref_normalize(peer.ip()))));
        let headers = headers_with_xff(&[spoofed]);
        let got = request_client_ip(Some(peer), &headers, &trusted);
        prop_assert_eq!(got, Some(ref_normalize(peer.ip())));
    }
}
