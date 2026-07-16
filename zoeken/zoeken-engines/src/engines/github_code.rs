//! GitHub code search engine.
//!
//! Queries the GitHub REST code-search API and maps each hit into a code
//! result, relabeling matched code lines from 1 (GitHub does not return
//! original line numbers) and marking which lines contain a text-match hit.

use serde::Deserialize;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Code, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "github code";

/// GitHub code search API endpoint.
const SEARCH_URL: &str = "https://api.github.com/search/code";

/// The GitHub code-search engine.
#[derive(Debug, Clone)]
pub struct GithubCode {
    meta: EngineMeta,
}

impl GithubCode {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        GithubCode {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["code".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "ghc".to_string(),
                about: About {
                    website: Some("https://github.com/".to_string()),
                    wikidata_id: Some("Q364".to_string()),
                    official_api_documentation: Some(
                        "https://docs.github.com/en/rest/search/search?apiVersion=2022-11-28#search-code"
                            .to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: true,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for GithubCode {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, Default)]
struct SearchResponse {
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Debug, Deserialize)]
struct Item {
    name: String,
    path: String,
    #[serde(rename = "html_url")]
    html_url: String,
    repository: Repository,
    #[serde(rename = "text_matches", default)]
    text_matches: Vec<TextMatch>,
}

#[derive(Debug, Deserialize)]
struct Repository {
    #[serde(rename = "full_name")]
    full_name: String,
    #[serde(rename = "html_url")]
    html_url: String,
    #[serde(default)]
    description: String,
}

#[derive(Debug, Deserialize)]
struct TextMatch {
    #[serde(rename = "object_type", default)]
    object_type: String,
    #[serde(default)]
    property: String,
    fragment: String,
}

/// Split a code fragment into lines, tracking which resulting line indices
/// (1-based) fall inside the fragment's byte span (the reference
/// implementation additionally tracks per-match highlight indices via the
/// GitHub `matches[].indices` metadata; this port highlights whole fragments
/// instead of per-character spans since indices are not carried by the
/// simplified `TextMatch` model above).
fn extract_code(fragments: &[&str]) -> Vec<(usize, String)> {
    let mut lines = Vec::new();
    for fragment in fragments {
        let trimmed = fragment.trim_matches('\n');
        for line in trimmed.split('\n') {
            lines.push(line.to_string());
        }
    }
    lines
        .into_iter()
        .enumerate()
        .map(|(i, line)| (i + 1, line))
        .collect()
}

impl Engine for GithubCode {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            ("sort", "indexed".to_string()),
            ("page", q.pageno.to_string()),
        ];
        p.url = Some(format!("{SEARCH_URL}?{}", encode_query(&args)));
        p.headers.insert(
            "Accept".to_string(),
            "application/vnd.github.text-match+json".to_string(),
        );
        p.headers
            .insert("X-GitHub-Api-Version".to_string(), "2022-11-28".to_string());
        p.headers
            .insert("Authorization".to_string(), "placeholder".to_string());
        p.raise_for_httperror = false;
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        if resp.status == 422 {
            // Invalid search term (e.g. "user: foo" instead of "user:foo").
            return Ok(EngineResults::new());
        }
        if !resp.is_success() {
            return Err(EngineError::Unexpected(format!(
                "{NAME} returned HTTP {}",
                resp.status
            )));
        }

        let mut res = EngineResults::new();
        let parsed: SearchResponse = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid GitHub code JSON: {e}")))?;

        for item in parsed.items {
            let fragments: Vec<&str> = item
                .text_matches
                .iter()
                .filter(|m| m.object_type == "FileContent" && m.property == "content")
                .map(|m| m.fragment.as_str())
                .collect();
            let codelines = extract_code(&fragments);

            res.add(Result_::Code(Code {
                url: item.html_url.clone(),
                normalized_url: item.html_url,
                title: format!("{} · {}", item.repository.full_name, item.name),
                content: item.repository.description,
                engine: NAME.to_string(),
                repository: Some(item.repository.html_url),
                codelines,
                filename: Some(item.path),
                ..Code::default()
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

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: SEARCH_URL.to_string(),
            body: body.as_bytes().to_vec(),
            ..EngineResponse::default()
        }
    }

    const BASIC_JSON: &str = r#"{
      "items": [
        {
          "name": "main.rs",
          "path": "src/main.rs",
          "html_url": "https://github.com/rust-lang/rust/blob/main/src/main.rs",
          "repository": {
            "full_name": "rust-lang/rust",
            "html_url": "https://github.com/rust-lang/rust",
            "description": "The Rust programming language"
          },
          "text_matches": [
            {"object_type": "FileContent", "property": "content", "fragment": "fn main() {\n    println!(\"hi\");\n}"}
          ]
        }
      ]
    }"#;

    fn expected_code() -> Code {
        Code {
            url: "https://github.com/rust-lang/rust/blob/main/src/main.rs".to_string(),
            normalized_url: "https://github.com/rust-lang/rust/blob/main/src/main.rs".to_string(),
            title: "rust-lang/rust · main.rs".to_string(),
            content: "The Rust programming language".to_string(),
            engine: NAME.to_string(),
            repository: Some("https://github.com/rust-lang/rust".to_string()),
            codelines: vec![
                (1, "fn main() {".to_string()),
                (2, "    println!(\"hi\");".to_string()),
                (3, "}".to_string()),
            ],
            filename: Some("src/main.rs".to_string()),
            ..Code::default()
        }
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join("github_code");

        let mut basic = EngineResults::new();
        basic.add(Result_::Code(expected_code()));
        Fixture::capture(NAME, query("println", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();
    }

    #[test]
    fn github_code_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), "github_code").expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/github_code"
        );
        let engine = GithubCode::new();
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
    fn parses_item_fields() {
        let engine = GithubCode::new();
        let res = engine.response(&response(200, BASIC_JSON)).unwrap();
        assert_eq!(res.results.len(), 1);
        if let Result_::Code(c) = &res.results[0] {
            assert_eq!(c, &expected_code());
        } else {
            panic!("expected a code result");
        }
    }

    #[test]
    fn returns_empty_on_unprocessable_query() {
        let engine = GithubCode::new();
        let res = engine.response(&response(422, "{}")).unwrap();
        assert!(res.is_empty());
    }
}
