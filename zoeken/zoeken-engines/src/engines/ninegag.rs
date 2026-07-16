//! 9GAG engine: queries the internal search-posts API and returns photo/animated posts plus
//! tag suggestions.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Image, MainResult, Result_, Suggestion};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "9gag";

const BASE_URL: &str = "https://9gag.com/v1/search-posts";
const PAGE_SIZE: u32 = 10;

/// The 9GAG engine.
#[derive(Debug, Clone)]
pub struct NineGag {
    meta: EngineMeta,
}

impl NineGag {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        NineGag {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["social media".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "9g".to_string(),
                about: About {
                    website: Some("https://9gag.com/".to_string()),
                    wikidata_id: Some("Q277421".to_string()),
                    official_api_documentation: None,
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for NineGag {
    fn default() -> Self {
        Self::new()
    }
}

fn thumbnail_url(image700: &serde_json::Value, thumbnail_fallback: &serde_json::Value) -> String {
    let height = image700.get("height").and_then(|v| v.as_i64()).unwrap_or(0);
    if height > 400 {
        thumbnail_fallback
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    } else {
        image700
            .get("url")
            .and_then(|v| v.as_str())
            .unwrap_or_default()
            .to_string()
    }
}

impl Engine for NineGag {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let offset = (q.pageno.max(1) - 1) * PAGE_SIZE;
        let query = encode_query(&[("query", q.query.clone()), ("c", offset.to_string())]);
        p.url = Some(format!("{BASE_URL}?{query}"));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid 9GAG JSON: {e}")))?;

        let empty = serde_json::Value::Object(serde_json::Map::new());
        let data = value.get("data").unwrap_or(&empty);

        let posts = data
            .get("posts")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for post in &posts {
            let result_type = post.get("type").and_then(|v| v.as_str()).unwrap_or("");
            let Some(url) = post.get("url").and_then(|v| v.as_str()) else {
                continue;
            };
            let title = post.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let content = post
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("");

            let images = post.get("images").unwrap_or(&empty);
            let image700 = images.get("image700").unwrap_or(&empty);
            let thumb_fallback = images.get("imageFbThumbnail").unwrap_or(&empty);
            let thumbnail = thumbnail_url(image700, thumb_fallback);

            match result_type {
                "Photo" => {
                    let img_src = image700
                        .get("url")
                        .and_then(|v| v.as_str())
                        .unwrap_or_default()
                        .to_string();
                    res.add(Result_::Image(Image {
                        url: url.to_string(),
                        normalized_url: url.to_string(),
                        title: title.to_string(),
                        content: content.to_string(),
                        engine: NAME.to_string(),
                        img_src,
                        thumbnail_src: thumbnail,
                        ..Image::default()
                    }));
                }
                "Animated" => {
                    res.add(Result_::Main(MainResult {
                        url: url.to_string(),
                        normalized_url: url.to_string(),
                        title: title.to_string(),
                        content: content.to_string(),
                        engine: NAME.to_string(),
                        ..MainResult::default()
                    }));
                }
                _ => {}
            }
        }

        if let Some(tags) = data.get("tags").and_then(|v| v.as_array()) {
            for tag in tags {
                if let Some(key) = tag.get("key").and_then(|v| v.as_str()) {
                    res.add(Result_::Suggestion(Suggestion {
                        suggestion: key.to_string(),
                        engine: NAME.to_string(),
                    }));
                }
            }
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
      "data": {
        "posts": [
          {
            "type": "Photo",
            "url": "https://9gag.com/gag/a1",
            "title": "Funny cat",
            "description": "a cat",
            "creationTs": 1000,
            "images": {
              "image700": {"url": "https://img.9gag.com/a1_700.jpg", "height": 300},
              "imageFbThumbnail": {"url": "https://img.9gag.com/a1_fb.jpg"}
            }
          },
          {
            "type": "Animated",
            "url": "https://9gag.com/gag/a2",
            "title": "Funny gif",
            "description": "a gif",
            "creationTs": 1000,
            "images": {
              "image700": {"url": "https://img.9gag.com/a2_700.jpg", "height": 500},
              "imageFbThumbnail": {"url": "https://img.9gag.com/a2_fb.jpg"}
            }
          },
          {
            "type": "Article",
            "url": "https://9gag.com/gag/a3",
            "title": "unsupported",
            "description": "",
            "images": {}
          }
        ],
        "tags": [{"key": "funny"}, {"key": "cats"}]
      }
    }"#;

    const EMPTY_JSON: &str = r#"{"data": {"posts": []}}"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(Result_::Image(Image {
            url: "https://9gag.com/gag/a1".to_string(),
            normalized_url: "https://9gag.com/gag/a1".to_string(),
            title: "Funny cat".to_string(),
            content: "a cat".to_string(),
            engine: NAME.to_string(),
            img_src: "https://img.9gag.com/a1_700.jpg".to_string(),
            thumbnail_src: "https://img.9gag.com/a1_700.jpg".to_string(),
            ..Image::default()
        }));
        basic.add(Result_::Main(MainResult {
            url: "https://9gag.com/gag/a2".to_string(),
            normalized_url: "https://9gag.com/gag/a2".to_string(),
            title: "Funny gif".to_string(),
            content: "a gif".to_string(),
            engine: NAME.to_string(),
            ..MainResult::default()
        }));
        basic.add(Result_::Suggestion(Suggestion {
            suggestion: "funny".to_string(),
            engine: NAME.to_string(),
        }));
        basic.add(Result_::Suggestion(Suggestion {
            suggestion: "cats".to_string(),
            engine: NAME.to_string(),
        }));
        Fixture::capture(NAME, query("cats", 1), response(200, BASIC_JSON), basic)
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

        let q = query("cats", 3);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}?query=cats&c=20"));
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
    fn ninegag_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = NineGag::new();
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
        let engine = NineGag::new();
        let q = query("cats", 3);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://9gag.com/v1/search-posts?query=cats&c=20")
        );
    }
}
