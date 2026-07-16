//! Genius engine: lyrics/song/artist/album search via the unofficial
//! `genius.com/api/search/multi` endpoint.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "genius";

const SEARCH_URL: &str = "https://genius.com/api/search/multi?";
const PAGE_SIZE: u32 = 5;

#[derive(Debug, Clone)]
pub struct Genius {
    meta: EngineMeta,
}

impl Genius {
    pub fn new() -> Self {
        Genius {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["music".to_string(), "lyrics".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "gen".to_string(),
                about: About {
                    website: Some("https://genius.com/".to_string()),
                    wikidata_id: Some("Q3419343".to_string()),
                    official_api_documentation: Some("https://docs.genius.com/".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Genius {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for Genius {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![("q", q.query.clone())];
        p.url = Some(format!(
            "{SEARCH_URL}{}&page={}&per_page={PAGE_SIZE}",
            encode_query(&args),
            p.pageno
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Genius JSON: {e}")))?;

        let sections = value
            .pointer("/response/sections")
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();

        for section in &sections {
            let hits = section
                .get("hits")
                .and_then(|h| h.as_array())
                .cloned()
                .unwrap_or_default();

            for hit in &hits {
                let Some(hit_type) = hit.get("type").and_then(|t| t.as_str()) else {
                    continue;
                };
                let Some(result) = parse_hit(hit_type, hit) else {
                    continue;
                };
                res.add(Result_::Main(result));
            }
        }

        Ok(res)
    }
}

fn parse_hit(hit_type: &str, hit: &serde_json::Value) -> Option<MainResult> {
    match hit_type {
        "lyric" | "song" => parse_lyric(hit),
        "artist" => parse_artist(hit),
        "album" => parse_album(hit),
        _ => None,
    }
}

fn parse_lyric(hit: &serde_json::Value) -> Option<MainResult> {
    let result = hit.get("result")?;
    let content = hit
        .get("highlights")
        .and_then(|h| h.as_array())
        .filter(|a| !a.is_empty())
        .and_then(|a| a[0].get("value"))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .or_else(|| {
            result
                .get("title_with_featured")
                .and_then(|t| t.as_str())
                .map(str::to_string)
        })
        .unwrap_or_default();

    let url = result.get("url").and_then(|u| u.as_str())?.to_string();
    let title = result
        .get("full_title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Some(MainResult {
        url: url.clone(),
        normalized_url: url,
        title,
        content,
        engine: NAME.to_string(),
        ..MainResult::default()
    })
}

fn parse_artist(hit: &serde_json::Value) -> Option<MainResult> {
    let result = hit.get("result")?;
    let url = result.get("url").and_then(|u| u.as_str())?.to_string();
    let title = result
        .get("name")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    Some(MainResult {
        url: url.clone(),
        normalized_url: url,
        title,
        content: String::new(),
        engine: NAME.to_string(),
        ..MainResult::default()
    })
}

fn parse_album(hit: &serde_json::Value) -> Option<MainResult> {
    let result = hit.get("result")?;
    let url = result.get("url").and_then(|u| u.as_str())?.to_string();
    let title = result
        .get("full_title")
        .and_then(|t| t.as_str())
        .unwrap_or("")
        .to_string();

    let mut content = result
        .get("name_with_artist")
        .and_then(|v| v.as_str())
        .or_else(|| result.get("name").and_then(|v| v.as_str()))
        .unwrap_or("")
        .to_string();
    if let Some(year) = result
        .pointer("/release_date_components/year")
        .and_then(|y| y.as_i64())
    {
        content = format!("{year} / {content}");
    }

    Some(MainResult {
        url: url.clone(),
        normalized_url: url,
        title,
        content: content.trim().to_string(),
        engine: NAME.to_string(),
        ..MainResult::default()
    })
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

    fn main_result(url: &str, title: &str, content: &str) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            engine: NAME.to_string(),
            ..MainResult::default()
        })
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
      "response": {
        "sections": [
          {
            "type": "song",
            "hits": [
              {
                "type": "song",
                "highlights": [],
                "result": {
                  "url": "https://genius.com/Artist-song-lyrics",
                  "full_title": "Song by Artist",
                  "title_with_featured": "Song"
                }
              }
            ]
          },
          {
            "type": "artist",
            "hits": [
              {
                "type": "artist",
                "result": {
                  "url": "https://genius.com/artists/Artist",
                  "name": "Artist"
                }
              }
            ]
          }
        ]
      }
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://genius.com/Artist-song-lyrics",
            "Song by Artist",
            "Song",
        ));
        basic.add(main_result(
            "https://genius.com/artists/Artist",
            "Artist",
            "",
        ));
        Fixture::capture(NAME, query("song", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        Fixture::capture(
            NAME,
            query("song", 1),
            response(200, r#"{"response":{"sections":[]}}"#),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();

        let q = query("song", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{SEARCH_URL}q=song&page=2&per_page=5"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"response":{"sections":[]}}"#),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn genius_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Genius::new();
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
    fn album_hit_includes_release_year() {
        let hit = serde_json::json!({
            "type": "album",
            "result": {
                "url": "https://genius.com/albums/Artist/Album",
                "full_title": "Album by Artist",
                "name_with_artist": "Album by Artist",
                "release_date_components": {"year": 2020}
            }
        });
        let result = parse_hit("album", &hit).unwrap();
        assert_eq!(result.content, "2020 / Album by Artist");
    }
}
