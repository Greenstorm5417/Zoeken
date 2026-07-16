//! Tootfinder engine: queries the Tootfinder REST API for Mastodon posts.
//!
//! The API occasionally appends server-side error HTML after the JSON payload, so the
//! response body is scanned line-by-line for the first line that looks like a JSON array.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView, html_to_text,
};
use zoeken_results::{MainResult, Result_};

/// Engine name / identifier.
pub const NAME: &str = "tootfinder";

const BASE_URL: &str = "https://www.tootfinder.ch";

/// The Tootfinder engine.
#[derive(Debug, Clone)]
pub struct Tootfinder {
    meta: EngineMeta,
}

impl Tootfinder {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Tootfinder {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["social media".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "tf".to_string(),
                about: About {
                    website: Some("https://www.tootfinder.ch".to_string()),
                    wikidata_id: None,
                    official_api_documentation: Some(
                        "https://wiki.tootfinder.ch/index.php?name=the-tootfinder-rest-api"
                            .to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Tootfinder {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for Tootfinder {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        p.url = Some(format!("{BASE_URL}/rest/api/search/{}", q.query));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let body = resp.text();
        let json_line = body
            .lines()
            .find(|line| line.starts_with("[{"))
            .unwrap_or("");

        if json_line.is_empty() {
            return Ok(res);
        }

        let value: serde_json::Value = serde_json::from_str(json_line)
            .map_err(|e| EngineError::Parse(format!("invalid Tootfinder JSON: {e}")))?;

        let entries = value.as_array().cloned().unwrap_or_default();

        for entry in &entries {
            let Some(url) = entry.get("url").and_then(|v| v.as_str()) else {
                continue;
            };
            let content_html = entry.get("content").and_then(|v| v.as_str()).unwrap_or("");
            let content = html_to_text(content_html);

            let title = entry
                .get("card")
                .and_then(|c| c.get("title"))
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .unwrap_or_else(|| content.chars().take(75).collect());

            res.add(Result_::Main(MainResult {
                url: url.to_string(),
                normalized_url: url.to_string(),
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

    fn query(q: &str) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno: 1,
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

    const BASIC_BODY: &str = "some server debug preamble\n[{\"url\": \"https://mastodon.social/@a/1\", \"content\": \"<p>Hello <b>Rust</b></p>\", \"card\": {\"title\": \"\"}, \"media_attachments\": []}]\ntrailing garbage";

    const EMPTY_BODY: &str = "no json here at all";

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://mastodon.social/@a/1",
            "Hello Rust",
            "Hello Rust",
        ));
        Fixture::capture(NAME, query("rust"), response(200, BASIC_BODY), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        Fixture::capture(
            NAME,
            query("nothing"),
            response(200, EMPTY_BODY),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();

        let q = query("rust lang");
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}/rest/api/search/rust lang"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, EMPTY_BODY),
            EngineResults::new(),
        )
        .with_case("request")
        .with_golden_request(golden)
        .save(dir.join("request.json"))
        .unwrap();
    }

    #[test]
    fn tootfinder_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Tootfinder::new();
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
    fn builds_request_url() {
        let engine = Tootfinder::new();
        let q = query("rust lang");
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://www.tootfinder.ch/rest/api/search/rust lang")
        );
    }
}
