//! Shared challenge/captcha classification.
//!
//! One taxonomy for transport-generic bot walls (Cloudflare, reCAPTCHA,
//! Fastly/AWS WAF/Anubis interstitials) shared by `zoeken-network` (pre-parse,
//! status + headers + raw body) and `zoeken-engines` (post-parse fallback for
//! engines with no vendor-specific selector). Engines add vendor signals on
//! top of [`classify_challenge`] rather than re-implementing detection.

/// Transport-generic challenge/captcha kind, detectable from status + body alone.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChallengeKind {
    CloudflareCaptcha,
    CloudflareFirewall,
    Recaptcha,
    GenericBotWall,
}

fn is_cloudflare_challenge(status: u16, body: &str) -> bool {
    if matches!(status, 429 | 503) {
        if body.contains("__cf_chl_jschl_tk__=") {
            return true;
        }
        if body.contains("/cdn-cgi/challenge-platform/")
            && body.contains("orchestrate/jsch/v1")
            && body.contains("window._cf_chl_enter(")
        {
            return true;
        }
    }
    status == 403 && body.contains("__cf_chl_captcha_tk__=")
}

fn is_cloudflare_firewall(status: u16, body: &str) -> bool {
    status == 403 && body.contains("<span class=\"cf-error-code\">1020</span>")
}

fn is_recaptcha_wall(status: u16, body: &str) -> bool {
    status == 503 && body.contains("\"https://www.google.com/recaptcha/")
}

/// Detect anti-bot JavaScript gates or captcha walls in an HTML body.
///
/// Some vendors (Cloudflare, DataDome) inject passive beacon scripts into
/// pages they serve normally, so those markers only count as a wall when the
/// site actually refused the request; markers unique to interstitial
/// challenge pages count at any status.
pub fn looks_like_bot_wall(status: u16, body: &str) -> bool {
    // Markers that only ever appear on a challenge/captcha interstitial.
    const CHALLENGE_MARKERS: &[&str] = &[
        // Fastly "Client Challenge" (pypi.org, metacpan.org)
        "<title>Client Challenge</title>",
        // Google sorry page
        "sorry/index",
        "sorry.google.com",
        // Cloudflare managed-challenge interstitial
        "<title>Just a moment...</title>",
        // AWS WAF challenge page (goodreads.com)
        "window.awsWafCookieDomainList",
        // Anubis proof-of-work wall (wiki.archlinux.org)
        "Making sure you&#39;re not a bot!",
        "Making sure you're not a bot!",
        "id=\"anubis_challenge\"",
    ];
    // Beacon scripts that Cloudflare/DataDome also embed in normal pages.
    const BLOCKED_ONLY_MARKERS: &[&str] = &["/cdn-cgi/challenge-platform/", "captcha-delivery.com"];
    if CHALLENGE_MARKERS.iter().any(|marker| body.contains(marker)) {
        return true;
    }
    matches!(status, 401 | 403 | 405 | 429 | 503)
        && BLOCKED_ONLY_MARKERS
            .iter()
            .any(|marker| body.contains(marker))
}

/// Classify a transport-generic challenge from status, a `Server: cloudflare*`
/// hint, and (a prefix of) the response body. Returns `None` for ordinary
/// error responses; callers still handle plain access-denied / rate-limit
/// status codes themselves.
pub fn classify_challenge(
    status: u16,
    server_is_cloudflare: bool,
    body: &str,
) -> Option<ChallengeKind> {
    if server_is_cloudflare {
        if is_cloudflare_challenge(status, body) {
            return Some(ChallengeKind::CloudflareCaptcha);
        }
        if is_cloudflare_firewall(status, body) {
            return Some(ChallengeKind::CloudflareFirewall);
        }
    }
    if is_recaptcha_wall(status, body) {
        return Some(ChallengeKind::Recaptcha);
    }
    if looks_like_bot_wall(status, body) {
        return Some(ChallengeKind::GenericBotWall);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cloudflare_challenge_detected_only_with_server_header() {
        let body = "__cf_chl_jschl_tk__=abc";
        assert_eq!(
            classify_challenge(503, true, body),
            Some(ChallengeKind::CloudflareCaptcha)
        );
        assert_eq!(classify_challenge(503, false, body), None);
    }

    #[test]
    fn cloudflare_firewall_detected() {
        let body = "<span class=\"cf-error-code\">1020</span>";
        assert_eq!(
            classify_challenge(403, true, body),
            Some(ChallengeKind::CloudflareFirewall)
        );
    }

    #[test]
    fn recaptcha_wall_detected() {
        let body = "\"https://www.google.com/recaptcha/api.js\"";
        assert_eq!(
            classify_challenge(503, false, body),
            Some(ChallengeKind::Recaptcha)
        );
    }

    #[test]
    fn generic_bot_wall_detected_as_fallback() {
        assert_eq!(
            classify_challenge(200, false, "<title>Just a moment...</title>"),
            Some(ChallengeKind::GenericBotWall)
        );
    }

    #[test]
    fn google_noscript_enablejs_is_not_a_bot_wall() {
        let body = r#"<html><head><title>Google Search</title></head><body><noscript><meta content="0;url=/httpservice/retry/enablejs?sei=x" http-equiv="refresh"></noscript></body></html>"#;
        assert!(!looks_like_bot_wall(200, body));
    }

    #[test]
    fn passive_cloudflare_beacon_on_ok_page_is_not_a_bot_wall() {
        // Cloudflare injects this invisible beacon into pages it serves
        // normally (e.g. wallhaven.cc search results).
        let body = r#"<html><body><div class="results">real results</div><script src="/cdn-cgi/challenge-platform/scripts/jsd/main.js"></script></body></html>"#;
        assert!(!looks_like_bot_wall(200, body));
        assert!(looks_like_bot_wall(403, body));
    }

    #[test]
    fn challenge_pages_are_bot_walls_at_any_status() {
        assert!(looks_like_bot_wall(200, "<title>Client Challenge</title>"));
        assert!(looks_like_bot_wall(200, "<title>Just a moment...</title>"));
        assert!(looks_like_bot_wall(
            202,
            "<script>window.awsWafCookieDomainList = [];</script>"
        ));
        assert!(looks_like_bot_wall(
            200,
            "<title>Making sure you&#39;re not a bot!</title>"
        ));
    }

    #[test]
    fn cloudflare_challenge_status_gating() {
        assert!(is_cloudflare_challenge(429, "x __cf_chl_jschl_tk__=y"));
        assert!(is_cloudflare_challenge(
            503,
            "/cdn-cgi/challenge-platform/x orchestrate/jsch/v1 window._cf_chl_enter("
        ));
        assert!(is_cloudflare_challenge(403, "x __cf_chl_captcha_tk__=zzz"));
        assert!(!is_cloudflare_challenge(200, "__cf_chl_jschl_tk__="));
    }

    #[test]
    fn cloudflare_firewall_status_gating() {
        assert!(is_cloudflare_firewall(
            403,
            "<span class=\"cf-error-code\">1020</span>"
        ));
        assert!(!is_cloudflare_firewall(403, "no marker"));
        assert!(!is_cloudflare_firewall(
            200,
            "<span class=\"cf-error-code\">1020</span>"
        ));
    }
}
