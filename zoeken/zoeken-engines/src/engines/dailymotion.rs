//! Dailymotion engine: JSON video search API.
//!
//! Language/region trait tables from upstream (fetched dynamically from
//! Dailymotion) are approximated here with a direct locale-subtag mapping,
//! which covers the common `xx` / `xx-YY` cases without a network fetch.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SafeSearch, SearchQueryView,
};
use zoeken_results::{MainResult, Result_, Template};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "dailymotion";

const SEARCH_URL: &str = "https://api.dailymotion.com/videos?";

const PAGE_SIZE: u32 = 10;

const RESULT_FIELDS: &str =
    "allow_embed,description,title,created_time,duration,url,thumbnail_360_url,id";

#[derive(Debug, Clone)]
pub struct Dailymotion {
    meta: EngineMeta,
}

impl Dailymotion {
    pub fn new() -> Self {
        Dailymotion {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["videos".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: true,
                safesearch: true,
                language_support: true,
                weight: 1,
                shortcut: "dm".to_string(),
                about: About {
                    website: Some("https://www.dailymotion.com".to_string()),
                    wikidata_id: Some("Q769222".to_string()),
                    official_api_documentation: Some(
                        "https://www.dailymotion.com/developer".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Dailymotion {
    fn default() -> Self {
        Self::new()
    }
}

fn family_filter(safesearch: SafeSearch) -> &'static str {
    match safesearch {
        SafeSearch::Off => "false",
        SafeSearch::Moderate | SafeSearch::Strict => "true",
    }
}

fn locale_language(locale: &str) -> &str {
    if locale.is_empty() || locale.eq_ignore_ascii_case("all") {
        return "en";
    }
    locale.split(['-', '_']).next().unwrap_or("en")
}

impl Engine for Dailymotion {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        if q.query.is_empty() {
            p.url = None;
            return;
        }
        p.method = HttpMethod::Get;

        let lang = locale_language(&q.locale);
        let mut args: Vec<(&str, String)> = vec![
            ("search", q.query.clone()),
            ("family_filter", family_filter(q.safesearch).to_string()),
            ("thumbnail_ratio", "original".to_string()),
            ("languages", lang.to_string()),
            ("page", p.pageno.to_string()),
            ("password_protected", "false".to_string()),
            ("private", "false".to_string()),
            ("sort", "relevance".to_string()),
            ("limit", PAGE_SIZE.to_string()),
            ("fields", RESULT_FIELDS.to_string()),
        ];

        if matches!(q.safesearch, SafeSearch::Moderate | SafeSearch::Strict) {
            args.push(("is_created_for_kids", "true".to_string()));
        }

        let parts: Vec<&str> = q.locale.split(['-', '_']).collect();
        if parts.len() == 2 {
            let region = format!("{}_{}", parts[0], parts[1].to_uppercase());
            args.push(("localization", region.clone()));
            args.push(("country", parts[1].to_uppercase()));
        }

        p.url = Some(format!("{SEARCH_URL}{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Dailymotion JSON: {e}")))?;

        if let Some(err) = value.get("error") {
            let msg = err
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("unknown error");
            return Err(EngineError::Unexpected(format!(
                "Dailymotion API error: {msg}"
            )));
        }

        let items = value
            .get("list")
            .and_then(|l| l.as_array())
            .cloned()
            .unwrap_or_default();

        for item in &items {
            let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("");
            if url.is_empty() {
                continue;
            }
            let title = item
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let description = item
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            let mut content = zoeken_engine_core::html_to_text(description);
            if content.chars().count() > 300 {
                content = content.chars().take(300).collect::<String>() + "...";
            }
            let thumbnail = item
                .get("thumbnail_360_url")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let iframe_src = item
                .get("id")
                .and_then(|id| id.as_str())
                .filter(|id| !id.is_empty())
                .map(|id| format!("https://www.dailymotion.com/embed/video/{id}"))
                .unwrap_or_default();
            let length = item
                .get("duration")
                .and_then(|v| v.as_u64())
                .map(super::util::format_duration_secs)
                .unwrap_or_default();
            let published_date = item
                .get("created_time")
                .and_then(|v| v.as_i64())
                .filter(|&ts| ts > 0)
                .and_then(|ts| {
                    use chrono::{TimeZone, Utc};
                    Utc.timestamp_opt(ts, 0)
                        .single()
                        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
                });

            res.add(Result_::Main(MainResult {
                url: url.to_string(),
                normalized_url: url.to_string(),
                title,
                content,
                engine: NAME.to_string(),
                template: Template::Videos,
                thumbnail,
                iframe_src,
                length,
                published_date,
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
            url: SEARCH_URL.to_string(),
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
      "list": [
        {
          "title": "Rust in 100 seconds",
          "url": "https://www.dailymotion.com/video/x123",
          "description": "A quick overview of Rust.",
          "created_time": 0,
          "duration": 100,
          "allow_embed": true,
          "id": "x123",
          "thumbnail_360_url": "http://s.dm/x123.jpg"
        }
      ]
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://www.dailymotion.com/video/x123",
            "Rust in 100 seconds",
            "A quick overview of Rust.",
        ));
        Fixture::capture(NAME, query("rust", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        Fixture::capture(
            NAME,
            query("rust", 1),
            response(200, r#"{"list":[]}"#),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();

        let q = query("rust", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        let encoded_fields = RESULT_FIELDS.replace(',', "%2C");
        golden.url = Some(format!(
            "{SEARCH_URL}search=rust&family_filter=false&thumbnail_ratio=original&languages=en&page=2&password_protected=false&private=false&sort=relevance&limit=10&fields={encoded_fields}"
        ));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"list":[]}"#),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn dailymotion_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Dailymotion::new();
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
    fn empty_query_clears_url() {
        let engine = Dailymotion::new();
        let q = query("", 1);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert!(p.url.is_none());
    }

    #[test]
    fn api_error_becomes_engine_error() {
        let engine = Dailymotion::new();
        let err = engine
            .response(&response(200, r#"{"error":{"message":"bad request"}}"#))
            .unwrap_err();
        assert!(matches!(err, EngineError::Unexpected(_)));
    }
}
