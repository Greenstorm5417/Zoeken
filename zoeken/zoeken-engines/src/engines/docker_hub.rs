//! Docker Hub search engine.
//!
//! Queries the catalog API and maps package entries into main results.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "docker hub";

/// Base URL of Docker Hub (result URLs are resolved against it).
const BASE_URL: &str = "https://hub.docker.com";

/// Results requested per page (the reference `page_size`).
const PAGE_SIZE: u32 = 10;

/// The Docker Hub engine.
#[derive(Debug, Clone)]
pub struct DockerHub {
    meta: EngineMeta,
}

impl DockerHub {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        DockerHub {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["it".to_string(), "packages".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "dh".to_string(),
                about: About {
                    website: Some("https://hub.docker.com".to_string()),
                    wikidata_id: Some("Q100769064".to_string()),
                    official_api_documentation: Some(
                        "https://docs.docker.com/registry/spec/api/".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for DockerHub {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for DockerHub {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let from = PAGE_SIZE * p.pageno.saturating_sub(1);
        let args: Vec<(&str, String)> = vec![
            ("query", q.query.clone()),
            ("from", from.to_string()),
            ("size", PAGE_SIZE.to_string()),
        ];
        p.url = Some(format!(
            "{BASE_URL}/api/search/v3/catalog/search?{}",
            encode_query(&args)
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Docker Hub JSON: {e}")))?;

        let items = value
            .get("results")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        for item in &items {
            let source = item.get("source").and_then(|s| s.as_str()).unwrap_or("");
            let is_official = source == "store" || source == "official";
            let slug = item.get("slug").and_then(|s| s.as_str()).unwrap_or("");
            let prefix = if is_official { "/_/" } else { "/r/" };
            let url = format!("{BASE_URL}{prefix}{slug}");

            let title = item
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let content = item
                .get("short_description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();

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
      "results": [
        {
          "source": "official",
          "slug": "nginx",
          "name": "nginx",
          "short_description": "Official build of Nginx."
        },
        {
          "source": "community",
          "slug": "someuser/myapp",
          "name": "myapp",
          "short_description": "A community image."
        }
      ]
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join("docker_hub");

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://hub.docker.com/_/nginx",
            "nginx",
            "Official build of Nginx.",
        ));
        basic.add(main_result(
            "https://hub.docker.com/r/someuser/myapp",
            "myapp",
            "A community image.",
        ));
        Fixture::capture(NAME, query("nginx", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        // request-page2: validates the built API URL and `from` offset.
        let q = query("web server", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{BASE_URL}/api/search/v3/catalog/search?query=web+server&from=10&size=10"
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
    fn docker_hub_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), "docker_hub").expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/docker_hub"
        );
        let engine = DockerHub::new();
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
    fn official_and_community_url_prefixes() {
        let engine = DockerHub::new();
        let res = engine.response(&response(200, BASIC_JSON)).unwrap();
        if let (Result_::Main(a), Result_::Main(b)) = (&res.results[0], &res.results[1]) {
            assert_eq!(a.url, "https://hub.docker.com/_/nginx");
            assert_eq!(b.url, "https://hub.docker.com/r/someuser/myapp");
        } else {
            panic!("expected two main results");
        }
    }
}
