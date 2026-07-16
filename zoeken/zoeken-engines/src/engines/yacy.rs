//! YaCy JSON search engine.

use serde::Deserialize;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, MainResult,
    Processor, RequestParams, SearchQueryView, html_to_text,
};
use zoeken_results::{Image, Result_};

use super::util::encode_query;

pub const NAME: &str = "yacy";
const PAGE_SIZE: u32 = 10;

#[derive(Debug, Clone, Deserialize)]
pub struct YacyConfig {
    #[serde(default, deserialize_with = "deserialize_base_urls")]
    pub base_url: Vec<String>,
    #[serde(default = "default_search_mode")]
    pub search_mode: String,
    #[serde(default = "default_search_type")]
    pub search_type: String,
    #[serde(default)]
    pub http_digest_auth_user: String,
    #[serde(default)]
    pub http_digest_auth_pass: String,
}

fn default_search_mode() -> String {
    "global".to_string()
}

fn default_search_type() -> String {
    "text".to_string()
}

fn deserialize_base_urls<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Value {
        One(String),
        Many(Vec<String>),
    }

    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(match value {
        Some(Value::One(url)) => vec![url],
        Some(Value::Many(urls)) => urls,
        None => Vec::new(),
    })
}

#[derive(Debug, Clone)]
pub struct Yacy {
    meta: EngineMeta,
    config: YacyConfig,
}

impl Yacy {
    pub fn new(config: YacyConfig) -> Result<Self, String> {
        if config.base_url.is_empty() {
            return Err("base_url is required".to_string());
        }
        if !matches!(config.search_type.as_str(), "text" | "image") {
            return Err(format!("unsupported search_type: {}", config.search_type));
        }
        if !matches!(config.search_mode.as_str(), "global" | "local") {
            return Err(format!("unsupported search_mode: {}", config.search_mode));
        }
        let categories = if config.search_type == "image" {
            vec!["images".to_string()]
        } else {
            vec!["general".to_string()]
        };
        Ok(Self {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories,
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: true,
                weight: 1,
                shortcut: "ya".to_string(),
                about: About {
                    website: Some("https://yacy.net/".to_string()),
                    wikidata_id: Some("Q1759675".to_string()),
                    official_api_documentation: Some(
                        "https://wiki.yacy.net/index.php/Dev:API".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
            config,
        })
    }

    fn base_url(&self) -> String {
        self.config.base_url[0].trim_end_matches('/').to_string()
    }
}

impl Engine for Yacy {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let mut args = vec![
            ("query", q.query.clone()),
            (
                "startRecord",
                q.pageno
                    .saturating_sub(1)
                    .saturating_mul(PAGE_SIZE)
                    .to_string(),
            ),
            ("maximumRecords", PAGE_SIZE.to_string()),
            ("contentdom", self.config.search_type.clone()),
            ("resource", self.config.search_mode.clone()),
        ];
        if q.locale != "all" && !q.locale.is_empty() {
            let lang = q.locale.split(['-', '_']).next().unwrap_or("all");
            if lang != "all" && !lang.is_empty() {
                args.push(("lr", format!("lang_{lang}")));
            }
        }
        p.url = Some(format!(
            "{}/yacysearch.json?{}",
            self.base_url(),
            encode_query(&args)
        ));
        if !self.config.http_digest_auth_user.is_empty()
            && !self.config.http_digest_auth_pass.is_empty()
        {
            p.auth = Some(format!(
                "{}:{}",
                self.config.http_digest_auth_user, self.config.http_digest_auth_pass
            ));
        }
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let body: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid YaCy JSON: {e}")))?;
        let mut out = EngineResults::new();
        let items = body
            .get("channels")
            .and_then(|channels| channels.as_array())
            .and_then(|channels| channels.first())
            .and_then(|channel| channel.get("items"))
            .and_then(|items| items.as_array())
            .cloned()
            .unwrap_or_default();

        for item in items {
            if self.config.search_type == "image" {
                let url = item
                    .get("url")
                    .or_else(|| item.get("link"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                if url.is_empty() {
                    continue;
                }
                out.add(Result_::Image(Image {
                    url: url.to_string(),
                    normalized_url: url.to_string(),
                    title: string_field(&item, "title"),
                    content: String::new(),
                    img_src: string_field(&item, "image"),
                    engine: NAME.to_string(),
                    ..Image::default()
                }));
            } else {
                let url = string_field(&item, "link");
                out.add(Result_::Main(MainResult {
                    url: url.clone(),
                    normalized_url: url,
                    title: string_field(&item, "title"),
                    content: html_to_text(&string_field(&item, "description")),
                    engine: NAME.to_string(),
                    ..MainResult::default()
                }));
            }
        }
        Ok(out)
    }
}

fn string_field(item: &serde_json::Value, key: &str) -> String {
    item.get(key)
        .and_then(|value| value.as_str())
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

    fn engine() -> Yacy {
        Yacy::new(YacyConfig {
            base_url: vec!["https://search.example.test".to_string()],
            search_mode: "global".to_string(),
            search_type: "text".to_string(),
            http_digest_auth_user: String::new(),
            http_digest_auth_pass: String::new(),
        })
        .expect("yacy engine")
    }

    #[test]
    fn builds_request_with_offset_and_language() {
        let mut params = RequestParams::default();
        let query = SearchQueryView {
            query: "rust search".to_string(),
            pageno: 2,
            locale: "de-DE".to_string(),
            ..SearchQueryView::default()
        };
        engine().request(&query, &mut params);
        assert_eq!(params.method, HttpMethod::Get);
        assert_eq!(
            params.url.as_deref(),
            Some(
                "https://search.example.test/yacysearch.json?query=rust+search&startRecord=10&maximumRecords=10&contentdom=text&resource=global&lr=lang_de"
            )
        );
    }

    #[test]
    fn yacy_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load yacy fixtures");
        run_all(&engine(), &fixtures).expect("yacy fixtures conform");
    }
}
