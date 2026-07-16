//! Startpage WEB engine: POSTs to search endpoint and extracts results from embedded JSON blob.
//!
//! Omits stateful `sc` token (cached separately); strips published-date prefix from content.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SafeSearch, SearchQueryView, TimeRange,
};
use zoeken_results::{MainResult, Result_};

use super::util::extr;

/// Engine name / identifier.
pub const NAME: &str = "startpage";

const BASE_URL: &str = "https://www.startpage.com";

const SEARCH_URL: &str = "https://www.startpage.com/sp/search";

const JSON_START: &str = "React.createElement(UIStartpage.AppSerpWeb, {";

const JSON_END: &str = "}})";

#[derive(Debug, Clone)]
pub struct Startpage {
    meta: EngineMeta,
}

impl Startpage {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Startpage {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string(), "web".to_string()],
                paging: true,
                max_page: 18,
                time_range_support: true,
                safesearch: true,
                language_support: true,
                weight: 1,
                shortcut: "sp".to_string(),
                about: About {
                    website: Some("https://startpage.com".to_string()),
                    wikidata_id: Some("Q2333295".to_string()),
                    official_api_documentation: None,
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for Startpage {
    fn default() -> Self {
        Self::new()
    }
}

fn time_range_code(time_range: TimeRange) -> &'static str {
    match time_range {
        TimeRange::Day => "d",
        TimeRange::Week => "w",
        TimeRange::Month => "m",
        TimeRange::Year => "y",
    }
}

fn safesearch_code(safesearch: SafeSearch) -> &'static str {
    match safesearch {
        SafeSearch::Off => "none",
        SafeSearch::Moderate => "moderate",
        SafeSearch::Strict => "heavy",
    }
}

/// Strip a leading published-date fragment from result content.
fn strip_published_date_prefix(content: &str) -> String {
    fn is_date_prefix(content: &str) -> bool {
        // "<1-31> <Mon> <YYYY> ... " where Mon is an uppercase-initial 3-letter
        // month abbreviation.
        let mut parts = content.splitn(4, ' ');
        let (day, mon, year) = match (parts.next(), parts.next(), parts.next()) {
            (Some(d), Some(m), Some(y)) => (d, m, y),
            _ => return false,
        };
        let day_ok = matches!(day.parse::<u32>(), Ok(1..=31));
        let mon_ok = mon.len() == 3
            && mon.chars().next().is_some_and(|c| c.is_ascii_uppercase())
            && mon.chars().skip(1).all(|c| c.is_ascii_lowercase());
        let year_ok = year.len() == 4 && year.chars().all(|c| c.is_ascii_digit());
        day_ok && mon_ok && year_ok
    }

    fn is_days_ago_prefix(content: &str) -> bool {
        // "<n> days ago ... " / "<n> day ago ... "
        let mut parts = content.splitn(3, ' ');
        match (parts.next(), parts.next(), parts.next()) {
            (Some(n), Some(unit), Some(_)) => {
                n.chars().all(|c| c.is_ascii_digit())
                    && !n.is_empty()
                    && (unit == "days" || unit == "day")
            }
            _ => false,
        }
    }

    if (is_date_prefix(content) || is_days_ago_prefix(content))
        && content.contains("... ")
        && let Some(pos) = content.find("...")
    {
        let date_pos = pos + 4;
        if date_pos <= content.len() {
            return content[date_pos..].to_string();
        }
    }
    content.to_string()
}

impl Engine for Startpage {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Post;
        p.url = Some(SEARCH_URL.to_string());
        p.headers.insert("Origin".to_string(), BASE_URL.to_string());
        p.headers
            .insert("Referer".to_string(), format!("{BASE_URL}/"));

        p.data.insert("query".to_string(), q.query.clone());
        p.data.insert("cat".to_string(), "web".to_string());
        p.data.insert("t".to_string(), "device".to_string());
        p.data.insert("abd".to_string(), "1".to_string());
        p.data.insert("abe".to_string(), "1".to_string());
        p.data.insert("qsr".to_string(), "all".to_string());
        p.data.insert(
            "qadf".to_string(),
            safesearch_code(q.safesearch).to_string(),
        );
        p.data.insert(
            "with_date".to_string(),
            p.time_range.map(time_range_code).unwrap_or("").to_string(),
        );

        if p.pageno > 1 {
            p.data.insert("page".to_string(), p.pageno.to_string());
            p.data
                .insert("segment".to_string(), "startpage.udog".to_string());
        }
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        if let Some(location) = resp.headers.get("Location")
            && location.starts_with("https://www.startpage.com/sp/captcha")
        {
            return Err(EngineError::Captcha(NAME.to_string()));
        }

