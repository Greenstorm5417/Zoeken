//! Dogpile JSON search engine.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::{Value, json};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, MainResult,
    Processor, RequestParams, SafeSearch, SearchQueryView, html_to_text,
};
use zoeken_results::{Image, Result_};

pub const NAME: &str = "dogpile";

#[derive(Debug, Clone, Deserialize)]
pub struct DogpileConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
    #[serde(default = "default_category")]
    pub dogpile_categ: String,
}

fn default_base_url() -> String {
    "https://www.dogpile.com".to_string()
}

fn default_category() -> String {
    "search".to_string()
}

#[derive(Debug, Clone)]
pub struct Dogpile {
    meta: EngineMeta,
    config: DogpileConfig,
}

impl Dogpile {
    pub fn new(config: DogpileConfig) -> Result<Self, String> {
        if !matches!(
            config.dogpile_categ.as_str(),
            "search" | "images" | "videos" | "news"
        ) {
            return Err(format!("invalid search type: {}", config.dogpile_categ));
        }
        Ok(Self {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec![
                    match config.dogpile_categ.as_str() {
                        "images" => "images",
                        "videos" => "videos",
                        "news" => "news",
                        _ => "general",
                    }
                    .to_string(),
                ],
                paging: true,
                max_page: 0,
                safesearch: true,
                shortcut: "dp".to_string(),
                about: About {
                    website: Some("https://www.dogpile.com".to_string()),
                    wikidata_id: Some("Q3595363".to_string()),
                    results: "JSON".to_string(),
                    ..About::default()
                },
                ..EngineMeta::default()
            },
            config,
        })
    }
}

impl Default for Dogpile {
    fn default() -> Self {
        Self::new(DogpileConfig {
            base_url: default_base_url(),
            dogpile_categ: default_category(),
        })
        .expect("default dogpile config")
    }
}

impl Engine for Dogpile {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Post;
        p.url = Some(format!(
            "{}/api/{}",
            self.config.base_url.trim_end_matches('/'),
            self.config.dogpile_categ
        ));
        p.json = Some(json!({
            "q": q.query,
            "qadf": dogpile_safe_search(q.safesearch),
            "page": q.pageno.max(1),
        }));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let value: Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Dogpile JSON: {e}")))?;
        let mut out = EngineResults::new();
        let results = value
            .get("results")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();
        for item in results {
            match self.config.dogpile_categ.as_str() {
                "images" => add_image(&mut out, &item),
                _ => add_main(&mut out, &item),
            }
        }
        Ok(out)
    }
}

fn dogpile_safe_search(value: SafeSearch) -> &'static str {
    match value {
        SafeSearch::Off => "none",
        SafeSearch::Moderate => "moderate",
        SafeSearch::Strict => "heavy",
    }
}

fn add_main(out: &mut EngineResults, item: &Value) {
    let url = text(item, "clickUrl");
    if url.is_empty() {
        return;
    }
    out.add(Result_::Main(MainResult {
        url: url.clone(),
        normalized_url: url,
        title: html_to_text(&text(item, "title")),
        content: html_to_text(&text(item, "description")),
        engine: NAME.to_string(),
        ..MainResult::default()
    }));
}

fn add_image(out: &mut EngineResults, item: &Value) {
    let url = text(item, "altClickUrl");
    let img_src = text(item, "clickUrl");
    if url.is_empty() || img_src.is_empty() {
        return;
    }
    out.add(Result_::Image(Image {
        url: url.clone(),
        normalized_url: url,
        title: html_to_text(&text(item, "title")),
        content: html_to_text(&text(item, "description")),
        engine: NAME.to_string(),
        img_src,
        thumbnail_src: text(item, "thumbnailUrl"),
        resolution: resolution(item),
        img_format: text(item, "format"),
        ..Image::default()
    }));
}

fn text(item: &Value, key: &str) -> String {
    item.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

fn resolution(item: &Value) -> String {
    let width = item.get("width").and_then(Value::as_i64).unwrap_or(0);
    let height = item.get("height").and_then(Value::as_i64).unwrap_or(0);
    if width > 0 && height > 0 {
        format!("{width}x{height}")
    } else {
        String::new()
    }
}

#[allow(dead_code)]
fn unix_date(value: i64) -> Option<String> {
    DateTime::<Utc>::from_timestamp(value, 0).map(|dt| dt.to_rfc3339())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::{load_fixtures_for, run_all};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
    }

    fn engine() -> Dogpile {
        Dogpile::default()
    }

    #[test]
    fn dogpile_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load dogpile fixtures");
        run_all(&engine(), &fixtures).expect("dogpile fixtures conform");
    }
}
