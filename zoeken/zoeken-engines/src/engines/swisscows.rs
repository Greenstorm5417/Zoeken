//! Swisscows JSON search engine.

use base64::Engine as _;
use base64::engine::general_purpose::{STANDARD, URL_SAFE_NO_PAD};
use serde::Deserialize;
use serde_json::Value;
use sha2::{Digest, Sha256};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, MainResult,
    Processor, RequestParams, SearchQueryView, TimeRange, html_to_text,
};
use zoeken_results::{Image, Result_};

use super::util::encode_query;

pub const NAME: &str = "swisscows";
pub const NEWS_NAME: &str = "swisscows news";
const NONCE: &str = "ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef";

#[derive(Debug, Clone, Deserialize)]
pub struct SwisscowsConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_category")]
    pub swisscows_category: String,
    #[serde(default = "default_results_per_page")]
    pub results_per_page: u32,
}

fn default_base_url() -> String {
    "https://api.swisscows.com".to_string()
}

fn default_category() -> String {
    "web".to_string()
}

fn default_results_per_page() -> u32 {
    20
}

#[derive(Debug, Clone)]
pub struct Swisscows {
    meta: EngineMeta,
    config: SwisscowsConfig,
}

impl Swisscows {
    pub fn new(config: SwisscowsConfig) -> Result<Self, String> {
        if !matches!(
            config.swisscows_category.as_str(),
            "web" | "images" | "videos" | "news"
        ) {
            return Err(format!(
                "illegal swisscows category: {}",
                config.swisscows_category
            ));
        }
        let max = match config.swisscows_category.as_str() {
            "images" => 50,
            "videos" => 10,
            _ => 20,
        };
        if config.results_per_page > max {
            return Err(format!("results_per_page can be at most {max}"));
        }
        let name = if config.swisscows_category == "news" {
            NEWS_NAME
        } else {
            NAME
        };
        Ok(Self {
            meta: EngineMeta {
                name: name.to_string(),
                engine_type: Processor::Online,
                categories: vec![
                    match config.swisscows_category.as_str() {
                        "images" => "images",
                        "videos" => "videos",
                        "news" => "news",
                        _ => "general",
                    }
                    .to_string(),
                ],
                paging: true,
                max_page: if config.swisscows_category == "images" {
                    2
                } else {
                    0
                },
                time_range_support: matches!(config.swisscows_category.as_str(), "web" | "news"),
                language_support: true,
                shortcut: "sw".to_string(),
                about: About {
                    website: Some("https://swisscows.com".to_string()),
                    wikidata_id: Some("Q22937452".to_string()),
                    results: "JSON".to_string(),
                    ..About::default()
                },
                ..EngineMeta::default()
            },
            config,
        })
    }
}

impl Default for Swisscows {
    fn default() -> Self {
        Self::new(SwisscowsConfig {
            base_url: default_base_url(),
            swisscows_category: default_category(),
            results_per_page: default_results_per_page(),
        })
        .expect("default swisscows config")
    }
}

