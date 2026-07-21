//! Brave WEB engine.
//!
//! Parses Brave search results and related-query suggestions.

use scraper::{Html, Selector};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SafeSearch, SearchQueryView, TimeRange,
};
use zoeken_results::{MainResult, Result_, Suggestion};

use super::util::{encode_query, looks_like_bot_wall};

/// Engine name / identifier.
pub const NAME: &str = "brave";

/// Base URL of Brave Search.
const BASE_URL: &str = "https://search.brave.com";

/// The Brave WEB engine.
#[derive(Debug, Clone)]
pub struct Brave {
    meta: EngineMeta,
}

impl Brave {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Brave {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string(), "web".to_string()],
                paging: true,
                max_page: 10,
                time_range_support: true,
                safesearch: true,
                language_support: true,
                weight: 1,
                shortcut: "br".to_string(),
                about: About {
                    website: Some("https://search.brave.com/".to_string()),
                    wikidata_id: Some("Q22906900".to_string()),
                    official_api_documentation: None,
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for Brave {
    fn default() -> Self {
        Self::new()
    }
}

/// Reference `safesearch_map`.
fn safesearch_cookie(safesearch: SafeSearch) -> &'static str {
    match safesearch {
        SafeSearch::Off => "off",
        SafeSearch::Moderate => "moderate",
        SafeSearch::Strict => "strict",
    }
}

/// Reference `time_range_map`.
fn time_range_tf(time_range: TimeRange) -> &'static str {
    match time_range {
        TimeRange::Day => "pd",
        TimeRange::Week => "pw",
        TimeRange::Month => "pm",
        TimeRange::Year => "py",
    }
}

/// Normalized text content of an element (mirrors the reference `extract_text`).
fn element_text(el: &scraper::ElementRef<'_>) -> String {
    zoeken_engine_core::normalize_whitespace(&el.text().collect::<String>())
}

/// Whether `url` has a network location (host), mirroring the reference's
/// `urlparse(url).netloc` truthiness check used to filter out ad/partial URLs.
fn has_netloc(url: &str) -> bool {
    let rest = match url.split_once("://") {
        Some((_scheme, rest)) => rest,
        None => match url.strip_prefix("//") {
            Some(rest) => rest,
            None => return false,
        },
    };
    let host = rest.split(['/', '?', '#']).next().unwrap_or("");
    !host.is_empty()
}

impl Engine for Brave {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;

        let mut args: Vec<(&str, String)> =
            vec![("q", q.query.clone()), ("source", "web".to_string())];

        if p.pageno > 1 {
            args.push(("offset", (p.pageno - 1).to_string()));
        }
        if let Some(time_range) = p.time_range {
            args.push(("tf", time_range_tf(time_range).to_string()));
        }

        p.url = Some(format!("{BASE_URL}/search?{}", encode_query(&args)));

        p.cookies.insert(
            "safesearch".to_string(),
            safesearch_cookie(q.safesearch).to_string(),
        );
        p.cookies.insert("useLocation".to_string(), "0".to_string());
        p.cookies.insert("summarizer".to_string(), "0".to_string());

        // country = last segment of the region, lowercased (reference
        // `engine_region.split("-")[-1].lower()`); ui_lang best-effort.
        let (country, ui_lang) = match q.locale.split_once('-') {
            Some((lang, territory)) if !territory.is_empty() => (
                territory.to_lowercase(),
                format!("{}-{}", lang.to_lowercase(), territory.to_lowercase()),
            ),
            _ => ("all".to_string(), "en-us".to_string()),
        };
        p.cookies.insert("country".to_string(), country);
        p.cookies.insert("ui_lang".to_string(), ui_lang);
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let html = resp.text();
        if looks_like_bot_wall(resp.status, &html) {
            return Err(EngineError::Captcha(NAME.to_string()));
        }
        if resp.status == 429 {
            return Err(EngineError::TooManyRequests(NAME.to_string()));
        }
        if resp.status == 403 {
            return Err(EngineError::AccessDenied(NAME.to_string()));
        }
        let doc = Html::parse_document(&html);

        let snippet_sel = Selector::parse("div.snippet").unwrap();
        let link_sel = Selector::parse("a[href]").unwrap();
        let title_sel = Selector::parse("div[class*=\"title\"]").unwrap();
        let content_sel = Selector::parse("div.content").unwrap();
        let date_sel = Selector::parse("span[class*=\"t-secondary\"]").unwrap();
        let suggestion_sel = Selector::parse("a.related-query").unwrap();

