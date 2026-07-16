//! PyPI search engine.
//!
//! Parses package snippets from the HTML search results page.

use scraper::{Html, Selector};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::{encode_query, looks_like_bot_wall};

/// Engine name / identifier.
pub const NAME: &str = "pypi";

/// Base URL used to resolve result hrefs.
const BASE_URL: &str = "https://pypi.org";

/// The PyPI engine.
#[derive(Debug, Clone)]
pub struct Pypi {
    meta: EngineMeta,
}

impl Pypi {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Pypi {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["it".to_string(), "packages".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "pypi".to_string(),
                about: About {
                    website: Some("https://pypi.org".to_string()),
                    wikidata_id: Some("Q2984686".to_string()),
                    official_api_documentation: Some(
                        "https://warehouse.readthedocs.io/api-reference/index.html".to_string(),
                    ),
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for Pypi {
    fn default() -> Self {
        Self::new()
    }
}

/// Normalized text content of an element (mirrors the reference `extract_text`).
fn element_text(el: &scraper::ElementRef<'_>) -> String {
    zoeken_engine_core::normalize_whitespace(&el.text().collect::<String>())
}

impl Engine for Pypi {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> =
            vec![("q", q.query.clone()), ("page", p.pageno.to_string())];
        p.url = Some(format!("{BASE_URL}/search/?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let html = resp.text();
        if looks_like_bot_wall(resp.status, &html) {
            return Err(EngineError::Captcha(NAME.to_string()));
        }
        let doc = Html::parse_document(&html);

        let entry_sel = Selector::parse("a.package-snippet").unwrap();
        let name_sel = Selector::parse("h3 span.package-snippet__name").unwrap();
        let content_sel = Selector::parse("p").unwrap();

        for entry in doc.select(&entry_sel) {
            let href = entry.value().attr("href").unwrap_or("");
            let url = format!("{BASE_URL}{href}");
            let title = entry
                .select(&name_sel)
                .next()
                .map(|el| element_text(&el))
                .unwrap_or_default();
            let content = entry
                .select(&content_sel)
                .next()
                .map(|el| element_text(&el))
                .unwrap_or_default();

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

    const BASIC_HTML: &str = r#"<!DOCTYPE html>
<html><body><main><div><div><div><form><div><ul>
  <li>
    <a class="package-snippet" href="/project/requests/">
      <h3>
        <span class="package-snippet__name">requests</span>
        <span class="package-snippet__version">2.31.0</span>
        <span class="package-snippet__created"><time datetime="2023-05-22T15:12:00+0000">May 22, 2023</time></span>
      </h3>
      <p class="package-snippet__description">Python HTTP for Humans.</p>
    </a>
  </li>
  <li>
    <a class="package-snippet" href="/project/flask/">
      <h3>
        <span class="package-snippet__name">Flask</span>
        <span class="package-snippet__version">3.0.0</span>
        <span class="package-snippet__created"><time datetime="2023-09-30T00:00:00+0000">Sep 30, 2023</time></span>
      </h3>
      <p class="package-snippet__description">A simple framework for building complex web applications.</p>
    </a>
  </li>
</ul></div></form></div></div></div></main></body></html>"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://pypi.org/project/requests/",
            "requests",
            "Python HTTP for Humans.",
        ));
        basic.add(main_result(
            "https://pypi.org/project/flask/",
            "Flask",
            "A simple framework for building complex web applications.",
        ));
        Fixture::capture(NAME, query("http", 1), response(200, BASIC_HTML), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        // request-page2: validates request URL building for page 2.
        let q = query("web framework", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}/search/?q=web+framework&page=2"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, "<html><body></body></html>"),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn pypi_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Pypi::new();
        if let Err(mismatches) = run_all(&engine, &fixtures) {
            let report = mismatches
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            panic!("conformance failures:\n{report}");
        }
    }
}
