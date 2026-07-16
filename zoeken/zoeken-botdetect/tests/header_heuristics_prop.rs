use zoeken_botdetect::HeaderHeuristics;
use zoeken_botdetect::heuristics::{
    HeaderView, HeuristicFailure, check_accept, check_accept_encoding, check_accept_language,
    check_connection, check_sec_fetch, check_user_agent, evaluate,
};

use proptest::prelude::*;

fn accept_strategy() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some(String::new())),
        Just(Some("text/html".to_string())),
        Just(Some(
            "text/html,application/xhtml+xml,application/xml;q=0.9".to_string()
        )),
        Just(Some("application/json".to_string())),
        Just(Some("*/*".to_string())),
        Just(Some("image/png".to_string())),
    ]
}

fn accept_encoding_strategy() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some(String::new())),
        Just(Some("gzip".to_string())),
        Just(Some("deflate".to_string())),
        Just(Some("gzip, deflate, br".to_string())),
        Just(Some("br".to_string())),
        Just(Some("identity".to_string())),
        Just(Some("br, zstd".to_string())),
    ]
}

fn accept_language_strategy() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some(String::new())),
        Just(Some("   ".to_string())),
        Just(Some("en".to_string())),
        Just(Some("en-US,en;q=0.9".to_string())),
        Just(Some("de-DE".to_string())),
    ]
}

fn connection_strategy() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some("keep-alive".to_string())),
        Just(Some("close".to_string())),
        Just(Some("  close  ".to_string())),
        Just(Some("upgrade".to_string())),
    ]
}

fn user_agent_strategy() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some(String::new())),
        Just(Some("   ".to_string())),
        Just(Some(
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
             Chrome/120.0 Safari/537.36"
                .to_string(),
        )),
        Just(Some(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:120.0) Gecko/20100101 Firefox/120.0"
                .to_string(),
        )),
        Just(Some(
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 \
             (KHTML, like Gecko) Version/16.4 Safari/605.1.15"
                .to_string(),
        )),
        Just(Some("curl/8.0".to_string())),
        Just(Some("python-requests/2.31".to_string())),
        Just(Some(
            "Googlebot/2.1 (+http://www.google.com/bot.html)".to_string()
        )),
        Just(Some("Mozilla/5.0 (compatible; PetalBot/1.0)".to_string())),
        Just(Some("Mozilla/5.0 Chrome/70.0".to_string())),
    ]
}

fn sec_fetch_strategy() -> impl Strategy<Value = Option<String>> {
    prop_oneof![
        Just(None),
        Just(Some("navigate".to_string())),
        Just(Some("cors".to_string())),
        Just(Some("no-cors".to_string())),
        Just(Some("same-origin".to_string())),
    ]
}

fn header_view_strategy() -> impl Strategy<Value = HeaderView> {
    (
        accept_strategy(),
        accept_encoding_strategy(),
        accept_language_strategy(),
        connection_strategy(),
        user_agent_strategy(),
        sec_fetch_strategy(),
        any::<bool>(),
    )
        .prop_map(
            |(
                accept,
                accept_encoding,
                accept_language,
                connection,
                user_agent,
                sec_fetch_mode,
                is_secure,
            )| {
                HeaderView {
                    accept,
                    accept_encoding,
                    accept_language,
                    connection,
                    user_agent,
                    sec_fetch_mode,
                    is_secure,
                }
            },
        )
}

fn heuristics_strategy() -> impl Strategy<Value = HeaderHeuristics> {
    (
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
        any::<bool>(),
    )
        .prop_map(
            |(accept, accept_encoding, accept_language, connection, sec_fetch, user_agent)| {
                HeaderHeuristics {
                    accept,
                    accept_encoding,
                    accept_language,
                    connection,
                    sec_fetch,
                    user_agent,
                }
            },
        )
}

fn failing_enabled(view: &HeaderView, cfg: &HeaderHeuristics) -> Vec<HeuristicFailure> {
    let mut failures = Vec::new();
    if cfg.accept && !check_accept(view.accept.as_deref()) {
        failures.push(HeuristicFailure::Accept);
    }
    if cfg.accept_encoding && !check_accept_encoding(view.accept_encoding.as_deref()) {
        failures.push(HeuristicFailure::AcceptEncoding);
    }
    if cfg.accept_language && !check_accept_language(view.accept_language.as_deref()) {
        failures.push(HeuristicFailure::AcceptLanguage);
    }
    if cfg.connection && !check_connection(view.connection.as_deref()) {
        failures.push(HeuristicFailure::Connection);
    }
    if cfg.user_agent && !check_user_agent(view.user_agent.as_deref()) {
        failures.push(HeuristicFailure::UserAgent);
    }
    if cfg.sec_fetch && !check_sec_fetch(view) {
        failures.push(HeuristicFailure::SecFetch);
    }
    failures
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(500))]

    #[test]
    fn header_heuristics_gate(view in header_view_strategy(), cfg in heuristics_strategy()) {
        let reference = failing_enabled(&view, &cfg);
        let result = evaluate(&view, &cfg);

        prop_assert_eq!(
            result.is_err(),
            !reference.is_empty(),
            "rejection disagreement for view={:?} cfg={:?}: evaluate={:?} reference_failures={:?}",
            view,
            cfg,
            result,
            reference
        );

        match result {
            Ok(()) => {
                prop_assert!(reference.is_empty());
            }
            Err(failure) => {
                prop_assert!(reference.contains(&failure));
                prop_assert_eq!(failure, reference[0]);
            }
        }
    }
}
