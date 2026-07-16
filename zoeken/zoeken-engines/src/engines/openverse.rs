//! Openverse image search engine.
//!
//! Queries the image API and maps each result into an image entry.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Image, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "openverse";

/// Openverse images API base (the query string is appended).
const BASE_URL: &str = "https://api.openverse.org/v1/images/";

/// Results requested per page (the reference `page_size`).
const PAGE_SIZE: u32 = 20;

/// The Openverse images engine.
#[derive(Debug, Clone)]
pub struct Openverse {
    meta: EngineMeta,
}

impl Openverse {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Openverse {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["images".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "opv".to_string(),
                about: About {
                    website: Some("https://openverse.org/".to_string()),
                    wikidata_id: None,
                    official_api_documentation: Some("https://api.openverse.org/v1/".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Openverse {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for Openverse {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        // Order mirrors the reference `search_string`: page, page_size, format, q.
        let args: Vec<(&str, String)> = vec![
            ("page", p.pageno.to_string()),
            ("page_size", PAGE_SIZE.to_string()),
            ("format", "json".to_string()),
            ("q", q.query.clone()),
        ];
        p.url = Some(format!("{BASE_URL}?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Openverse JSON: {e}")))?;

        let results = value
            .get("results")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        for item in &results {
            let url = item
                .get("foreign_landing_url")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string();
            let title = item
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let img_src = item
                .get("url")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string();
            res.add(Result_::Image(Image {
                url: url.clone(),
                normalized_url: url,
                title,
                img_src,
                engine: NAME.to_string(),
                ..Image::default()
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

    fn image(url: &str, title: &str, img_src: &str) -> Result_ {
        Result_::Image(Image {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            img_src: img_src.to_string(),
            engine: NAME.to_string(),
            ..Image::default()
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
      "result_count": 2,
      "results": [
        {
          "title": "Blue Cat",
          "foreign_landing_url": "https://example.com/photos/1",
          "url": "https://images.example.com/1.jpg"
        },
        {
          "title": "Green Cat",
          "foreign_landing_url": "https://example.com/photos/2",
          "url": "https://images.example.com/2.jpg"
        }
      ]
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(image(
            "https://example.com/photos/1",
            "Blue Cat",
            "https://images.example.com/1.jpg",
        ));
        basic.add(image(
            "https://example.com/photos/2",
            "Green Cat",
            "https://images.example.com/2.jpg",
        ));
        Fixture::capture(NAME, query("cat", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        // request-page2: validates the built API URL for page 2.
        let q = query("blue cat", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{BASE_URL}?page=2&page_size=20&format=json&q=blue+cat"
        ));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"results":[]}"#),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn openverse_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Openverse::new();
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
    fn empty_results_yield_nothing() {
        let engine = Openverse::new();
        let res = engine
            .response(&response(200, r#"{"results":[]}"#))
            .unwrap();
        assert!(res.is_empty());
    }
}
