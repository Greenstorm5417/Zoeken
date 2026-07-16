//! Photon (Komoot) map geocoding engine: queries the Photon JSON API and returns map results.
//!
//! Builds OpenStreetMap URLs from the returned `osm_type`/`osm_id`, restricts the `lang`
//! parameter to Photon's supported language set, and skips entries without a name.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "photon";

const BASE_URL: &str = "https://photon.komoot.io/";
const PAGE_SIZE: u32 = 10;

/// Languages Photon has dedicated name translations for.
const SUPPORTED_LANGUAGES: &[&str] = &["de", "en", "fr", "it"];

/// The Photon engine.
#[derive(Debug, Clone)]
pub struct Photon {
    meta: EngineMeta,
}

impl Photon {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Photon {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["map".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: true,
                weight: 1,
                shortcut: "ph".to_string(),
                about: About {
                    website: Some("https://photon.komoot.io".to_string()),
                    wikidata_id: None,
                    official_api_documentation: Some("https://photon.komoot.io/".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Photon {
    fn default() -> Self {
        Self::new()
    }
}

fn osm_type_letter(letter: &str) -> Option<&'static str> {
    match letter {
        "N" => Some("node"),
        "W" => Some("way"),
        "R" => Some("relation"),
        _ => None,
    }
}

impl Engine for Photon {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let query = encode_query(&[("q", q.query.clone())]);
        let mut url = format!("{BASE_URL}api/?{query}&limit={PAGE_SIZE}");

        if !q.locale.is_empty() && q.locale != "all" {
            let lang = q.locale.split('_').next().unwrap_or("");
            let lang = lang.split('-').next().unwrap_or(lang);
            if SUPPORTED_LANGUAGES.contains(&lang) {
                url.push_str("&lang=");
                url.push_str(lang);
            }
        }

        p.url = Some(url);
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Photon JSON: {e}")))?;

        let features = value
            .get("features")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for feature in &features {
            let Some(properties) = feature.get("properties") else {
                continue;
            };

            let Some(title) = properties
                .get("name")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
            else {
                continue;
            };

            let Some(osm_type) = properties
                .get("osm_type")
                .and_then(|v| v.as_str())
                .and_then(osm_type_letter)
            else {
                continue;
            };

            let osm_id = properties
                .get("osm_id")
                .map(|v| match v {
                    serde_json::Value::String(s) => s.clone(),
                    serde_json::Value::Number(n) => n.to_string(),
                    _ => String::new(),
                })
                .unwrap_or_default();

            let url = format!("https://openstreetmap.org/{osm_type}/{osm_id}");

            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title: title.to_string(),
                content: String::new(),
                engine: NAME.to_string(),
                ..MainResult::default()
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

    fn query(q: &str) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        }
    }

    fn main_result(url: &str, title: &str) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            content: String::new(),
            engine: NAME.to_string(),
            ..MainResult::default()
        })
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
      "features": [
        {
          "properties": {
            "osm_type": "N",
            "osm_id": 123,
            "name": "Cafe Central"
          }
        },
        {
          "properties": {
            "osm_type": "W",
            "osm_id": 456,
            "name": "Hauptstrasse"
          }
        },
        {
          "properties": {
            "osm_id": 789
          }
        }
      ]
    }"#;

    const EMPTY_JSON: &str = r#"{"features": []}"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://openstreetmap.org/node/123",
            "Cafe Central",
        ));
        basic.add(main_result(
            "https://openstreetmap.org/way/456",
            "Hauptstrasse",
        ));
        Fixture::capture(NAME, query("cafe"), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        Fixture::capture(
            NAME,
            query("nothing"),
            response(200, EMPTY_JSON),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();

        let mut q = query("berlin cafe");
        q.locale = "de".to_string();
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{BASE_URL}api/?q=berlin+cafe&limit={PAGE_SIZE}&lang=de"
        ));
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
    fn photon_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Photon::new();
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
    fn builds_request_url_with_language() {
        let engine = Photon::new();
        let mut q = query("berlin cafe");
        q.locale = "de".to_string();
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://photon.komoot.io/api/?q=berlin+cafe&limit=10&lang=de")
        );
    }

    #[test]
    fn omits_language_for_all_locale() {
        let engine = Photon::new();
        let q = query("berlin cafe");
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://photon.komoot.io/api/?q=berlin+cafe&limit=10")
        );
    }
}
