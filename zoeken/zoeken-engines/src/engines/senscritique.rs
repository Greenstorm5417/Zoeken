//! SensCritique engine: POSTs a GraphQL search query to the Apollo endpoint and returns
//! movie/book/game/etc. entries.

use serde_json::json;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

/// Engine name / identifier.
pub const NAME: &str = "senscritique";

const GRAPHQL_URL: &str = "https://apollo.senscritique.com/";
const PAGE_SIZE: u32 = 16;

const GRAPHQL_QUERY: &str = "query SearchProductExplorer($query: String, $offset: Int, $limit: Int,\n                    $sortBy: SearchProductExplorerSort) {\n  searchProductExplorer(\n    query: $query\n    filters: []\n    sortBy: $sortBy\n    offset: $offset\n    limit: $limit\n  ) {\n    items {\n      category\n      dateRelease\n      duration\n      id\n      originalTitle\n      rating\n      title\n      url\n      yearOfProduction\n      medias {\n        picture\n      }\n      countries {\n        name\n      }\n      genresInfos {\n        label\n      }\n      directors {\n        name\n      }\n      stats {\n        ratingCount\n      }\n    }\n  }\n}";

/// The SensCritique engine.
#[derive(Debug, Clone)]
pub struct SensCritique {
    meta: EngineMeta,
}

impl SensCritique {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        SensCritique {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["movies".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "sc".to_string(),
                about: About {
                    website: Some("https://www.senscritique.com/".to_string()),
                    wikidata_id: Some("Q16676060".to_string()),
                    official_api_documentation: None,
                    use_official_api: false,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for SensCritique {
    fn default() -> Self {
        Self::new()
    }
}

fn text_of<'a>(item: &'a serde_json::Value, key: &str) -> Option<&'a str> {
    item.get(key).and_then(|v| v.as_str())
}

fn build_content(item: &serde_json::Value, title: &str, original_title: Option<&str>) -> String {
    let mut parts: Vec<String> = Vec::new();

    if let Some(category) = text_of(item, "category") {
        parts.push(category.to_string());
    }
    if let Some(original) = original_title {
        if original != title {
            parts.push(format!("Original title: {original}"));
        }
    }
    if let Some(directors) = item.get("directors").and_then(|v| v.as_array()) {
        let names: Vec<&str> = directors
            .iter()
            .filter_map(|d| d.get("name").and_then(|v| v.as_str()))
            .collect();
        if !names.is_empty() {
            parts.push(format!("Director(s): {}", names.join(", ")));
        }
    }
    if let Some(countries) = item.get("countries").and_then(|v| v.as_array()) {
        let names: Vec<&str> = countries
            .iter()
            .filter_map(|c| c.get("name").and_then(|v| v.as_str()))
            .collect();
        if !names.is_empty() {
            parts.push(format!("Country: {}", names.join(", ")));
        }
    }
    if let Some(genres) = item.get("genresInfos").and_then(|v| v.as_array()) {
        let labels: Vec<&str> = genres
            .iter()
            .filter_map(|g| g.get("label").and_then(|v| v.as_str()))
            .collect();
        if !labels.is_empty() {
            parts.push(format!("Genre(s): {}", labels.join(", ")));
        }
    }
    if let Some(duration) = item.get("duration").and_then(|v| v.as_i64()) {
        let minutes = duration / 60;
        if minutes > 0 {
            parts.push(format!("Duration: {minutes} min"));
        }
    }
    if let (Some(rating), Some(count)) = (
        item.get("rating").and_then(|v| v.as_f64()),
        item.get("stats")
            .and_then(|s| s.get("ratingCount"))
            .and_then(|v| v.as_i64()),
    ) {
        parts.push(format!("Rating: {rating}/10 ({count} votes)"));
    }

    parts.join(" | ")
}

impl Engine for SensCritique {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        let offset = (q.pageno.max(1) - 1) * PAGE_SIZE;

        p.method = HttpMethod::Post;
        p.url = Some(GRAPHQL_URL.to_string());
        p.headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        p.json = Some(json!({
            "operationName": "SearchProductExplorer",
            "variables": {
                "offset": offset,
                "limit": PAGE_SIZE,
                "query": q.query,
                "sortBy": "RELEVANCE",
            },
            "query": GRAPHQL_QUERY,
        }));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid SensCritique JSON: {e}")))?;

        let items = value
            .get("data")
            .and_then(|d| d.get("searchProductExplorer"))
            .and_then(|s| s.get("items"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for item in &items {
            let Some(title) = text_of(item, "title").filter(|t| !t.is_empty()) else {
                continue;
            };
            let Some(path) = text_of(item, "url") else {
                continue;
            };

            let year = item.get("yearOfProduction").and_then(|v| v.as_i64());
            let full_title = match year {
                Some(y) => format!("{title} ({y})"),
                None => title.to_string(),
            };

            let original_title = text_of(item, "originalTitle");
            let content = build_content(item, title, original_title);
            let url = format!("https://www.senscritique.com{path}");

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
            url: GRAPHQL_URL.to_string(),
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
      "data": {
        "searchProductExplorer": {
          "items": [
            {
              "category": "Film",
              "duration": 7200,
              "id": 1,
              "originalTitle": "Blade Runner",
              "rating": 8.1,
              "title": "Blade Runner",
              "url": "/film/blade_runner/1",
              "yearOfProduction": 1982,
              "medias": {"picture": "https://example.com/br.jpg"},
              "countries": [{"name": "USA"}],
              "genresInfos": [{"label": "Science-fiction"}],
              "directors": [{"name": "Ridley Scott"}],
              "stats": {"ratingCount": 1000}
            }
          ]
        }
      }
    }"#;

    const EMPTY_JSON: &str = r#"{"data": {"searchProductExplorer": {"items": []}}}"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://www.senscritique.com/film/blade_runner/1",
            "Blade Runner (1982)",
            "Film | Director(s): Ridley Scott | Country: USA | Genre(s): Science-fiction | Duration: 120 min | Rating: 8.1/10 (1000 votes)",
        ));
        Fixture::capture(
            NAME,
            query("blade runner", 1),
            response(200, BASIC_JSON),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        Fixture::capture(
            NAME,
            query("nothing", 1),
            response(200, EMPTY_JSON),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();

        let q = query("blade runner", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Post;
        golden.url = Some(GRAPHQL_URL.to_string());
        golden
            .headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        golden.json = Some(json!({
            "operationName": "SearchProductExplorer",
            "variables": {
                "offset": 16,
                "limit": PAGE_SIZE,
                "query": "blade runner",
                "sortBy": "RELEVANCE",
            },
            "query": GRAPHQL_QUERY,
        }));
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
    fn senscritique_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = SensCritique::new();
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
    fn builds_paged_offset() {
        let engine = SensCritique::new();
        let q = query("blade runner", 3);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        let offset = p
            .json
            .as_ref()
            .unwrap()
            .get("variables")
            .unwrap()
            .get("offset")
            .unwrap()
            .as_i64()
            .unwrap();
        assert_eq!(offset, 32);
    }
}
