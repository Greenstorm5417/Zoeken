use axum::http::Method;
use proptest::prelude::*;
use zoeken_server::static_assets::{AssetDecision, SERVED_EXTENSIONS, decide};

const NON_SERVED_EXTENSIONS: &[&str] = &[
    "exe", "bin", "zip", "tar", "gz", "rs", "py", "md", "yml", "toml", "lock", "bak", "dat",
];

fn method_strategy() -> impl Strategy<Value = Method> {
    prop_oneof![
        Just(Method::GET),
        Just(Method::HEAD),
        Just(Method::POST),
        Just(Method::PUT),
        Just(Method::DELETE),
        Just(Method::PATCH),
        Just(Method::OPTIONS),
        Just(Method::TRACE),
        Just(Method::CONNECT),
    ]
}

fn served_ext_path() -> impl Strategy<Value = (String, bool)> {
    (
        prop::collection::vec("[a-z0-9_]{1,8}", 0..3),
        prop::collection::vec("[a-z0-9]{1,8}", 1..3),
        prop::sample::select(SERVED_EXTENSIONS.to_vec()),
    )
        .prop_map(|(dirs, name_segs, ext)| {
            let mut path = String::new();
            for d in &dirs {
                path.push_str(d);
                path.push('/');
            }
            path.push_str(&name_segs.join("."));
            path.push('.');
            path.push_str(ext);
            (path, true)
        })
}

fn extensionless_path() -> impl Strategy<Value = (String, bool)> {
    prop::collection::vec("[a-z0-9_]{1,8}", 0..4).prop_map(|segs| (segs.join("/"), false))
}

fn non_served_ext_path() -> impl Strategy<Value = (String, bool)> {
    (
        prop::collection::vec("[a-z0-9_]{1,8}", 0..3),
        "[a-z0-9]{1,8}",
        prop::sample::select(NON_SERVED_EXTENSIONS.to_vec()),
    )
        .prop_map(|(dirs, name, ext)| {
            let mut path = String::new();
            for d in &dirs {
                path.push_str(d);
                path.push('/');
            }
            path.push_str(&name);
            path.push('.');
            path.push_str(ext);
            (path, false)
        })
}

fn dotfile_path() -> impl Strategy<Value = (String, bool)> {
    prop_oneof![
        "[a-z0-9_]{1,8}".prop_map(|s| (format!(".{s}"), false)),
        "[a-z0-9_]{1,8}".prop_map(|s| (format!("{s}."), false)),
    ]
}

fn path_strategy() -> impl Strategy<Value = (String, bool)> {
    prop_oneof![
        served_ext_path(),
        extensionless_path(),
        non_served_ext_path(),
        dotfile_path(),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn static_asset_routing_decision(
        method in method_strategy(),
        (path, has_served_ext) in path_strategy(),
        asset_exists in any::<bool>(),
    ) {
        let decision = decide(&method, &path, |_| asset_exists);

        let expected = if method != Method::GET && method != Method::HEAD {
            AssetDecision::MethodNotAllowed
        } else if has_served_ext {
            if asset_exists {
                AssetDecision::ServeAsset { path: path.clone() }
            } else {
                AssetDecision::NotFound
            }
        } else {
            AssetDecision::ServeIndex
        };

        prop_assert_eq!(
            decision,
            expected,
            "method={:?} path={:?} has_served_ext={} asset_exists={}",
            method,
            path,
            has_served_ext,
            asset_exists,
        );
    }
}
