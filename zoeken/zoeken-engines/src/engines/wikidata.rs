//! Wikidata engine: queries the SPARQL endpoint and returns infobox results.
//!
//! Resolves entity labels/descriptions and handles de-duplication and dummy-entity filtering.

use std::collections::HashSet;

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Infobox, InfoboxUrl, Result_};

/// Engine name / identifier.
pub const NAME: &str = "wikidata";

/// The Wikidata SPARQL endpoint (the reference `SPARQL_ENDPOINT_URL`).
const SPARQL_ENDPOINT_URL: &str = "https://query.wikidata.org/sparql";

fn dummy_entity_urls() -> HashSet<String> {
    ["Q4115189", "Q13406268", "Q15397819", "Q17339402"]
        .iter()
        .map(|wid| format!("http://www.wikidata.org/entity/{wid}"))
        .collect()
}

/// The Wikidata (SPARQL) engine.
#[derive(Debug, Clone)]
pub struct Wikidata {
    meta: EngineMeta,
}

impl Wikidata {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Wikidata {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: true,
                weight: 1,
                shortcut: "wd".to_string(),
                about: About {
                    website: Some("https://wikidata.org/".to_string()),
                    wikidata_id: Some("Q2013".to_string()),
                    official_api_documentation: Some("https://query.wikidata.org/".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Wikidata {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_language(locale: &str) -> String {
    if locale.is_empty() || locale == "all" {
        return "en".to_string();
    }
    let lang = locale.split(['-', '_']).next().unwrap_or("en");
    if lang.is_empty() {
        "en".to_string()
    } else {
        lang.to_lowercase()
    }
}

fn sparql_string_escape(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for ch in value.chars() {
        match ch {
            '\t' => out.push_str("\\\t"),
            '\n' => out.push_str("\\\n"),
            '\r' => out.push_str("\\\r"),
            '\u{0008}' => out.push_str("\\\u{0008}"),
            '\u{000C}' => out.push_str("\\\u{000C}"),
            '"' => out.push_str("\\\""),
            '\'' => out.push_str("\\'"),
            '\\' => out.push_str("\\\\"),
            other => out.push(other),
        }
    }
    out
}

fn build_query(query: &str, language: &str) -> String {
    let search = sparql_string_escape(query);
    format!(
        "SELECT ?item ?itemLabel ?itemDescription\n\
         WHERE\n\
         {{\n\
         \x20\x20SERVICE wikibase:mwapi {{\n\
         \x20\x20\x20\x20\x20\x20\x20\x20bd:serviceParam wikibase:endpoint \"www.wikidata.org\";\n\
         \x20\x20\x20\x20\x20\x20\x20\x20wikibase:api \"EntitySearch\";\n\
         \x20\x20\x20\x20\x20\x20\x20\x20wikibase:limit 1;\n\
         \x20\x20\x20\x20\x20\x20\x20\x20mwapi:search \"{search}\";\n\
         \x20\x20\x20\x20\x20\x20\x20\x20mwapi:language \"{language}\".\n\
         \x20\x20\x20\x20\x20\x20\x20\x20?item wikibase:apiOutputItem mwapi:item.\n\
         \x20\x20}}\n\
         \x20\x20SERVICE wikibase:label {{\n\
         \x20\x20\x20\x20\x20\x20bd:serviceParam wikibase:language \"{language},en\".\n\
         \x20\x20\x20\x20\x20\x20?item rdfs:label ?itemLabel .\n\
         \x20\x20\x20\x20\x20\x20?item schema:description ?itemDescription .\n\
         \x20\x20}}\n\
         }}"
    )
}

fn replace_http_by_https(value: &str) -> String {
    value.replace("http:", "https:")
}

impl Engine for Wikidata {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        let language = resolve_language(&q.locale);
        let query = build_query(&q.query, &language);

        p.method = HttpMethod::Post;
        p.url = Some(SPARQL_ENDPOINT_URL.to_string());
        p.data.insert("query".to_string(), query);
        p.headers.insert(
            "Accept".to_string(),
            "application/sparql-results+json".to_string(),
        );
        // The public WDQS endpoint rate-limits aggressively (HTTP 429). Handle
        // the status ourselves so a transient limit becomes a brief back-off
        // rather than the default hour-long `too_many_requests` suspension that
        // would take an optional infobox engine offline for everyone.
        p.raise_for_httperror = false;
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        // WDQS rate-limit / overload: surface as a plain transient error so the
        // engine is suspended only for the short default ban window, not the
        // hour reserved for genuine `TooManyRequests`.
        if matches!(resp.status, 429 | 500 | 502 | 503 | 504) {
            return Err(EngineError::Timeout);
        }
        // Any other non-success (e.g. a 400 from a malformed query): no
        // infobox, but don't penalise the engine.
        if resp.status != 200 {
            return Ok(res);
        }

        let json: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Wikidata SPARQL JSON: {e}")))?;

        let bindings = json
            .get("results")
            .and_then(|r| r.get("bindings"))
            .and_then(|b| b.as_array())
            .cloned()
            .unwrap_or_default();

        let dummy = dummy_entity_urls();
        let mut seen_entities: HashSet<String> = HashSet::new();

        for binding in &bindings {
            // Collect {key: value["value"]} into a flat attribute map.
            let obj = match binding.as_object() {
                Some(o) => o,
                None => continue,
            };

            // `item` is the entity URL used for de-duplication.
            let Some(item) = obj
                .get("item")
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_str())
            else {
                continue;
            };
            let item = item.to_string();

            // Skip dummy entities and entities already emitted.
            if dummy.contains(&item) || seen_entities.contains(&item) {
                continue;
            }
            seen_entities.insert(item.clone());

            let infobox = obj
                .get("itemLabel")
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let content = obj
                .get("itemDescription")
                .and_then(|v| v.get("value"))
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            res.add(Result_::Infobox(Infobox {
                infobox,
                // Rewrite http: -> https: in the entity ID
                id: Some(replace_http_by_https(&item)),
                content,
                img_src: None,
                // Use original (unrewritten) item URL in the link
                urls: vec![InfoboxUrl {
                    title: "Wikidata".to_string(),
                    url: item,
                }],
                attributes: Vec::new(),
                related_topics: Vec::new(),
                engine: NAME.to_string(),
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

    fn query(q: &str, locale: &str) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno: 1,
            locale: locale.to_string(),
            ..SearchQueryView::default()
        }
    }

    fn infobox_result(label: &str, description: &str, item: &str) -> Result_ {
        Result_::Infobox(Infobox {
            infobox: label.to_string(),
            id: Some(replace_http_by_https(item)),
            content: description.to_string(),
            img_src: None,
            urls: vec![InfoboxUrl {
                title: "Wikidata".to_string(),
                url: item.to_string(),
            }],
            attributes: Vec::new(),
            related_topics: Vec::new(),
            engine: NAME.to_string(),
        })
    }

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: SPARQL_ENDPOINT_URL.to_string(),
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
      "results": {
        "bindings": [
          {
            "item": {"type": "uri", "value": "http://www.wikidata.org/entity/Q42"},
            "itemLabel": {"type": "literal", "value": "Douglas Adams"},
            "itemDescription": {"type": "literal", "value": "English author and humorist"}
          }
        ]
      }
    }"#;

    const DEDUP_JSON: &str = r#"{
      "results": {
        "bindings": [
          {
            "item": {"type": "uri", "value": "http://www.wikidata.org/entity/Q42"},
            "itemLabel": {"type": "literal", "value": "Douglas Adams"},
            "itemDescription": {"type": "literal", "value": "English author and humorist"}
          },
          {
            "item": {"type": "uri", "value": "http://www.wikidata.org/entity/Q42"},
            "itemLabel": {"type": "literal", "value": "Douglas Adams"},
            "itemDescription": {"type": "literal", "value": "English author and humorist"}
          }
        ]
      }
    }"#;

    const DUMMY_SKIPPED_JSON: &str = r#"{
      "results": {
        "bindings": [
          {
            "item": {"type": "uri", "value": "http://www.wikidata.org/entity/Q4115189"},
            "itemLabel": {"type": "literal", "value": "Wikidata Sandbox"},
            "itemDescription": {"type": "literal", "value": "dummy value"}
          },
          {
            "item": {"type": "uri", "value": "http://www.wikidata.org/entity/Q64"},
            "itemLabel": {"type": "literal", "value": "Berlin"},
            "itemDescription": {"type": "literal", "value": "capital and largest city of Germany"}
          }
        ]
      }
    }"#;

    const EMPTY_JSON: &str = r#"{"results": {"bindings": []}}"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(infobox_result(
            "Douglas Adams",
            "English author and humorist",
            "http://www.wikidata.org/entity/Q42",
        ));
        Fixture::capture(
            NAME,
            query("Douglas Adams", "all"),
            response(200, BASIC_JSON),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        let mut dedup = EngineResults::new();
        dedup.add(infobox_result(
            "Douglas Adams",
            "English author and humorist",
            "http://www.wikidata.org/entity/Q42",
        ));
        Fixture::capture(
            NAME,
            query("Douglas Adams", "all"),
            response(200, DEDUP_JSON),
            dedup,
        )
        .with_case("dedup")
        .save(dir.join("dedup.json"))
        .unwrap();

        let mut dummy = EngineResults::new();
        dummy.add(infobox_result(
            "Berlin",
            "capital and largest city of Germany",
            "http://www.wikidata.org/entity/Q64",
        ));
        Fixture::capture(
            NAME,
            query("Berlin", "all"),
            response(200, DUMMY_SKIPPED_JSON),
            dummy,
        )
        .with_case("dummy-skipped")
        .save(dir.join("dummy-skipped.json"))
        .unwrap();

        Fixture::capture(
            NAME,
            query("nothing", "all"),
            response(200, EMPTY_JSON),
            EngineResults::new(),
        )
        .with_case("empty")
        .save(dir.join("empty.json"))
        .unwrap();
    }

