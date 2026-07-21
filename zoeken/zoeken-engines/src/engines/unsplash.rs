//! Unsplash engine: images via the unofficial `napi/search/photos` endpoint.

use url::Url;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Image, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "unsplash";

const BASE_URL: &str = "https://unsplash.com/";
const SEARCH_URL: &str = "https://unsplash.com/napi/search/photos?";
const PAGE_SIZE: u32 = 20;

#[derive(Debug, Clone)]
pub struct Unsplash {
    meta: EngineMeta,
}

impl Unsplash {
    pub fn new() -> Self {
        Unsplash {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["images".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "us".to_string(),
                about: About {
                    website: Some(BASE_URL.to_string()),
                    wikidata_id: Some("Q28233552".to_string()),
                    official_api_documentation: Some("https://unsplash.com/developers".to_string()),
                    use_official_api: false,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Unsplash {
    fn default() -> Self {
        Self::new()
    }
}

/// Strip the `ixid` query parameter, mirroring the reference `clean_url`.
fn clean_url(raw: &str) -> String {
    let Ok(mut url) = Url::parse(raw) else {
        return raw.to_string();
    };
    let pairs: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(k, _)| k != "ixid")
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if pairs.is_empty() {
        url.set_query(None);
    } else {
        let query = pairs
            .iter()
            .map(|(k, v)| {
                format!(
                    "{}={}",
                    super::util::encode_component(k),
                    super::util::encode_component(v)
                )
            })
            .collect::<Vec<_>>()
            .join("&");
        url.set_query(Some(&query));
    }
    url.to_string()
}

impl Engine for Unsplash {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![
            ("query", q.query.clone()),
            ("page", p.pageno.to_string()),
            ("per_page", PAGE_SIZE.to_string()),
        ];
        p.url = Some(format!("{SEARCH_URL}{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Unsplash JSON: {e}")))?;

        let results = value
            .get("results")
            .and_then(|r| r.as_array())
            .cloned()
            .unwrap_or_default();

        for item in &results {
            let url = item
                .pointer("/links/html")
                .and_then(|u| u.as_str())
                .map(clean_url)
                .unwrap_or_default();
            let thumbnail_src = item
                .pointer("/urls/thumb")
                .and_then(|u| u.as_str())
                .map(clean_url)
                .unwrap_or_default();
            let img_src = item
                .pointer("/urls/regular")
                .and_then(|u| u.as_str())
                .map(clean_url)
                .unwrap_or_default();
            if url.is_empty() || img_src.is_empty() {
                continue;
            }
            let thumbnail_src = if thumbnail_src.is_empty() {
                img_src.clone()
            } else {
                thumbnail_src
            };
            let title = item
                .get("alt_description")
                .and_then(|t| t.as_str())
                .filter(|s| !s.is_empty())
                .or_else(|| {
                    item.get("description")
                        .and_then(|d| d.as_str())
                        .filter(|s| !s.is_empty())
                })
                .unwrap_or("Image")
                .to_string();
            let content = item
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("")
                .to_string();
            let width = item.get("width").and_then(|v| v.as_u64()).unwrap_or(0);
            let height = item.get("height").and_then(|v| v.as_u64()).unwrap_or(0);
            let resolution = if width > 0 && height > 0 {
                format!("{width}x{height}")
            } else {
                "unknown".to_string()
            };

            res.add(Result_::Image(Image {
                url: url.clone(),
                normalized_url: url,
                title,
                content,
                thumbnail_src,
                img_src,
                resolution,
                engine: NAME.to_string(),
                ..Image::default()
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

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: SEARCH_URL.to_string(),
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
      "results": [
        {
          "alt_description": "a blue cat",
          "description": "A very blue cat.",
          "width": 4000,
          "height": 3000,
          "links": {"html": "https://unsplash.com/photos/1?ixid=abc"},
          "urls": {
            "thumb": "https://images.unsplash.com/1?ixid=abc&w=200",
            "regular": "https://images.unsplash.com/1?ixid=abc&w=1080"
          }
        }
      ]
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(Result_::Image(Image {
            url: "https://unsplash.com/photos/1".to_string(),
            normalized_url: "https://unsplash.com/photos/1".to_string(),
            title: "a blue cat".to_string(),
            content: "A very blue cat.".to_string(),
            thumbnail_src: "https://images.unsplash.com/1?w=200".to_string(),
            img_src: "https://images.unsplash.com/1?w=1080".to_string(),
            resolution: "4000x3000".to_string(),
            engine: NAME.to_string(),
            ..Image::default()
        }));
        Fixture::capture(NAME, query("cat", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        Fixture::capture(
            NAME,
            query("cat", 1),
            response(200, r#"{"results":[]}"#),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();

        let q = query("blue cat", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{SEARCH_URL}query=blue+cat&page=2&per_page=20"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"results":[]}"#),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn unsplash_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Unsplash::new();
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
    fn clean_url_strips_ixid() {
        assert_eq!(
            clean_url("https://unsplash.com/photos/1?ixid=abc&w=200"),
            "https://unsplash.com/photos/1?w=200"
        );
        assert_eq!(
            clean_url("https://unsplash.com/photos/1?ixid=abc"),
            "https://unsplash.com/photos/1"
        );
    }

    #[test]
    fn missing_alt_description_falls_back_to_image() {
        let engine = Unsplash::new();
        let body = r#"{"results":[{"links":{"html":"https://unsplash.com/photos/2"},"urls":{"thumb":"t","regular":"r"}}]}"#;
        let res = engine.response(&response(200, body)).unwrap();
        match &res.results[0] {
            Result_::Image(img) => assert_eq!(img.title, "Image"),
            _ => panic!("expected image result"),
        }
    }
}
