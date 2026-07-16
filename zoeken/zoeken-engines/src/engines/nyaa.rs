//! Nyaa.si (anime BitTorrent tracker) search engine.

use scraper::{Html, Selector};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{FileResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "nyaa";

/// Base URL for both the search page and result links.
const BASE_URL: &str = "https://nyaa.si/";

/// The Nyaa.si engine.
#[derive(Debug, Clone)]
pub struct Nyaa {
    meta: EngineMeta,
}

impl Nyaa {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Nyaa {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["files".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "nyaa".to_string(),
                about: About {
                    website: Some("https://nyaa.si/".to_string()),
                    wikidata_id: None,
                    official_api_documentation: None,
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for Nyaa {
    fn default() -> Self {
        Self::new()
    }
}

fn text_of(el: &scraper::ElementRef<'_>) -> String {
    zoeken_engine_core::normalize_whitespace(&el.text().collect::<String>())
}

fn int_or_zero(value: &str) -> i64 {
    value.trim().parse().unwrap_or(0)
}

impl Engine for Nyaa {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![("q", q.query.clone()), ("p", q.pageno.to_string())];
        p.url = Some(format!("{BASE_URL}?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let html = resp.text();
        let doc = Html::parse_document(&html);

        let row_sel = Selector::parse("table.torrent-list tbody tr").unwrap();
        let category_sel = Selector::parse("td:nth-of-type(1) a").unwrap();
        let title_link_sel = Selector::parse("td:nth-of-type(2) a").unwrap();
        let td_links_sel = Selector::parse("td:nth-of-type(3) a").unwrap();
        let filesize_sel = Selector::parse("td:nth-of-type(4)").unwrap();
        let seeds_sel = Selector::parse("td:nth-of-type(6)").unwrap();
        let leeches_sel = Selector::parse("td:nth-of-type(7)").unwrap();
        let downloads_sel = Selector::parse("td:nth-of-type(8)").unwrap();

        for row in doc.select(&row_sel) {
            let category = row
                .select(&category_sel)
                .next()
                .and_then(|el| el.value().attr("title"))
                .unwrap_or_default()
                .to_string();

            // The title cell can contain more than one `<a>` (e.g. a comment
            // count badge); the reference implementation takes the last one.
            let title_link = row.select(&title_link_sel).last();
            let Some(title_link) = title_link else {
                continue;
            };
            let title = text_of(&title_link);
            let href = title_link.value().attr("href").unwrap_or_default();
            let url = format!("{BASE_URL}{}", href.trim_start_matches('/'));

            // The reference implementation also keeps a separate `.torrent`
            // file link (`torrentfile`); `FileResult` has no field for it, so
            // it is dropped (a known, documented gap).
            let mut magnet_link = String::new();
            for link in row.select(&td_links_sel) {
                if let Some(href) = link.value().attr("href")
                    && href.starts_with("magnet")
                {
                    magnet_link = href.to_string();
                }
            }

            let filesize = row
                .select(&filesize_sel)
                .next()
                .map(|el| text_of(&el))
                .unwrap_or_default();
            let seed = row
                .select(&seeds_sel)
                .next()
                .map(|el| int_or_zero(&text_of(&el)))
                .unwrap_or(0);
            let leech = row
                .select(&leeches_sel)
                .next()
                .map(|el| int_or_zero(&text_of(&el)))
                .unwrap_or(0);
            let downloads = row
                .select(&downloads_sel)
                .next()
                .map(|el| int_or_zero(&text_of(&el)))
                .unwrap_or(0);

            let content = format!("Category: \"{category}\". Downloaded {downloads} times.");

            res.add(Result_::File(FileResult {
                url: url.clone(),
                normalized_url: url,
                title: title.clone(),
                filename: title,
                content,
                engine: NAME.to_string(),
                seed: Some(seed),
                leech: Some(leech),
                magnetlink: if magnet_link.is_empty() {
                    None
                } else {
                    Some(magnet_link)
                },
                filesize: if filesize.is_empty() {
                    None
                } else {
                    Some(filesize)
                },
                ..FileResult::default()
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

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: BASE_URL.to_string(),
            body: body.as_bytes().to_vec(),
            ..EngineResponse::default()
        }
    }

    const BASIC_HTML: &str = r#"<table class="torrent-list"><tbody>
      <tr>
        <td><a href="/?c=1_2" title="Anime - English-translated">Anime</a></td>
        <td><a href="/view/123" class="comments">1</a><a href="/view/123">Some Anime Episode 01</a></td>
        <td><a href="/download/123.torrent">Torrent</a><a href="magnet:?xt=urn:btih:hash">Magnet</a></td>
        <td>1.3 GiB</td>
        <td>2023-01-01</td>
        <td>12</td>
        <td>4</td>
        <td>100</td>
      </tr>
    </tbody></table>"#;

    fn expected_file() -> FileResult {
        FileResult {
            url: "https://nyaa.si/view/123".to_string(),
            normalized_url: "https://nyaa.si/view/123".to_string(),
            title: "Some Anime Episode 01".to_string(),
            filename: "Some Anime Episode 01".to_string(),
            content: "Category: \"Anime - English-translated\". Downloaded 100 times.".to_string(),
            engine: NAME.to_string(),
            seed: Some(12),
            leech: Some(4),
            magnetlink: Some("magnet:?xt=urn:btih:hash".to_string()),
            filesize: Some("1.3 GiB".to_string()),
            ..FileResult::default()
        }
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(Result_::File(expected_file()));
        Fixture::capture(NAME, query("anime", 1), response(200, BASIC_HTML), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();
    }

    #[test]
    fn nyaa_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Nyaa::new();
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
    fn parses_row_fields() {
        let engine = Nyaa::new();
        let res = engine.response(&response(200, BASIC_HTML)).unwrap();
        assert_eq!(res.results.len(), 1);
        if let Result_::File(f) = &res.results[0] {
            assert_eq!(f, &expected_file());
        } else {
            panic!("expected a file result");
        }
    }
}
