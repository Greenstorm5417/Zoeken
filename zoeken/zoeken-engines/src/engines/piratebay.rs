//! The Pirate Bay (via apibay.org) torrent search engine.

use serde::Deserialize;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{FileResult, Result_};

use super::util::encode_component;

/// Engine name / identifier.
pub const NAME: &str = "piratebay";

/// Torrent detail page base.
const URL: &str = "https://thepiratebay.org/";

/// JSON search API.
const SEARCH_URL: &str = "https://apibay.org/q.php";

/// Default trackers appended to constructed magnet links.
const TRACKERS: &[&str] = &[
    "udp://tracker.coppersurfer.tk:6969/announce",
    "udp://9.rarbg.to:2920/announce",
    "udp://tracker.opentrackr.org:1337",
    "udp://tracker.internetwarriors.net:1337/announce",
    "udp://tracker.leechers-paradise.org:6969/announce",
    "udp://tracker.coppersurfer.tk:6969/announce",
    "udp://tracker.pirateparty.gr:6969/announce",
    "udp://tracker.cyberia.is:6969/announce",
];

/// The Pirate Bay engine.
#[derive(Debug, Clone)]
pub struct Piratebay {
    meta: EngineMeta,
}

impl Piratebay {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Piratebay {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["files".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "tpb".to_string(),
                about: About {
                    website: Some("https://thepiratebay.org".to_string()),
                    wikidata_id: Some("Q22663".to_string()),
                    official_api_documentation: Some("https://apibay.org/".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }

    /// Map a search category to the apibay `cat` code (`files` = 0).
    fn search_type(categories: &[String]) -> &'static str {
        if categories.iter().any(|c| c == "music") {
            "100"
        } else if categories.iter().any(|c| c == "videos") {
            "200"
        } else {
            "0"
        }
    }
}

impl Default for Piratebay {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize)]
struct Torrent {
    id: String,
    name: String,
    info_hash: String,
    seeders: String,
    leechers: String,
    size: String,
    added: String,
}

impl Engine for Piratebay {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let search_type = Self::search_type(&q.categories);
        p.url = Some(format!(
            "{SEARCH_URL}?q={}&cat={search_type}",
            encode_component(&q.query)
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let torrents: Vec<Torrent> = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid apibay JSON: {e}")))?;

        if torrents.len() == 1 && torrents[0].name == "No results returned" {
            return Ok(res);
        }

        let mut items: Vec<(i64, FileResult)> = torrents
            .into_iter()
            .map(|t| {
                let link = format!("{URL}description.php?id={}", t.id);
                let magnetlink = format!(
                    "magnet:?xt=urn:btih:{}&dn={}&tr={}",
                    t.info_hash,
                    t.name,
                    TRACKERS.join("&tr=")
                );
                let seed: i64 = t.seeders.parse().unwrap_or(0);
                let leech: i64 = t.leechers.parse().unwrap_or(0);
                let time = t
                    .added
                    .parse::<i64>()
                    .ok()
                    .map(|secs| secs.to_string())
                    .unwrap_or_default();
                let filesize = humanize_bytes(t.size.parse().unwrap_or(0));
                (
                    seed,
                    FileResult {
                        url: link.clone(),
                        normalized_url: link,
                        title: t.name.clone(),
                        filename: t.name,
                        engine: NAME.to_string(),
                        seed: Some(seed),
                        leech: Some(leech),
                        magnetlink: Some(magnetlink),
                        filesize: Some(filesize),
                        time,
                        ..FileResult::default()
                    },
                )
            })
            .collect();

        items.sort_by_key(|item| std::cmp::Reverse(item.0));

        for (_, item) in items {
            res.add(Result_::File(item));
        }

        Ok(res)
    }
}

/// Render a byte count as a human-readable size (binary units, one decimal).
fn humanize_bytes(bytes: u64) -> String {
    const UNITS: &[&str] = &["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut value = bytes as f64;
    let mut unit = 0;
    while value >= 1024.0 && unit < UNITS.len() - 1 {
        value /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} {}", UNITS[0])
    } else {
        format!("{value:.1} {}", UNITS[unit])
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

    const BASIC_JSON: &str = r#"[
      {
        "id": "1",
        "name": "debian-12.5.0-amd64-netinst.iso",
        "info_hash": "abcdef0123456789",
        "seeders": "42",
        "leechers": "3",
        "size": "660602880",
        "added": "1700000000"
      }
    ]"#;

    fn expected_file() -> FileResult {
        FileResult {
            url: "https://thepiratebay.org/description.php?id=1".to_string(),
            normalized_url: "https://thepiratebay.org/description.php?id=1".to_string(),
            title: "debian-12.5.0-amd64-netinst.iso".to_string(),
            filename: "debian-12.5.0-amd64-netinst.iso".to_string(),
            engine: NAME.to_string(),
            seed: Some(42),
            leech: Some(3),
            magnetlink: Some(format!(
                "magnet:?xt=urn:btih:abcdef0123456789&dn=debian-12.5.0-amd64-netinst.iso&tr={}",
                TRACKERS.join("&tr=")
            )),
            filesize: Some("630.0 MiB".to_string()),
            time: "1700000000".to_string(),
            ..FileResult::default()
        }
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(Result_::File(expected_file()));
        Fixture::capture(NAME, query("debian"), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();
    }

    #[test]
    fn piratebay_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Piratebay::new();
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
    fn parses_torrent_fields() {
        let engine = Piratebay::new();
        let res = engine.response(&response(200, BASIC_JSON)).unwrap();
        assert_eq!(res.results.len(), 1);
        if let Result_::File(f) = &res.results[0] {
            assert_eq!(f, &expected_file());
        } else {
            panic!("expected a file result");
        }
    }

    #[test]
    fn handles_no_results_marker() {
        let engine = Piratebay::new();
        let res = engine
            .response(&response(200, r#"[{"id":"0","name":"No results returned","info_hash":"0000000000000000000000000000000000000000","seeders":"0","leechers":"0","size":"0","added":"0"}]"#))
            .unwrap();
        assert!(res.is_empty());
    }
}
