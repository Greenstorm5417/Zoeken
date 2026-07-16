//! Apple App Store engine: queries the iTunes Search API for iOS/macOS apps.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SafeSearch, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "apple_app_store";

const BASE_URL: &str = "https://itunes.apple.com/search";

/// The Apple App Store engine.
#[derive(Debug, Clone)]
pub struct AppleAppStore {
    meta: EngineMeta,
}

impl AppleAppStore {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        AppleAppStore {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["files".to_string(), "apps".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: true,
                language_support: false,
                weight: 1,
                shortcut: "aps".to_string(),
                about: About {
                    website: Some("https://www.apple.com/app-store/".to_string()),
                    wikidata_id: Some("Q368215".to_string()),
                    official_api_documentation: Some(
                        "https://developer.apple.com/library/archive/documentation/AudioVideo/Conceptual/iTuneSearchAPI/UnderstandingSearchResults.html"
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

impl Default for AppleAppStore {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for AppleAppStore {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let explicit = if q.safesearch != SafeSearch::Off {
            "No"
        } else {
            "Yes"
        };
        let query = encode_query(&[
            ("term", q.query.clone()),
            ("media", "software".to_string()),
            ("explicit", explicit.to_string()),
        ]);
        p.url = Some(format!("{BASE_URL}?{query}"));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid iTunes search JSON: {e}")))?;

        let results = value
            .get("results")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for item in &results {
            let Some(url) = item.get("trackViewUrl").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(title) = item.get("trackName").and_then(|v| v.as_str()) else {
                continue;
            };
            let content = item
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or_default();

            res.add(Result_::Main(MainResult {
                url: url.to_string(),
                normalized_url: url.to_string(),
                title: title.to_string(),
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

    const BASIC_JSON: &str = r#"{
      "resultCount": 1,
      "results": [
        {
          "trackName": "Rust Playground",
          "trackViewUrl": "https://apps.apple.com/us/app/rust-playground/id123",
          "description": "Try Rust snippets on the go.",
          "artworkUrl100": "https://example.com/art.png",
          "sellerName": "Example Dev"
        }
      ]
    }"#;

    const EMPTY_JSON: &str = r#"{"resultCount": 0, "results": []}"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://apps.apple.com/us/app/rust-playground/id123",
            "Rust Playground",
            "Try Rust snippets on the go.",
        ));
        Fixture::capture(NAME, query("rust"), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        Fixture::capture(
            NAME,
            query("nothing"),
            response(200, EMPTY_JSON),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();

        let mut q = query("rust");
        q.safesearch = SafeSearch::Strict;
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}?term=rust&media=software&explicit=No"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, EMPTY_JSON),
            EngineResults::new(),
        )
        .with_case("request")
        .with_golden_request(golden)
        .save(dir.join("request.json"))
        .unwrap();
    }

    #[test]
    fn apple_app_store_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = AppleAppStore::new();
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
    fn safesearch_off_requests_explicit_yes() {
        let engine = AppleAppStore::new();
        let q = query("rust");
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://itunes.apple.com/search?term=rust&media=software&explicit=Yes")
        );
    }
}
