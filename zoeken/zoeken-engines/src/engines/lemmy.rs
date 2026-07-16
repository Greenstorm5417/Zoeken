//! Lemmy search engine.
//!
//! Searches a Lemmy instance and reduces markdown bodies to plain text.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::{encode_query, markdown_to_text};

pub const NAME: &str = "lemmy";

const DEFAULT_BASE_URL: &str = "https://lemmy.ml/";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LemmyType {
    Communities,
    Users,
    Posts,
    Comments,
}

impl LemmyType {
    fn as_str(&self) -> &'static str {
        match self {
            LemmyType::Communities => "Communities",
            LemmyType::Users => "Users",
            LemmyType::Posts => "Posts",
            LemmyType::Comments => "Comments",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Lemmy {
    meta: EngineMeta,
    base_url: String,
    lemmy_type: LemmyType,
}

impl Lemmy {
    pub fn new() -> Self {
        Lemmy {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["social media".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "lm".to_string(),
                about: About {
                    website: Some("https://lemmy.ml/".to_string()),
                    wikidata_id: Some("Q84777032".to_string()),
                    official_api_documentation: Some("https://join-lemmy.org/api/".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
            base_url: DEFAULT_BASE_URL.to_string(),
            lemmy_type: LemmyType::Communities,
        }
    }

    pub fn with_base_url(mut self, base_url: impl Into<String>) -> Self {
        let mut url = base_url.into();
        if !url.ends_with('/') {
            url.push('/');
        }
        self.base_url = url;
        self
    }

    pub fn with_type(mut self, lemmy_type: LemmyType) -> Self {
        self.lemmy_type = lemmy_type;
        self
    }

    fn get_communities(&self, json: &serde_json::Value) -> Vec<Result_> {
        let mut results = Vec::new();
        let items = json.get("communities").and_then(|c| c.as_array());
        for result in items.into_iter().flatten() {
            let community = result.get("community").cloned().unwrap_or_default();
            let url = community
                .get("actor_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = community
                .get("title")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let content = markdown_to_text(
                community
                    .get("description")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            );
            results.push(main(url, title, content));
        }
        results
    }

    fn get_users(&self, json: &serde_json::Value) -> Vec<Result_> {
        let mut results = Vec::new();
        let items = json.get("users").and_then(|c| c.as_array());
        for result in items.into_iter().flatten() {
            let person = result.get("person").cloned().unwrap_or_default();
            let url = person
                .get("actor_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = person
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let content =
                markdown_to_text(person.get("bio").and_then(|v| v.as_str()).unwrap_or(""));
            results.push(main(url, title, content));
        }
        results
    }

    fn get_posts(&self, json: &serde_json::Value) -> Vec<Result_> {
        let mut results = Vec::new();
        let items = json.get("posts").and_then(|c| c.as_array());
        for result in items.into_iter().flatten() {
            let post = result.get("post").cloned().unwrap_or_default();
            let url = post
                .get("ap_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = post
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let body = post.get("body").and_then(|v| v.as_str()).unwrap_or("");
            let content = if body.trim().is_empty() {
                String::new()
            } else {
                markdown_to_text(body)
            };
            results.push(main(url, title, content));
        }
        results
    }

    fn get_comments(&self, json: &serde_json::Value) -> Vec<Result_> {
        let mut results = Vec::new();
        let items = json.get("comments").and_then(|c| c.as_array());
        for result in items.into_iter().flatten() {
            let comment = result.get("comment").cloned().unwrap_or_default();
            let post = result.get("post").cloned().unwrap_or_default();
            let url = comment
                .get("ap_id")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let title = post
                .get("name")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let content = markdown_to_text(
                comment
                    .get("content")
                    .and_then(|v| v.as_str())
                    .unwrap_or(""),
            );
            results.push(main(url, title, content));
        }
        results
    }
}

impl Default for Lemmy {
    fn default() -> Self {
        Self::new()
    }
}

fn main(url: String, title: String, content: String) -> Result_ {
    Result_::Main(MainResult {
        url: url.clone(),
        normalized_url: url,
        title,
        content,
        engine: NAME.to_string(),
        ..MainResult::default()
    })
}

impl Engine for Lemmy {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            ("page", p.pageno.to_string()),
            ("type_", self.lemmy_type.as_str().to_string()),
        ];
        p.url = Some(format!(
            "{}api/v3/search?{}",
            self.base_url,
            encode_query(&args)
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let json: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Lemmy JSON: {e}")))?;

        let results = match self.lemmy_type {
            LemmyType::Communities => self.get_communities(&json),
            LemmyType::Users => self.get_users(&json),
            LemmyType::Posts => self.get_posts(&json),
            LemmyType::Comments => self.get_comments(&json),
        };

        for r in results {
            res.add(r);
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
        main(url.to_string(), title.to_string(), content.to_string())
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

    const COMMUNITIES_JSON: &str = r#"{
      "communities": [
        {
          "community": {
            "actor_id": "https://lemmy.ml/c/rust",
            "title": "Rust Programming",
            "description": "A community about the [Rust](https://rust-lang.org) language."
          },
          "counts": {"subscribers": 100, "posts": 50, "users_active_half_year": 20, "published": "2023-01-01T12:00:00Z"}
        }
      ]
    }"#;

    const POSTS_JSON: &str = r#"{
      "posts": [
        {
          "post": {
            "ap_id": "https://lemmy.ml/post/1",
            "name": "Announcing something",
            "body": "We **shipped** it."
          },
          "creator": {"name": "alice"},
          "community": {"title": "Rust"},
          "counts": {"upvotes": 10, "downvotes": 1, "comments": 3}
        }
      ]
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        // Communities (default type).
        let dir = fixtures_root().join(NAME);
        let mut communities = EngineResults::new();
        communities.add(main_result(
            "https://lemmy.ml/c/rust",
            "Rust Programming",
            "A community about the Rust language.",
        ));
        Fixture::capture(
            NAME,
            query("rust", 1),
            response(200, COMMUNITIES_JSON),
            communities,
        )
        .with_case("communities")
        .save(dir.join("communities.json"))
        .unwrap();

        // request: validates the built search URL and parameter order.
        let q = query("rust", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{DEFAULT_BASE_URL}api/v3/search?q=rust&page=2&type_=Communities"
        ));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"communities":[]}"#),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn lemmy_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Lemmy::new();
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
    fn parses_posts_type() {
        let engine = Lemmy::new().with_type(LemmyType::Posts);
        let res = engine
            .response(&response(200, POSTS_JSON))
            .expect("parse ok");
        assert_eq!(res.results.len(), 1);
        if let Result_::Main(r) = &res.results[0] {
            assert_eq!(r.url, "https://lemmy.ml/post/1");
            assert_eq!(r.title, "Announcing something");
            assert_eq!(r.content, "We shipped it.");
        } else {
            panic!("expected main result");
        }
    }

    #[test]
    fn builds_request_for_posts_type() {
        let engine = Lemmy::new().with_type(LemmyType::Posts);
        let q = query("rust", 1);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://lemmy.ml/api/v3/search?q=rust&page=1&type_=Posts")
        );
    }
}
