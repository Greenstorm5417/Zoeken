use proptest::prelude::*;
use zoeken_botdetect::link_token::{LinkTokenVerifier, token_is_valid};

#[derive(Debug, Clone)]
enum Presented {
    Matching,
    Different(String),
    None,
}

fn resolve(case: &Presented, token: &str) -> Option<String> {
    match case {
        Presented::Matching => Some(token.to_string()),
        Presented::Different(s) if s == token => Some(format!("{s}-x")),
        Presented::Different(s) => Some(s.clone()),
        Presented::None => None,
    }
}

fn token_strategy() -> impl Strategy<Value = String> {
    ".{0,32}"
}

fn presented_strategy() -> impl Strategy<Value = Presented> {
    prop_oneof![
        Just(Presented::Matching),
        ".{0,32}".prop_map(Presented::Different),
        Just(Presented::None),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn token_is_valid_iff_presented_equals_current(
        token in token_strategy(),
        case in presented_strategy(),
    ) {
        let presented = resolve(&case, &token);
        let oracle = presented.as_deref() == Some(token.as_str());
        prop_assert_eq!(token_is_valid(presented.as_deref(), &token), oracle);
        match case {
            Presented::Matching => prop_assert!(oracle),
            Presented::None => prop_assert!(!oracle),
            Presented::Different(_) => prop_assert!(!oracle),
        }
    }

    #[test]
    fn fresh_verifier_suspicion_matches_valid_presentation(
        token in token_strategy(),
        network_key in ".{0,24}",
        case in presented_strategy(),
    ) {
        let presented = resolve(&case, &token);
        let presents_valid = presented.as_deref() == Some(token.as_str());

        let verifier = LinkTokenVerifier::new(token.clone());
        let suspicious = verifier.is_suspicious(&network_key, presented.as_deref());
        prop_assert_eq!(suspicious, !presents_valid);

        prop_assert_eq!(verifier.is_verified(&network_key), presents_valid);

        if presents_valid {
            prop_assert!(!verifier.is_suspicious(&network_key, None));
        }
    }

    #[test]
    fn ping_verifies_only_with_valid_token(
        token in token_strategy(),
        network_key in ".{0,24}",
        case in presented_strategy(),
    ) {
        let presented = resolve(&case, &token);
        let presents_valid = presented.as_deref() == Some(token.as_str());

        let verifier = LinkTokenVerifier::new(token.clone());
        prop_assert_eq!(verifier.ping(&network_key, presented.as_deref()), presents_valid);
        prop_assert_eq!(verifier.is_verified(&network_key), presents_valid);
    }
}
