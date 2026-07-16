//! Qwant WEB engine.
//!
//! Queries the web API and maps web items into main results.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SafeSearch, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "qwant";

/// Qwant JSON API base (the `web` category endpoint is appended).
const API_URL: &str = "https://api.qwant.com/v3/search/web";

/// The Qwant WEB engine.
#[derive(Debug, Clone)]
pub struct Qwant {
    meta: EngineMeta,
}

impl Qwant {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Qwant {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string(), "web".to_string()],
                paging: true,
                max_page: 5,
                time_range_support: false,
                safesearch: true,
                language_support: true,
                weight: 1,
                shortcut: "qw".to_string(),
                about: About {
                    website: Some("https://www.qwant.com/".to_string()),
                    wikidata_id: Some("Q14657870".to_string()),
                    official_api_documentation: None,
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Qwant {
    fn default() -> Self {
        Self::new()
    }
}

/// Derive Qwant's `locale` (`lang_COUNTRY`) from a Upstream locale, defaulting to
/// `en_US` (the reference `traits.get_region(..., default="en_US")`).
fn qwant_locale(locale: &str) -> String {
    if let Some((lang, territory)) = locale.split_once('-')
        && !lang.is_empty()
        && !territory.is_empty()
    {
        return format!("{}_{}", lang.to_lowercase(), territory.to_uppercase());
    }
    "en_US".to_string()
}

impl Engine for Qwant {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        if q.query.is_empty() {
            p.url = None;
            return;
        }
        p.method = HttpMethod::Get;
        p.raise_for_httperror = false;

        let results_per_page = 10u32;
        let offset = (p.pageno.saturating_sub(1)) * results_per_page;
        let safesearch = match q.safesearch {
            SafeSearch::Off => "0",
            SafeSearch::Moderate => "1",
            SafeSearch::Strict => "2",
        };

        let args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            ("count", results_per_page.to_string()),
            ("locale", qwant_locale(&q.locale)),
            ("offset", offset.to_string()),
            ("device", "desktop".to_string()),
            ("safesearch", safesearch.to_string()),
            ("tgp", "1".to_string()),
            ("display", "True".to_string()),
            ("llm", "True".to_string()),
        ];

        p.url = Some(format!("{API_URL}?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body).unwrap_or_default();
        let data = value.get("data").cloned().unwrap_or_default();

        // API error handling (mirrors the reference status check).
        if value.get("status").and_then(|s| s.as_str()) != Some("success") {
            let error_code = data.get("error_code").and_then(|c| c.as_i64());
            if error_code == Some(24) {
                return Err(EngineError::TooManyRequests(NAME.to_string()));
            }
            if data
                .get("error_data")
                .and_then(|d| d.get("captchaUrl"))
                .map(|c| !c.is_null())
                .unwrap_or(false)
            {
                return Err(EngineError::Captcha(NAME.to_string()));
            }
            if resp.status == 403 {
                return Err(EngineError::AccessDenied(NAME.to_string()));
            }
            let msg = match data.get("message") {
                Some(serde_json::Value::Array(items)) => items
                    .iter()
                    .filter_map(|m| m.as_str())
                    .collect::<Vec<_>>()
                    .join(","),
                Some(serde_json::Value::String(s)) => s.clone(),
                _ => "unknown".to_string(),
            };
            return Err(EngineError::Parse(format!(
                "qwant API error: {msg} ({error_code:?})"
            )));
        }

        let mainline = data
            .get("result")
            .and_then(|r| r.get("items"))
            .and_then(|i| i.get("mainline"))
            .and_then(|m| m.as_array())
            .cloned()
            .unwrap_or_default();

        for row in &mainline {
            let row_type = row.get("type").and_then(|t| t.as_str()).unwrap_or("web");
            if row_type != "web" {
                // `ads` and any non-web block are ignored in the web category.
                continue;
            }
            let items = row.get("items").and_then(|i| i.as_array());
            let Some(items) = items else { continue };
            for item in items {
                let title = item.get("title").and_then(|t| t.as_str()).unwrap_or("");
                let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("");
                let content = item.get("desc").and_then(|d| d.as_str()).unwrap_or("");
                res.add(Result_::Main(MainResult {
                    url: url.to_string(),
                    normalized_url: url.to_string(),
                    title: title.to_string(),
                    content: content.to_string(),
                    engine: NAME.to_string(),
                    ..MainResult::default()
                }));
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

    fn query(q: &str, locale: &str, pageno: u32) -> SearchQueryView {
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
            url: API_URL.to_string(),
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
      "status": "success",
      "data": {
        "result": {
          "items": {
            "mainline": [
              {
                "type": "ads",
                "items": [{"title": "Ad", "url": "https://ad.example.com/", "desc": "ignored"}]
              },
              {
                "type": "web",
                "items": [
                  {"title": "Rust Programming Language", "url": "https://www.rust-lang.org/", "desc": "A language empowering everyone."},
                  {"title": "The Rust Book", "url": "https://doc.rust-lang.org/book/", "desc": "Learn Rust."}
                ]
              }
            ]
          }
        }
      }
    }"#;

    const CAPTCHA_JSON: &str = r#"{"status":"error","data":{"error_code":9,"error_data":{"captchaUrl":"https://qwant.com/captcha"}}}"#;

    const RATELIMIT_JSON: &str = r#"{"status":"error","data":{"error_code":24}}"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://www.rust-lang.org/",
            "Rust Programming Language",
            "A language empowering everyone.",
        ));
        basic.add(main_result(
            "https://doc.rust-lang.org/book/",
            "The Rust Book",
            "Learn Rust.",
        ));
        Fixture::capture(
            NAME,
            query("rust", "all", 1),
            response(200, BASIC_JSON),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        // request-page2: validates the API URL for page 2 (offset 10, en_US).
        let q = query("rust", "en-US", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.raise_for_httperror = false;
        golden.url = Some(format!(
            "{API_URL}?q=rust&count=10&locale=en_US&offset=10&device=desktop&safesearch=0&tgp=1&display=True&llm=True"
        ));
        // An empty-but-successful response so response conformance also holds
        // (this case's purpose is validating the built request URL).
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"status":"success","data":{}}"#),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn qwant_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Qwant::new();
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
    fn maps_captcha_error() {
        let engine = Qwant::new();
        assert!(matches!(
            engine.response(&response(200, CAPTCHA_JSON)),
            Err(EngineError::Captcha(_))
        ));
    }

    #[test]
    fn maps_rate_limit_error() {
        let engine = Qwant::new();
        assert!(matches!(
            engine.response(&response(200, RATELIMIT_JSON)),
            Err(EngineError::TooManyRequests(_))
        ));
    }

    #[test]
    fn empty_query_clears_url() {
        let engine = Qwant::new();
        let q = query("", "all", 1);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert!(p.url.is_none());
    }
}
