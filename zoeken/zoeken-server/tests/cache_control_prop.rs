use proptest::prelude::*;
use zoeken_server::static_assets::{cache_control_for, is_fingerprinted};

const MIN_IMMUTABLE_MAX_AGE: u64 = 31_536_000;

#[derive(Debug, Clone)]
enum PathCase {
    Fingerprinted(String),
    Plain(String),
}

fn parse_max_age(header: &str) -> Option<u64> {
    header
        .split(',')
        .map(str::trim)
        .find_map(|directive| directive.strip_prefix("max-age="))
        .and_then(|value| value.trim().parse::<u64>().ok())
}

fn dir_prefix() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        Just("assets/".to_string()),
        Just("static/".to_string()),
        Just("static/js/".to_string()),
    ]
}

fn ext() -> impl Strategy<Value = String> {
    prop_oneof![
        Just("js".to_string()),
        Just("mjs".to_string()),
        Just("css".to_string()),
        Just("html".to_string()),
        Just("svg".to_string()),
        Just("woff2".to_string()),
        Just("png".to_string()),
    ]
}

fn fingerprinted_path() -> impl Strategy<Value = PathCase> {
    (dir_prefix(), "[a-z][a-z0-9]{0,10}", "[0-9a-z]{8,24}", ext()).prop_map(
        |(dir, name, hash, ext)| PathCase::Fingerprinted(format!("{dir}{name}.{hash}.{ext}")),
    )
}

fn plain_path() -> impl Strategy<Value = PathCase> {
    prop_oneof![
        Just(PathCase::Plain("index.html".to_string())),
        (dir_prefix(), "[a-z][a-z0-9]{0,10}", ext())
            .prop_map(|(dir, name, ext)| PathCase::Plain(format!("{dir}{name}.{ext}"))),
        (dir_prefix(), "[a-z][a-z0-9]{0,10}", "[0-9a-z]{1,7}", ext()).prop_map(
            |(dir, name, short, ext)| PathCase::Plain(format!("{dir}{name}.{short}.{ext}"))
        ),
        (dir_prefix(), "[a-z][a-z0-9]{0,12}")
            .prop_map(|(dir, name)| PathCase::Plain(format!("{dir}{name}"))),
    ]
}

fn any_path() -> impl Strategy<Value = PathCase> {
    prop_oneof![fingerprinted_path(), plain_path()]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn cache_control_policy_matches_fingerprinting(case in any_path()) {
        let (path, expected_fingerprinted) = match &case {
            PathCase::Fingerprinted(p) => (p.as_str(), true),
            PathCase::Plain(p) => (p.as_str(), false),
        };

        prop_assert_eq!(
            is_fingerprinted(path),
            expected_fingerprinted,
            "generator/category mismatch for path {:?}",
            path,
        );

        let policy = cache_control_for(path);

        if is_fingerprinted(path) {
            let max_age = parse_max_age(policy).unwrap_or_else(|| {
                panic!("fingerprinted policy {policy:?} has no parseable max-age")
            });
            prop_assert!(
                max_age >= MIN_IMMUTABLE_MAX_AGE,
                "fingerprinted path {:?}: max-age {} < {}",
                path,
                max_age,
                MIN_IMMUTABLE_MAX_AGE,
            );
            prop_assert!(
                policy.contains("immutable"),
                "fingerprinted path {:?}: policy {:?} not marked immutable",
                path,
                policy,
            );
            prop_assert!(
                !policy.contains("no-cache"),
                "fingerprinted path {:?}: policy {:?} unexpectedly revalidates",
                path,
                policy,
            );
        } else {
            prop_assert!(
                policy.contains("no-cache") && policy.contains("must-revalidate"),
                "non-fingerprinted path {:?}: policy {:?} does not require revalidation",
                path,
                policy,
            );
            prop_assert!(
                !policy.contains("immutable"),
                "non-fingerprinted path {:?}: policy {:?} unexpectedly immutable",
                path,
                policy,
            );
            if let Some(max_age) = parse_max_age(policy) {
                prop_assert!(
                    max_age < MIN_IMMUTABLE_MAX_AGE,
                    "non-fingerprinted path {:?}: policy {:?} has immutable-scale max-age {}",
                    path,
                    policy,
                    max_age,
                );
            }
        }
    }
}
