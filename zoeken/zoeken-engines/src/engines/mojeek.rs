//! Mojeek WEB engine.
//!
//! Parses general web results and spell suggestions from the HTML search page.

use scraper::{Html, Selector};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SafeSearch, SearchQueryView, TimeRange,
};
use zoeken_results::{MainResult, Result_, Suggestion};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "mojeek";

/// Base URL of the Mojeek instance.
const BASE_URL: &str = "https://www.mojeek.com";

/// The Mojeek general WEB engine.
#[derive(Debug, Clone)]
pub struct Mojeek {
    meta: EngineMeta,
}

impl Mojeek {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Mojeek {
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
                shortcut: "mjk".to_string(),
                about: About {
                    website: Some("https://mojeek.com".to_string()),
                    wikidata_id: Some("Q60747299".to_string()),
                    official_api_documentation: Some(
                        "https://www.mojeek.com/support/api/search/request_parameters.html"
                            .to_string(),
                    ),
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for Mojeek {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalized text content of an element (mirrors the reference `extract_text`).
fn element_text(el: &scraper::ElementRef<'_>) -> String {
    zoeken_engine_core::normalize_whitespace(&el.text().collect::<String>())
}

/// The `YYYYMMDD` date `n` of the given time-range units in the past, mirroring
/// the reference `datetime.now() - relativedelta(...)`.
fn since_date(time_range: TimeRange) -> String {
    use chrono::{Datelike, Duration, Local, Months};
    let now = Local::now().date_naive();
    let past = match time_range {
        TimeRange::Day => now - Duration::days(1),
        TimeRange::Week => now - Duration::weeks(1),
        TimeRange::Month => now.checked_sub_months(Months::new(1)).unwrap_or(now),
        TimeRange::Year => now.checked_sub_months(Months::new(12)).unwrap_or(now),
    };
    format!("{:04}{:02}{:02}", past.year(), past.month(), past.day())
}

impl Engine for Mojeek {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;

        let safe = match q.safesearch {
            SafeSearch::Off => "0",
            // `min(safesearch, 1)`: both moderate and strict map to 1.
            SafeSearch::Moderate | SafeSearch::Strict => "1",
        };

        let mut args: Vec<(&str, String)> =
            vec![("q", q.query.clone()), ("safe", safe.to_string())];

        if p.pageno > 1 {
            args.push(("s", (10 * (p.pageno - 1)).to_string()));
        }

        if let Some(time_range) = p.time_range {
            args.push(("since", since_date(time_range)));
        }

        p.url = Some(format!("{BASE_URL}/search?{}", encode_query(&args)));

        // Best-effort locale cookies (see the module docs). Only set for a
        // concrete `lang-REGION` locale; the trait-based mapping is wired later.
        if !q.locale.is_empty()
            && q.locale != "all"
            && let Some((lang, territory)) = q.locale.split_once('-')
            && !lang.is_empty()
            && !territory.is_empty()
        {
            p.cookies.insert("lb".to_string(), lang.to_lowercase());
            p.cookies
                .insert("arc".to_string(), territory.to_uppercase());
        }
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let html = resp.text();
        let doc = Html::parse_document(&html);

        let li_sel = Selector::parse("ul.results-standard > li").unwrap();
        let url_sel = Selector::parse("a.ob").unwrap();
        let title_sel = Selector::parse("h2 a").unwrap();
        let content_sel = Selector::parse("p.s").unwrap();
        let suggestion_sel = Selector::parse("p.spell em a").unwrap();

        for li in doc.select(&li_sel) {
            let Some(url_a) = li.select(&url_sel).next() else {
                continue;
            };
            let Some(href) = url_a.value().attr("href") else {
                continue;
            };
            let title = li
                .select(&title_sel)
                .next()
                .map(|el| element_text(&el))
                .unwrap_or_default();
            let content = li
                .select(&content_sel)
                .next()
                .map(|el| element_text(&el))
                .unwrap_or_default();

            res.add(Result_::Main(MainResult {
                url: href.to_string(),
                normalized_url: href.to_string(),
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
<ul class="results-standard">
  <li>
    <h2><a href="https://www.rust-lang.org/">Rust Programming Language</a></h2>
    <a class="ob" href="https://www.rust-lang.org/">https://www.rust-lang.org</a>
    <p class="s">A language empowering everyone to build reliable and efficient software.</p>
  </li>
  <li>
    <h2><a href="https://doc.rust-lang.org/book/">The Rust Programming Language - Book</a></h2>
    <a class="ob" href="https://doc.rust-lang.org/book/">https://doc.rust-lang.org/book</a>
    <p class="s">This book teaches you the concepts of the Rust programming language.</p>
  </li>
</ul>
</div>
</body></html>"#;

    const SUGGESTION_HTML: &str = r#"<!DOCTYPE html>
<html><body>
<div class="top-info"><p class="top-info spell">Did you mean <em><a href="/search?q=rust">rust</a></em></p></div>
<div id="results">
<ul class="results-standard">
  <li>
    <h2><a href="https://en.wikipedia.org/wiki/Rust">Rust - Wikipedia</a></h2>
    <a class="ob" href="https://en.wikipedia.org/wiki/Rust">https://en.wikipedia.org/wiki/Rust</a>
    <p class="s">Rust is an iron oxide.</p>
  </li>
</ul>
</div>
</body></html>"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://www.rust-lang.org/",
            "Rust Programming Language",
            "A language empowering everyone to build reliable and efficient software.",
        ));
        basic.add(main_result(
            "https://doc.rust-lang.org/book/",
            "The Rust Programming Language - Book",
            "This book teaches you the concepts of the Rust programming language.",
        ));
        Fixture::capture(
            NAME,
            query("rust", "all", 1),
            response(200, BASIC_HTML),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        let mut suggestion = EngineResults::new();
        suggestion.add(main_result(
            "https://en.wikipedia.org/wiki/Rust",
            "Rust - Wikipedia",
            "Rust is an iron oxide.",
        ));
        suggestion.add(Result_::Suggestion(Suggestion {
            suggestion: "rust".to_string(),
            engine: NAME.to_string(),
        }));
        Fixture::capture(
            NAME,
            query("rust", "all", 1),
            response(200, SUGGESTION_HTML),
            suggestion,
        )
        .with_case("suggestion")
        .save(dir.join("suggestion.json"))
        .unwrap();

        // request-page2: validates request URL building for page 2 (offset s).
        let q = query("rust programming", "all", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}/search?q=rust+programming&safe=0&s=10"));
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
    fn mojeek_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Mojeek::new();
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
    fn request_sets_offset_on_later_pages() {
        let engine = Mojeek::new();
        let q = query("rust", "all", 3);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert!(p.url.as_deref().unwrap().contains("s=20"));
    }

    #[test]
    fn request_sets_locale_cookies_for_concrete_locale() {
        let engine = Mojeek::new();
        let q = query("rust", "en-US", 1);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(p.cookies.get("lb").map(String::as_str), Some("en"));
        assert_eq!(p.cookies.get("arc").map(String::as_str), Some("US"));
    }
}
