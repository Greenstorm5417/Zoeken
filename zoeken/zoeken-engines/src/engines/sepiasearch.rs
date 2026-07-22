//! SepiaSearch engine.
//!
//! Queries the PeerTube-compatible video API and maps each data entry into a
//! main result.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SafeSearch, SearchQueryView, TimeRange,
};
use zoeken_results::{MainResult, Result_, Template};

use super::util::encode_query;

pub const NAME: &str = "sepiasearch";

const BASE_URL: &str = "https://sepiasearch.org";

#[derive(Debug, Clone)]
pub struct SepiaSearch {
    meta: EngineMeta,
}

impl SepiaSearch {
    pub fn new() -> Self {
        SepiaSearch {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["videos".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: true,
                safesearch: true,
                language_support: true,
                weight: 1,
                shortcut: "sep".to_string(),
                about: About {
                    website: Some("https://sepiasearch.org".to_string()),
                    wikidata_id: None,
                    official_api_documentation: Some(
                        "https://docs.joinpeertube.org/api-rest-reference.html".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for SepiaSearch {
    fn default() -> Self {
        Self::new()
    }
}

fn nsfw_flag(safesearch: SafeSearch) -> &'static str {
    match safesearch {
        SafeSearch::Off => "both",
        SafeSearch::Moderate | SafeSearch::Strict => "false",
    }
}

/// The `startDate` ISO date for a time range.
fn start_date(time_range: TimeRange) -> String {
    use chrono::{Duration, Local, Months};
    let now = Local::now().date_naive();
    let date = match time_range {
        TimeRange::Day => now,
        TimeRange::Week => now - Duration::weeks(1),
        TimeRange::Month => now.checked_sub_months(Months::new(1)).unwrap_or(now),
        TimeRange::Year => now.checked_sub_months(Months::new(12)).unwrap_or(now),
    };
    date.format("%Y-%m-%d").to_string()
}

impl Engine for SepiaSearch {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        if q.query.is_empty() {
            p.url = None;
            return;
        }
        p.method = HttpMethod::Get;

        let start = p.pageno.saturating_sub(1) * 10;
        let args: Vec<(&str, String)> = vec![
            ("search", q.query.clone()),
            ("start", start.to_string()),
            ("count", "10".to_string()),
            ("sort", "-match".to_string()),
            ("nsfw", nsfw_flag(q.safesearch).to_string()),
        ];
        let mut url = format!("{BASE_URL}/api/v1/search/videos?{}", encode_query(&args));

        if let Some(time_range) = p.time_range {
            url.push_str(&format!("&startDate={}", start_date(time_range)));
        }

        p.url = Some(url);
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid SepiaSearch JSON: {e}")))?;

        let Some(data) = value.get("data").and_then(|d| d.as_array()) else {
            return Ok(res);
        };

        for item in data {
            let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("");
            let title = item
                .get("name")
                .and_then(|n| n.as_str())
                .unwrap_or("")
                .to_string();
            let description = item
                .get("description")
                .and_then(|d| d.as_str())
                .unwrap_or("");
            let content = zoeken_engine_core::html_to_text(description);
            let thumbnail = peertube_style_thumbnail(item, url);
            let iframe_src = peertube_style_embed(item, url);
            let length = item
                .get("duration")
                .and_then(|v| v.as_u64())
                .map(super::util::format_duration_secs)
                .unwrap_or_default();
            let author = peertube_author(item);
            let published_date = item
                .get("publishedAt")
                .and_then(|v| v.as_str())
                .filter(|s| !s.is_empty())
                .map(str::to_string);

            res.add(Result_::Main(MainResult {
                url: url.to_string(),
                normalized_url: url.to_string(),
                title,
                content,
                engine: NAME.to_string(),
                template: Template::Videos,
                thumbnail,
                iframe_src,
                length,
                author,
                published_date,
                ..MainResult::default()
            }));
        }

        Ok(res)
    }
}

fn peertube_author(item: &serde_json::Value) -> String {
    item.get("account")
        .and_then(|a| a.get("displayName").or_else(|| a.get("name")))
        .and_then(|v| v.as_str())
        .or_else(|| {
            item.get("channel")
                .and_then(|c| c.get("displayName").or_else(|| c.get("name")))
                .and_then(|v| v.as_str())
        })
        .unwrap_or("")
        .to_string()
}

fn peertube_style_embed(item: &serde_json::Value, video_url: &str) -> String {
    if let Some(embed) = item
        .get("embedUrl")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
    {
        return embed.to_string();
    }
    let Ok(base) = url::Url::parse(video_url) else {
        return String::new();
    };
    let path = base.path();
    let uuid = path
        .strip_prefix("/w/")
        .or_else(|| path.strip_prefix("/videos/watch/"))
        .unwrap_or("")
        .trim_matches('/');
    if uuid.is_empty() {
        return String::new();
    }
    format!(
        "{}://{}{}/videos/embed/{}",
        base.scheme(),
        base.host_str().unwrap_or(""),
        base.port().map(|p| format!(":{p}")).unwrap_or_default(),
        uuid
    )
}

fn peertube_style_thumbnail(item: &serde_json::Value, video_url: &str) -> String {
    for key in ["thumbnailUrl", "previewUrl"] {
        if let Some(abs) = item
            .get(key)
            .and_then(|v| v.as_str())
            .filter(|s| !s.is_empty())
        {
            return abs.to_string();
        }
    }
    let path = item
        .get("thumbnailPath")
        .or_else(|| item.get("previewPath"))
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if path.is_empty() {
        return String::new();
    }
    if path.starts_with("http://") || path.starts_with("https://") {
        return path.to_string();
    }
    if let Ok(base) = url::Url::parse(video_url)
        && let Ok(joined) = base.join(path)
    {
        return joined.to_string();
    }
    String::new()
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

    fn main_result(
        url: &str,
        title: &str,
        content: &str,
        length: &str,
        published: &str,
        iframe_src: &str,
    ) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            engine: NAME.to_string(),
            template: Template::Videos,
            iframe_src: iframe_src.to_string(),
            length: length.to_string(),
            published_date: Some(published.to_string()),
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
      "total": 2,
      "data": [
        {
          "url": "https://framatube.org/w/abc",
          "name": "Intro to PeerTube",
          "description": "A <b>short</b> introduction.",
          "views": 1200,
          "duration": 615,
          "publishedAt": "2021-01-02T03:04:05.000Z"
        },
        {
          "url": "https://tube.example/w/xyz",
          "name": "Rust in 100 seconds",
          "description": "Quick overview.",
          "views": 9001,
          "duration": 100,
          "publishedAt": "2022-02-03T00:00:00.000Z"
        }
      ]
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://framatube.org/w/abc",
            "Intro to PeerTube",
            "A short introduction.",
            "10:15",
            "2021-01-02T03:04:05.000Z",
            "https://framatube.org/videos/embed/abc",
        ));
        basic.add(main_result(
            "https://tube.example/w/xyz",
            "Rust in 100 seconds",
            "Quick overview.",
            "1:40",
            "2022-02-03T00:00:00.000Z",
            "https://tube.example/videos/embed/xyz",
        ));
        Fixture::capture(NAME, query("peertube", 1), response(200, BASIC_JSON), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        Fixture::capture(
            NAME,
            query("peertube", 1),
            response(200, r#"{"total":0}"#),
            EngineResults::new(),
        )
        .with_case("no-data")
        .save(dir.join("no-data.json"))
        .unwrap();

        let q = query("rust videos", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{BASE_URL}/api/v1/search/videos?search=rust+videos&start=10&count=10&sort=-match&nsfw=both"
        ));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"data":[]}"#),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn sepiasearch_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = SepiaSearch::new();
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
    fn empty_query_clears_url() {
        let engine = SepiaSearch::new();
        let q = query("", 1);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert!(p.url.is_none());
    }
}
