// Property-based tests for bind address/port resolution.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr};

use proptest::prelude::*;
use zoeken_settings::{IntOrString, ServerSettings, resolve_bind};

fn server(bind_address: String, port: Option<IntOrString>) -> ServerSettings {
    ServerSettings {
        bind_address,
        port,
        ..ServerSettings::default()
    }
}

fn valid_ip() -> impl Strategy<Value = IpAddr> {
    prop_oneof![
        any::<[u8; 4]>().prop_map(|b| IpAddr::V4(Ipv4Addr::from(b))),
        any::<[u8; 16]>().prop_map(|b| IpAddr::V6(Ipv6Addr::from(b))),
    ]
}

fn valid_port() -> impl Strategy<Value = (u16, IntOrString)> {
    any::<u16>().prop_flat_map(|p| {
        prop_oneof![
            Just((p, IntOrString::Int(i64::from(p)))),
            Just((p, IntOrString::Str(p.to_string()))),
        ]
    })
}

fn malformed_address() -> impl Strategy<Value = String> {
    "[g-z]{1,12}".prop_map(|s| s.to_string())
}

fn blank_address() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        "[ \t]{1,4}".prop_map(|s| s.to_string())
    ]
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn wellformed_inputs_resolve_to_matching_socket_addr(
        ip in valid_ip(),
        (port, port_val) in valid_port(),
    ) {
        let s = server(ip.to_string(), Some(port_val));
        let resolved = resolve_bind(&s);
        prop_assert_eq!(resolved, Ok(SocketAddr::new(ip, port)));
    }

    #[test]
    fn blank_address_defaults_to_loopback(
        addr in blank_address(),
        port in prop_oneof![
            Just(None),
            any::<u16>().prop_map(|p| Some(IntOrString::Int(i64::from(p)))),
            Just(Some(IntOrString::Str(String::new()))),
        ],
    ) {
        let expected_port = match &port {
            None | Some(IntOrString::Str(_)) => 8888u16,
            Some(IntOrString::Int(n)) => u16::try_from(*n).unwrap(),
        };
        let s = server(addr, port);
        let resolved = resolve_bind(&s);
        prop_assert_eq!(
            resolved,
            Ok(SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), expected_port))
        );
    }

    #[test]
    fn malformed_address_errors_and_names_offender(
        addr in malformed_address(),
        (_p, port_val) in valid_port(),
    ) {
        let s = server(addr.clone(), Some(port_val));
        let err = resolve_bind(&s).expect_err("malformed address must error");
        prop_assert!(
            err.to_string().contains(&addr),
            "error message {:?} should contain offending address {:?}",
            err.to_string(),
            addr
        );
    }

    #[test]
    fn out_of_range_port_errors_and_names_offender(
        ip in valid_ip(),
        n in prop_oneof![i64::MIN..0i64, 65_536i64..=i64::MAX],
    ) {
        let s = server(ip.to_string(), Some(IntOrString::Int(n)));
        let err = resolve_bind(&s).expect_err("out-of-range port must error");
        prop_assert!(
            err.to_string().contains(&n.to_string()),
            "error message {:?} should contain offending port {:?}",
            err.to_string(),
            n
        );
    }

    #[test]
    fn non_numeric_port_errors_and_names_offender(
        ip in valid_ip(),
        port_str in "[g-z]{1,8}",
    ) {
        let s = server(ip.to_string(), Some(IntOrString::Str(port_str.clone())));
        let err = resolve_bind(&s).expect_err("non-numeric port must error");
        prop_assert!(
            err.to_string().contains(&port_str),
            "error message {:?} should contain offending port {:?}",
            err.to_string(),
            port_str
        );
    }
}