        for snippet in doc.select(&snippet_sel) {
            let url = snippet
                .select(&link_sel)
                .next()
                .and_then(|a| a.value().attr("href"))
                .unwrap_or("");
            let title_tag = snippet.select(&title_sel).next();
            if url.is_empty() || title_tag.is_none() || !has_netloc(url) {
                continue;
            }
            let title = element_text(&title_tag.unwrap());

            let mut content = String::new();
            if let Some(content_el) = snippet.select(&content_sel).next() {
                content = element_text(&content_el);
                let pub_date = content_el
                    .select(&date_sel)
                    .next()
                    .map(|el| element_text(&el))
                    .unwrap_or_default();
                if !pub_date.is_empty() {
                    // Reference `content.lstrip(_pub_date).strip("- \n\t")`:
                    // `str.lstrip` treats its argument as a *set* of characters.
                    content = content
                        .trim_start_matches(|c: char| pub_date.contains(c))
                        .trim_matches(|c: char| matches!(c, '-' | ' ' | '\n' | '\t'))
                        .to_string();
                }
            }

            res.add(Result_::Main(MainResult {
                url: url.to_string(),
                normalized_url: url.to_string(),
                title,
                content,
                engine: NAME.to_string(),
                ..MainResult::default()
            }));
        }

        for sug in doc.select(&suggestion_sel) {
            let suggestion = element_text(&sug);
            if !suggestion.is_empty() {
                res.add(Result_::Suggestion(Suggestion {
                    suggestion,
                    engine: NAME.to_string(),
                }));
            }
        }

        // A normal no-result page says so explicitly. A populated Brave search
        // shell with neither results nor the no-result marker is their silent
        // bot-wall variant and must not be cached as a successful empty page.
        let lower = html.to_ascii_lowercase();
        let has_search_shell = lower.contains("data-testid=\"web-results\"")
            || lower.contains("id=\"results\"")
            || lower.contains("class=\"search-results");
        let explicit_zero = lower.contains("no results found")
            || lower.contains("couldn't find any results")
            || lower.contains("could not find any results");
        if res.is_empty() && has_search_shell && !explicit_zero {
            return Err(EngineError::Captcha(NAME.to_string()));
        }

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::{Fixture, load_fixtures_for, run_all};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
    }

    fn query(q: &str, locale: &str, pageno: u32) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno,
            locale: locale.to_string(),
            ..SearchQueryView::default()
        }
    }

    fn main_result(url: &str, title: &str, content: &str) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            engine: NAME.to_string(),
            ..MainResult::default()
        })
    }

    #[test]
    fn challenge_and_silent_shell_are_not_successful_empty_results() {
        let engine = Brave::new();
        assert!(matches!(
            engine.response(&response(200, "<title>Just a moment...</title>")),
            Err(EngineError::Captcha(_))
        ));
        assert!(matches!(
            engine.response(&response(
                200,
                "<main id=\"results\" data-testid=\"web-results\"></main>"
            )),
            Err(EngineError::Captcha(_))
        ));
    }

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: BASE_URL.to_string(),
            body: body.as_bytes().to_vec(),
            ..EngineResponse::default()
        }
    }

    fn prepopulated(q: &SearchQueryView) -> RequestParams {
        RequestParams {
            query: q.query.clone(),
            pageno: q.pageno,
            safesearch: q.safesearch,
            time_range: q.time_range,
            locale_key: q.locale.clone(),
            ..RequestParams::default()
        }
    }

    const BASIC_HTML: &str = r#"<!DOCTYPE html>
<html><body>
<div id="results">
  <div class="snippet ">
    <a href="https://www.rust-lang.org/"><div class="title">Rust Programming Language</div></a>
    <div class="content ">A language empowering everyone to build reliable software.</div>
  </div>
  <div class="snippet ">
    <a href="https://doc.rust-lang.org/book/"><div class="title">The Rust Book</div></a>
    <div class="content ">This book teaches the concepts of Rust.</div>
  </div>
  <div class="snippet ">
    <a href="/partial/ad"><div class="title">An advert</div></a>
    <div class="content ">Should be ignored.</div>
  </div>
</div>
<a class="related-query" href="/search?q=rustup">rustup</a>
</body></html>"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://www.rust-lang.org/",
            "Rust Programming Language",
            "A language empowering everyone to build reliable software.",
        ));
        basic.add(main_result(
            "https://doc.rust-lang.org/book/",
            "The Rust Book",
            "This book teaches the concepts of Rust.",
        ));
        basic.add(Result_::Suggestion(Suggestion {
            suggestion: "rustup".to_string(),
            engine: NAME.to_string(),
        }));
        Fixture::capture(
            NAME,
            query("rust", "all", 1),
            response(200, BASIC_HTML),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        // request-page2: offset on page 2.
        let q = query("rust", "all", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}/search?q=rust&source=web&offset=1"));
        for (k, v) in [
            ("safesearch", "off"),
            ("useLocation", "0"),
            ("summarizer", "0"),
            ("country", "all"),
            ("ui_lang", "en-us"),
        ] {
            golden.cookies.insert(k.to_string(), v.to_string());
        }
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, "<html><body></body></html>"),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn brave_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Brave::new();
        if let Err(mismatches) = run_all(&engine, &fixtures) {
            let report = mismatches
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            panic!("conformance failures:\n{report}");
        }
    }

    #[test]
    fn has_netloc_filters_partial_urls() {
        assert!(has_netloc("https://example.com/x"));
        assert!(!has_netloc("/partial/ad"));
    }
}
