//! Vimeo engine: scrapes the embedded `var data = ...;` JSON blob from the
//! Vimeo search results page (no official public search API is used).

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::{encode_query, extr};

/// Engine name / identifier.
pub const NAME: &str = "vimeo";

const BASE_URL: &str = "https://vimeo.com/";

#[derive(Debug, Clone)]
pub struct Vimeo {
    meta: EngineMeta,
}

impl Vimeo {
    pub fn new() -> Self {
        Vimeo {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["videos".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "vm".to_string(),
                about: About {
                    website: Some("https://vimeo.com/".to_string()),
                    wikidata_id: Some("Q156376".to_string()),
                    official_api_documentation: Some("http://developer.vimeo.com/api".to_string()),
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for Vimeo {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for Vimeo {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![("q", q.query.clone())];
        p.url = Some(format!(
            "{BASE_URL}search/page:{}?{}",
            p.pageno,
            encode_query(&args)
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let text = resp.text();
        let blob = extr(&text, "var data = ", ";\n");
        if blob.is_empty() {
            return Ok(res);
        }

        let value: serde_json::Value = serde_json::from_str(blob)
            .map_err(|e| EngineError::Parse(format!("invalid Vimeo embedded JSON: {e}")))?;

        let Some(entries) = value.pointer("/filtered/data").and_then(|d| d.as_array()) else {
            return Ok(res);
        };

        for entry in entries {
            let Some(type_) = entry.get("type").and_then(|t| t.as_str()) else {
                continue;
            };
            let Some(result) = entry.get(type_) else {
                continue;
            };

            let uri = result.get("uri").and_then(|u| u.as_str()).unwrap_or("");
            let video_id = uri.rsplit('/').next().unwrap_or("");
            if video_id.is_empty() {
                continue;
            }
            let url = format!("{BASE_URL}{video_id}");
            let title = result
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();

            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title,
                content: String::new(),
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

    fn main_result(url: &str, title: &str) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            content: String::new(),
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

    fn page_html(data_json: &str) -> String {
        format!("<html><body><script>var data = {data_json};\n</script></body></html>")
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let data_json = r#"{"filtered":{"data":[
          {"type":"clip","clip":{"uri":"/videos/123456","name":"Rust in 100 seconds","created_time":"2021-01-02T03:04:05+00:00","pictures":{"sizes":[{"link":"https://i.vimeocdn.com/x.jpg"}]}}}
        ]}}"#;

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://vimeo.com/123456",
            "Rust in 100 seconds",
        ));
        Fixture::capture(
            NAME,
            query("rust", 1),
            response(200, &page_html(data_json)),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        Fixture::capture(
            NAME,
            query("rust", 1),
            response(200, "<html><body>no data here</body></html>"),
            EngineResults::new(),
        )
        .with_case("no-data-blob")
        .save(dir.join("no-data-blob.json"))
        .unwrap();

        let q = query("dance", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}search/page:2?q=dance"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, &page_html(r#"{"filtered":{"data":[]}}"#)),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn vimeo_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Vimeo::new();
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
    fn missing_data_blob_yields_no_results() {
        let engine = Vimeo::new();
        let res = engine
            .response(&response(200, "<html><body>nothing</body></html>"))
            .unwrap();
        assert!(res.is_empty());
    }
}
