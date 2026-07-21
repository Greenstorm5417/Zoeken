//! Dictionary engine backed by the Wiktionary REST definition API.
//!
//! Only fires on `define <word>` / `definition of <word>` / `meaning of
//! <word>` queries; other queries produce no request.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Answer, InteractiveAnswer, Result_};

/// Engine name / identifier.
pub const NAME: &str = "dictionary";

const BASE_URL: &str = "https://en.wiktionary.org/api/rest_v1/page/definition";

const MAX_DEFINITIONS: usize = 3;

/// The word to define, or `None` when the query is not dictionary-shaped.
pub fn define_term(query: &str) -> Option<String> {
    let lower = query.trim().to_ascii_lowercase();
    let term = lower
        .strip_prefix("define ")
        .or_else(|| lower.strip_prefix("definition of "))
        .or_else(|| lower.strip_prefix("meaning of "))?
        .trim();
    // Single words / short phrases only; long strings are not lookups.
    (!term.is_empty() && term.split_whitespace().count() <= 3).then(|| term.to_string())
}

/// Strip HTML tags and entities from a Wiktionary definition string.
fn strip_html(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut in_tag = false;
    for c in input.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The Wiktionary dictionary engine.
#[derive(Debug, Clone)]
pub struct Dictionary {
    meta: EngineMeta,
}

impl Dictionary {
    pub fn new() -> Self {
        Dictionary {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "def".to_string(),
                about: About {
                    website: Some("https://en.wiktionary.org/".to_string()),
                    wikidata_id: None,
                    official_api_documentation: Some(
                        "https://en.wiktionary.org/api/rest_v1/".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Dictionary {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for Dictionary {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        let Some(term) = define_term(&q.query) else {
            return;
        };
        if q.pageno > 1 {
            return;
        }
        p.method = HttpMethod::Get;
        let encoded: String =
            url::form_urlencoded::byte_serialize(term.replace(' ', "_").as_bytes()).collect();
        p.url = Some(format!("{BASE_URL}/{encoded}?redirect=true"));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Wiktionary JSON: {e}")))?;

        // Response shape: {"en": [{"partOfSpeech": "...", "definitions":
        // [{"definition": "<html>"}]}], ...}
        let entries = value
            .get("en")
            .or_else(|| value.as_object().and_then(|map| map.values().next()))
            .and_then(|v| v.as_array())
            .ok_or_else(|| EngineError::Parse("no definition entries".to_string()))?;

        let term = resp
            .url
            .rsplit('/')
            .next()
            .and_then(|tail| tail.split('?').next())
            .unwrap_or("")
            .replace('_', " ");

        let mut lines = Vec::new();
        let mut defs = Vec::new();
        for entry in entries {
            let part = entry
                .get("partOfSpeech")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let definitions = entry
                .get("definitions")
                .and_then(|v| v.as_array())
                .cloned()
                .unwrap_or_default();
            for definition in definitions {
                let text = definition
                    .get("definition")
                    .and_then(|v| v.as_str())
                    .map(strip_html)
                    .unwrap_or_default();
                if text.is_empty() {
                    continue;
                }
                let numbered = lines.len() + 1;
                if part.is_empty() {
                    lines.push(format!("{numbered}. {text}"));
                    defs.push(text);
                } else {
                    lines.push(format!("{numbered}. ({part}) {text}"));
                    defs.push(format!("({part}) {text}"));
                }
                if lines.len() >= MAX_DEFINITIONS {
                    break;
                }
            }
            if lines.len() >= MAX_DEFINITIONS {
                break;
            }
        }

        if lines.is_empty() {
            return Err(EngineError::Parse("no usable definitions".to_string()));
        }

        res.add(Result_::Answer(Answer {
            answer: format!("{term}: {}", lines.join(" ")),
            url: Some(format!(
                "https://en.wiktionary.org/wiki/{}",
                term.replace(' ', "_")
            )),
            engine: NAME.to_string(),
            interactive: Some(InteractiveAnswer::Dictionary {
                term: term.clone(),
                definitions: defs,
            }),
            ..Answer::default()
        }));

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const WIKTIONARY_JSON: &str = r#"{
      "en": [
        {
          "partOfSpeech": "Noun",
          "definitions": [
            {"definition": "A <b>fortunate</b> discovery made by accident."},
            {"definition": "Good luck in making unexpected finds."}
          ]
        }
      ]
    }"#;

    #[test]
    fn detects_dictionary_queries() {
        assert_eq!(
            define_term("define serendipity"),
            Some("serendipity".to_string())
        );
        assert_eq!(
            define_term("definition of ad hoc"),
            Some("ad hoc".to_string())
        );
        assert_eq!(define_term("meaning of life"), Some("life".to_string()));
        assert_eq!(define_term("define"), None, "no term");
        assert_eq!(define_term("rust programming"), None);
        assert_eq!(
            define_term("define a very long phrase that is not a lookup"),
            None
        );
    }

    #[test]
    fn non_dictionary_query_builds_no_request() {
        let engine = Dictionary::new();
        let q = SearchQueryView {
            query: "rust tutorial".to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        };
        let mut p = RequestParams::default();
        engine.request(&q, &mut p);
        assert!(p.url.is_none());
    }

    #[test]
    fn builds_wiktionary_url() {
        let engine = Dictionary::new();
        let q = SearchQueryView {
            query: "define serendipity".to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        };
        let mut p = RequestParams::default();
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://en.wiktionary.org/api/rest_v1/page/definition/serendipity?redirect=true")
        );
    }

    #[test]
    fn parses_definitions_into_an_answer() {
        let engine = Dictionary::new();
        let resp = EngineResponse {
            status: 200,
            url: format!("{BASE_URL}/serendipity?redirect=true"),
            body: WIKTIONARY_JSON.as_bytes().to_vec(),
            ..EngineResponse::default()
        };
        let results = engine.response(&resp).unwrap();
        assert_eq!(results.answers.len(), 1);
        assert_eq!(
            results.answers[0].answer,
            "serendipity: 1. (Noun) A fortunate discovery made by accident. \
             2. (Noun) Good luck in making unexpected finds."
        );
        assert_eq!(
            results.answers[0].url.as_deref(),
            Some("https://en.wiktionary.org/wiki/serendipity")
        );
        assert_eq!(
            results.answers[0].interactive,
            Some(InteractiveAnswer::Dictionary {
                term: "serendipity".to_string(),
                definitions: vec![
                    "(Noun) A fortunate discovery made by accident.".to_string(),
                    "(Noun) Good luck in making unexpected finds.".to_string(),
                ],
            })
        );
    }

    #[test]
    fn strip_html_removes_tags_and_entities() {
        assert_eq!(strip_html("a <b>bold</b> claim"), "a bold claim");
        assert_eq!(strip_html("x &amp; y"), "x & y");
        assert_eq!(strip_html("<a href=\"z\">link</a>"), "link");
    }

    #[test]
    fn empty_definitions_are_a_parse_error() {
        let engine = Dictionary::new();
        let resp = EngineResponse {
            status: 200,
            url: format!("{BASE_URL}/x?redirect=true"),
            body: br#"{"en": []}"#.to_vec(),
            ..EngineResponse::default()
        };
        assert!(engine.response(&resp).is_err());
    }
}
