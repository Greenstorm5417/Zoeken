//! Hacker News engine.
//!
//! Queries Algolia's HN API and maps hits into main results.

use chrono::{Datelike, Utc};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView, TimeRange, html_to_text,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "hackernews";

/// Algolia Hacker News API base.
const BASE_URL: &str = "https://hn.algolia.com/api/v1";

/// Results requested per page for a keyword search (the reference
/// `results_per_page`).
const RESULTS_PER_PAGE: u32 = 30;

/// The Hacker News engine.
#[derive(Debug, Clone)]
pub struct Hackernews {
    meta: EngineMeta,
}

impl Hackernews {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Hackernews {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["it".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: true,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "hn".to_string(),
                about: About {
                    website: Some("https://news.ycombinator.com/".to_string()),
                    wikidata_id: Some("Q686797".to_string()),
                    official_api_documentation: Some("https://hn.algolia.com/api".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Hackernews {
    fn default() -> Self {
        Self::new()
    }
}

/// Compute the reference `now - 1 <unit>` epoch seconds for a time range.
///
/// Mirrors `datetime.now() - relativedelta(<unit>s=1)`; the result is inherently
/// time-dependent (based on the current instant).
fn time_range_epoch(range: TimeRange) -> i64 {
    let now = Utc::now();
    let past = match range {
        TimeRange::Day => now - chrono::Duration::days(1),
        TimeRange::Week => now - chrono::Duration::weeks(1),
        TimeRange::Month => {
            let (y, m) = if now.month() == 1 {
                (now.year() - 1, 12)
            } else {
                (now.year(), now.month() - 1)
            };
            now.with_year(y)
                .and_then(|d| d.with_month(m))
                .unwrap_or(now)
        }
        TimeRange::Year => now.with_year(now.year() - 1).unwrap_or(now),
    };
    past.timestamp()
}

impl Engine for Hackernews {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;

        let page = p.pageno.saturating_sub(1);

        if q.query.is_empty() {
            // Empty query -> HN front page, ordered by date.
            let args: Vec<(&str, String)> = vec![
                ("tags", "front_page".to_string()),
                ("page", page.to_string()),
            ];
            p.url = Some(format!("{BASE_URL}/search_by_date?{}", encode_query(&args)));
            return;
        }

        let mut search_type = "search";
        let mut numeric_filters = "[]".to_string();
        if let Some(range) = p.time_range {
            search_type = "search_by_date";
            numeric_filters = format!("created_at_i>{}", time_range_epoch(range));
        }

        let args: Vec<(&str, String)> = vec![
            ("query", q.query.clone()),
            ("page", page.to_string()),
            ("hitsPerPage", RESULTS_PER_PAGE.to_string()),
            ("minWordSizefor1Typo", "4".to_string()),
            ("minWordSizefor2Typos", "8".to_string()),
            ("advancedSyntax", "true".to_string()),
            ("ignorePlurals", "false".to_string()),
            ("minProximity", "7".to_string()),
            ("numericFilters", numeric_filters),
            ("tagFilters", "[\"story\",[]]".to_string()),
            ("typoTolerance", "true".to_string()),
            ("queryType", "prefixLast".to_string()),
            (
                "restrictSearchableAttributes",
                "[\"title\",\"comment_text\",\"url\",\"story_text\",\"author\"]".to_string(),
            ),
            ("getRankingInfo", "true".to_string()),
        ];

        p.url = Some(format!("{BASE_URL}/{search_type}?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let data: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Hacker News JSON: {e}")))?;

        let hits = data
            .get("hits")
            .and_then(|h| h.as_array())
            .cloned()
            .unwrap_or_default();

        for hit in &hits {
            let object_id = hit
                .get("objectID")
                .and_then(|o| o.as_str())
                .unwrap_or("")
                .to_string();

            let title_field = hit.get("title").and_then(|t| t.as_str()).unwrap_or("");
            let author = hit.get("author").and_then(|a| a.as_str()).unwrap_or("");
            let title = if title_field.is_empty() {
                format!("author: {author}")
            } else {
                title_field.to_string()
            };

            // content = url or html_to_text(comment_text) or html_to_text(story_text)
            let url_field = hit.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let content = if !url_field.is_empty() {
                url_field.to_string()
            } else {
                let comment = html_to_text(
                    hit.get("comment_text")
                        .and_then(|c| c.as_str())
                        .unwrap_or(""),
                );
                if !comment.is_empty() {
                    comment
                } else {
                    html_to_text(hit.get("story_text").and_then(|s| s.as_str()).unwrap_or(""))
                }
            };

            let url = format!("https://news.ycombinator.com/item?id={object_id}");
            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title,
                content,
                engine: NAME.to_string(),
                ..MainResult::default()
            }));
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

    fn query(q: &str, pageno: u32) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno,
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

    const BASIC_JSON: &str = r#"{
      "hits": [
        {
          "objectID": "1",
          "title": "Show HN: A cool project",
          "author": "alice",
          "url": "https://example.com/project",
          "points": 42,
          "num_comments": 10,
          "created_at_i": 1700000000
        },
        {
          "objectID": "2",
          "author": "bob",
          "comment_text": "<p>Great <b>point</b> here</p>",
          "points": 5,
          "num_comments": 0,
          "created_at_i": 1700000100
        }
      ]
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        // Has a url -> content is the url; has a title.
        basic.add(main_result(
            "https://news.ycombinator.com/item?id=1",
            "Show HN: A cool project",
            "https://example.com/project",
        ));
        // No title -> "author: <author>"; no url -> html_to_text(comment_text).
        basic.add(main_result(
            "https://news.ycombinator.com/item?id=2",
            "author: bob",
            "Great point here",
        ));
        Fixture::capture(NAME, query("rust", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        // request-search: validates the built keyword-search URL (page 0).
        let q = query("rust", 1);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{BASE_URL}/search?query=rust&page=0&hitsPerPage=30\
&minWordSizefor1Typo=4&minWordSizefor2Typos=8&advancedSyntax=true\
&ignorePlurals=false&minProximity=7&numericFilters=%5B%5D\
&tagFilters=%5B%22story%22%2C%5B%5D%5D&typoTolerance=true&queryType=prefixLast\
&restrictSearchableAttributes=%5B%22title%22%2C%22comment_text%22%2C%22url%22%2C%22story_text%22%2C%22author%22%5D\
&getRankingInfo=true"
        ));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"hits":[]}"#),
            EngineResults::new(),
        )
        .with_case("request-search")
        .with_golden_request(golden)
        .save(dir.join("request-search.json"))
        .unwrap();

        // request-frontpage: empty query -> front page via search_by_date.
        let q = query("", 1);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}/search_by_date?tags=front_page&page=0"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"hits":[]}"#),
            EngineResults::new(),
        )
        .with_case("request-frontpage")
        .with_golden_request(golden)
        .save(dir.join("request-frontpage.json"))
        .unwrap();
    }

    #[test]
    fn hackernews_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Hackernews::new();
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
    fn empty_query_uses_front_page() {
        let engine = Hackernews::new();
        let q = query("", 1);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://hn.algolia.com/api/v1/search_by_date?tags=front_page&page=0")
        );
    }

    #[test]
    fn time_range_switches_to_search_by_date() {
        let engine = Hackernews::new();
        let mut q = query("rust", 1);
        q.time_range = Some(TimeRange::Week);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        let url = p.url.unwrap();
        assert!(url.starts_with("https://hn.algolia.com/api/v1/search_by_date?"));
        assert!(url.contains("numericFilters=created_at_i%3E"));
    }
}
