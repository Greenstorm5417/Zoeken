//! Bandcamp search engine.
//!
//! Searches the HTML endpoint and maps linked results into main results.

use scraper::{Html, Selector};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::{encode_query, looks_like_bot_wall};

/// Engine name / identifier.
pub const NAME: &str = "bandcamp";

/// Base URL of the Bandcamp instance.
const BASE_URL: &str = "https://bandcamp.com/";

/// The Bandcamp music engine.
#[derive(Debug, Clone)]
pub struct Bandcamp {
    meta: EngineMeta,
}

impl Bandcamp {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Bandcamp {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["music".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "bc".to_string(),
                about: About {
                    website: Some("https://bandcamp.com/".to_string()),
                    wikidata_id: Some("Q545966".to_string()),
                    official_api_documentation: Some("https://bandcamp.com/developer".to_string()),
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for Bandcamp {
    fn default() -> Self {
        Self::new()
    }
}

fn element_text(el: &scraper::ElementRef<'_>) -> String {
    zoeken_engine_core::normalize_whitespace(&el.text().collect::<String>())
}

impl Engine for Bandcamp {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> =
            vec![("q", q.query.clone()), ("page", p.pageno.to_string())];
        p.url = Some(format!("{BASE_URL}search?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let html = resp.text();
        if looks_like_bot_wall(resp.status, &html) {
            return Err(EngineError::Captcha(NAME.to_string()));
        }
        let doc = Html::parse_document(&html);

        let li_sel = Selector::parse("li.searchresult").unwrap();
        let url_sel = Selector::parse("div.itemurl a").unwrap();
        let title_sel = Selector::parse("div.heading a").unwrap();
        let content_sel = Selector::parse("div.subhead").unwrap();

        for li in doc.select(&li_sel) {
            let Some(link) = li.select(&url_sel).next() else {
                continue;
            };
            let url = element_text(&link);
            let title = li
                .select(&title_sel)
                .next()
                .map(|el| element_text(&el))
                .unwrap_or_default();
            let content = li
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
<html><body>
<ul class="result-items">
  <li class="searchresult track">
    <div class="art"><img src="https://f4.bcbits.com/img/a1.jpg"></div>
    <div class="heading"><a href="https://artist.bandcamp.com/track/song">Great Song</a></div>
    <div class="subhead">from Great Album by The Artist</div>
    <div class="itemtype">TRACK</div>
    <div class="itemurl"><a href="https://artist.bandcamp.com/track/song?search_item_id=123">artist.bandcamp.com/track/song</a></div>
    <div class="released">released April 5, 2020</div>
  </li>
  <li class="searchresult album">
    <div class="heading"><a href="https://artist.bandcamp.com/album/rec">Great Album</a></div>
    <div class="subhead">by The Artist</div>
    <div class="itemtype">ALBUM</div>
    <div class="itemurl"><a href="https://artist.bandcamp.com/album/rec?search_item_id=456">artist.bandcamp.com/album/rec</a></div>
  </li>
  <li class="searchresult noskip">
    <div class="heading"><a href="https://x.example/">No Item URL</a></div>
    <div class="subhead">skipped, has no itemurl link</div>
  </li>
</ul>
</body></html>"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "artist.bandcamp.com/track/song",
            "Great Song",
            "from Great Album by The Artist",
        ));
        basic.add(main_result(
            "artist.bandcamp.com/album/rec",
            "Great Album",
            "by The Artist",
        ));
        Fixture::capture(NAME, query("great", 1), response(200, BASIC_HTML), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        let q = query("dream pop", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{BASE_URL}search?q=dream+pop&page=2"));
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
    fn bandcamp_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Bandcamp::new();
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
    fn skips_entries_without_item_link() {
        let engine = Bandcamp::new();
        let res = engine.response(&response(200, BASIC_HTML)).unwrap();
        assert_eq!(res.results.len(), 2);
    }
}
