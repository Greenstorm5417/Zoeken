//! Elasticsearch engine (upstream `searx/engines/elasticsearch.py`).
//!
//! Settings-driven JSON-over-HTTP engine: `base_url` and `index` are required,
//! `query_type` selects how the user's `key:value` query is translated into an
//! Elasticsearch query DSL body (`match`, `simple_query_string`, `term`,
//! `terms`, or `custom`).

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD as BASE64;
use serde::Deserialize;
use serde_json::{Map, Value, json};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{KeyValue, Result_};

/// Engine name / identifier (upstream module name).
pub const NAME: &str = "elasticsearch";

const QUERY_TYPES: &[&str] = &["match", "simple_query_string", "term", "terms", "custom"];

/// Settings accepted from `settings.yml` (`EngineSettings.extra`).
#[derive(Debug, Clone, Deserialize)]
pub struct ElasticsearchConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub password: String,
    pub index: String,
    #[serde(default = "default_query_type")]
    pub query_type: String,
    #[serde(default)]
    pub custom_query_json: Map<String, Value>,
    #[serde(default)]
    pub show_metadata: bool,
    #[serde(default = "default_page_size")]
    pub page_size: i64,
}

fn default_base_url() -> String {
    "http://localhost:9200".to_string()
}
fn default_query_type() -> String {
    "match".to_string()
}
fn default_page_size() -> i64 {
    10
}

/// The Elasticsearch engine.
#[derive(Debug, Clone)]
pub struct Elasticsearch {
    meta: EngineMeta,
    config: ElasticsearchConfig,
}

impl Elasticsearch {
    /// Build the engine from validated settings.
    ///
    /// Mirrors upstream `init()`: `index` must be non-empty and, if set,
    /// `query_type` must be one of the known query builders.
    pub fn new(config: ElasticsearchConfig) -> Result<Self, String> {
        if config.index.trim().is_empty() {
            return Err("index cannot be empty".to_string());
        }
        if !QUERY_TYPES.contains(&config.query_type.as_str()) {
            return Err(format!("unsupported query type: {}", config.query_type));
        }
        Ok(Elasticsearch {
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
                shortcut: "els".to_string(),
                about: About {
                    website: Some("https://www.elastic.co".to_string()),
                    wikidata_id: Some("Q3050461".to_string()),
                    official_api_documentation: Some(
                        "https://www.elastic.co/guide/en/elasticsearch/reference/current/search-search.html"
                            .to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
            config,
        })
    }

    fn build_query(&self, query: &str) -> Option<Value> {
        match self.config.query_type.as_str() {
            "match" => {
                let (key, value) = query.split_once(':')?;
                Some(json!({"query": {"match": {key: {"query": value}}}}))
            }
            "simple_query_string" => {
                Some(json!({"query": {"simple_query_string": {"query": query}}}))
            }
            "term" => {
                let (key, value) = query.split_once(':')?;
                Some(json!({"query": {"term": {key: value}}}))
            }
            "terms" => {
                let (key, values) = query.split_once(':')?;
                let values: Vec<&str> = values.split(',').collect();
                Some(json!({"query": {"terms": {key: values}}}))
            }
            "custom" => {
                let (key, value) = query.split_once(':')?;
                let mut custom = self.config.custom_query_json.clone();
                // Mirror upstream's shallow (top-level only) placeholder substitution.
                if let Some(placeholder) = custom.remove("{{KEY}}") {
                    custom.insert(key.to_string(), placeholder);
                }
                for (_, v) in custom.iter_mut() {
                    if v.as_str() == Some("{{VALUE}}") {
                        *v = Value::String(value.to_string());
                    }
                }
                Some(Value::Object(custom))
            }
            _ => None,
        }
    }
}

impl Engine for Elasticsearch {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        let Some(mut data) = self.build_query(&q.query) else {
            // Malformed `key:value` query: leave the URL unset. The executor
            // treats this like any other online engine that opted out of a
            // request and returns an empty result set instead of erroring.
            return;
        };
        if !self.config.username.is_empty() && !self.config.password.is_empty() {
            let creds = format!("{}:{}", self.config.username, self.config.password);
            p.auth = Some(format!("Basic {}", BASE64.encode(creds)));
        }
        if let Value::Object(ref mut map) = data {
            map.insert(
                "from".to_string(),
                json!((i64::from(q.pageno.max(1)) - 1) * self.config.page_size),
            );
            map.insert("size".to_string(), json!(self.config.page_size));
        }
        p.method = HttpMethod::Get;
        p.url = Some(format!(
            "{}/{}/_search",
            self.config.base_url, self.config.index
        ));
        p.json = Some(data);
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let body: Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("elasticsearch: invalid JSON: {e}")))?;

