//! Shared helpers: URL encoding, HTML entities, Markdown to text, and bot-wall detection.

/// Percent-encode a query component (spaces → `+`, others %XX-escaped like Python's quote_plus).
pub fn encode_component(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            b' ' => out.push('+'),
            other => {
                out.push('%');
                out.push_str(&format!("{other:02X}"));
            }
        }
    }
    out
}

/// Build a form-urlencoded query string; order is preserved for deterministic output.
pub fn encode_query(pairs: &[(&str, String)]) -> String {
    pairs
        .iter()
        .map(|(k, v)| format!("{}={}", encode_component(k), encode_component(v)))
        .collect::<Vec<_>>()
        .join("&")
}

/// Percent-encode a URL path component (like Python's quote with safe='/').
pub fn encode_path(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' | b'/' => {
                out.push(*byte as char)
            }
            other => {
                out.push('%');
                out.push_str(&format!("{other:02X}"));
            }
        }
    }
    out
}

/// Extract substring between start and end markers; returns empty if not found.
pub fn extr<'a>(text: &'a str, start: &str, end: &str) -> &'a str {
    let Some(start_idx) = text.find(start) else {
        return "";
    };
    let after = start_idx + start.len();
    match text[after..].find(end) {
        Some(rel) => &text[after..after + rel],
        None => "",
    }
}

