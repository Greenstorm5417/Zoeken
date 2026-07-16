//! DuckDuckGo WEB engine.
//!
//! Parses the HTML endpoint for results and instant answers.

use scraper::{Html, Selector};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView, TimeRange,
};
use zoeken_results::{Answer, MainResult, Result_, Template};

pub const NAME: &str = "duckduckgo";

const DDG_URL: &str = "https://html.duckduckgo.com/html/";

#[derive(Debug, Clone)]
pub struct DuckDuckGo {
    meta: EngineMeta,
}

impl DuckDuckGo {
    pub fn new() -> Self {
        DuckDuckGo {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string(), "web".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: true,
                safesearch: true,
                language_support: true,
                weight: 1,
                shortcut: "ddg".to_string(),
                about: About {
                    website: Some("https://lite.duckduckgo.com/lite/".to_string()),
                    wikidata_id: Some("Q12805".to_string()),
                    official_api_documentation: None,
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for DuckDuckGo {
    fn default() -> Self {
        Self::new()
    }
}

fn quote_ddg_bangs(query: &str) -> String {
    query.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn ddg_region(locale: &str) -> String {
    if locale.is_empty() || locale == "all" {
        return "wt-wt".to_string();
    }
    match locale.split_once('-') {
        Some((lang, territory)) if !lang.is_empty() && !territory.is_empty() => {
            format!("{}-{}", territory.to_lowercase(), lang.to_lowercase())
        }
        _ => "wt-wt".to_string(),
    }
}

fn time_range_df(time_range: TimeRange) -> &'static str {
    match time_range {
        TimeRange::Day => "d",
        TimeRange::Week => "w",
        TimeRange::Month => "m",
        TimeRange::Year => "y",
    }
}

fn element_text(el: &scraper::ElementRef<'_>) -> String {
    zoeken_engine_core::normalize_whitespace(&el.text().collect::<String>())
}

impl Engine for DuckDuckGo {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        if q.query.chars().count() >= 500 {
            p.url = None;
            return;
        }

        let query = quote_ddg_bangs(&q.query);
        let region = ddg_region(&q.locale);

        p.method = HttpMethod::Post;
        p.url = Some(DDG_URL.to_string());

        p.headers
            .insert("Sec-Fetch-Dest".to_string(), "document".to_string());
        p.headers
            .insert("Sec-Fetch-Mode".to_string(), "navigate".to_string());
        p.headers
            .insert("Sec-Fetch-Site".to_string(), "same-origin".to_string());
        p.headers
            .insert("Sec-Fetch-User".to_string(), "?1".to_string());
        p.headers.insert(
            "Content-Type".to_string(),
            "application/x-www-form-urlencoded".to_string(),
        );
        p.headers.insert("Referer".to_string(), DDG_URL.to_string());

        if !q.locale.is_empty() && q.locale != "all" {
            let loc = &q.locale;
            p.headers
                .entry("Accept-Language".to_string())
                .or_insert_with(|| format!("{loc},{loc}-{};q=0.7", loc.to_uppercase()));
        }

        p.data.insert("q".to_string(), query);

        if p.pageno <= 1 {
            p.data.insert("b".to_string(), String::new());
        } else {
            p.data.insert("nextParams".to_string(), String::new());
            p.data.insert("api".to_string(), "d.js".to_string());
            p.data.insert("o".to_string(), "json".to_string());
            p.data.insert("v".to_string(), "l".to_string());
            let offset = 10 + (p.pageno - 2) * 15;
            p.data.insert("dc".to_string(), (offset + 1).to_string());
            p.data.insert("s".to_string(), offset.to_string());
        }

        if region == "wt-wt" {
            p.data.insert("kl".to_string(), "wt-wt".to_string());
        } else {
            p.data.insert("kl".to_string(), region.clone());
            p.cookies.insert("kl".to_string(), region);
        }

        if let Some(time_range) = p.time_range {
            let df = time_range_df(time_range);
            p.data.insert("df".to_string(), df.to_string());
            p.cookies.insert("df".to_string(), df.to_string());
        }
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        if resp.status == 303 {
            return Ok(res);
        }

        let html = resp.text();
        let doc = Html::parse_document(&html);

        let challenge_sel = Selector::parse("form#challenge-form").unwrap();
        let result_sel = Selector::parse("div#links > div.web-result").unwrap();
        let title_sel = Selector::parse("h2 a").unwrap();
        let snippet_sel = Selector::parse("a.result__snippet").unwrap();
        let zero_click_sel = Selector::parse("div#zero_click_abstract").unwrap();
        let zero_click_link_sel = Selector::parse("div#zero_click_abstract > a").unwrap();

        if doc.select(&challenge_sel).next().is_some() {
            return Err(EngineError::Captcha(NAME.to_string()));
        }

        for div in doc.select(&result_sel) {
            let Some(title_a) = div.select(&title_sel).next() else {
                continue;
            };
            let Some(href) = title_a.value().attr("href") else {
                continue;
            };
            let title = element_text(&title_a);
            let content = div
                .select(&snippet_sel)
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

        if let Some(zc) = doc.select(&zero_click_sel).next() {
            let answer = element_text(&zc);
            if !answer.is_empty()
                && !answer.contains("Your IP address is")
                && !answer.contains("Your user agent:")
                && !answer.contains("URL Decoded:")
            {
                let url = zc
                    .select(&zero_click_link_sel)
                    .next()
                    .and_then(|a| a.value().attr("href"))
                    .map(str::to_string);
                res.add(Result_::Answer(Answer {
                    answer,
                    url,
                    engine: NAME.to_string(),
                    template: Template::Answer,
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

    fn golden_request_page1(q: &SearchQueryView) -> RequestParams {
        let mut p = prepopulated(q);
        p.method = HttpMethod::Post;
        p.url = Some(DDG_URL.to_string());
        for (k, v) in [
            ("Sec-Fetch-Dest", "document"),
            ("Sec-Fetch-Mode", "navigate"),
            ("Sec-Fetch-Site", "same-origin"),
            ("Sec-Fetch-User", "?1"),
            ("Content-Type", "application/x-www-form-urlencoded"),
            ("Referer", DDG_URL),
        ] {
            p.headers.insert(k.to_string(), v.to_string());
        }
        p.data.insert("q".to_string(), q.query.clone());
        p.data.insert("b".to_string(), String::new());
        p.data.insert("kl".to_string(), "wt-wt".to_string());
        p
    }

    const BASIC_HTML: &str = r#"<!DOCTYPE html>
<html><body>
<form action="/html/" method="post"><input type="hidden" name="vqd" value="4-123456789"></form>
<div id="links" class="results">
  <div class="result results_links results_links_deep web-result">
    <div class="links_main links_deep result__body">
      <h2 class="result__title"><a rel="nofollow" class="result__a" href="https://www.rust-lang.org/">Rust Programming Language</a></h2>
      <a class="result__snippet" href="https://www.rust-lang.org/">A language empowering everyone to build reliable and efficient software.</a>
    </div>
  </div>
  <div class="result results_links results_links_deep web-result">
    <div class="links_main links_deep result__body">
      <h2 class="result__title"><a rel="nofollow" class="result__a" href="https://doc.rust-lang.org/book/">The Rust Programming Language - The Book</a></h2>
      <a class="result__snippet" href="https://doc.rust-lang.org/book/">This book teaches you the concepts of the Rust programming language.</a>
    </div>
  </div>
  <div class="result result--ad result--ad--small">
    <div class="links_main"><h2 class="result__title"><a class="result__a" href="https://ad.example.com/">Sponsored result</a></h2>
    <a class="result__snippet">An advertisement that must be ignored.</a></div>
  </div>
</div>
</body></html>"#;

    const ANSWER_HTML: &str = r#"<!DOCTYPE html>
<html><body>
<div id="links" class="results">
  <div class="result results_links web-result">
    <h2 class="result__title"><a class="result__a" href="https://en.wikipedia.org/wiki/Rust_(programming_language)">Rust (programming language) - Wikipedia</a></h2>
    <a class="result__snippet">Rust is a general-purpose programming language emphasizing performance, type safety, and concurrency.</a>
  </div>
</div>
<div id="zero_click_abstract">Rust is a multi-paradigm, general-purpose programming language that emphasizes performance, type safety, and concurrency. <a href="https://en.wikipedia.org/wiki/Rust_(programming_language)">More at Wikipedia</a></div>
</body></html>"#;

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: DDG_URL.to_string(),
            body: body.as_bytes().to_vec(),
            ..EngineResponse::default()
        }
    }

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
            "The Rust Programming Language - The Book",
            "This book teaches you the concepts of the Rust programming language.",
        ));
        Fixture::capture(
            NAME,
            query("rust programming", "all", 1),
            response(200, BASIC_HTML),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        let mut answer = EngineResults::new();
        answer.add(main_result(
            "https://en.wikipedia.org/wiki/Rust_(programming_language)",
            "Rust (programming language) - Wikipedia",
            "Rust is a general-purpose programming language emphasizing performance, type safety, and concurrency.",
        ));
        answer.add(Result_::Answer(Answer {
            answer: "Rust is a multi-paradigm, general-purpose programming language that emphasizes performance, type safety, and concurrency. More at Wikipedia".to_string(),
            url: Some("https://en.wikipedia.org/wiki/Rust_(programming_language)".to_string()),
            engine: NAME.to_string(),
            template: Template::Answer,
        }));
        Fixture::capture(
            NAME,
            query("rust", "all", 1),
            response(200, ANSWER_HTML),
            answer,
        )
        .with_case("answer")
        .save(dir.join("answer.json"))
        .unwrap();

        Fixture::capture(
            NAME,
            query("rust", "all", 1),
            response(303, ""),
            EngineResults::new(),
        )
        .with_case("status-303")
        .save(dir.join("status-303.json"))
        .unwrap();

        let q = query("rust programming", "all", 1);
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, "<html><body></body></html>"),
            EngineResults::new(),
        )
        .with_case("request-page1")
        .with_golden_request(golden_request_page1(&q))
        .save(dir.join("request-page1.json"))
        .unwrap();
    }

    #[test]
    fn duckduckgo_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}; run the ignored generate_fixtures test"
        );
        let engine = DuckDuckGo::new();
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
    fn request_clears_url_for_overlong_query() {
        let engine = DuckDuckGo::new();
        let q = query(&"x".repeat(500), "all", 1);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert!(p.url.is_none());
    }

    #[test]
    fn request_sets_region_cookie_for_non_default_locale() {
        let engine = DuckDuckGo::new();
        let q = query("rust", "en-US", 1);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(p.data.get("kl").map(String::as_str), Some("us-en"));
        assert_eq!(p.cookies.get("kl").map(String::as_str), Some("us-en"));
    }

    #[test]
    fn response_maps_challenge_form_to_captcha() {
        let engine = DuckDuckGo::new();
        let resp = response(
            200,
            r#"<html><body><form id="challenge-form"></form></body></html>"#,
        );
        assert!(matches!(
            engine.response(&resp),
            Err(EngineError::Captcha(_))
        ));
    }
}
