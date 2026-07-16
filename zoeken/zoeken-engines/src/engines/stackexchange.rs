//! Stack Exchange search engine.
//!
//! Ports the advanced search API for the stock Stack Exchange instances.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::{encode_query, html_unescape};

const SEARCH_API: &str = "https://api.stackexchange.com/2.3/search/advanced";

const PAGESIZE: u32 = 10;

#[derive(Debug, Clone)]
pub struct Stackexchange {
    meta: EngineMeta,
    api_site: String,
}

impl Stackexchange {
    pub fn new(name: &str, shortcut: &str, api_site: &str) -> Self {
        Stackexchange {
            meta: EngineMeta {
                name: name.to_string(),
                engine_type: Processor::Online,
                categories: vec!["it".to_string(), "q&a".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: shortcut.to_string(),
                about: About {
                    website: Some("https://stackexchange.com".to_string()),
                    wikidata_id: Some("Q3495447".to_string()),
                    official_api_documentation: Some(
                        "https://api.stackexchange.com/docs".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
            api_site: api_site.to_string(),
        }
    }

    pub fn stackoverflow() -> Self {
        Self::new("stackoverflow", "st", "stackoverflow")
    }

    pub fn askubuntu() -> Self {
        Self::new("askubuntu", "ubuntu", "askubuntu")
    }

    pub fn superuser() -> Self {
        Self::new("superuser", "su", "superuser")
    }
}

impl Default for Stackexchange {
    fn default() -> Self {
        Self::stackoverflow()
    }
}

impl Engine for Stackexchange {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            ("page", p.pageno.to_string()),
            ("pagesize", PAGESIZE.to_string()),
            ("site", self.api_site.clone()),
            ("sort", "activity".to_string()),
            ("order", "desc".to_string()),
        ];
        p.url = Some(format!("{SEARCH_API}?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Stack Exchange JSON: {e}")))?;

        let items = value
            .get("items")
            .and_then(|i| i.as_array())
            .cloned()
            .unwrap_or_default();

        for item in &items {
            let question_id = item
                .get("question_id")
                .map(|id| match id {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default();

            let tags: Vec<String> = item
                .get("tags")
                .and_then(|t| t.as_array())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|t| t.as_str().map(str::to_string))
                        .collect()
                })
                .unwrap_or_default();

            let owner = item
                .get("owner")
                .and_then(|o| o.get("display_name"))
                .and_then(|d| d.as_str())
                .unwrap_or("");

            let is_answered = item
                .get("is_answered")
                .and_then(|a| a.as_bool())
                .unwrap_or(false);

            let score = item
                .get("score")
                .map(|s| match s {
                    serde_json::Value::Number(n) => n.to_string(),
                    serde_json::Value::String(s) => s.clone(),
                    _ => String::new(),
                })
                .unwrap_or_default();

            let mut content = format!("[{}]", tags.join(", "));
            content.push_str(&format!(" {owner}"));
            if is_answered {
                content.push_str(" // is answered");
            }
            content.push_str(&format!(" // score: {score}"));

            let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
            let url = format!("https://{}.com/q/{}", self.api_site, question_id);

            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title: html_unescape(title),
                content: html_unescape(&content),
                engine: self.meta.name.clone(),
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

    fn main_result(url: &str, title: &str, content: &str, engine: &str) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            engine: engine.to_string(),
            ..MainResult::default()
        })
    }

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: SEARCH_API.to_string(),
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
          "tags": ["python", "rust"],
          "owner": {"display_name": "Ferris & Guido"},
          "is_answered": true,
          "score": 42,
          "question_id": 123,
          "title": "How to call Rust from Python &amp; back?"
        },
        {
          "tags": ["c"],
          "owner": {"display_name": "Dennis"},
          "is_answered": false,
          "score": 7,
          "question_id": 456,
          "title": "Pointers explained"
        }
      ]
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join("stackexchange");

        // stackoverflow instance, basic parse (note html.unescape of `&amp;`).
        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://stackoverflow.com/q/123",
            "How to call Rust from Python & back?",
            "[python, rust] Ferris & Guido // is answered // score: 42",
            "stackoverflow",
        ));
        basic.add(main_result(
            "https://stackoverflow.com/q/456",
            "Pointers explained",
            "[c] Dennis // score: 7",
            "stackoverflow",
        ));
        Fixture::capture(
            "stackoverflow",
            query("rust python", 1),
            response(200, BASIC_JSON),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        // askubuntu instance: validates per-instance url/site and request build.
        let q = query("install package", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{SEARCH_API}?q=install+package&page=2&pagesize=10&site=askubuntu&sort=activity&order=desc"
        ));
        let mut ubuntu = EngineResults::new();
        ubuntu.add(main_result(
            "https://askubuntu.com/q/99",
            "How do I install a .deb?",
            "[apt] Alice // score: 3",
            "askubuntu",
        ));
        Fixture::capture(
            "askubuntu",
            q.clone(),
            response(
                200,
                r#"{"items":[{"tags":["apt"],"owner":{"display_name":"Alice"},"is_answered":false,"score":3,"question_id":99,"title":"How do I install a .deb?"}]}"#,
            ),
            ubuntu,
        )
        .with_case("askubuntu-request")
        .with_golden_request(golden)
        .save(dir.join("askubuntu-request.json"))
        .unwrap();
    }

    #[test]
    fn stackexchange_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), "stackexchange").expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/stackexchange"
        );
        for fixture in &fixtures {
            let engine = match fixture.engine.as_str() {
                "askubuntu" => Stackexchange::askubuntu(),
                "superuser" => Stackexchange::superuser(),
                _ => Stackexchange::stackoverflow(),
            };
            if let Err(mismatches) = run_all(&engine, std::slice::from_ref(fixture)) {
                let report = mismatches
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                panic!("conformance failures:\n{report}");
            }
        }
    }

    #[test]
    fn builds_per_instance_result_urls() {
        let engine = Stackexchange::superuser();
        let res = engine
            .response(&response(
                200,
                r#"{"items":[{"tags":[],"owner":{"display_name":"x"},"is_answered":false,"score":0,"question_id":5,"title":"t"}]}"#,
            ))
            .unwrap();
        if let Result_::Main(r) = &res.results[0] {
            assert_eq!(r.url, "https://superuser.com/q/5");
        } else {
            panic!("expected main result");
        }
    }
}