/// Decode %XX percent-escapes in a string (like Python's unquote; + is literal).
pub fn percent_decode(value: &str) -> String {
    let bytes = value.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// Decode HTML character references (&amp;, &#NN;, &#xNN;, etc.) leniently.
pub fn html_unescape(input: &str) -> String {
    let bytes = input.as_bytes();
    let mut out = String::with_capacity(input.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'&' {
            out.push(bytes[i] as char);
            i += 1;
            continue;
        }
        // Find the terminating ';' within a reasonable window.
        let Some(rel) = input[i..].find(';') else {
            out.push('&');
            i += 1;
            continue;
        };
        let entity = &input[i + 1..i + rel];
        let decoded: Option<String> = match entity {
            "amp" => Some("&".to_string()),
            "lt" => Some("<".to_string()),
            "gt" => Some(">".to_string()),
            "quot" => Some("\"".to_string()),
            "apos" | "#39" => Some("'".to_string()),
            "nbsp" => Some("\u{00A0}".to_string()),
            _ => {
                if let Some(hex) = entity
                    .strip_prefix("#x")
                    .or_else(|| entity.strip_prefix("#X"))
                {
                    u32::from_str_radix(hex, 16)
                        .ok()
                        .and_then(char::from_u32)
                        .map(|c| c.to_string())
                } else if let Some(dec) = entity.strip_prefix('#') {
                    dec.parse::<u32>()
                        .ok()
                        .and_then(char::from_u32)
                        .map(|c| c.to_string())
                } else {
                    None
                }
            }
        };
        match decoded {
            Some(text) => {
                out.push_str(&text);
                i += rel + 1;
            }
            None => {
                out.push('&');
                i += 1;
            }
        }
    }
    out
}

/// Reduce Markdown to plain text: strips links, headings, emphasis, and normalizes whitespace.
pub fn markdown_to_text(markdown: &str) -> String {
    let without_links = strip_markdown_links(markdown);
    let mut out = String::with_capacity(without_links.len());
    for line in without_links.lines() {
        // Strip leading heading and blockquote markers.
        let line = line.trim_start();
        let line = line.trim_start_matches('#').trim_start();
        let line = line.trim_start_matches('>').trim_start();
        for ch in line.chars() {
            match ch {
                '*' | '_' | '`' | '~' => {}
                c => out.push(c),
            }
        }
        out.push(' ');
    }
    zoeken_engine_core::normalize_whitespace(&out)
}

fn strip_markdown_links(s: &str) -> String {
    let chars: Vec<char> = s.chars().collect();
    let mut out = String::with_capacity(s.len());
    let mut i = 0;
    while i < chars.len() {
        // Image: `![alt](url)` -> alt.
        if chars[i] == '!'
            && i + 1 < chars.len()
            && chars[i + 1] == '['
            && let Some((text, next)) = parse_markdown_link(&chars, i + 1)
        {
            out.push_str(&text);
            i = next;
            continue;
        }
        // Link: `[text](url)` -> text.
        if chars[i] == '['
            && let Some((text, next)) = parse_markdown_link(&chars, i)
        {
            out.push_str(&text);
            i = next;
            continue;
        }
        out.push(chars[i]);
        i += 1;
    }
    out
}

fn parse_markdown_link(chars: &[char], start: usize) -> Option<(String, usize)> {
    debug_assert_eq!(chars[start], '[');
    let mut i = start + 1;
    let text_start = i;
    while i < chars.len() && chars[i] != ']' {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    let text: String = chars[text_start..i].iter().collect();
    i += 1; // past ']'
    if i >= chars.len() || chars[i] != '(' {
        return None;
    }
    i += 1; // past '('
    while i < chars.len() && chars[i] != ')' {
        i += 1;
    }
    if i >= chars.len() {
        return None;
    }
    i += 1; // past ')'
    Some((text, i))
}

/// Extract normalized text from an element, skipping specified classes and script/style tags.
pub fn text_content_skipping(el: scraper::ElementRef<'_>, skip_classes: &[&str]) -> String {
    fn has_skipped_class(el: &scraper::node::Element, skip_classes: &[&str]) -> bool {
        match el.attr("class") {
            Some(class_attr) => class_attr
                .split_whitespace()
                .any(|tok| skip_classes.contains(&tok)),
            None => false,
        }
    }

    fn walk(node: ego_tree::NodeRef<'_, scraper::node::Node>, skip: &[&str], out: &mut String) {
        match node.value() {
            scraper::node::Node::Text(text) => out.push_str(text),
            scraper::node::Node::Element(element) => {
                let name = element.name();
                if name.eq_ignore_ascii_case("script")
                    || name.eq_ignore_ascii_case("style")
                    || has_skipped_class(element, skip)
                {
                    return;
                }
                for child in node.children() {
                    walk(child, skip, out);
                }
            }
            _ => {
                for child in node.children() {
                    walk(child, skip, out);
                }
            }
        }
    }

    let mut out = String::new();
    walk(*el, skip_classes, &mut out);
    zoeken_engine_core::normalize_whitespace(&out)
}

/// Detect anti-bot JavaScript gates or captcha walls in HTML body.
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_component_matches_quote_plus() {
        assert_eq!(encode_component("a b"), "a+b");
        assert_eq!(encode_component("c++"), "c%2B%2B");
        assert_eq!(encode_component("rust-lang.org"), "rust-lang.org");
        assert_eq!(encode_component("日"), "%E6%97%A5");
    }

    #[test]
    fn encode_query_preserves_order() {
        let q = encode_query(&[("q", "a b".to_string()), ("p", "2".to_string())]);
        assert_eq!(q, "q=a+b&p=2");
    }

    #[test]
    fn markdown_to_text_reduces_links_and_headings() {
        assert_eq!(
            markdown_to_text("[example](https://example.com)"),
            "example"
        );
        assert_eq!(markdown_to_text("## Headline"), "Headline");
        assert_eq!(
            markdown_to_text("A community about the [Rust](https://rust-lang.org) language."),
            "A community about the Rust language."
        );
        assert_eq!(
            markdown_to_text("## Big news\n\nWe shipped **it**."),
            "Big news We shipped it."
        );
        assert_eq!(
            markdown_to_text("![alt text](https://img.example/x.png)"),
            "alt text"
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
    fn text_content_skips_marked_classes_and_scripts() {
        use scraper::{Html, Selector};
        let html =
            r#"<p><span class="algoSlug_icon">ICON</span>Web <script>var x=1;</script>result</p>"#;
        let doc = Html::parse_fragment(html);
        let sel = Selector::parse("p").unwrap();
        let p = doc.select(&sel).next().unwrap();
        assert_eq!(text_content_skipping(p, &["algoSlug_icon"]), "Web result");
    }
}
