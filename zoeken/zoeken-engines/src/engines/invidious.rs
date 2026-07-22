//! Invidious engine: YouTube frontend, JSON search API.
//!
//! Requires a configured `base_url` (an Invidious instance); there is no
//! reliable public default so callers should configure one via
//! [`Invidious::with_base_url`].

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView, TimeRange,
};
use zoeken_results::{MainResult, Result_, Template};

/// Engine name / identifier.
pub const NAME: &str = "invidious";

const DEFAULT_BASE_URL: &str = "https://invidious.example";

/// The Invidious engine.
#[derive(Debug, Clone)]
pub struct Invidious {
    meta: EngineMeta,
    base_url: String,
}

impl Invidious {
    /// Create the engine with its reference metadata and a placeholder
    /// `base_url`; call [`Invidious::with_base_url`] to point at a real
    /// instance.
    pub fn new() -> Self {
        Invidious {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["videos".to_string(), "music".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: true,
                safesearch: false,
                language_support: true,
                weight: 1,
                shortcut: "inv".to_string(),
                about: About {
                    website: Some("https://api.invidious.io/".to_string()),
                    wikidata_id: Some("Q79343316".to_string()),
                    official_api_documentation: Some("https://docs.invidious.io/api/".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
            base_url: DEFAULT_BASE_URL.to_string(),
        }
    }

    /// Override the Invidious instance base URL (no trailing slash expected).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into();
        self
    }
}

impl Default for Invidious {
    fn default() -> Self {
        Self::new()
    }
}

fn time_range_value(time_range: TimeRange) -> &'static str {
    match time_range {
        TimeRange::Day => "today",
        TimeRange::Week => "week",
        TimeRange::Month => "month",
        TimeRange::Year => "year",
    }
}

impl Engine for Invidious {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;

        let mut url = format!(
            "{}/api/v1/search?q={}&page={}",
            self.base_url,
            super::util::encode_component(&q.query),
            p.pageno
        );

        if let Some(time_range) = p.time_range {
            url.push_str(&format!("&date={}", time_range_value(time_range)));
        }

        if !q.locale.is_empty() && !q.locale.eq_ignore_ascii_case("all") {
            let parts: Vec<&str> = q.locale.split('-').collect();
            if parts.len() == 2 {
                url.push_str(&format!("&range={}", parts[1]));
            }
        }

        p.url = Some(url);
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Invidious JSON: {e}")))?;

        let Some(items) = value.as_array() else {
            return Ok(res);
        };

        for item in items {
            if item.get("type").and_then(|t| t.as_str()) != Some("video") {
                continue;
            }
            let Some(video_id) = item.get("videoId").and_then(|v| v.as_str()) else {
                continue;
            };

            let url = format!("{}/watch?v={video_id}", self.base_url);
            let title = item
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let content = item
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let thumbnail = invidious_thumbnail(item, video_id);
            let iframe_src = format!("{}/embed/{video_id}", self.base_url);
            let length = item
                .get("lengthSeconds")
                .and_then(|v| v.as_u64())
                .map(super::util::format_duration_secs)
                .unwrap_or_default();
            let author = item
                .get("author")
                .and_then(|a| a.as_str())
                .unwrap_or("")
                .to_string();
            let published_date = item
                .get("published")
                .and_then(|v| v.as_i64())
                .filter(|&ts| ts > 0)
                .and_then(unix_to_iso);

            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title,
                content,
                engine: NAME.to_string(),
                template: Template::Videos,
                thumbnail,
                iframe_src,
                length,
                author,
                published_date,
                ..MainResult::default()
            }));
        }

        Ok(res)
    }
}

fn invidious_thumbnail(item: &serde_json::Value, video_id: &str) -> String {
    if let Some(thumbs) = item.get("videoThumbnails").and_then(|v| v.as_array()) {
        for quality in ["medium", "high", "default", "maxres"] {
            if let Some(url) = thumbs.iter().find_map(|t| {
                let q = t.get("quality").and_then(|q| q.as_str()).unwrap_or("");
                if q == quality {
                    t.get("url").and_then(|u| u.as_str())
                } else {
                    None
                }
            }) {
                return url.to_string();
            }
        }
        if let Some(url) = thumbs
            .first()
            .and_then(|t| t.get("url"))
            .and_then(|u| u.as_str())
        {
            return url.to_string();
        }
    }
    format!("https://i.ytimg.com/vi/{video_id}/hqdefault.jpg")
}

fn unix_to_iso(ts: i64) -> Option<String> {
    use chrono::{TimeZone, Utc};
    Utc.timestamp_opt(ts, 0)
        .single()
        .map(|dt| dt.format("%Y-%m-%dT%H:%M:%SZ").to_string())
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
            template: Template::Videos,
            thumbnail: "https://i.ytimg.com/vi/abc123/hqdefault.jpg".to_string(),
            iframe_src: "https://invidious.example/embed/abc123".to_string(),
            length: "1:40".to_string(),
            author: "Fireship".to_string(),
            published_date: Some("2024-01-01T00:00:00Z".to_string()),
            ..MainResult::default()
        })
    }

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: DEFAULT_BASE_URL.to_string(),
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

    const BASIC_JSON: &str = r#"[
      {
        "type": "video",
        "videoId": "abc123",
        "title": "Rust in 100 seconds",
        "description": "A quick overview.",
        "author": "Fireship",
        "viewCount": 5000,
        "lengthSeconds": 100,
        "published": 1704067200
      },
      {
        "type": "channel",
        "author": "Someone"
      }
    ]"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://invidious.example/watch?v=abc123",
            "Rust in 100 seconds",
            "A quick overview.",
        ));
        Fixture::capture(NAME, query("rust", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        Fixture::capture(
            NAME,
            query("rust", 1),
            response(200, "[]"),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();

        let mut q = query("rust", 2);
        q.time_range = Some(TimeRange::Week);
        q.locale = "en-US".to_string();
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{DEFAULT_BASE_URL}/api/v1/search?q=rust&page=2&date=week&range=US"
        ));
        Fixture::capture(NAME, q.clone(), response(200, "[]"), EngineResults::new())
            .with_case("request-page2-timerange")
            .with_golden_request(golden)
            .save(dir.join("request-page2-timerange.json"))
            .unwrap();
    }

    #[test]
    fn invidious_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Invidious::new();
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
    fn with_base_url_overrides_default() {
        let engine = Invidious::new().with_base_url("https://yewtu.be");
        let q = query("rust", 1);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://yewtu.be/api/v1/search?q=rust&page=1")
        );
    }

    #[test]
    fn non_video_entries_are_skipped() {
        let engine = Invidious::new();
        let res = engine.response(&response(200, BASIC_JSON)).unwrap();
        assert_eq!(res.results.len(), 1);
    }
}