        if let Some(error) = body.get("error") {
            return Err(EngineError::Unexpected(format!("elasticsearch: {error}")));
        }

        let hits = body
            .get("hits")
            .and_then(|h| h.get("hits"))
            .and_then(|h| h.as_array())
            .cloned()
            .unwrap_or_default();

        for hit in hits {
            let source = hit.get("_source").and_then(|s| s.as_object());
            let mut kvmap: Vec<(String, String)> = Vec::new();
            if let Some(source) = source {
                for (key, value) in source {
                    kvmap.push((key.clone(), value_to_string(value)));
                }
            }
            if self.config.show_metadata {
                let metadata = json!({
                    "index": hit.get("_index"),
                    "id": hit.get("_id"),
                    "score": hit.get("_score"),
                });
                kvmap.push(("metadata".to_string(), metadata.to_string()));
            }
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

    fn config() -> ElasticsearchConfig {
        ElasticsearchConfig {
            base_url: "http://localhost:9200".to_string(),
            username: String::new(),
            password: String::new(),
            index: "my-index".to_string(),
            query_type: "match".to_string(),
            custom_query_json: Map::new(),
            show_metadata: false,
            page_size: 10,
        }
    }

    #[test]
    fn rejects_empty_index() {
        let mut cfg = config();
        cfg.index = String::new();
        assert!(Elasticsearch::new(cfg).is_err());
    }

    #[test]
    fn rejects_unknown_query_type() {
        let mut cfg = config();
        cfg.query_type = "bogus".to_string();
        assert!(Elasticsearch::new(cfg).is_err());
    }

    #[test]
    fn builds_match_query_request() {
        let engine = Elasticsearch::new(config()).unwrap();
        let q = SearchQueryView {
            query: "city:berlin".to_string(),
            pageno: 2,
            ..SearchQueryView::default()
        };
        let mut params = RequestParams::default();
        engine.request(&q, &mut params);
        assert_eq!(
            params.url.as_deref(),
            Some("http://localhost:9200/my-index/_search")
        );
        let body = params.json.unwrap();
        assert_eq!(body["query"]["match"]["city"]["query"], "berlin");
        assert_eq!(body["from"], 10);
        assert_eq!(body["size"], 10);
    }

    #[test]
    fn parses_hits_into_keyvalue_results() {
        let engine = Elasticsearch::new(config()).unwrap();
        let body = json!({
            "hits": {"hits": [
                {"_index": "my-index", "_id": "1", "_score": 1.0, "_source": {"city": "berlin", "country": "de"}}
            ]}
        });
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
                assert!(
                    kv.kvmap
                        .contains(&("country".to_string(), "de".to_string()))
                );
            }
            other => panic!("expected KeyValue, got {other:?}"),
        }
    }

    #[test]
    fn errors_on_error_payload() {
        let engine = Elasticsearch::new(config()).unwrap();
        let resp = EngineResponse {
            status: 400,
            body: br#"{"error": "boom"}"#.to_vec(),
            ..EngineResponse::default()
        };
        assert!(engine.response(&resp).is_err());
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);
        let engine = Elasticsearch::new(config()).unwrap();
        let q = SearchQueryView {
            query: "city:berlin".to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        };
        let body = json!({
            "hits": {"hits": [
                {"_index": "my-index", "_id": "1", "_score": 1.5, "_source": {"city": "berlin"}}
            ]}
        });
        let resp = EngineResponse {
            status: 200,
            url: "http://localhost:9200/my-index/_search".to_string(),
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
            .with_case("match-basic")
            .with_golden_request(golden)
            .save(dir.join("match-basic.json"))
            .unwrap();
    }

    #[test]
    fn elasticsearch_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Elasticsearch::new(config()).unwrap();
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
