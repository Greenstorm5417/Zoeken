use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use ipnet::IpNet;
use proptest::prelude::*;
use zoeken_botdetect::config::LimiterConfig;
use zoeken_botdetect::{Decision, Detector, HeaderView, RequestFeatures, ip_lists};

fn ref_contains(net: &IpNet, ip: IpAddr) -> bool {
    match (net, ip) {
        (IpNet::V4(n), IpAddr::V4(a)) => {
            let p = n.prefix_len() as u32;
            let mask: u32 = if p == 0 { 0 } else { u32::MAX << (32 - p) };
            let net_bits = u32::from(n.network());
            let addr_bits = u32::from(a);
            (net_bits & mask) == (addr_bits & mask)
        }
        (IpNet::V6(n), IpAddr::V6(a)) => {
            let p = n.prefix_len() as u32;
            let mask: u128 = if p == 0 { 0 } else { u128::MAX << (128 - p) };
            let net_bits = u128::from(n.network());
            let addr_bits = u128::from(a);
            (net_bits & mask) == (addr_bits & mask)
        }
        _ => false,
    }
}

fn block_config(block: Vec<IpNet>) -> LimiterConfig {
    LimiterConfig {
        pass_reserved_nets: false,
        pass_ip: Vec::new(),
        block_ip: block,
        ..LimiterConfig::default()
    }
}

fn clean_features(ip: IpAddr) -> RequestFeatures {
    RequestFeatures {
        path: "/search".to_string(),
        client_ip: ip,
        headers: HeaderView {
            accept: Some("text/html".to_string()),
            accept_encoding: Some("gzip, deflate".to_string()),
            accept_language: Some("en-US".to_string()),
            connection: Some("keep-alive".to_string()),
            user_agent: Some("Mozilla/5.0 (X11; Linux x86_64) Firefox/120.0".to_string()),
            sec_fetch_mode: Some("navigate".to_string()),
            is_secure: false,
        },
        link_token: None,
    }
}

fn any_net() -> impl Strategy<Value = IpNet> {
    prop_oneof![
        (any::<u32>(), 0u8..=32).prop_map(|(addr, prefix)| {
            IpNet::new(IpAddr::V4(Ipv4Addr::from(addr)), prefix)
                .unwrap()
                .trunc()
        }),
        (any::<u128>(), 0u8..=128).prop_map(|(addr, prefix)| {
            IpNet::new(IpAddr::V6(Ipv6Addr::from(addr)), prefix)
                .unwrap()
                .trunc()
        }),
    ]
}

fn any_ip() -> impl Strategy<Value = IpAddr> {
    prop_oneof![
        any::<u32>().prop_map(|a| IpAddr::V4(Ipv4Addr::from(a))),
        any::<u128>().prop_map(|a| IpAddr::V6(Ipv6Addr::from(a))),
    ]
}

fn net_and_member() -> impl Strategy<Value = (IpNet, IpAddr)> {
    prop_oneof![
        (any::<u32>(), 0u8..=32, any::<u32>()).prop_map(|(base, prefix, host)| {
            let net = IpNet::new(IpAddr::V4(Ipv4Addr::from(base)), prefix)
                .unwrap()
                .trunc();
            let mask: u32 = if prefix == 0 {
                0
            } else {
                u32::MAX << (32 - prefix as u32)
            };
            let net_bits = match net.network() {
                IpAddr::V4(a) => u32::from(a),
                _ => unreachable!(),
            };
            let member = (net_bits & mask) | (host & !mask);
            (net, IpAddr::V4(Ipv4Addr::from(member)))
        }),
        (any::<u128>(), 0u8..=128, any::<u128>()).prop_map(|(base, prefix, host)| {
            let net = IpNet::new(IpAddr::V6(Ipv6Addr::from(base)), prefix)
                .unwrap()
                .trunc();
            let mask: u128 = if prefix == 0 {
                0
            } else {
                u128::MAX << (128 - prefix as u32)
            };
            let net_bits = match net.network() {
                IpAddr::V6(a) => u128::from(a),
                _ => unreachable!(),
            };
            let member = (net_bits & mask) | (host & !mask);
            (net, IpAddr::V6(Ipv6Addr::from(member)))
        }),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn block_listed_ip_is_rejected(
        (target_net, member) in net_and_member(),
        mut extra_blocks in prop::collection::vec(any_net(), 0..4),
    ) {
        extra_blocks.push(target_net);
        let cfg = block_config(extra_blocks);

        prop_assert!(
            cfg.block_ip.iter().any(|n| ref_contains(n, member)),
            "reference membership should hold for constructed member {member} in {target_net}"
        );

        prop_assert!(
            ip_lists::block_ip(member, &cfg),
            "block_ip should be true for {member} inside block network {target_net}"
        );

        prop_assert!(!ip_lists::pass_ip(member, &cfg));

        let detector = Detector::new(cfg, "tok");
        let decision = detector.evaluate(&clean_features(member));
        prop_assert!(
            matches!(decision, Decision::Block(_)),
            "expected Decision::Block for block-listed {member}, got {decision:?}"
        );
    }

    #[test]
    fn block_ip_matches_independent_membership(
        ip in any_ip(),
        blocks in prop::collection::vec(any_net(), 0..6),
    ) {
        let expected = blocks.iter().any(|n| ref_contains(n, ip));
        let cfg = block_config(blocks);
        prop_assert_eq!(
            ip_lists::block_ip(ip, &cfg),
            expected,
            "block_ip disagreed with independent membership reference for {}",
            ip
        );
    }
}
