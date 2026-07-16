//! Semantic Scholar search engine.
//!
//! POSTs a JSON search request to the (undocumented) web-app search API and
//! maps each hit into a paper result.
//!
//! The reference implementation fetches a rotating `X-S2-UI-Version` header
//! value from the Semantic Scholar homepage before issuing the search
//! request. That two-step handshake needs a side-effecting network call
//! during `request()`, which this engine trait does not support, so we send
//! a fixed placeholder header instead (a known, documented difference).

use serde::Deserialize;
use serde_json::json;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView, html_to_text,
};
use zoeken_results::{Paper, Result_};

/// Engine name / identifier.
pub const NAME: &str = "semantic scholar";

/// Web-app search endpoint.
const SEARCH_URL: &str = "https://www.semanticscholar.org/api/1/search";

/// Paper landing page base.
const BASE_URL: &str = "https://www.semanticscholar.org";

/// The Semantic Scholar engine.
#[derive(Debug, Clone)]
pub struct SemanticScholar {
    meta: EngineMeta,
}

impl SemanticScholar {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        SemanticScholar {
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
                shortcut: "se".to_string(),
                about: About {
                    website: Some("https://www.semanticscholar.org/".to_string()),
                    wikidata_id: Some("Q22908627".to_string()),
                    official_api_documentation: Some(
                        "https://api.semanticscholar.org/".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for SemanticScholar {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, Default)]
struct SearchResponse {
    #[serde(default)]
    results: Vec<Hit>,
}

#[derive(Debug, Deserialize, Default)]
struct Hit {
    id: Option<String>,
    title: Option<TextField>,
    #[serde(rename = "paperAbstract")]
    paper_abstract: Option<TextField>,
    venue: Option<TextField>,
    journal: Option<Journal>,
    #[serde(rename = "doiInfo")]
    doi_info: Option<DoiInfo>,
    #[serde(rename = "fieldsOfStudy", default)]
    fields_of_study: Vec<String>,
    #[serde(default)]
    authors: Vec<Vec<AuthorName>>,
    #[serde(rename = "primaryPaperLink")]
    primary_paper_link: Option<Link>,
    #[serde(default)]
    links: Vec<String>,
    #[serde(rename = "alternatePaperLinks", default)]
    alternate_paper_links: Vec<AlternateLink>,
    #[serde(rename = "pubDate")]
    pub_date: Option<String>,
    #[serde(rename = "citationStats")]
    citation_stats: Option<CitationStats>,
}

#[derive(Debug, Deserialize)]
struct TextField {
    #[serde(default)]
    text: String,
}

#[derive(Debug, Deserialize)]
struct Journal {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct DoiInfo {
    doi: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AuthorName {
    #[serde(default)]
    name: String,
}

#[derive(Debug, Deserialize)]
struct Link {
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
struct AlternateLink {
    url: Option<String>,
    #[serde(rename = "linkType")]
    link_type: Option<String>,
}

#[derive(Debug, Deserialize)]
struct CitationStats {
    #[serde(default, rename = "numCitations")]
    num_citations: i64,
    #[serde(default, rename = "firstCitationVelocityYear")]
    first_citation_velocity_year: i64,
    #[serde(default, rename = "lastCitationVelocityYear")]
    last_citation_velocity_year: i64,
}

impl Engine for SemanticScholar {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Post;
        p.url = Some(SEARCH_URL.to_string());
        p.headers
            .insert("Content-Type".to_string(), "application/json".to_string());
        p.headers
            .insert("X-S2-UI-Version".to_string(), "unknown".to_string());
        p.headers
            .insert("X-S2-Client".to_string(), "webapp-browser".to_string());
        p.json = Some(json!({
            "queryString": q.query,
            "page": q.pageno,
            "pageSize": 10,
            "sort": "relevance",
            "getQuerySuggestions": false,
            "authors": [],
            "coAuthors": [],
            "venues": [],
            "performTitleMatch": true,
        }));
        p.content = serde_json::to_vec(p.json.as_ref().unwrap()).unwrap_or_default();
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let parsed: SearchResponse = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Semantic Scholar JSON: {e}")))?;

        for hit in parsed.results {
            let url = hit
                .primary_paper_link
                .and_then(|l| l.url)
                .or_else(|| hit.links.first().cloned())
                .or_else(|| {
                    hit.alternate_paper_links
                        .first()
                        .and_then(|l| l.url.clone())
                })
                .unwrap_or_else(|| {
                    format!("{BASE_URL}/paper/{}", hit.id.clone().unwrap_or_default())
                });

            let authors: Vec<String> = hit
                .authors
                .iter()
                .filter_map(|group| group.first())
                .map(|a| a.name.clone())
                .collect();

            let pdf_url = hit
                .alternate_paper_links
                .iter()
                .find(|l| !matches!(l.link_type.as_deref(), Some("crawler") | Some("doi")))
                .and_then(|l| l.url.clone())
                .unwrap_or_default();

            let comments = hit.citation_stats.map(|stats| {
                format!(
                    "{} citations from the year {} to {}",
                    stats.num_citations,
                    stats.first_citation_velocity_year,
                    stats.last_citation_velocity_year
                )
            });

            res.add(Result_::Paper(Paper {
                url: url.clone(),
                normalized_url: url,
                title: hit.title.map(|t| t.text).unwrap_or_default(),
                content: hit
                    .paper_abstract
                    .map(|t| html_to_text(&t.text))
                    .unwrap_or_default(),
                engine: NAME.to_string(),
                authors,
                doi: hit.doi_info.and_then(|d| d.doi).unwrap_or_default(),
                journal: hit
                    .venue
                    .map(|v| v.text)
                    .or_else(|| hit.journal.and_then(|j| j.name))
                    .unwrap_or_default(),
                published_date: hit.pub_date,
                tags: hit.fields_of_study,
                pdf_url,
                comments: comments.unwrap_or_default(),
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
            url: SEARCH_URL.to_string(),
            body: body.as_bytes().to_vec(),
            ..EngineResponse::default()
        }
    }

    const BASIC_JSON: &str = r#"{
      "results": [
        {
          "id": "123",
          "title": {"text": "Attention Is All You Need"},
          "paperAbstract": {"text": "We propose a new architecture."},
          "venue": {"text": "NeurIPS"},
          "doiInfo": {"doi": "10.5555/abc"},
          "fieldsOfStudy": ["Computer Science"],
          "authors": [[{"name": "Ashish Vaswani"}], [{"name": "Noam Shazeer"}]],
          "links": ["https://arxiv.org/abs/1706.03762"],
          "alternatePaperLinks": [{"url": "https://arxiv.org/pdf/1706.03762", "linkType": "pdf"}],
          "pubDate": "2017-06-12",
          "citationStats": {"numCitations": 100, "firstCitationVelocityYear": 2017, "lastCitationVelocityYear": 2020}
        }
      ]
    }"#;

    fn expected_paper() -> Paper {
        Paper {
            url: "https://arxiv.org/abs/1706.03762".to_string(),
            normalized_url: "https://arxiv.org/abs/1706.03762".to_string(),
            title: "Attention Is All You Need".to_string(),
            content: "We propose a new architecture.".to_string(),
            engine: NAME.to_string(),
            authors: vec!["Ashish Vaswani".to_string(), "Noam Shazeer".to_string()],
            doi: "10.5555/abc".to_string(),
            journal: "NeurIPS".to_string(),
            published_date: Some("2017-06-12".to_string()),
            tags: vec!["Computer Science".to_string()],
            pdf_url: "https://arxiv.org/pdf/1706.03762".to_string(),
            comments: "100 citations from the year 2017 to 2020".to_string(),
            ..Paper::default()
        }
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME.replace(' ', "_"));
        std::fs::create_dir_all(&dir).unwrap();

        let mut basic = EngineResults::new();
        basic.add(Result_::Paper(expected_paper()));
        Fixture::capture(
            NAME,
            query("transformers", 1),
            response(200, BASIC_JSON),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();
    }

    #[test]
    fn semantic_scholar_conformance() {
        let fixtures =
            load_fixtures_for(fixtures_root(), &NAME.replace(' ', "_")).expect("load fixtures");
        assert!(!fixtures.is_empty(), "no fixtures found for {NAME}");
        let engine = SemanticScholar::new();
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
    fn parses_hit_fields() {
        let engine = SemanticScholar::new();
        let res = engine.response(&response(200, BASIC_JSON)).unwrap();
        assert_eq!(res.results.len(), 1);
        if let Result_::Paper(p) = &res.results[0] {
            assert_eq!(p, &expected_paper());
        } else {
            panic!("expected a paper result");
        }
    }

    #[test]
    fn builds_post_request() {
        let engine = SemanticScholar::new();
        let mut params = RequestParams::default();
        engine.request(&query("rust", 2), &mut params);
        assert_eq!(params.method, HttpMethod::Post);
        assert_eq!(params.url.as_deref(), Some(SEARCH_URL));
        assert_eq!(
            params.headers.get("Content-Type").map(String::as_str),
            Some("application/json")
        );
    }
}
