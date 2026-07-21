//! OpenStreetMap (Nominatim) engine: queries the JSON API for places and returns map results.
//!
//! Derives titles from category and type, builds OSM URLs, and filters results with no title.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "openstreetmap";

const BASE_URL: &str = "https://nominatim.openstreetmap.org/";

const SEARCH_SUFFIX: &str =
    "&polygon_geojson=1&format=jsonv2&addressdetails=1&extratags=1&dedupe=1";

/// The OpenStreetMap engine.
#[derive(Debug, Clone)]
pub struct Openstreetmap {
    meta: EngineMeta,
}

impl Openstreetmap {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Openstreetmap {
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
                shortcut: "osm".to_string(),
                about: About {
                    website: Some("https://www.openstreetmap.org/".to_string()),
                    wikidata_id: Some("Q936".to_string()),
                    official_api_documentation: Some(
                        "http://wiki.openstreetmap.org/wiki/Nominatim".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Openstreetmap {
    fn default() -> Self {
        Self::new()
    }
}

fn scalar_to_string(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.to_string(),
        _ => String::new(),
    }
}

fn get_title(result: &serde_json::Value) -> Option<String> {
    let empty = serde_json::Value::Object(serde_json::Map::new());
    let address = result.get("address").unwrap_or(&empty);
    let category = result
        .get("category")
        .and_then(|c| c.as_str())
        .unwrap_or("");
    let type_ = result.get("type").and_then(|t| t.as_str()).unwrap_or("");

    let address_name: Option<String> =
        if matches!(category, "amenity" | "shop" | "tourism" | "leisure") {
            if let Some(a29) = address.get("address29").and_then(|v| v.as_str()) {
                Some(a29.to_string())
            } else {
                address
                    .get(category)
                    .and_then(|v| v.as_str())
                    .map(|s| s.to_string())
            }
        } else if address.get(type_).is_some() {
            address
                .get(type_)
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        } else {
            None
        };

    let title = match address_name {
        Some(name) => Some(name),
        None => result
            .get("display_name")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string()),
    };

    title.filter(|t| !t.is_empty())
}

fn get_url(result: &serde_json::Value) -> String {
    let lat = result.get("lat").map(scalar_to_string).unwrap_or_default();
    let lon = result.get("lon").map(scalar_to_string).unwrap_or_default();
    let osm_type = result
        .get("osm_type")
        .or_else(|| result.get("type"))
        .map(scalar_to_string)
        .unwrap_or_default();

    let mut url = match result.get("osm_id") {
        Some(osm_id) => {
            let osm_id = scalar_to_string(osm_id);
            format!("https://openstreetmap.org/{osm_type}/{osm_id}")
        }
        None => format!("https://www.openstreetmap.org/?mlat={lat}&mlon={lon}&zoom=12&layers=M"),
    };
    // SPA map canvas reads mlat/mlon from the result URL.
    if !lat.is_empty() && !lon.is_empty() && !url.contains("mlat=") {
        let sep = if url.contains('?') { '&' } else { '?' };
        url = format!("{url}{sep}mlat={lat}&mlon={lon}");
    }
    url
}

impl Engine for Openstreetmap {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let query = encode_query(&[("q", q.query.clone())]);
        p.url = Some(format!("{BASE_URL}search?{query}{SEARCH_SUFFIX}"));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Nominatim JSON: {e}")))?;

        let places = value.as_array().cloned().unwrap_or_default();

        for place in &places {
            let Some(title) = get_title(place) else {
                continue;
            };

            let url = get_url(place);
            let content = place
                .get("display_name")
                .and_then(|v| v.as_str())
                .filter(|s| *s != title.as_str())
                .unwrap_or("")
                .to_string();

            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title,
                content,
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

    const BASIC_JSON: &str = r#"[
      {
        "osm_type": "node",
        "osm_id": 1234,
        "category": "amenity",
        "type": "cafe",
        "lat": "48.1",
        "lon": "11.5",
        "display_name": "Cafe Central, Munich, Germany",
        "address": {
          "amenity": "Cafe Central",
          "road": "Hauptstrasse",
          "city": "Munich",
          "country": "Germany"
        }
      },
      {
        "type": "administrative",
        "lat": "51.5",
        "lon": "-0.12",
        "display_name": "London, EC1M 5RF, United Kingdom",
        "address": {
          "administrative": "London",
          "postcode": "EC1M 5RF",
          "country": "United Kingdom"
        }
      },
      {
        "osm_type": "way",
        "osm_id": 98765,
        "category": "highway",
        "type": "residential",
        "lat": "40.0",
        "lon": "-3.0",
        "address": {
          "road": "Some Road"
        }
      }
    ]"#;

    const EMPTY_JSON: &str = r#"[]"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(Result_::Main(MainResult {
            url: "https://openstreetmap.org/node/1234?mlat=48.1&mlon=11.5".to_string(),
            normalized_url: "https://openstreetmap.org/node/1234?mlat=48.1&mlon=11.5".to_string(),
            title: "Cafe Central".to_string(),
            content: "Cafe Central, Munich, Germany".to_string(),
            engine: NAME.to_string(),
            ..MainResult::default()
        }));
        basic.add(Result_::Main(MainResult {
            url: "https://www.openstreetmap.org/?mlat=51.5&mlon=-0.12&zoom=12&layers=M".to_string(),
            normalized_url: "https://www.openstreetmap.org/?mlat=51.5&mlon=-0.12&zoom=12&layers=M"
                .to_string(),
            title: "London".to_string(),
            content: "London, EC1M 5RF, United Kingdom".to_string(),
            engine: NAME.to_string(),
            ..MainResult::default()
        }));
        Fixture::capture(NAME, query("cafe"), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        // empty: empty array -> no results.
        Fixture::capture(
            NAME,
            query("nothing"),
            response(200, EMPTY_JSON),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();

        // request: validates the built Nominatim URL.
        let q = query("berlin cafe");
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}search?q=berlin+cafe{SEARCH_SUFFIX}"));
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
    fn openstreetmap_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Openstreetmap::new();
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
    fn builds_request_url() {
        let engine = Openstreetmap::new();
        let q = query("berlin cafe");
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some(
                "https://nominatim.openstreetmap.org/search?q=berlin+cafe\
                 &polygon_geojson=1&format=jsonv2&addressdetails=1&extratags=1&dedupe=1"
            )
        );
    }
}
