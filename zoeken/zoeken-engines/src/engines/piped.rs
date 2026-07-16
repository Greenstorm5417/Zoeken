//! Piped engine: privacy-friendly YouTube frontend, JSON search API.
//!
//! Supports two content filters mirroring the upstream `piped_filter` setting:
//! `videos` (categories: videos) and `music_songs` (categories: music).
//! Paging follows the *nextpage*-driven API: the JSON response's `nextpage`
//! value is threaded back through `engine_data` on subsequent requests.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "piped";

const BACKEND_URL: &str = "https://pipedapi.kavin.rocks";
const FRONTEND_URL: &str = "https://piped.video";

const NEXTPAGE_KEY: &str = "nextpage";

/// Content filter selecting which Piped result set to query.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PipedFilter {
    Videos,
    MusicSongs,
}

impl PipedFilter {
    fn as_str(self) -> &'static str {
        match self {
            PipedFilter::Videos => "videos",
            PipedFilter::MusicSongs => "music_songs",
        }
    }
}

/// The Piped engine.
#[derive(Debug, Clone)]
pub struct Piped {
    meta: EngineMeta,
    filter: PipedFilter,
}

impl Piped {
    fn build(filter: PipedFilter, categories: Vec<&str>, shortcut: &str) -> Self {
        Piped {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: categories.into_iter().map(str::to_string).collect(),
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: shortcut.to_string(),
                about: About {
                    website: Some("https://github.com/TeamPiped/Piped/".to_string()),
                    wikidata_id: Some("Q107565255".to_string()),
                    official_api_documentation: Some(
                        "https://docs.piped.video/docs/api-documentation/".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
            filter,
        }
    }

    /// Videos filter (categories: `videos`).
    pub fn videos() -> Self {
        Self::build(PipedFilter::Videos, vec!["videos"], "ppd")
    }

    /// Music songs filter (categories: `music`).
    pub fn music() -> Self {
        Self::build(PipedFilter::MusicSongs, vec!["music"], "ppdm")
    }
}

impl Default for Piped {
    fn default() -> Self {
        Self::videos()
    }
}

impl Engine for Piped {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;

        if p.pageno > 1
            && let Some(nextpage) = q.engine_data.get(NEXTPAGE_KEY)
        {
            let args: Vec<(&str, String)> = vec![
                ("q", q.query.clone()),
                ("filter", self.filter.as_str().to_string()),
                ("nextpage", nextpage.clone()),
            ];
            p.url = Some(format!(
                "{BACKEND_URL}/nextpage/search?{}",
                encode_query(&args)
            ));
            return;
        }

        let args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            ("filter", self.filter.as_str().to_string()),
        ];
        p.url = Some(format!("{BACKEND_URL}/search?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Piped JSON: {e}")))?;

        let items = value
            .get("items")
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default();

        for item in &items {
            let path = item.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let url = format!("{FRONTEND_URL}{path}");
            let title = item
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();

            let content = if self.filter == PipedFilter::Videos {
                item.get("shortDescription")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string()
            } else {
                item.get("uploaderName")
                    .and_then(|d| d.as_str())
                    .unwrap_or("")
                    .to_string()
            };

            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title,
                content,
                engine: NAME.to_string(),
                ..MainResult::default()
            }));
        }

        if let Some(nextpage) = value.get("nextpage") {
            if let Some(s) = nextpage.as_str() {
                res.engine_data
                    .insert(NEXTPAGE_KEY.to_string(), s.to_string());
            } else if !nextpage.is_null() {
                res.engine_data
                    .insert(NEXTPAGE_KEY.to_string(), nextpage.to_string());
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
            url: BACKEND_URL.to_string(),
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
      "items": [
        {
          "url": "/watch?v=abc123",
          "title": "Rust in 100 seconds",
          "shortDescription": "A quick overview of Rust.",
          "uploaded": 1234567890000,
          "duration": 100,
          "views": 5000
        }
      ],
      "nextpage": "opaque-token-1"
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://piped.video/watch?v=abc123",
            "Rust in 100 seconds",
            "A quick overview of Rust.",
        ));
        basic
            .engine_data
            .insert(NEXTPAGE_KEY.to_string(), "opaque-token-1".to_string());
        Fixture::capture(NAME, query("rust", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        Fixture::capture(
            NAME,
            query("rust", 1),
            response(200, r#"{"items":[]}"#),
            EngineResults::new(),
        )
        .with_case("no-items")
        .save(dir.join("no-items.json"))
        .unwrap();

        let q = query("rust", 1);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BACKEND_URL}/search?q=rust&filter=videos"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"items":[]}"#),
            EngineResults::new(),
        )
        .with_case("request-basic")
        .with_golden_request(golden)
        .save(dir.join("request-basic.json"))
        .unwrap();
    }

    #[test]
    fn piped_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Piped::videos();
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
    fn nextpage_token_is_threaded_into_request() {
        let engine = Piped::videos();
        let mut q = query("rust", 2);
        q.engine_data
            .insert(NEXTPAGE_KEY.to_string(), "tok".to_string());
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://pipedapi.kavin.rocks/nextpage/search?q=rust&filter=videos&nextpage=tok")
        );
    }

    #[test]
    fn music_filter_uses_uploader_as_content() {
        let engine = Piped::music();
        let body = r#"{"items":[{"url":"/watch?v=xyz","title":"Song","uploaderName":"Artist"}]}"#;
        let res = engine.response(&response(200, body)).unwrap();
        match &res.results[0] {
            Result_::Main(m) => assert_eq!(m.content, "Artist"),
            _ => panic!("expected main result"),
        }
    }
}
