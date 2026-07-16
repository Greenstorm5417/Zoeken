//! Marginalia Search API engine.

use serde::Deserialize;
use serde_json::Value;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, MainResult,
    Processor, RequestParams, SafeSearch, SearchQueryView,
};
use zoeken_results::Result_;

use super::util::encode_query;

pub const NAME: &str = "marginalia";
const RESULTS_PER_PAGE: u32 = 20;

#[derive(Debug, Clone, Deserialize)]
pub struct MarginaliaConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    pub api_key: String,
}

fn default_base_url() -> String {
    "https://api2.marginalia-search.com".to_string()
}

#[derive(Debug, Clone)]
pub struct Marginalia {
    meta: EngineMeta,
    config: MarginaliaConfig,
}

impl Marginalia {
    pub fn new(config: MarginaliaConfig) -> Result<Self, String> {
        if config.api_key.trim().is_empty() || config.api_key == "public" {
            return Err("valid api_key is required".to_string());
        }
        Ok(Self {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string(), "blogs".to_string()],
                paging: true,
                max_page: 0,
                safesearch: true,
                shortcut: "mar".to_string(),
                about: About {
                    website: Some("https://marginalia.nu".to_string()),
                    official_api_documentation: Some(
                        "https://about.marginalia-search.com/article/api/".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: true,
                    results: "JSON".to_string(),
                    ..About::default()
                },
                ..EngineMeta::default()
            },
            config,
        })
    }
}

impl Engine for Marginalia {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        p.headers
            .insert("API-Key".to_string(), self.config.api_key.clone());
        let pairs = [
            ("page", q.pageno.max(1).to_string()),
            ("count", RESULTS_PER_PAGE.to_string()),
            ("nsfw", nsfw(q.safesearch).to_string()),
            ("query", q.query.clone()),
        ];
        p.url = Some(format!(
            "{}/search?{}",
            self.config.base_url.trim_end_matches('/'),
            encode_query(&pairs)
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let value: Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Marginalia JSON: {e}")))?;
        let mut out = EngineResults::new();
        let results = value
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for item in results {
            let url = text(&item, "url");
            if url.is_empty() {
                continue;
            }
            out.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title: text(&item, "title"),
                content: text(&item, "description"),
                engine: NAME.to_string(),
                ..MainResult::default()
            }));
        }
        Ok(out)
    }
}

fn nsfw(safesearch: SafeSearch) -> u8 {
    u8::from(!matches!(safesearch, SafeSearch::Off))
}

fn text(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::{load_fixtures_for, run_all};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
    }

    fn engine() -> Marginalia {
        Marginalia::new(MarginaliaConfig {
            base_url: default_base_url(),
            api_key: "test-key".to_string(),
        })
        .expect("marginalia engine")
    }

    #[test]
    fn rejects_missing_api_key() {
        assert!(
            Marginalia::new(MarginaliaConfig {
                base_url: default_base_url(),
                api_key: String::new(),
            })
            .is_err()
        );
    }

    #[test]
    fn marginalia_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load marginalia fixtures");
        run_all(&engine(), &fixtures).expect("marginalia fixtures conform");
    }
}
