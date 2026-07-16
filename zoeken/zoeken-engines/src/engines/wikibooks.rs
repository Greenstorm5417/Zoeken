//! Wikibooks engine: queries MediaWiki API for book pages with language-specific URLs.
//!
//! Filters redirect snippets; defaults response URL language to 'en' (unavailable at response time).

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::{encode_path, encode_query};

/// Engine name / identifier.
pub const NAME: &str = "wikibooks";

const PAGE_SIZE: u32 = 5;

/// The Wikibooks engine.
#[derive(Debug, Clone)]
pub struct Wikibooks {
    meta: EngineMeta,
}

impl Wikibooks {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Wikibooks {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string(), "wikimedia".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: true,
                weight: 1,
                shortcut: "wb".to_string(),
                about: About {
                    website: Some("https://www.wikibooks.org/".to_string()),
                    wikidata_id: Some("Q367".to_string()),
                    official_api_documentation: Some(
                        "https://www.mediawiki.org/w/api.php?action=help&modules=query".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Wikibooks {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve the MediaWiki search language from a Upstream locale, mirroring the
/// reference: `all` or empty becomes `en`, otherwise the subtag before the
/// first `-` is used (e.g. `de-DE` -> `de`).
fn resolve_language(locale: &str) -> String {
    if locale.is_empty() || locale == "all" {
        return "en".to_string();
    }
    let lang = locale.split('-').next().unwrap_or("en");
    if lang.is_empty() {
        "en".to_string()
    } else {
        lang.to_string()
    }
}

impl Engine for Wikibooks {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        let language = resolve_language(&q.locale);
        let offset = (p.pageno.saturating_sub(1)) * PAGE_SIZE;

        let args: Vec<(&str, String)> = vec![
            ("action", "query".to_string()),
            ("list", "search".to_string()),
            ("format", "json".to_string()),
            ("srsearch", q.query.clone()),
            ("sroffset", offset.to_string()),
            ("srlimit", PAGE_SIZE.to_string()),
            ("srwhat", "text".to_string()),
            (
                "srprop",
                "sectiontitle|snippet|timestamp|categorysnippet".to_string(),
            ),
            ("srsort", "relevance".to_string()),
            ("srenablerewrites", "1".to_string()),
        ];

        p.method = HttpMethod::Get;
        p.url = Some(format!(
            "https://{language}.wikibooks.org/w/api.php?{}",
            encode_query(&args)
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Wikibooks JSON: {e}")))?;

        let search = value
            .get("query")
            .and_then(|q| q.get("search"))
            .and_then(|s| s.as_array())
            .cloned()
            .unwrap_or_default();

        // Return no results when `query.search` is absent or empty.
        if search.is_empty() {
            return Ok(res);
        }

        // The reference derives the URL base from the request language; the
        // Engine trait has no shared state, so default to `en` (deviation).
        let language = "en";

        for result in &search {
            let snippet = result.get("snippet").and_then(|s| s.as_str()).unwrap_or("");
            if snippet.starts_with("#REDIRECT") {
                continue;
            }

            let raw_title = result.get("title").and_then(|t| t.as_str()).unwrap_or("");
            let sectiontitle = result
                .get("sectiontitle")
                .and_then(|s| s.as_str())
                .filter(|s| !s.is_empty());

            let content = zoeken_engine_core::html_to_text(snippet);

            let mut url = format!(
                "https://{language}.wikibooks.org/wiki/{}",
                encode_path(&raw_title.replace(' ', "_"))
            );
            let mut title = raw_title.to_string();
            if let Some(section) = sectiontitle {
                url.push('#');
                url.push_str(&encode_path(&section.replace(' ', "_")));
                title.push_str(" / ");
                title.push_str(section);
            }

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
            url: "https://en.wikibooks.org/".to_string(),
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

    // Two hits, the second carrying a sectiontitle.
    const BASIC_JSON: &str = r#"{
      "batchcomplete": "",
      "query": {
        "search": [
          {
            "ns": 0,
            "title": "Rust Programming",
            "snippet": "A book about the <span class=\"searchmatch\">Rust</span> language",
            "timestamp": "2024-01-02T03:04:05Z"
          },
          {
            "ns": 0,
            "title": "Cookbook",
            "sectiontitle": "Sauces",
            "snippet": "The best <span class=\"searchmatch\">sauces</span> chapter",
            "timestamp": "2024-02-03T04:05:06Z"
          }
        ]
      }
    }"#;

    // A hit whose snippet begins with #REDIRECT is skipped; the other is kept.
    const REDIRECT_JSON: &str = r##"{
      "query": {
        "search": [
          {
            "ns": 0,
            "title": "Old Title",
            "snippet": "#REDIRECT [[New Title]]",
            "timestamp": "2024-01-02T03:04:05Z"
          },
          {
            "ns": 0,
            "title": "New Title",
            "snippet": "Actual <span class=\"searchmatch\">content</span> here",
            "timestamp": "2024-01-02T03:04:05Z"
          }
        ]
      }
    }"##;

    // No `query.search` -> no results.
    const EMPTY_JSON: &str = r#"{"batchcomplete":"","query":{"searchinfo":{"totalhits":0}}}"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        // basic: two hits, one with a sectiontitle.
        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://en.wikibooks.org/wiki/Rust_Programming",
            "Rust Programming",
            "A book about the Rust language",
        ));
        basic.add(main_result(
            "https://en.wikibooks.org/wiki/Cookbook#Sauces",
            "Cookbook / Sauces",
            "The best sauces chapter",
        ));
        Fixture::capture(
            NAME,
            query("rust", 1, "all"),
            response(200, BASIC_JSON),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        // redirect-skipped: the #REDIRECT hit is dropped.
        let mut redirect = EngineResults::new();
        redirect.add(main_result(
            "https://en.wikibooks.org/wiki/New_Title",
            "New Title",
            "Actual content here",
        ));
        Fixture::capture(
            NAME,
            query("title", 1, "all"),
            response(200, REDIRECT_JSON),
            redirect,
        )
        .with_case("redirect-skipped")
        .save(dir.join("redirect-skipped.json"))
        .unwrap();

        // empty: no `query.search` -> no results.
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
        let q = query("rust", 2, "all");
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(
            "https://en.wikibooks.org/w/api.php?action=query&list=search&format=json\
             &srsearch=rust&sroffset=5&srlimit=5&srwhat=text\
             &srprop=sectiontitle%7Csnippet%7Ctimestamp%7Ccategorysnippet\
             &srsort=relevance&srenablerewrites=1"
                .to_string(),
        );
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
    fn wikibooks_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Wikibooks::new();
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
        let engine = Wikibooks::new();
        let q = query("rust", 2, "de-DE");
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some(
                "https://de.wikibooks.org/w/api.php?action=query&list=search&format=json\
                 &srsearch=rust&sroffset=5&srlimit=5&srwhat=text\
                 &srprop=sectiontitle%7Csnippet%7Ctimestamp%7Ccategorysnippet\
                 &srsort=relevance&srenablerewrites=1"
            )
        );
    }

    #[test]
    fn resolves_language_from_locale() {
        assert_eq!(resolve_language("de-DE"), "de");
        assert_eq!(resolve_language("all"), "en");
        assert_eq!(resolve_language(""), "en");
        assert_eq!(resolve_language("fr"), "fr");
    }
}
