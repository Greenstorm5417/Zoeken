//! crates.io search engine.
//!
//! Queries the crates.io API and maps each package into a main result.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "crates";

/// crates.io v1 search endpoint.
const SEARCH_URL: &str = "https://crates.io/api/v1/crates";

/// Results requested per page (the reference `page_size`).
const PAGE_SIZE: u32 = 10;

/// The crates.io engine.
#[derive(Debug, Clone)]
pub struct Crates {
    meta: EngineMeta,
}

impl Crates {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Crates {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec![
                    "it".to_string(),
                    "packages".to_string(),
                    "cargo".to_string(),
                ],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "crates".to_string(),
                about: About {
                    website: Some("https://crates.io/".to_string()),
                    wikidata_id: None,
                    official_api_documentation: Some("https://crates.io/data-access".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Crates {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for Crates {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![
            ("page", p.pageno.to_string()),
            ("q", q.query.clone()),
            ("per_page", PAGE_SIZE.to_string()),
        ];
        p.url = Some(format!("{SEARCH_URL}?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid crates.io JSON: {e}")))?;

        let packages = value
            .get("crates")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        for package in &packages {
            let name = package.get("name").and_then(|n| n.as_str()).unwrap_or("");
            let content = package
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            let url = format!("https://crates.io/crates/{name}");
            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title: name.to_string(),
                content: content.to_string(),
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
      "crates": [
        {
          "name": "serde",
          "description": "A generic serialization/deserialization framework",
          "newest_version": "1.0.0",
          "max_version": "1.0.0",
          "keywords": ["serde", "serialization"],
          "updated_at": "2024-01-02T03:04:05.000000+00:00"
        },
        {
          "name": "tokio",
          "description": "An asynchronous runtime for Rust",
          "newest_version": "1.35.0",
          "keywords": ["async"],
          "updated_at": "2024-02-03T04:05:06.000000+00:00"
        }
      ]
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://crates.io/crates/serde",
            "serde",
            "A generic serialization/deserialization framework",
        ));
        basic.add(main_result(
            "https://crates.io/crates/tokio",
            "tokio",
            "An asynchronous runtime for Rust",
        ));
        Fixture::capture(NAME, query("serde", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        // request-page2: validates the built API URL and parameter order.
        let q = query("serde", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{SEARCH_URL}?page=2&q=serde&per_page=10"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"crates":[]}"#),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn crates_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Crates::new();
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
    fn builds_paged_request_url() {
        let engine = Crates::new();
        let q = query("ripgrep", 3);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://crates.io/api/v1/crates?page=3&q=ripgrep&per_page=10")
        );
    }
}
