//! Request-header heuristics for bot detection.

use crate::config::HeaderHeuristics;

/// Request headers relevant to the heuristics.
#[derive(Debug, Clone, Default)]
pub struct HeaderView {
    pub accept: Option<String>,
    pub accept_encoding: Option<String>,
    pub accept_language: Option<String>,
    pub connection: Option<String>,
    pub user_agent: Option<String>,
    pub sec_fetch_mode: Option<String>,
    pub is_secure: bool,
}

/// Identifies which heuristic rejected a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HeuristicFailure {
    Accept,
    AcceptEncoding,
    AcceptLanguage,
    Connection,
    UserAgent,
    SecFetch,
}

impl HeuristicFailure {
    pub fn reason(self) -> &'static str {
        match self {
            HeuristicFailure::Accept => {
                "HTTP header Accept did not contain text/html or application/json"
            }
            HeuristicFailure::AcceptEncoding => {
                "HTTP header Accept-Encoding did not contain gzip nor deflate"
            }
            HeuristicFailure::AcceptLanguage => "missing HTTP header Accept-Language",
            HeuristicFailure::Connection => "HTTP header Connection=close",
            HeuristicFailure::UserAgent => "bot detected via HTTP header User-Agent",
            HeuristicFailure::SecFetch => "invalid Sec-Fetch-Mode",
        }
    }
}

/// Browsers navigate with `text/html`; the SPA fetches `/search` with
/// `application/json`. Both are legitimate; scrapers commonly send neither.
pub fn check_accept(accept: Option<&str>) -> bool {
    accept.is_some_and(|value| {
        value.contains("text/html") || value.contains("application/json") || value.contains("*/*")
    })
}

pub fn check_accept_encoding(accept_encoding: Option<&str>) -> bool {
    let value = accept_encoding.unwrap_or("");
    value
        .split(',')
        .map(|token| token.trim())
        .any(|token| token == "gzip" || token == "deflate")
}

pub fn check_accept_language(accept_language: Option<&str>) -> bool {
    accept_language.is_some_and(|value| !value.trim().is_empty())
}

pub fn check_connection(connection: Option<&str>) -> bool {
    connection.map(|value| value.trim()) != Some("close")
}

pub fn check_user_agent(user_agent: Option<&str>) -> bool {
    match user_agent {
        None => false,
        Some(value) if value.trim().is_empty() => false,
        Some(value) => !is_bot_user_agent(value),
    }
}

/// Only enforce `Sec-Fetch-Mode` for secure requests from browsers that send it.
pub fn check_sec_fetch(view: &HeaderView) -> bool {
    if !view.is_secure {
        return true;
    }
    let ua = view.user_agent.as_deref().unwrap_or("");
    if !is_browser_supported(ua) {
        return true;
    }
    // `navigate`: address-bar / form navigation; `cors` and `same-origin`:
    // the SPA's own `fetch` calls.
    matches!(
        view.sec_fetch_mode.as_deref(),
        Some("navigate") | Some("cors") | Some("same-origin")
    )
}

const BOT_PREFIXES: &[&str] = &[
    "unknown",
    "curl",
    "wget",
    "scrapy",
    "splash",
    "javafx",
    "feedfetcher",
    "python-requests",
    "go-http-client",
    "java",
    "jakarta",
    "okhttp",
    "httpclient",
    "jersey",
    "python",
    "libwww-perl",
    "ruby",
    "synhttpclient",
    "universalfeedparser",
    "googlebot",
    "googleimageproxy",
    "bingbot",
    "baiduspider",
    "yacybot",
    "yandexmobilebot",
    "yandexbot",
    "yahoo! slurp",
    "mj12bot",
    "ahrefsbot",
    "archive.org_bot",
    "msnbot",
    "seznambot",
    "linkdexbot",
    "netvibes",
    "smtbot",
    "zgrab",
    "james bot",
    "sogou",
    "abonti",
    "pixray",
    "spinn3r",
    "semrushbot",
    "exabot",
    "zmeu",
    "blexbot",
    "bitlybot",
    "headlesschrome",
];

const BOT_SUBSTRINGS: &[&str] = &["petalbot"];

const BOT_EXACT: &[&str] = &["mozilla/5.0 (compatible; farside/0.1.0; +https://farside.link)"];

pub fn is_bot_user_agent(user_agent: &str) -> bool {
    let ua = user_agent.trim().to_ascii_lowercase();
    if ua.is_empty() {
        return true;
    }
    if BOT_EXACT.contains(&ua.as_str()) {
        return true;
    }
    if BOT_PREFIXES.iter().any(|prefix| ua.starts_with(prefix)) {
        return true;
    }
    BOT_SUBSTRINGS.iter().any(|needle| ua.contains(needle))
}

/// Browser versions known to send `Sec-Fetch-*` headers consistently.
pub fn is_browser_supported(user_agent: &str) -> bool {
    let ua = user_agent.to_ascii_lowercase();

    if let Some(version) = version_after(&ua, "chrome/") {
        return version >= 80;
    }
    if let Some(version) = version_after(&ua, "firefox/") {
        return version >= 90;
    }
    if let Some((major, minor)) = version_pair_after(&ua, "version/") {
        return major > 16 || (major == 16 && minor >= 4);
    }
    false
}

/// Parse the leading integer immediately following `marker` in `haystack`.
fn version_after(haystack: &str, marker: &str) -> Option<u32> {
    let rest = haystack.split(marker).nth(1)?;
    let digits: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    digits.parse().ok()
}

