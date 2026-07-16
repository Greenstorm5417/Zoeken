//! SolidTorrents search engine.

use scraper::{Html, Selector};
use url::Url;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{FileResult, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "solidtorrents";

/// Default instance base URL.
const BASE_URL: &str = "https://solidtorrents.to";

/// The SolidTorrents engine.
#[derive(Debug, Clone)]
pub struct SolidTorrents {
    meta: EngineMeta,
    base_url: String,
}

impl SolidTorrents {
    /// Create the engine with its reference metadata and the default instance.
    pub fn new() -> Self {
        Self::with_base_url(BASE_URL.to_string())
    }

    /// Create the engine pointed at a custom (self-hosted) instance.
    pub fn with_base_url(base_url: String) -> Self {
        SolidTorrents {
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
                shortcut: "solid".to_string(),
                about: About {
                    website: Some("https://www.solidtorrents.to/".to_string()),
                    wikidata_id: None,
                    official_api_documentation: None,
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
            base_url,
        }
    }
}

impl Default for SolidTorrents {
    fn default() -> Self {
        Self::new()
    }
}

fn text_of(el: &scraper::ElementRef<'_>) -> String {
    zoeken_engine_core::normalize_whitespace(&el.text().collect::<String>())
}

fn absolute_url(base_url: &str, href: &str) -> String {
    Url::parse(base_url)
        .and_then(|base| base.join(href))
        .map(|url| url.to_string())
        .unwrap_or_else(|_| format!("{base_url}{href}"))
}

fn parse_count(value: &str) -> Option<i64> {
    value.replace(',', "").trim().parse().ok()
}

impl Engine for SolidTorrents {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> =
            vec![("q", q.query.clone()), ("page", q.pageno.to_string())];
        p.url = Some(format!("{}/search?{}", self.base_url, encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let html = resp.text();
        let doc = Html::parse_document(&html);

        let row_sel = Selector::parse("li.search-result").unwrap();
        let torrent_sel = Selector::parse("a.dl-torrent").unwrap();
        let magnet_sel = Selector::parse("a.dl-magnet").unwrap();
        let title_sel = Selector::parse("h5.title").unwrap();
        let title_link_sel = Selector::parse("h5.title a").unwrap();
        let category_sel = Selector::parse("a.category").unwrap();
        let stats_sel = Selector::parse("div.stats > div").unwrap();

        for row in doc.select(&row_sel) {
            if row.select(&torrent_sel).next().is_none() {
                continue;
            }
            let Some(magnet_href) = row
                .select(&magnet_sel)
                .next()
                .and_then(|el| el.value().attr("href"))
            else {
                continue;
            };

            let title = row
                .select(&title_sel)
                .next()
                .map(|el| text_of(&el))
                .unwrap_or_default();
            let Some(href) = row
                .select(&title_link_sel)
                .next()
                .and_then(|el| el.value().attr("href"))
            else {
                continue;
            };
            let url = absolute_url(&self.base_url, href);

            let categories: Vec<String> =
                row.select(&category_sel).map(|el| text_of(&el)).collect();
            let stats: Vec<scraper::ElementRef<'_>> = row.select(&stats_sel).collect();
            let seed = stats
                .get(3)
                .map(|el| text_of(el))
                .and_then(|s| s.trim().parse().ok());
            let leech = stats
                .get(2)
                .map(|el| text_of(el))
                .and_then(|s| s.trim().parse().ok());
            let filesize = stats.get(1).map(|el| text_of(el));

            res.add(Result_::File(FileResult {
                url: url.clone(),
                normalized_url: url,
                title: title.clone(),
                filename: title,
                content: categories.join(", "),
                engine: NAME.to_string(),
                seed,
                leech,
                magnetlink: Some(magnet_href.to_string()),
                filesize,
                ..FileResult::default()
            }));
        }

        if !res.results.is_empty() {
            return Ok(res);
        }

        let live_base_url = Url::parse(&resp.url)
            .ok()
            .and_then(|url| Some(format!("{}://{}", url.scheme(), url.host_str()?)))
            .unwrap_or_else(|| self.base_url.clone());
        let card_sel = Selector::parse("div[data-impression-ids] > div").unwrap();
        let live_title_sel = Selector::parse("h3 a[href^=\"/torrent/\"]").unwrap();
        let live_torrent_sel = Selector::parse("a[href^=\"/download/torrent/\"]").unwrap();
        let live_magnet_sel = Selector::parse("a[href^=\"magnet:\"]").unwrap();
        let info_sel = Selector::parse("div.text-gray-600.mb-3 > span").unwrap();
        let seed_sel = Selector::parse(".text-green-600 .font-medium").unwrap();
        let leech_sel = Selector::parse(".text-red-600 .font-medium").unwrap();

        for card in doc.select(&card_sel) {
            if card.select(&live_torrent_sel).next().is_none() {
                continue;
            }
            let Some(title_link) = card.select(&live_title_sel).next() else {
                continue;
            };
            let Some(href) = title_link.value().attr("href") else {
                continue;
            };
            let Some(magnet_href) = card
                .select(&live_magnet_sel)
                .next()
                .and_then(|el| el.value().attr("href"))
            else {
                continue;
            };

            let info: Vec<String> = card.select(&info_sel).map(|el| text_of(&el)).collect();
            let content = info.first().cloned().unwrap_or_default();
            let filesize = info.get(1).cloned();
            let title = text_of(&title_link);
            let url = absolute_url(&live_base_url, href);

            res.add(Result_::File(FileResult {
                url: url.clone(),
                normalized_url: url,
                title: title.clone(),
                filename: title,
                content,
                engine: NAME.to_string(),
                seed: card
                    .select(&seed_sel)
                    .next()
                    .map(|el| text_of(&el))
                    .and_then(|value| parse_count(&value)),
                leech: card
                    .select(&leech_sel)
                    .next()
                    .map(|el| text_of(&el))
                    .and_then(|value| parse_count(&value)),
                magnetlink: Some(magnet_href.to_string()),
                filesize,
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

    const BASIC_HTML: &str = r#"<ul>
      <li class="search-result">
        <h5 class="title"><a href="/view/abc123">Ubuntu 24.04 Desktop</a></h5>
        <a class="category">Software</a>
        <a class="dl-torrent" href="/download/abc123.torrent">Torrent</a>
        <a class="dl-magnet" href="magnet:?xt=urn:btih:deadbeef">Magnet</a>
        <div class="stats">
          <div>0</div>
          <div>4.7 GiB</div>
          <div>7</div>
          <div>25</div>
          <div>Jan 02, 2024</div>
        </div>
      </li>
    </ul>"#;

    const BITSEARCH_HTML: &str = r#"<div class="space-y-4" data-impression-ids="[&quot;abc&quot;]">
      <div class="bg-white rounded-lg">
        <h3><a href="/torrent/abc">ubuntu-24.04-desktop-amd64.iso</a></h3>
        <div class="flex flex-wrap items-center gap-4 text-sm text-gray-600 mb-3">
          <span><i class="fas fa-video"></i><span>Other/DiskImage</span></span>
          <span><i class="fas fa-download"></i><span>5.8 GB</span></span>
          <span><i class="fas fa-calendar"></i><span>4/25/2024</span></span>
        </div>
        <span class="inline-flex items-center space-x-1 text-green-600"><span class="font-medium">1,234</span><span>seeders</span></span>
        <span class="inline-flex items-center space-x-1 text-red-600"><span class="font-medium">56</span><span>leechers</span></span>
        <a href="/download/torrent/ABC?title=ubuntu">Torrent</a>
        <a href="magnet:?xt=urn:btih:abc">Magnet</a>
      </div>
    </div>"#;

    fn expected_file() -> FileResult {
        FileResult {
            url: "https://solidtorrents.to/view/abc123".to_string(),
            normalized_url: "https://solidtorrents.to/view/abc123".to_string(),
            title: "Ubuntu 24.04 Desktop".to_string(),
            filename: "Ubuntu 24.04 Desktop".to_string(),
            content: "Software".to_string(),
            engine: NAME.to_string(),
            seed: Some(25),
            leech: Some(7),
            magnetlink: Some("magnet:?xt=urn:btih:deadbeef".to_string()),
            filesize: Some("4.7 GiB".to_string()),
            ..FileResult::default()
        }
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(Result_::File(expected_file()));
        Fixture::capture(NAME, query("ubuntu", 1), response(200, BASIC_HTML), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();
    }

    #[test]
    fn solidtorrents_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = SolidTorrents::new();
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
        let engine = SolidTorrents::new();
        let res = engine.response(&response(200, BASIC_HTML)).unwrap();
        assert_eq!(res.results.len(), 1);
        if let Result_::File(f) = &res.results[0] {
            assert_eq!(f, &expected_file());
        } else {
            panic!("expected a file result");
        }
    }

    #[test]
    fn parses_bitsearch_redirect_markup() {
        let engine = SolidTorrents::new();
        let mut resp = response(200, BITSEARCH_HTML);
        resp.url = "https://bitsearch.eu/search?q=ubuntu".to_string();
        let res = engine.response(&resp).unwrap();
        assert_eq!(res.results.len(), 1);
        if let Result_::File(f) = &res.results[0] {
            assert_eq!(f.url, "https://bitsearch.eu/torrent/abc");
            assert_eq!(f.title, "ubuntu-24.04-desktop-amd64.iso");
            assert_eq!(f.content, "Other/DiskImage");
            assert_eq!(f.filesize.as_deref(), Some("5.8 GB"));
            assert_eq!(f.seed, Some(1234));
            assert_eq!(f.leech, Some(56));
            assert_eq!(f.magnetlink.as_deref(), Some("magnet:?xt=urn:btih:abc"));
        } else {
            panic!("expected a file result");
        }
    }
}
