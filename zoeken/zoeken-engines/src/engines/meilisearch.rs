//! Meilisearch engine (upstream `searx/engines/meilisearch.py`).
//!
//! Settings-driven JSON-over-HTTP engine: `index` is required, `base_url`
//! defaults to `http://localhost:7700`, and an optional `auth_key` is sent as
//! the `Authorization` header verbatim (upstream expects the admin to supply
//! the full `Bearer ...` value).

use serde::Deserialize;
use serde_json::{Value, json};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{KeyValue, Result_};

/// Engine name / identifier (upstream module name).
pub const NAME: &str = "meilisearch";

/// Settings accepted from `settings.yml` (`EngineSettings.extra`).
#[derive(Debug, Clone, Deserialize)]
pub struct MeilisearchConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    pub index: String,
    #[serde(default)]
    pub auth_key: String,
    #[serde(default)]
    pub facet_filters: Vec<Value>,
}

fn default_base_url() -> String {
    "http://localhost:7700".to_string()
}

/// The Meilisearch engine.
#[derive(Debug, Clone)]
pub struct Meilisearch {
    meta: EngineMeta,
    config: MeilisearchConfig,
    search_url: String,
}

impl Meilisearch {
    /// Build the engine from validated settings. Mirrors upstream `init()`:
    /// `index` must be non-empty.
    pub fn new(config: MeilisearchConfig) -> Result<Self, String> {
        if config.index.trim().is_empty() {
            return Err("index cannot be empty".to_string());
        }
        let search_url = format!("{}/indexes/{}/search", config.base_url, config.index);
        Ok(Meilisearch {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "mes".to_string(),
                about: About {
                    website: Some("https://www.meilisearch.com".to_string()),
                    wikidata_id: None,
                    official_api_documentation: Some("https://docs.meilisearch.com/".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
            config,
            search_url,
        })
    }
}

impl Engine for Meilisearch {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        if !self.config.auth_key.is_empty() {
            p.auth = Some(self.config.auth_key.clone());
        }
        p.method = HttpMethod::Post;
        p.url = Some(self.search_url.clone());

        let mut data = json!({
            "q": q.query,
            "offset": 10 * (i64::from(q.pageno.max(1)) - 1),
            "limit": 10,
        });
        if !self.config.facet_filters.is_empty() {
            if let Value::Object(ref mut map) = data {
                map.insert(
                    "facetFilters".to_string(),
                    Value::Array(self.config.facet_filters.clone()),
                );
            }
        }
        p.json = Some(data);
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let body: Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("meilisearch: invalid JSON: {e}")))?;

        let hits = body
            .get("hits")
            .and_then(|h| h.as_array())
            .cloned()
            .unwrap_or_default();

        for hit in hits {
            let Some(object) = hit.as_object() else {
                continue;
            };
            let kvmap: Vec<(String, String)> = object
                .iter()
                .map(|(key, value)| (key.clone(), value_to_string(value)))
                .collect();
            res.add(Result_::KeyValue(KeyValue {
                kvmap,
                engine: NAME.to_string(),
                ..KeyValue::default()
            }));
        }

        Ok(res)
    }
}

fn value_to_string(value: &Value) -> String {
    match value {
        Value::String(s) => s.clone(),
        other => other.to_string(),
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

    fn config() -> MeilisearchConfig {
        MeilisearchConfig {
            base_url: "http://localhost:7700".to_string(),
            index: "my-index".to_string(),
            auth_key: String::new(),
            facet_filters: Vec::new(),
        }
    }

    #[test]
    fn rejects_empty_index() {
        let mut cfg = config();
        cfg.index = String::new();
        assert!(Meilisearch::new(cfg).is_err());
    }

    #[test]
    fn builds_search_request() {
        let engine = Meilisearch::new(config()).unwrap();
        let q = SearchQueryView {
            query: "berlin".to_string(),
            pageno: 2,
            ..SearchQueryView::default()
        };
        let mut params = RequestParams::default();
        engine.request(&q, &mut params);
        assert_eq!(
            params.url.as_deref(),
            Some("http://localhost:7700/indexes/my-index/search")
        );
        assert_eq!(params.method, HttpMethod::Post);
        let body = params.json.unwrap();
        assert_eq!(body["q"], "berlin");
        assert_eq!(body["offset"], 10);
        assert_eq!(body["limit"], 10);
    }

    #[test]
    fn parses_hits_into_keyvalue_results() {
        let engine = Meilisearch::new(config()).unwrap();
        let body = json!({"hits": [{"city": "berlin", "country": "de"}]});
        let resp = EngineResponse {
            status: 200,
            body: body.to_string().into_bytes(),
            ..EngineResponse::default()
        };
        let results = engine.response(&resp).unwrap();
        assert_eq!(results.results.len(), 1);
        match &results.results[0] {
            Result_::KeyValue(kv) => {
                assert!(
                    kv.kvmap
                        .contains(&("city".to_string(), "berlin".to_string()))
                );
            }
            other => panic!("expected KeyValue, got {other:?}"),
        }
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);
        let engine = Meilisearch::new(config()).unwrap();
        let q = SearchQueryView {
            query: "berlin".to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        };
        let body = json!({"hits": [{"city": "berlin"}]});
        let resp = EngineResponse {
            status: 200,
            url: "http://localhost:7700/indexes/my-index/search".to_string(),
            body: body.to_string().into_bytes(),
            ..EngineResponse::default()
        };
        let mut golden = RequestParams {
            query: q.query.clone(),
            pageno: q.pageno,
            safesearch: q.safesearch,
            time_range: q.time_range,
            locale_key: q.locale.clone(),
            ..RequestParams::default()
        };
        engine.request(&q, &mut golden);
        let mut expected = EngineResults::new();
        expected.add(Result_::KeyValue(KeyValue {
            kvmap: vec![("city".to_string(), "berlin".to_string())],
            engine: NAME.to_string(),
            ..KeyValue::default()
        }));
        Fixture::capture(NAME, q, resp, expected)
            .with_case("basic")
            .with_golden_request(golden)
            .save(dir.join("basic.json"))
            .unwrap();
    }

    #[test]
    fn meilisearch_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Meilisearch::new(config()).unwrap();
        if let Err(mismatches) = run_all(&engine, &fixtures) {
            let report = mismatches
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            panic!("conformance failures:\n{report}");
        }
    }
}
