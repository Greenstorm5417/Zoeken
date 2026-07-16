//! Reddit search engine.
//!
//! Parses Reddit search results into image and text entries.

use url::Url;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Image, MainResult, Result_};

use super::util::encode_query;

pub const NAME: &str = "reddit";

const BASE_URL: &str = "https://www.reddit.com/";

const SEARCH_URL: &str = "https://www.reddit.com/search.json";

const PAGE_SIZE: u32 = 25;

#[derive(Debug, Clone)]
pub struct Reddit {
    meta: EngineMeta,
}

impl Reddit {
    pub fn new() -> Self {
        Reddit {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["social media".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "re".to_string(),
                about: About {
                    website: Some("https://www.reddit.com/".to_string()),
                    wikidata_id: Some("Q1136".to_string()),
                    official_api_documentation: Some("https://www.reddit.com/dev/api".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Reddit {
    fn default() -> Self {
        Self::new()
    }
}

/// Whether a `thumbnail` value denotes a real image URL (has both a network
/// location and a path), mirroring the reference `urlparse` netloc/path check.
/// Reddit's non-image sentinels (`self`, `default`, `nsfw`, `""`, ...) have no
/// host and fall through to the text branch.
fn is_image_thumbnail(thumbnail: &str) -> bool {
    match Url::parse(thumbnail) {
        Ok(u) => {
            let has_host = u.host_str().map(|h| !h.is_empty()).unwrap_or(false);
            let has_path = !u.path().is_empty() && u.path() != "/";
            has_host && has_path
        }
        Err(_) => false,
    }
}

/// Join a permalink against the Reddit base URL, mirroring `urljoin`.
fn join_permalink(permalink: &str) -> String {
    match Url::parse(BASE_URL).and_then(|base| base.join(permalink)) {
        Ok(u) => u.to_string(),
        Err(_) => permalink.to_string(),
    }
}

impl Engine for Reddit {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> =
            vec![("q", q.query.clone()), ("limit", PAGE_SIZE.to_string())];
        p.url = Some(format!("{SEARCH_URL}?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Reddit JSON: {e}")))?;

        let Some(data) = value.get("data") else {
            return Ok(res);
        };

        let posts = data
            .get("children")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        let mut img_results: Vec<Result_> = Vec::new();
        let mut text_results: Vec<Result_> = Vec::new();

        for post in &posts {
            let Some(data) = post.get("data") else {
                continue;
            };
            let permalink = data.get("permalink").and_then(|p| p.as_str()).unwrap_or("");
            let title = data
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();
            let url = join_permalink(permalink);
            let thumbnail = data.get("thumbnail").and_then(|t| t.as_str()).unwrap_or("");

            if is_image_thumbnail(thumbnail) {
                let img_src = data
                    .get("url")
                    .and_then(|u| u.as_str())
                    .unwrap_or("")
                    .to_string();
                img_results.push(Result_::Image(Image {
                    url: url.clone(),
                    normalized_url: url,
                    title,
                    img_src,
                    thumbnail_src: thumbnail.to_string(),
                    engine: NAME.to_string(),
                    ..Image::default()
                }));
            } else {
                let selftext = data.get("selftext").and_then(|s| s.as_str()).unwrap_or("");
                let content = if selftext.chars().count() > 500 {
                    let truncated: String = selftext.chars().take(500).collect();
                    format!("{truncated}...")
                } else {
                    selftext.to_string()
                };
                text_results.push(Result_::Main(MainResult {
                    url: url.clone(),
                    normalized_url: url,
                    title,
                    content,
                    engine: NAME.to_string(),
                    ..MainResult::default()
                }));
            }
        }

        for r in img_results.into_iter().chain(text_results) {
            res.add(r);
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

    fn query(q: &str) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno: 1,
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
      "data": {
        "children": [
          {
            "data": {
              "permalink": "/r/rust/comments/1/text_post/",
              "title": "A text post",
              "thumbnail": "self",
              "selftext": "Some discussion text",
              "url": "https://www.reddit.com/r/rust/comments/1/text_post/",
              "created_utc": 1700000000
            }
          },
          {
            "data": {
              "permalink": "/r/pics/comments/2/an_image/",
              "title": "An image post",
              "thumbnail": "https://b.thumbs.redditmedia.com/abc.jpg",
              "selftext": "",
              "url": "https://i.redd.it/abc.jpg",
              "created_utc": 1700000100
            }
          }
        ]
      }
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(Result_::Image(Image {
            url: "https://www.reddit.com/r/pics/comments/2/an_image/".to_string(),
            normalized_url: "https://www.reddit.com/r/pics/comments/2/an_image/".to_string(),
            title: "An image post".to_string(),
            img_src: "https://i.redd.it/abc.jpg".to_string(),
            thumbnail_src: "https://b.thumbs.redditmedia.com/abc.jpg".to_string(),
            engine: NAME.to_string(),
            ..Image::default()
        }));
        basic.add(Result_::Main(MainResult {
            url: "https://www.reddit.com/r/rust/comments/1/text_post/".to_string(),
            normalized_url: "https://www.reddit.com/r/rust/comments/1/text_post/".to_string(),
            title: "A text post".to_string(),
            content: "Some discussion text".to_string(),
            engine: NAME.to_string(),
            ..MainResult::default()
        }));
        Fixture::capture(NAME, query("rust"), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        Fixture::capture(
            NAME,
            query("rust"),
            response(200, r#"{"error": 404}"#),
            EngineResults::new(),
        )
        .with_case("no-data")
        .save(dir.join("no-data.json"))
        .unwrap();

        let q = query("rust");
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{SEARCH_URL}?q=rust&limit=25"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"data":{"children":[]}}"#),
            EngineResults::new(),
        )
        .with_case("request")
        .with_golden_request(golden)
        .save(dir.join("request.json"))
        .unwrap();
    }

    #[test]
    fn reddit_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Reddit::new();
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
    fn classifies_thumbnails() {
        assert!(is_image_thumbnail(
            "https://b.thumbs.redditmedia.com/abc.jpg"
        ));
        assert!(!is_image_thumbnail("self"));
        assert!(!is_image_thumbnail("default"));
        assert!(!is_image_thumbnail(""));
    }

    #[test]
    fn truncates_long_selftext() {
        let engine = Reddit::new();
        let long = "x".repeat(600);
        let body = format!(
            r#"{{"data":{{"children":[{{"data":{{"permalink":"/r/a/1/","title":"t","thumbnail":"self","selftext":"{long}","url":"u"}}}}]}}}}"#
        );
        let res = engine.response(&response(200, &body)).expect("parse ok");
        if let Result_::Main(r) = &res.results[0] {
            assert_eq!(r.content.chars().count(), 503);
            assert!(r.content.ends_with("..."));
        } else {
            panic!("expected main result");
        }
    }
}
