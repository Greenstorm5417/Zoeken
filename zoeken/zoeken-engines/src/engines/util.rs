//! Shared helpers: URL encoding, HTML entities, Markdown to text, and bot-wall detection.

use url::form_urlencoded;

/// Percent-encode a query component (spaces → `+`, others %XX-escaped like Python's quote_plus).
pub fn encode_component(value: &str) -> String {
    form_urlencoded::byte_serialize(value.as_bytes()).collect()
}

/// Build a form-urlencoded query string; order is preserved for deterministic output.
pub fn encode_query(pairs: &[(&str, String)]) -> String {
    let mut ser = form_urlencoded::Serializer::new(String::new());
    for (k, v) in pairs {
        ser.append_pair(k, v);
    }
    ser.finish()
}

/// Percent-encode a URL path (like Python's quote with safe='/'): `/` stays literal, space → `%20`.
pub fn encode_path(value: &str) -> String {
    value
        .split('/')
        .map(|seg| {
            form_urlencoded::byte_serialize(seg.as_bytes())
                .collect::<String>()
                .replace('+', "%20")
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Decode %XX percent-escapes (like Python's unquote; `+` stays literal, not space).
pub fn percent_decode(value: &str) -> String {
    // form_urlencoded::parse treats `+` as space and `&` as a pair break — neutralize both.
    let safe = value.replace('+', "%2B").replace('&', "%26");
    form_urlencoded::parse(format!("x={safe}").as_bytes())
        .next()
        .map(|(_, v)| v.into_owned())
        .unwrap_or_default()
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

/// Decode HTML character references (&amp;, &#NN;, &#xNN;, etc.) leniently.
pub fn html_unescape(input: &str) -> String {
    html_escape::decode_html_entities(input).into_owned()
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
    use scraper::ElementRef;
    use scraper::node::Node;

    fn has_skipped_class(el: &scraper::node::Element, skip_classes: &[&str]) -> bool {
        match el.attr("class") {
            Some(class_attr) => class_attr
                .split_whitespace()
                .any(|tok| skip_classes.contains(&tok)),
            None => false,
        }
    }

    fn walk_element(el: ElementRef<'_>, skip: &[&str], out: &mut String) {
        let element = el.value();
        let name = element.name();
        if name.eq_ignore_ascii_case("script")
            || name.eq_ignore_ascii_case("style")
            || has_skipped_class(element, skip)
        {
            return;
        }
        for child in el.children() {
            match child.value() {
                Node::Text(text) => out.push_str(text),
                Node::Element(_) => {
                    if let Some(child_el) = ElementRef::wrap(child) {
                        walk_element(child_el, skip, out);
                    }
                }
                _ => {}
            }
        }
    }

    let mut out = String::new();
    walk_element(el, skip_classes, &mut out);
    zoeken_engine_core::normalize_whitespace(&out)
}

/// Format a duration in seconds as `H:MM:SS` / `M:SS`.
pub fn format_duration_secs(secs: u64) -> String {
    let hours = secs / 3600;
    let minutes = (secs % 3600) / 60;
    let seconds = secs % 60;
    if hours > 0 {
        format!("{hours}:{minutes:02}:{seconds:02}")
    } else {
        format!("{minutes}:{seconds:02}")
    }
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
    fn encode_path_keeps_slashes_literal() {
        assert_eq!(encode_path("/a b/c"), "/a%20b/c");
        assert_eq!(encode_path("rust-lang.org/docs"), "rust-lang.org/docs");
    }

    #[test]
    fn percent_decode_reverses_encoding_and_keeps_plus_literal() {
        assert_eq!(percent_decode("a%2Bb"), "a+b");
        assert_eq!(percent_decode("a+b"), "a+b");
        assert_eq!(percent_decode("%E6%97%A5"), "日");
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
    fn text_content_skips_marked_classes_and_scripts() {
        use scraper::{Html, Selector};
        let html =
            r#"<p><span class="algoSlug_icon">ICON</span>Web <script>var x=1;</script>result</p>"#;
        let doc = Html::parse_fragment(html);
        let sel = Selector::parse("p").unwrap();
        let p = doc.select(&sel).next().unwrap();
        assert_eq!(text_content_skipping(p, &["algoSlug_icon"]), "Web result");
    }

    #[test]
    fn formats_duration_secs() {
        assert_eq!(super::format_duration_secs(100), "1:40");
        assert_eq!(super::format_duration_secs(615), "10:15");
        assert_eq!(super::format_duration_secs(3661), "1:01:01");
    }
}
