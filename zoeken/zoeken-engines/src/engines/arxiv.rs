//! arXiv search engine.
//!
//! Paged Atom search requests map each `<entry>` into a paper result and keep
//! the raw published timestamp string.

use serde::Deserialize;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Paper, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "arxiv";

/// arXiv Atom API endpoint.
const BASE_URL: &str = "https://export.arxiv.org/api/query";

/// Results requested per page (the reference `arxiv_max_results`).
const MAX_RESULTS: u32 = 10;

/// The arXiv engine.
#[derive(Debug, Clone)]
pub struct Arxiv {
    meta: EngineMeta,
}

impl Arxiv {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Arxiv {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["science".to_string(), "scientific publications".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "arx".to_string(),
                about: About {
                    website: Some("https://arxiv.org".to_string()),
                    wikidata_id: Some("Q118398".to_string()),
                    official_api_documentation: Some(
                        "https://info.arxiv.org/help/api/user-manual.html".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "XML-RSS".to_string(),
                },
            },
        }
    }
}

impl Default for Arxiv {
    fn default() -> Self {
        Self::new()
    }
}

/// Atom feed wrapper: only the `<entry>` elements are of interest.
#[derive(Debug, Deserialize)]
struct Feed {
    #[serde(rename = "entry", default)]
    entries: Vec<Entry>,
}

/// A single Atom `<entry>` (arXiv article).
#[derive(Debug, Deserialize)]
struct Entry {
    #[serde(default)]
    title: String,
    #[serde(default)]
    id: String,
    #[serde(default)]
    summary: String,
    #[serde(default)]
    published: String,
    #[serde(rename = "author", default)]
    authors: Vec<Author>,
    #[serde(rename = "link", default)]
    links: Vec<Link>,
    #[serde(rename = "category", default)]
    categories: Vec<Category>,
    #[serde(rename = "doi", default)]
    doi: Option<String>,
    #[serde(rename = "journal_ref", default)]
    journal_ref: Option<String>,
    #[serde(rename = "comment", default)]
    comment: Option<String>,
}

/// An `<author>` element with a `<name>`.
#[derive(Debug, Deserialize)]
struct Author {
    #[serde(default)]
    name: String,
}

/// A `<link>` element (the `title="pdf"` link carries the PDF `href`).
#[derive(Debug, Deserialize)]
struct Link {
    #[serde(rename = "@title", default)]
    title: Option<String>,
    #[serde(rename = "@href", default)]
    href: Option<String>,
}

/// A `<category>` element with a `term`.
#[derive(Debug, Deserialize)]
struct Category {
    #[serde(rename = "@term", default)]
    term: String,
}

impl Engine for Arxiv {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let start = p.pageno.saturating_sub(1) * MAX_RESULTS;
        let args: Vec<(&str, String)> = vec![
            ("search_query", format!("all:{}", q.query)),
            ("start", start.to_string()),
            ("max_results", MAX_RESULTS.to_string()),
        ];
        p.url = Some(format!("{BASE_URL}?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let feed: Feed = quick_xml::de::from_str(&resp.text())
            .map_err(|e| EngineError::Parse(format!("invalid arXiv Atom feed: {e}")))?;

        for entry in &feed.entries {
            let authors: Vec<String> = entry
                .authors
                .iter()
                .map(|a| a.name.clone())
                .collect::<Vec<_>>();

            let pdf_url = entry
                .links
                .iter()
                .find(|l| l.title.as_deref() == Some("pdf"))
                .and_then(|l| l.href.clone())
                .unwrap_or_default();

            let tags: Vec<String> = entry.categories.iter().map(|c| c.term.clone()).collect();

            res.add(Result_::Paper(Paper {
                url: entry.id.clone(),
                normalized_url: entry.id.clone(),
                title: entry.title.clone(),
                content: entry.summary.clone(),
                engine: NAME.to_string(),
                authors,
                doi: entry.doi.clone().unwrap_or_default(),
                journal: entry.journal_ref.clone().unwrap_or_default(),
                published_date: Some(entry.published.clone()),
                tags,
                pdf_url,
                comments: entry.comment.clone().unwrap_or_default(),
                ..Paper::default()
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

    const BASIC_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<feed xmlns="http://www.w3.org/2005/Atom" xmlns:arxiv="http://arxiv.org/schemas/atom">
  <title>ArXiv Query</title>
  <id>http://arxiv.org/api/feed</id>
  <entry>
    <id>http://arxiv.org/abs/0704.0001v1</id>
    <published>2007-04-05T00:00:00Z</published>
    <title>Calculation of prompt diphoton production</title>
    <summary>A fully differential calculation in perturbative QCD.</summary>
    <author><name>Alice Author</name></author>
    <author><name>Bob Builder</name></author>
    <arxiv:doi>10.1103/PhysRevD.76.013009</arxiv:doi>
    <arxiv:journal_ref>Phys.Rev.D76:013009,2007</arxiv:journal_ref>
    <arxiv:comment>37 pages, 15 figures</arxiv:comment>
    <link href="http://arxiv.org/abs/0704.0001v1" rel="alternate" type="text/html"/>
    <link title="pdf" href="http://arxiv.org/pdf/0704.0001v1" rel="related" type="application/pdf"/>
    <category term="hep-ph" scheme="http://arxiv.org/schemas/atom"/>
    <category term="hep-ex" scheme="http://arxiv.org/schemas/atom"/>
  </entry>
</feed>"#;

    fn expected_paper() -> Paper {
        Paper {
            url: "http://arxiv.org/abs/0704.0001v1".to_string(),
            normalized_url: "http://arxiv.org/abs/0704.0001v1".to_string(),
            title: "Calculation of prompt diphoton production".to_string(),
            content: "A fully differential calculation in perturbative QCD.".to_string(),
            engine: NAME.to_string(),
            authors: vec!["Alice Author".to_string(), "Bob Builder".to_string()],
            doi: "10.1103/PhysRevD.76.013009".to_string(),
            journal: "Phys.Rev.D76:013009,2007".to_string(),
            published_date: Some("2007-04-05T00:00:00Z".to_string()),
            tags: vec!["hep-ph".to_string(), "hep-ex".to_string()],
            pdf_url: "http://arxiv.org/pdf/0704.0001v1".to_string(),
            comments: "37 pages, 15 figures".to_string(),
            ..Paper::default()
        }
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(Result_::Paper(expected_paper()));
        Fixture::capture(NAME, query("diphoton", 1), response(200, BASIC_XML), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        // request-page2: validates the built API URL and offset.
        let q = query("quantum gravity", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{BASE_URL}?search_query=all%3Aquantum+gravity&start=10&max_results=10"
        ));
        Fixture::capture(
            NAME,
            q.clone(),
            response(
                200,
                r#"<?xml version="1.0"?><feed xmlns="http://www.w3.org/2005/Atom"></feed>"#,
            ),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn arxiv_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Arxiv::new();
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
    fn parses_entry_fields() {
        let engine = Arxiv::new();
        let res = engine.response(&response(200, BASIC_XML)).unwrap();
        assert_eq!(res.results.len(), 1);
        if let Result_::Paper(p) = &res.results[0] {
            assert_eq!(p, &expected_paper());
        } else {
            panic!("expected a paper result");
        }
    }
}
