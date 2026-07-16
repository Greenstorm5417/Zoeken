//! IMDB engine: queries the (undocumented) suggestion API and returns title/name/company hits.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_component;

/// Engine name / identifier.
pub const NAME: &str = "imdb";

const BASE_URL: &str = "https://v2.sg.media-imdb.com/suggestion/";

/// The IMDB engine.
#[derive(Debug, Clone)]
pub struct Imdb {
    meta: EngineMeta,
}

impl Imdb {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Imdb {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["movies".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "imdb".to_string(),
                about: About {
                    website: Some("https://imdb.com/".to_string()),
                    wikidata_id: Some("Q37312".to_string()),
                    official_api_documentation: None,
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for Imdb {
    fn default() -> Self {
        Self::new()
    }
}

fn category_for(tag: &str) -> Option<&'static str> {
    match tag {
        "nm" => Some("name"),
        "tt" => Some("title"),
        "kw" => Some("keyword"),
        "co" => Some("company"),
        "ep" => Some("episode"),
        _ => None,
    }
}

impl Engine for Imdb {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let normalized = q.query.replace(' ', "_").to_lowercase();
        let letter = normalized.chars().next().unwrap_or('a');
        p.url = Some(format!(
            "{BASE_URL}{letter}/{}.json",
            encode_component(&normalized)
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid IMDB suggestion JSON: {e}")))?;

        let entries = value
            .get("d")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for entry in &entries {
            let Some(entry_id) = entry.get("id").and_then(|v| v.as_str()) else {
                continue;
            };
            let Some(category) = category_for(&entry_id[..entry_id.len().min(2)]) else {
                continue;
            };
            let Some(title) = entry.get("l").and_then(|v| v.as_str()) else {
                continue;
            };

            let mut full_title = title.to_string();
            if let Some(q) = entry.get("q").and_then(|v| v.as_str()) {
                full_title.push_str(" (");
                full_title.push_str(q);
                full_title.push(')');
            }

            let mut content = String::new();
            if let Some(rank) = entry.get("rank").and_then(|v| v.as_i64()) {
                content.push('(');
                content.push_str(&rank.to_string());
                content.push_str(") ");
            }
            if let Some(year) = entry.get("y").and_then(|v| v.as_i64()) {
                content.push_str(&year.to_string());
                content.push_str(" - ");
            }
            if let Some(s) = entry.get("s").and_then(|v| v.as_str()) {
                content.push_str(s);
            }

            let url = format!("https://imdb.com/{category}/{entry_id}");

            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title: full_title,
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

    const BASIC_JSON: &str = r#"{
      "d": [
        {"id": "tt0111161", "l": "The Shawshank Redemption", "y": 1994, "s": "Frank Darabont", "rank": 1},
        {"id": "nm0000209", "l": "Tim Robbins", "q": "actor"},
        {"id": "xx0000000", "l": "unknown tag"}
      ]
    }"#;

    const EMPTY_JSON: &str = r#"{"d": []}"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://imdb.com/title/tt0111161",
            "The Shawshank Redemption",
            "(1) 1994 - Frank Darabont",
        ));
        basic.add(main_result(
            "https://imdb.com/name/nm0000209",
            "Tim Robbins (actor)",
            "",
        ));
        Fixture::capture(NAME, query("shawshank"), response(200, BASIC_JSON), basic)
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

        let q = query("Tom Cruise");
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}t/tom_cruise.json"));
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
    fn imdb_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Imdb::new();
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
    fn builds_request_url_lowercased_with_underscores() {
        let engine = Imdb::new();
        let q = query("Tom Cruise");
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://v2.sg.media-imdb.com/suggestion/t/tom_cruise.json")
        );
    }
}
