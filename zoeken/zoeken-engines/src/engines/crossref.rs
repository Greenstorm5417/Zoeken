//! Crossref search engine.
//!
//! Queries the Crossref works API and maps each item into a paper result.

use serde::Deserialize;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Paper, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "crossref";

/// Crossref works search endpoint.
const SEARCH_URL: &str = "https://api.crossref.org/works";

/// Results requested per page (the reference API default).
const PAGE_SIZE: u32 = 20;

/// The Crossref engine.
#[derive(Debug, Clone)]
pub struct Crossref {
    meta: EngineMeta,
}

impl Crossref {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Crossref {
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
                shortcut: "cr".to_string(),
                about: About {
                    website: Some("https://www.crossref.org/".to_string()),
                    wikidata_id: Some("Q5188229".to_string()),
                    official_api_documentation: Some(
                        "https://api.crossref.org/swagger-ui/".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Crossref {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Deserialize, Default)]
struct WorksResponse {
    message: Message,
}

#[derive(Debug, Deserialize, Default)]
struct Message {
    #[serde(default)]
    items: Vec<Item>,
}

#[derive(Debug, Deserialize, Default)]
struct Item {
    #[serde(rename = "type", default)]
    type_: String,
    #[serde(default)]
    title: Vec<String>,
    #[serde(rename = "container-title", default)]
    container_title: Vec<String>,
    #[serde(rename = "abstract", default)]
    abstract_: Option<String>,
    #[serde(rename = "DOI", default)]
    doi: Option<String>,
    #[serde(default)]
    page: Option<String>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    subject: Vec<String>,
    #[serde(rename = "URL", default)]
    url: Option<String>,
    #[serde(default)]
    volume: Option<String>,
    #[serde(default)]
    resource: Option<Resource>,
    #[serde(default)]
    published: Option<Published>,
    #[serde(default)]
    author: Vec<Author>,
    #[serde(default)]
    isbn: Vec<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Resource {
    primary: Option<PrimaryResource>,
}

#[derive(Debug, Deserialize, Default)]
struct PrimaryResource {
    #[serde(rename = "URL", default)]
    url: Option<String>,
}

#[derive(Debug, Deserialize, Default)]
struct Published {
    #[serde(rename = "date-parts", default)]
    date_parts: Vec<Vec<i64>>,
}

#[derive(Debug, Deserialize, Default)]
struct Author {
    #[serde(default)]
    given: String,
    #[serde(default)]
    family: String,
}

impl Engine for Crossref {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![
            ("query", q.query.clone()),
            (
                "offset",
                (PAGE_SIZE * q.pageno.saturating_sub(1)).to_string(),
            ),
        ];
        p.url = Some(format!("{SEARCH_URL}?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let parsed: WorksResponse = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Crossref JSON: {e}")))?;

        for item in parsed.message.items {
            if item.type_ == "component" {
                // Files published alongside papers; not searchable content.
                continue;
            }

            let (title, journal) = if item.type_ == "book-chapter" {
                let mut title = item.container_title.first().cloned().unwrap_or_default();
                if let Some(t) = item.title.first()
                    && t.to_lowercase().trim() != title.to_lowercase().trim()
                {
                    title.push_str(&format!(" ({t})"));
                }
                (title, String::new())
            } else if !item.title.is_empty() {
                (
                    item.title.first().cloned().unwrap_or_default(),
                    String::new(),
                )
            } else {
                (
                    item.container_title.first().cloned().unwrap_or_default(),
                    String::new(),
                )
            };

            let mut url = item.url.clone().unwrap_or_default();
            if let Some(primary) = item.resource.as_ref().and_then(|r| r.primary.as_ref())
                && let Some(primary_url) = &primary.url
            {
                url = primary_url.clone();
            }

            let published_date = item
                .published
                .as_ref()
                .and_then(|p| p.date_parts.first())
                .map(|parts| {
                    let mut parts = parts.clone();
                    parts.resize(3, 1);
                    format!("{:04}-{:02}-{:02}", parts[0], parts[1], parts[2])
                });

            let authors: Vec<String> = item
                .author
                .iter()
                .map(|a| format!("{} {}", a.given, a.family))
                .collect();

            res.add(Result_::Paper(Paper {
                url: url.clone(),
                normalized_url: url,
                title,
                content: item.abstract_.unwrap_or_default(),
                engine: NAME.to_string(),
                authors,
                doi: item.doi.unwrap_or_default(),
                journal,
                published_date,
                publisher: item.publisher.unwrap_or_default(),
                volume: item.volume.unwrap_or_default(),
                pages: item.page.unwrap_or_default(),
                type_: item.type_,
                tags: item.subject,
                isbn: item.isbn,
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
      "message": {
        "items": [
          {
            "type": "journal-article",
            "title": ["Deep Learning"],
            "container-title": ["Nature"],
            "abstract": "A review of deep learning.",
            "DOI": "10.1038/nature14539",
            "publisher": "Springer",
            "subject": ["Computer Science"],
            "URL": "https://doi.org/10.1038/nature14539",
            "volume": "521",
            "page": "436-444",
            "published": {"date-parts": [[2015, 5, 27]]},
            "author": [{"given": "Yann", "family": "LeCun"}]
          }
        ]
      }
    }"#;

    fn expected_paper() -> Paper {
        Paper {
            url: "https://doi.org/10.1038/nature14539".to_string(),
            normalized_url: "https://doi.org/10.1038/nature14539".to_string(),
            title: "Deep Learning".to_string(),
            content: "A review of deep learning.".to_string(),
            engine: NAME.to_string(),
            authors: vec!["Yann LeCun".to_string()],
            doi: "10.1038/nature14539".to_string(),
            journal: String::new(),
            published_date: Some("2015-05-27".to_string()),
            publisher: "Springer".to_string(),
            volume: "521".to_string(),
            pages: "436-444".to_string(),
            type_: "journal-article".to_string(),
            tags: vec!["Computer Science".to_string()],
            ..Paper::default()
        }
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(Result_::Paper(expected_paper()));
        Fixture::capture(
            NAME,
            query("deep learning", 1),
            response(200, BASIC_JSON),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        let q = query("neural networks", 2);
        let mut golden = RequestParams {
            query: q.query.clone(),
            pageno: q.pageno,
            ..RequestParams::default()
        };
        golden.method = HttpMethod::Get;
        golden.url = Some(format!("{SEARCH_URL}?query=neural+networks&offset=20"));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"{"message": {"items": []}}"#),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn crossref_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Crossref::new();
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
    fn parses_item_fields() {
        let engine = Crossref::new();
        let res = engine.response(&response(200, BASIC_JSON)).unwrap();
        assert_eq!(res.results.len(), 1);
        if let Result_::Paper(p) = &res.results[0] {
            assert_eq!(p, &expected_paper());
        } else {
            panic!("expected a paper result");
        }
    }
}
