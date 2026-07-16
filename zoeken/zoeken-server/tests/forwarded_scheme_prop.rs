use proptest::prelude::*;
use zoeken_server::middleware::{Scheme, forwarded_scheme};

fn scheme_strategy() -> impl Strategy<Value = Scheme> {
    prop_oneof![Just(Scheme::Http), Just(Scheme::Https)]
}

fn xfp_header_strategy() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some("http".to_string())),
        Just(Some("https".to_string())),
        Just(Some("HTTPS".to_string())),
        Just(Some("Http".to_string())),
        Just(Some("  https  ".to_string())),
        Just(Some("https, http".to_string())),
        Just(Some("http, https".to_string())),
        Just(Some(String::new())),
        Just(Some("ftp".to_string())),
        Just(Some("httpsx".to_string())),
        Just(Some("wss".to_string())),
        "[a-zA-Z0-9 ,:/.-]{0,24}".prop_map(Some),
    ]
}

fn reference_parse(header: Option<&str>) -> Option<Scheme> {
    let value = header?;
    let first = value.split(',').next().unwrap_or("").trim();
    if first.eq_ignore_ascii_case("https") {
        Some(Scheme::Https)
    } else if first.eq_ignore_ascii_case("http") {
        Some(Scheme::Http)
    } else {
        None
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn forwarded_protocol_trust(
        is_trusted_proxy in any::<bool>(),
        xfp in xfp_header_strategy(),
        conn_scheme in scheme_strategy(),
    ) {
        let result = forwarded_scheme(is_trusted_proxy, xfp.as_deref(), conn_scheme);

        let parsed = reference_parse(xfp.as_deref());
        let expected = match (is_trusted_proxy, parsed) {
            (true, Some(scheme)) => scheme,
            _ => conn_scheme,
        };

        prop_assert_eq!(
            result,
            expected,
            "scheme mismatch for (is_trusted_proxy={}, xfp={:?}, conn_scheme={:?})",
            is_trusted_proxy,
            xfp,
            conn_scheme,
        );

        if !is_trusted_proxy {
            prop_assert_eq!(
                result,
                conn_scheme,
                "untrusted peer must yield the connection scheme (xfp={:?})",
                xfp,
            );
        }

        if reference_parse(xfp.as_deref()).is_none() {
            prop_assert_eq!(
                result,
                conn_scheme,
                "an unparseable/absent header must fall back to the connection scheme (xfp={:?})",
                xfp,
            );
        }
    }
}