impl Engine for Swisscows {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        if self.config.swisscows_category == "images" && q.pageno > 2 {
            p.url = None;
            return;
        }
        p.method = HttpMethod::Get;
        let locale = swisscows_locale(&q.locale);
        let offset = q
            .pageno
            .saturating_sub(1)
            .saturating_mul(self.config.results_per_page);
        let (path, pairs) = request_parts(&self.config, q, &locale, offset);
        if self.config.swisscows_category != "news" {
            let (nonce, signature) = signature(&path, &pairs);
            p.headers.insert("X-Request-Nonce".to_string(), nonce);
            p.headers
                .insert("X-Request-Signature".to_string(), signature);
        }
        p.url = Some(format!(
            "{}{}?{}",
            self.config.base_url.trim_end_matches('/'),
            path,
            encode_query(&pairs)
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut value: Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Swisscows JSON: {e}")))?;
        if let Some(payload) = value.get("payload").and_then(Value::as_str) {
            value = decode_payload(payload)?;
        }

        let mut out = EngineResults::new();
        for item in value
            .get("items")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default()
        {
            match item.get("type").and_then(Value::as_str).unwrap_or_default() {
                "WebPage" => add_main(&mut out, &item, &self.meta.name),
                "ImageObject" => add_image(&mut out, &item, &self.meta.name),
                "VideoCollection" => {
                    for video in item
                        .get("hasPart")
                        .and_then(Value::as_array)
                        .cloned()
                        .unwrap_or_default()
                    {
                        add_main(&mut out, &video, &self.meta.name);
                    }
                }
                "video" => add_main(&mut out, &item, &self.meta.name),
                _ if self.config.swisscows_category == "news" => {
                    add_news(&mut out, &item, &self.meta.name);
                }
                _ => {}
            }
        }
        Ok(out)
    }
}

fn request_parts(
    config: &SwisscowsConfig,
    q: &SearchQueryView,
    locale: &str,
    offset: u32,
) -> (String, Vec<(&'static str, String)>) {
    match config.swisscows_category.as_str() {
        "images" => (
            "/v5/images/search".to_string(),
            vec![
                ("itemsCount", config.results_per_page.to_string()),
                ("locale", locale.to_string()),
                ("offset", offset.to_string()),
                ("query", q.query.clone()),
                ("spellcheck", "true".to_string()),
            ],
        ),
        "videos" => (
            "/v2/videos/search".to_string(),
            vec![
                ("itemsCount", config.results_per_page.to_string()),
                ("offset", offset.to_string()),
                ("query", q.query.clone()),
                ("region", locale.to_string()),
                ("spellcheck", "true".to_string()),
            ],
        ),
        "news" => {
            let region = if locale.starts_with("de") {
                locale.to_string()
            } else {
                "de-DE".to_string()
            };
            let language = region.split('-').next().unwrap_or("de").to_string();
            (
                "/news/search".to_string(),
                vec![
                    ("query", q.query.clone()),
                    ("itemsCount", config.results_per_page.to_string()),
                    ("region", region),
                    ("language", language),
                    ("offset", offset.to_string()),
                    ("freshness", freshness(q.time_range).to_string()),
                    ("sortOrder", "Desc".to_string()),
                    ("sortBy", "Created".to_string()),
                ],
            )
        }
        _ => (
            "/v5/web/search".to_string(),
            vec![
                ("freshness", freshness(q.time_range).to_string()),
                ("itemsCount", config.results_per_page.to_string()),
                ("locale", locale.to_string()),
                ("offset", offset.to_string()),
                ("query", q.query.clone()),
                ("spellcheck", "true".to_string()),
            ],
        ),
    }
}

fn freshness(time_range: Option<TimeRange>) -> &'static str {
    match time_range {
        Some(TimeRange::Day) => "Day",
        Some(TimeRange::Week) => "Week",
        Some(TimeRange::Month) => "Month",
        Some(TimeRange::Year) => "Year",
        None => "All",
    }
}

fn swisscows_locale(locale: &str) -> String {
    if locale.is_empty() || locale == "all" {
        "en-US".to_string()
    } else {
        locale.to_string()
    }
}

fn signature(path: &str, pairs: &[(&'static str, String)]) -> (String, String) {
    let mut sorted = pairs.to_vec();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let query = sorted
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("&");
    let nonce_shifted = caesar_switch(NONCE);
    let mut hasher = Sha256::new();
    hasher.update(format!("{path}?{query}{nonce_shifted}").as_bytes());
    (NONCE.to_string(), URL_SAFE_NO_PAD.encode(hasher.finalize()))
}

fn caesar_switch(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if c.is_ascii_alphabetic() {
                let base = if c.is_ascii_uppercase() { b'A' } else { b'a' };
                let shifted = ((c as u8 - base + 13) % 26) + base;
                let shifted = shifted as char;
                if c.is_ascii_uppercase() {
                    shifted.to_ascii_lowercase()
                } else {
                    shifted.to_ascii_uppercase()
                }
            } else {
                c
            }
        })
        .collect()
}

fn decode_payload(payload: &str) -> Result<Value, EngineError> {
    let Some(encoded) = payload.split('.').nth(1) else {
        return Err(EngineError::Parse("invalid Swisscows payload".to_string()));
    };
    let mut padded = encoded.to_string();
    let rem = padded.len() % 4;
    if rem != 0 {
        padded.extend(std::iter::repeat_n('=', 4 - rem));
    }
    let bytes = STANDARD
        .decode(padded.replace('-', "+").replace('_', "/"))
        .map_err(|e| EngineError::Parse(format!("invalid Swisscows payload base64: {e}")))?;
    serde_json::from_slice(&bytes)
        .map_err(|e| EngineError::Parse(format!("invalid Swisscows payload JSON: {e}")))
}

fn add_main(out: &mut EngineResults, item: &Value, engine: &str) {
    let url = text(item, "url");
    if url.is_empty() {
        return;
    }
    out.add(Result_::Main(MainResult {
        url: url.clone(),
        normalized_url: url,
        title: text(item, "name"),
        content: html_to_text(&text(item, "description")),
        engine: engine.to_string(),
        ..MainResult::default()
    }));
}

fn add_news(out: &mut EngineResults, item: &Value, engine: &str) {
    let url = text(item, "uri");
    if url.is_empty() {
        return;
    }
    let published_date = item
        .get("created")
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_string);
    let thumbnail = item
        .get("og:image")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string();
    out.add(Result_::Main(MainResult {
        url: url.clone(),
        normalized_url: url,
        title: html_to_text(&text(item, "title")),
        content: text(item, "description"),
        engine: engine.to_string(),
        published_date,
        thumbnail,
        ..MainResult::default()
    }));
}

fn add_image(out: &mut EngineResults, item: &Value, engine: &str) {
    let url = text(item, "url");
    let img_src = text(item, "contentUrl");
    if url.is_empty() || img_src.is_empty() {
        return;
    }
    out.add(Result_::Image(Image {
        url: url.clone(),
        normalized_url: url,
        title: text(item, "name"),
        engine: engine.to_string(),
        img_src,
        thumbnail_src: item
            .get("thumbnail")
            .and_then(|v| v.get("url"))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        ..Image::default()
    }));
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

    #[test]
    fn swisscows_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load swisscows fixtures");
        run_all(&Swisscows::default(), &fixtures).expect("swisscows fixtures conform");

        let news = Swisscows::new(SwisscowsConfig {
            base_url: default_base_url(),
            swisscows_category: "news".to_string(),
            results_per_page: default_results_per_page(),
        })
        .expect("swisscows news engine");
        let fixtures = load_fixtures_for(fixtures_root(), "swisscows_news")
            .expect("load swisscows news fixtures");
        run_all(&news, &fixtures).expect("swisscows news fixtures conform");
    }
}