/// Parse `major.minor` immediately following `marker` in `haystack`.
fn version_pair_after(haystack: &str, marker: &str) -> Option<(u32, u32)> {
    let rest = haystack.split(marker).nth(1)?;
    let major: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    let after_major = &rest[major.len()..];
    let minor_part = after_major.strip_prefix('.')?;
    let minor: String = minor_part
        .chars()
        .take_while(|c| c.is_ascii_digit())
        .collect();
    Some((major.parse().ok()?, minor.parse().ok()?))
}

pub fn evaluate(view: &HeaderView, cfg: &HeaderHeuristics) -> Result<(), HeuristicFailure> {
    if cfg.accept && !check_accept(view.accept.as_deref()) {
        return Err(HeuristicFailure::Accept);
    }
    if cfg.accept_encoding && !check_accept_encoding(view.accept_encoding.as_deref()) {
        return Err(HeuristicFailure::AcceptEncoding);
    }
    if cfg.accept_language && !check_accept_language(view.accept_language.as_deref()) {
        return Err(HeuristicFailure::AcceptLanguage);
    }
    if cfg.connection && !check_connection(view.connection.as_deref()) {
        return Err(HeuristicFailure::Connection);
    }
    if cfg.user_agent && !check_user_agent(view.user_agent.as_deref()) {
        return Err(HeuristicFailure::UserAgent);
    }
    if cfg.sec_fetch && !check_sec_fetch(view) {
        return Err(HeuristicFailure::SecFetch);
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn browser_view() -> HeaderView {
        HeaderView {
            accept: Some("text/html,application/xhtml+xml".to_string()),
            accept_encoding: Some("gzip, deflate, br".to_string()),
            accept_language: Some("en-US,en;q=0.9".to_string()),
            connection: Some("keep-alive".to_string()),
            user_agent: Some(
                "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) \
                 Chrome/120.0 Safari/537.36"
                    .to_string(),
            ),
            sec_fetch_mode: Some("navigate".to_string()),
            is_secure: true,
        }
    }

    #[test]
    fn realistic_browser_passes_all_heuristics() {
        assert_eq!(
            evaluate(&browser_view(), &HeaderHeuristics::default()),
            Ok(())
        );
    }

    #[test]
    fn accept_without_html_or_json_is_rejected() {
        let mut view = browser_view();
        view.accept = Some("application/xml".to_string());
        assert_eq!(
            evaluate(&view, &HeaderHeuristics::default()),
            Err(HeuristicFailure::Accept)
        );
    }

    /// The SPA fetches `/search` with a JSON Accept and `Sec-Fetch-Mode: cors`;
    /// that traffic must pass the heuristics.
    #[test]
    fn spa_json_fetch_passes_all_heuristics() {
        let mut view = browser_view();
        view.accept = Some("application/json".to_string());
        view.sec_fetch_mode = Some("cors".to_string());
        assert_eq!(evaluate(&view, &HeaderHeuristics::default()), Ok(()));
    }

    #[test]
    fn missing_accept_encoding_is_rejected() {
        assert!(!check_accept_encoding(None));
        assert!(!check_accept_encoding(Some("br")));
        assert!(check_accept_encoding(Some("gzip")));
        assert!(check_accept_encoding(Some("br, deflate")));
    }

    #[test]
    fn missing_accept_language_is_rejected() {
        assert!(!check_accept_language(None));
        assert!(!check_accept_language(Some("   ")));
        assert!(check_accept_language(Some("en")));
    }

    #[test]
    fn connection_close_is_rejected() {
        assert!(!check_connection(Some("close")));
        assert!(check_connection(Some("keep-alive")));
        assert!(check_connection(None));
    }

    #[test]
    fn known_bots_and_missing_ua_are_rejected() {
        assert!(!check_user_agent(None));
        assert!(!check_user_agent(Some("")));
        assert!(!check_user_agent(Some("curl/8.0")));
        assert!(!check_user_agent(Some("python-requests/2.31")));
        assert!(!check_user_agent(Some("Mozilla/5.0 ... PetalBot/1.0 ...")));
        assert!(check_user_agent(Some(
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) Firefox/120.0"
        )));
    }

    #[test]
    fn sec_fetch_only_enforced_for_supported_secure_browsers() {
        let mut view = browser_view();
        view.sec_fetch_mode = Some("no-cors".to_string());
        assert!(!check_sec_fetch(&view));

        view.is_secure = false;
        assert!(check_sec_fetch(&view));

        let mut old = browser_view();
        old.user_agent = Some("Mozilla/5.0 Chrome/70.0".to_string());
        old.sec_fetch_mode = Some("no-cors".to_string());
        assert!(check_sec_fetch(&old));
    }

    #[test]
    fn browser_support_version_thresholds() {
        assert!(is_browser_supported("mozilla/5.0 chrome/80.0"));
        assert!(!is_browser_supported("mozilla/5.0 chrome/79.0"));
        assert!(is_browser_supported("mozilla/5.0 firefox/90.0"));
        assert!(!is_browser_supported("mozilla/5.0 firefox/89.0"));
        assert!(is_browser_supported("version/16.4 safari/605"));
        assert!(!is_browser_supported("version/16.3 safari/605"));
        assert!(!is_browser_supported("some-random-agent"));
    }

    #[test]
    fn disabled_heuristics_are_skipped() {
        let cfg = HeaderHeuristics {
            accept: false,
            accept_encoding: false,
            accept_language: false,
            connection: false,
            sec_fetch: false,
            user_agent: false,
        };
        let view = HeaderView {
            user_agent: Some("curl/8.0".to_string()),
            ..HeaderView::default()
        };
        assert_eq!(evaluate(&view, &cfg), Ok(()));
    }
}
