//! SoundCloud engine: queries JSON API for tracks and playlists.
//!
//! Requires guest client_id injected via engine_data; builds locale-mapped app_locale parameter.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "soundcloud";

const SEARCH_URL: &str = "https://api-v2.soundcloud.com/search";

const PAGE_SIZE: u32 = 10;

const FACET: &str = "model";

pub const CLIENT_ID_KEY: &str = "client_id";

/// The SoundCloud engine.
#[derive(Debug, Clone)]
pub struct Soundcloud {
    meta: EngineMeta,
}

impl Soundcloud {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Soundcloud {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["music".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: true,
                weight: 1,
                shortcut: "sc".to_string(),
                about: About {
                    website: Some("https://soundcloud.com".to_string()),
                    wikidata_id: Some("Q568769".to_string()),
                    official_api_documentation: Some(
                        "https://developers.soundcloud.com/docs/api/guide".to_string(),
                    ),
                    use_official_api: false,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Soundcloud {
    fn default() -> Self {
        Self::new()
    }
}

/// Map a Upstream locale to the SoundCloud `app_locale`, mirroring the
/// reference `app_locale_map`. The map is keyed by the language subtag before
/// the first `-` (e.g. `pt-BR` -> `pt`), and unknown subtags (and `all`/empty)
/// resolve to `en`.
fn resolve_app_locale(locale: &str) -> &'static str {
    if locale.is_empty() || locale == "all" {
        return "en";
    }
    let lang = locale.split('-').next().unwrap_or("en");
    match lang {
        "de" => "de",
        "en" => "en",
        "es" => "es",
        "fr" | "oc" => "fr",
        "it" => "it",
        "nl" => "nl",
        "pl" | "szl" => "pl",
        "pt" | "pap" => "pt_BR",
        "sv" => "sv",
        _ => "en",
    }
}

impl Engine for Soundcloud {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        let offset = (p.pageno.saturating_sub(1)) * PAGE_SIZE;
        let client_id = p
            .engine_data
            .get(CLIENT_ID_KEY)
            .cloned()
            .unwrap_or_default();

        let args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            ("offset", offset.to_string()),
            ("limit", PAGE_SIZE.to_string()),
            ("facet", FACET.to_string()),
            ("client_id", client_id),
            ("app_locale", resolve_app_locale(&q.locale).to_string()),
        ];

        p.method = HttpMethod::Get;
        p.url = Some(format!("{SEARCH_URL}?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid SoundCloud JSON: {e}")))?;

        let collection = value
            .get("collection")
            .and_then(|c| c.as_array())
            .cloned()
            .unwrap_or_default();

        for entry in &collection {
            let kind = entry.get("kind").and_then(|k| k.as_str()).unwrap_or("");
            if kind != "track" && kind != "playlist" {
                continue;
            }

            let url = entry
                .get("permalink_url")
                .and_then(|u| u.as_str())
                .unwrap_or("");
            if url.is_empty() {
                continue;
            }

            let title = entry
                .get("title")
                .and_then(|t| t.as_str())
                .unwrap_or("")
                .to_string();

            let description = entry.get("description").and_then(|d| d.as_str());
            let label_name = entry.get("label_name").and_then(|l| l.as_str());
            let content = [description, label_name]
                .into_iter()
                .flatten()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" / ");

            res.add(Result_::Main(MainResult {
                url: url.to_string(),
                normalized_url: url.to_string(),
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

    fn query(q: &str, pageno: u32, locale: &str) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno,
            locale: locale.to_string(),
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

    // A track and a playlist are kept; a `user` kind is skipped; a track
    // missing `permalink_url` is skipped.
    const BASIC_JSON: &str = r#"{
      "collection": [
        {
          "kind": "track",
          "title": "Cool Track",
          "permalink_url": "https://soundcloud.com/artist/cool-track",
          "description": "A cool track",
          "label_name": "Cool Records"
        },
        {
          "kind": "playlist",
          "title": "Chill Playlist",
          "permalink_url": "https://soundcloud.com/artist/chill-playlist",
          "description": "Chill vibes"
        },
        {
          "kind": "user",
          "title": "Some Artist",
          "permalink_url": "https://soundcloud.com/artist"
        },
        {
          "kind": "track",
          "title": "No URL Track",
          "description": "This track has no permalink_url"
        }
      ],
      "total_results": 4
    }"#;

    // Empty collection -> no results.
    const EMPTY_JSON: &str = r#"{"collection":[],"total_results":0}"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        // basic: a track + a playlist kept; a `user` kind and a permalink-less
        // track skipped.
        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://soundcloud.com/artist/cool-track",
            "Cool Track",
            "A cool track / Cool Records",
        ));
        basic.add(main_result(
            "https://soundcloud.com/artist/chill-playlist",
            "Chill Playlist",
            "Chill vibes",
        ));
        Fixture::capture(
            NAME,
            query("cool", 1, "all"),
            response(200, BASIC_JSON),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        // empty: empty collection -> no results.
        Fixture::capture(
            NAME,
            query("nothing", 1, "all"),
            response(200, EMPTY_JSON),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();

        // request-page2: validates the built API URL and parameter order.
        let q = query("cool", 2, "all");
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{SEARCH_URL}?q=cool&offset=10&limit=10&facet=model&client_id=&app_locale=en"
        ));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, EMPTY_JSON),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn soundcloud_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Soundcloud::new();
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
    fn builds_paged_request_url() {
        let engine = Soundcloud::new();
        let q = query("cool", 2, "de-DE");
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some(
                "https://api-v2.soundcloud.com/search?q=cool&offset=10&limit=10\
                 &facet=model&client_id=&app_locale=de"
            )
        );
    }

    #[test]
    fn resolves_app_locale_from_locale() {
        assert_eq!(resolve_app_locale("de-DE"), "de");
        assert_eq!(resolve_app_locale("all"), "en");
        assert_eq!(resolve_app_locale(""), "en");
        assert_eq!(resolve_app_locale("oc"), "fr");
        assert_eq!(resolve_app_locale("szl"), "pl");
        assert_eq!(resolve_app_locale("pt-BR"), "pt_BR");
        assert_eq!(resolve_app_locale("pap"), "pt_BR");
        assert_eq!(resolve_app_locale("zh"), "en");
    }
}