    #[test]
    fn wikidata_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Wikidata::new();
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
    fn builds_post_request_to_endpoint() {
        let engine = Wikidata::new();
        let q = query("Douglas Adams", "all");
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(p.method, HttpMethod::Post);
        assert_eq!(p.url.as_deref(), Some(SPARQL_ENDPOINT_URL));
        assert!(p.data.contains_key("query"));
        assert_eq!(
            p.headers.get("Accept").map(String::as_str),
            Some("application/sparql-results+json")
        );
        // We handle WDQS status codes ourselves rather than letting a 429
        // become an hour-long `TooManyRequests` suspension.
        assert!(!p.raise_for_httperror);
    }

    #[test]
    fn wdqs_rate_limit_is_a_transient_timeout_not_a_parse_error() {
        let engine = Wikidata::new();
        // A 429 body is HTML, not SPARQL JSON; it must not be parsed, and must
        // map to a short-suspension Timeout, not a Parse error or (worse) a
        // `TooManyRequests` that suspends for an hour.
        for status in [429, 500, 503] {
            let result = engine.response(&response(status, "<html>rate limited</html>"));
            assert!(
                matches!(result, Err(EngineError::Timeout)),
                "status {status} should map to Timeout, got {result:?}"
            );
        }
    }

    #[test]
    fn other_non_200_yields_empty_without_penalty() {
        let engine = Wikidata::new();
        // A 400 (e.g. malformed query) yields no infobox but no error, so the
        // engine is not suspended.
        let results = engine
            .response(&response(400, "bad request"))
            .expect("non-200 non-retryable is a soft miss");
        assert!(results.infoboxes.is_empty());
    }

    #[test]
    fn resolves_language_from_locale() {
        assert_eq!(resolve_language("de-DE"), "de");
        assert_eq!(resolve_language("all"), "en");
        assert_eq!(resolve_language(""), "en");
    }
}