        let mut res = EngineResults::new();
        let text = resp.text();
        let between = extr(&text, JSON_START, JSON_END);
        if between.is_empty() {
            return Ok(res);
        }
        let results_raw = format!("{{{between}}}}}");

        let json: serde_json::Value = serde_json::from_str(&results_raw)
            .map_err(|e| EngineError::Parse(format!("invalid Startpage JSON: {e}")))?;

        let mainline = json
            .get("render")
            .and_then(|r| r.get("presenter"))
            .and_then(|p| p.get("regions"))
            .and_then(|r| r.get("mainline"))
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();

        for categ in &mainline {
            let display_type = categ
                .get("display_type")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            let Some(items) = categ.get("results").and_then(|r| r.as_array()) else {
                continue;
            };
            if display_type != "web-google" {
                continue;
            }
            for item in items {
                let url = item.get("clickUrl").and_then(|u| u.as_str()).unwrap_or("");
                let title = zoeken_engine_core::html_to_text(
                    item.get("title").and_then(|t| t.as_str()).unwrap_or(""),
                );
                let content = zoeken_engine_core::html_to_text(
                    item.get("description")
                        .and_then(|d| d.as_str())
                        .unwrap_or(""),
                );
                let content = strip_published_date_prefix(&content);

                res.add(Result_::Main(MainResult {
                    url: url.to_string(),
                    normalized_url: url.to_string(),
                    title,
                    content,
                    engine: NAME.to_string(),
                    ..MainResult::default()
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
            url: SEARCH_URL.to_string(),
            body: body.as_bytes().to_vec(),
            ..EngineResponse::default()
        }
    }

    /// The inner JSON object the page embeds. `results_raw` reconstructs this
    /// exact object as `"{" + between + "}}"`, so `between` is this string with
    /// its leading `{` and trailing `}}` removed. The object contains no `)` so
    /// the `}})` end marker is unambiguous.
    const RESULT_JSON: &str = r#"{"render":{"presenter":{"regions":{"mainline":[{"display_type":"web-google","results":[{"clickUrl":"https://www.rust-lang.org/","title":"Rust Programming Language","description":"A language empowering everyone."},{"clickUrl":"https://doc.rust-lang.org/book/","title":"The Rust Book","description":"Learn Rust."}]}]}}}}"#;

    /// Build a fake Startpage HTML page embedding `json` after the marker the
    /// parser looks for. The page text is `...AppSerpWeb, <json>)`: since the
    /// marker ends in `{` (the JSON's first char) and the end marker `}})`
    /// matches the JSON's trailing `}}` plus the appended `)`, the parser's
    /// `"{" + extr(...) + "}}"` reconstructs `json` exactly.
    fn embed(json: &str) -> String {
        format!(
            "<html><body><script>window.x=1;React.createElement(UIStartpage.AppSerpWeb, {json});</script></body></html>"
        )
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://www.rust-lang.org/",
            "Rust Programming Language",
            "A language empowering everyone.",
        ));
        basic.add(main_result(
            "https://doc.rust-lang.org/book/",
            "The Rust Book",
            "Learn Rust.",
        ));
        Fixture::capture(
            NAME,
            query("rust", "all", 1),
            response(200, &embed(RESULT_JSON)),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();
    }

    #[test]
    fn startpage_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Startpage::new();
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
    fn captcha_redirect_maps_to_captcha() {
        let engine = Startpage::new();
        let mut resp = response(200, "<html></html>");
        resp.headers.insert(
            "Location".to_string(),
            "https://www.startpage.com/sp/captcha?x=1".to_string(),
        );
        assert!(matches!(
            engine.response(&resp),
            Err(EngineError::Captcha(_))
        ));
    }

    #[test]
    fn strips_leading_published_date() {
        assert_eq!(
            strip_published_date_prefix("2 Sep 2014 ... actual content here"),
            "actual content here"
        );
        assert_eq!(
            strip_published_date_prefix("5 days ago ... fresh content"),
            "fresh content"
        );
        assert_eq!(strip_published_date_prefix("no date here"), "no date here");
    }

    #[test]
    fn request_builds_post_form() {
        let engine = Startpage::new();
        let q = query("rust", "all", 2);
        let mut p = RequestParams {
            query: q.query.clone(),
            pageno: q.pageno,
            ..RequestParams::default()
        };
        engine.request(&q, &mut p);
        assert_eq!(p.method, HttpMethod::Post);
        assert_eq!(p.data.get("query").map(String::as_str), Some("rust"));
        assert_eq!(p.data.get("page").map(String::as_str), Some("2"));
    }
}
