//! GitHub repository search engine.
//!
//! Queries the repository search API and keeps the visible fields from each
//! result item.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "github";

/// GitHub repository search endpoint (with the fixed sort/order prefix).
const SEARCH_URL: &str = "https://api.github.com/search/repositories?sort=stars&order=desc";

/// The `Accept` header the reference sends (text-match preview media type).
const ACCEPT_HEADER: &str = "application/vnd.github.preview.text-match+json";

/// The GitHub engine.
#[derive(Debug, Clone)]
pub struct Github {
    meta: EngineMeta,
}

impl Github {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Github {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["it".to_string(), "repos".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "gh".to_string(),
                about: About {
                    website: Some("https://github.com/".to_string()),
                    wikidata_id: Some("Q364".to_string()),
                    official_api_documentation: Some(
                        "https://developer.github.com/v3/".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Github {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for Github {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![("q", q.query.clone())];
        p.url = Some(format!("{SEARCH_URL}&{}", encode_query(&args)));
        p.headers
            .insert("Accept".to_string(), ACCEPT_HEADER.to_string());
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid GitHub JSON: {e}")))?;

        let items = value
            .get("items")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        for item in &items {
            let url = item
                .get("html_url")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string();
            let title = item
                .get("full_name")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();

            let content = ["language", "description"]
                .iter()
                .filter_map(|key| item.get(*key).and_then(|v| v.as_str()))
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" / ");

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
            url: "https://api.github.com/".to_string(),
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
          "html_url": "https://github.com/rust-lang/rust",
          "full_name": "rust-lang/rust",
          "name": "rust",
          "language": "Rust",
          "description": "Empowering everyone to build reliable software."
        },
        {
          "html_url": "https://github.com/BurntSushi/ripgrep",
          "full_name": "BurntSushi/ripgrep",
          "name": "ripgrep",
          "description": "recursively search directories"
        }
      ]
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://github.com/rust-lang/rust",
            "rust-lang/rust",
            "Rust / Empowering everyone to build reliable software.",
        ));
        basic.add(main_result(
            "https://github.com/BurntSushi/ripgrep",
            "BurntSushi/ripgrep",
            "recursively search directories",
        ));
        Fixture::capture(NAME, query("rust"), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        let q = query("rust");
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{SEARCH_URL}&q=rust"));
        golden
            .headers
            .insert("Accept".to_string(), ACCEPT_HEADER.to_string());
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"items":[]}"#),
            EngineResults::new(),
        )
        .with_case("request-accept-header")
        .with_golden_request(golden)
        .save(dir.join("request-accept-header.json"))
        .unwrap();
    }

    #[test]
    fn github_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Github::new();
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
    fn joins_language_and_description() {
        let engine = Github::new();
        let res = engine
            .response(&response(200, BASIC_JSON))
            .expect("parse ok");
        assert_eq!(res.results.len(), 2);
        if let Result_::Main(r) = &res.results[0] {
            assert_eq!(
                r.content,
                "Rust / Empowering everyone to build reliable software."
            );
        } else {
            panic!("expected main result");
        }
    }
}
