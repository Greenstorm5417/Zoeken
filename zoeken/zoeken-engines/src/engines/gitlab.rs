//! GitLab project search engine.
//!
//! Queries a GitLab host's projects API and keeps the visible fields.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "gitlab";

/// Default GitLab host base URL (the reference's canonical example instance).
const DEFAULT_BASE_URL: &str = "https://gitlab.com";

/// Default project search API path.
const DEFAULT_API_PATH: &str = "api/v4/projects";

/// The GitLab engine, parameterized by host base URL and API path.
#[derive(Debug, Clone)]
pub struct Gitlab {
    meta: EngineMeta,
    base_url: String,
    api_path: String,
}

impl Gitlab {
    /// Create the engine targeting `https://gitlab.com` with the default API
    /// path.
    pub fn new() -> Self {
        Gitlab {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["it".to_string(), "repos".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "gl".to_string(),
                about: About {
                    website: Some("https://gitlab.com/".to_string()),
                    wikidata_id: Some("Q16639197".to_string()),
                    official_api_documentation: Some("https://docs.gitlab.com/ee/api/".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
            base_url: DEFAULT_BASE_URL.to_string(),
            api_path: DEFAULT_API_PATH.to_string(),
        }
    }

    /// Override the GitLab host base URL (trailing slashes are trimmed).
    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        self.base_url = base_url.into().trim_end_matches('/').to_string();
        self
    }

    /// Override the project search API path.
    pub fn with_api_path(mut self, api_path: impl Into<String>) -> Self {
        self.api_path = api_path.into();
        self
    }
}

impl Default for Gitlab {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for Gitlab {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> =
            vec![("search", q.query.clone()), ("page", p.pageno.to_string())];
        p.url = Some(format!(
            "{}/{}?{}",
            self.base_url,
            self.api_path,
            encode_query(&args)
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid GitLab JSON: {e}")))?;

        let items = value.as_array().cloned().unwrap_or_default();

        for item in &items {
            let url = item
                .get("web_url")
                .and_then(|u| u.as_str())
                .unwrap_or("")
                .to_string();
            let title = item
                .get("name")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let content = item
                .get("description")
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
            url: "https://gitlab.com/".to_string(),
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
        "web_url": "https://gitlab.com/gitlab-org/gitlab",
        "name": "GitLab",
        "description": "GitLab is an open source end-to-end software development platform"
      },
      {
        "web_url": "https://gitlab.com/inkscape/inkscape",
        "name": "Inkscape",
        "description": null
      }
    ]"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://gitlab.com/gitlab-org/gitlab",
            "GitLab",
            "GitLab is an open source end-to-end software development platform",
        ));
        // Null description -> empty content.
        basic.add(main_result(
            "https://gitlab.com/inkscape/inkscape",
            "Inkscape",
            "",
        ));
        Fixture::capture(NAME, query("gitlab", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        // request-page2: validates the built API URL and parameter order.
        let q = query("gitlab", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{DEFAULT_BASE_URL}/{DEFAULT_API_PATH}?search=gitlab&page=2"
        ));
        Fixture::capture(NAME, q.clone(), response(200, "[]"), EngineResults::new())
            .with_case("request-page2")
            .with_golden_request(golden)
            .save(dir.join("request-page2.json"))
            .unwrap();
    }

    #[test]
    fn gitlab_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Gitlab::new();
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
    fn honors_custom_base_url() {
        let engine = Gitlab::new().with_base_url("https://gitlab.gnome.org/");
        let q = query("nautilus", 1);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://gitlab.gnome.org/api/v4/projects?search=nautilus&page=1")
        );
    }
}
