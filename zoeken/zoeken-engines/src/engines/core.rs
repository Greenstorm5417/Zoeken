//! CORE (COnnecting REpositories) search engine.
//!
//! Requires an API key (`api_key`); requests without one still build a URL
//! but the upstream API will reject them with 401/403, which is mapped to
//! [`EngineError::AccessDenied`].

use serde::{Deserialize, Deserializer};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Paper, Result_};

use super::util::encode_query;

/// Engine name / identifier.
pub const NAME: &str = "core.ac.uk";

/// CORE works search endpoint (v3).
const BASE_URL: &str = "https://api.core.ac.uk/v3/search/works/";

/// Results requested per page.
const PAGE_SIZE: u32 = 10;

/// The CORE engine.
#[derive(Debug, Clone)]
pub struct Core {
    meta: EngineMeta,
    api_key: String,
}

impl Core {
    /// Create the engine with its reference metadata and no API key.
    pub fn new() -> Self {
        Core::with_api_key(String::new())
    }

    /// Create the engine configured with an API key.
    pub fn with_api_key(api_key: String) -> Self {
        Core {
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
                shortcut: "cor".to_string(),
                about: About {
                    website: Some("https://core.ac.uk".to_string()),
                    wikidata_id: Some("Q22661180".to_string()),
                    official_api_documentation: Some("https://api.core.ac.uk/docs/v3".to_string()),
                    use_official_api: true,
                    require_api_key: true,
                    results: "JSON".to_string(),
                },
            },
            api_key,
        }
    }
}

impl Default for Core {
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
    #[serde(default)]
    title: Option<String>,
    #[serde(default)]
    id: Option<serde_json::Value>,
    #[serde(rename = "downloadUrl", default)]
    download_url: Option<String>,
    #[serde(
        rename = "sourceFulltextUrls",
        default,
        deserialize_with = "string_list"
    )]
    source_fulltext_urls: Vec<String>,
    #[serde(default)]
    doi: Option<String>,
    #[serde(rename = "fullText", default)]
    full_text: Option<String>,
    #[serde(rename = "fieldOfStudy", default, deserialize_with = "string_list")]
    field_of_study: Vec<String>,
    #[serde(rename = "publishedDate", default)]
    published_date: Option<String>,
    #[serde(rename = "depositedDate", default)]
    deposited_date: Option<String>,
    #[serde(rename = "documentType", default)]
    document_type: Option<String>,
    #[serde(default)]
    authors: Vec<AuthorName>,
    #[serde(default, deserialize_with = "string_list")]
    contributors: Vec<String>,
    #[serde(default)]
    publisher: Option<String>,
    #[serde(default)]
    journals: Vec<Journal>,
}

#[derive(Debug, Deserialize)]
struct AuthorName {
    name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Journal {
    title: Option<String>,
}

fn string_list<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(match value {
        serde_json::Value::Null => Vec::new(),
        serde_json::Value::String(value) => {
            if value.is_empty() {
                Vec::new()
            } else {
                vec![value]
            }
        }
        serde_json::Value::Array(values) => values
            .into_iter()
            .filter_map(|value| match value {
                serde_json::Value::String(value) if !value.is_empty() => Some(value),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    })
}

impl Engine for Core {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            (
                "offset",
                (PAGE_SIZE * q.pageno.saturating_sub(1)).to_string(),
            ),
            ("limit", PAGE_SIZE.to_string()),
            ("sort", "relevance".to_string()),
        ];
        p.url = Some(format!("{BASE_URL}?{}", encode_query(&args)));
        p.headers.insert(
            "Authorization".to_string(),
            format!("Bearer {}", self.api_key),
        );
        p.raise_for_httperror = false;
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        if matches!(resp.status, 401 | 403) {
            return Err(EngineError::AccessDenied(NAME.to_string()));
        }
        let mut res = EngineResults::new();
        let parsed: SearchResponse = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid CORE JSON: {e}")))?;

        for hit in parsed.results {
            let Some(title) = hit.title.filter(|t| !t.is_empty()) else {
                continue;
            };

            let url = if let Some(doi) = &hit.doi {
                format!("https://doi.org/{doi}")
            } else if let Some(id) = &hit.id {
                format!(
                    "https://core.ac.uk/works/{}",
                    id.as_str()
                        .map(str::to_string)
                        .unwrap_or_else(|| id.to_string())
                )
            } else if let Some(url) = &hit.download_url {
                url.clone()
            } else if let Some(url) = hit.source_fulltext_urls.first() {
                url.clone()
            } else {
                continue;
            };

            let published_date = hit.published_date.clone().or(hit.deposited_date.clone());

            let journal = hit
                .journals
                .iter()
                .filter_map(|j| j.title.clone())
                .collect::<Vec<_>>()
                .join(", ");

            let authors: Vec<String> = hit.authors.into_iter().filter_map(|a| a.name).collect();

            res.add(Result_::Paper(Paper {
                url: url.clone(),
                normalized_url: url,
                title,
                content: hit.full_text.unwrap_or_default(),
                engine: NAME.to_string(),
                authors,
                doi: hit.doi.unwrap_or_default(),
                journal,
                published_date,
                editor: hit.contributors.join(", "),
                publisher: hit
                    .publisher
                    .unwrap_or_default()
                    .trim_matches('\'')
                    .to_string(),
                type_: hit.document_type.unwrap_or_default(),
                tags: hit.field_of_study,
                pdf_url: hit
                    .download_url
                    .or_else(|| hit.source_fulltext_urls.into_iter().next())
                    .unwrap_or_default(),
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

    const BASIC_JSON: &str = r#"{
      "results": [
        {
          "title": "Open Access Repositories",
          "id": "12345",
          "doi": "10.1000/xyz123",
          "fullText": "This paper discusses open access.",
          "fieldOfStudy": ["Library Science"],
          "publishedDate": "2020-01-15T00:00:00Z",
          "documentType": "research",
          "authors": [{"name": "Jane Doe"}],
          "publisher": "'CORE'",
          "journals": [{"title": "Journal of OA"}]
        }
      ]
    }"#;

    fn expected_paper() -> Paper {
        Paper {
            url: "https://doi.org/10.1000/xyz123".to_string(),
            normalized_url: "https://doi.org/10.1000/xyz123".to_string(),
            title: "Open Access Repositories".to_string(),
            content: "This paper discusses open access.".to_string(),
            engine: NAME.to_string(),
            authors: vec!["Jane Doe".to_string()],
            doi: "10.1000/xyz123".to_string(),
            journal: "Journal of OA".to_string(),
            published_date: Some("2020-01-15T00:00:00Z".to_string()),
            publisher: "CORE".to_string(),
            type_: "research".to_string(),
            tags: vec!["Library Science".to_string()],
            ..Paper::default()
        }
    }

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join("core");

        let mut basic = EngineResults::new();
        basic.add(Result_::Paper(expected_paper()));
        Fixture::capture(
            NAME,
            query("open access", 1),
            response(200, BASIC_JSON),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();
    }

    #[test]
    fn core_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), "core").expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/core"
        );
        let engine = Core::new();
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
        let engine = Core::new();
        let res = engine.response(&response(200, BASIC_JSON)).unwrap();
        assert_eq!(res.results.len(), 1);
        if let Result_::Paper(p) = &res.results[0] {
            assert_eq!(p, &expected_paper());
        } else {
            panic!("expected a paper result");
        }
    }

    #[test]
    fn maps_unauthorized_to_access_denied() {
        let engine = Core::new();
        assert!(matches!(
            engine.response(&response(401, "")),
            Err(EngineError::AccessDenied(name)) if name == NAME
        ));
    }
}
