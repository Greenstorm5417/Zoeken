use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};

use ipnet::IpNet;
use proptest::prelude::*;
use zoeken_botdetect::config::LimiterConfig;
use zoeken_botdetect::{
    Decision, Detector, HeaderView, RateLimitConfig, RequestFeatures, ip_lists,
};

fn v4_case() -> impl Strategy<Value = (IpNet, IpAddr)> {
    (any::<u32>(), any::<u32>(), 0u8..=32u8).prop_map(|(base, host, prefix)| {
        let mask: u32 = if prefix == 0 {
            0
        } else {
            u32::MAX << (32 - prefix)
        };
        let net_addr = base & mask;
        let member = net_addr | (host & !mask);
        let net = IpNet::new(IpAddr::V4(Ipv4Addr::from(net_addr)), prefix)
            .expect("valid IPv4 prefix")
            .trunc();
        (net, IpAddr::V4(Ipv4Addr::from(member)))
    })
}

fn v6_case() -> impl Strategy<Value = (IpNet, IpAddr)> {
    (any::<u128>(), any::<u128>(), 0u8..=128u8).prop_map(|(base, host, prefix)| {
        let mask: u128 = if prefix == 0 {
            0
        } else {
            u128::MAX << (128 - prefix)
        };
        let net_addr = base & mask;
        let member = net_addr | (host & !mask);
        let net = IpNet::new(IpAddr::V6(Ipv6Addr::from(net_addr)), prefix)
            .expect("valid IPv6 prefix")
            .trunc();
        (net, IpAddr::V6(Ipv6Addr::from(member)))
    })
}

fn pass_case() -> impl Strategy<Value = (IpNet, IpAddr)> {
    prop_oneof![v4_case(), v6_case()]
}

fn hostile_config(net: IpNet) -> LimiterConfig {
    LimiterConfig {
        pass_reserved_nets: false,
        pass_ip: vec![net],
        block_ip: vec![net],
        rate_limit: RateLimitConfig {
            capacity: 0.0,
            refill_per_second: 0.0,
            suspicious_capacity: 0.0,
            suspicious_refill_per_second: 0.0,
        },
        filter_link_local: true,
        ..LimiterConfig::default()
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn pass_list_bypasses_all_checks((net, ip) in pass_case()) {
        let cfg = hostile_config(net);

        prop_assert!(
            ip_lists::pass_ip(ip, &cfg),
            "constructed IP {ip} should be inside pass network {net}"
        );

        let detector = Detector::new(cfg, "tok");

        let features = RequestFeatures {
            path: "/search".to_string(),
            client_ip: ip,
            headers: HeaderView::default(),
            link_token: None,
        };

        prop_assert_eq!(
            detector.evaluate(&features),
            Decision::Allow,
            "pass-listed IP {} must bypass block/rate/heuristics",
            ip
        );
    }
}
