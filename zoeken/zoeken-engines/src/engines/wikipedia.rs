//! Wikipedia search engine.
//!
//! Queries the page summary API and maps standard pages to infoboxes.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod,
    LocaleTranslate, Processor, RequestParams, SearchQueryView,
};
use zoeken_results::{
    Answer, Infobox, InfoboxAttribute, InfoboxUrl, InteractiveAnswer, MainResult, Result_,
};

use super::util::encode_path;

pub const NAME: &str = "wikipedia";

#[derive(Debug, Clone)]
pub struct Wikipedia {
    meta: EngineMeta,
}

impl Wikipedia {
    pub fn new() -> Self {
        Wikipedia {
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
                shortcut: "wp".to_string(),
                about: About {
                    website: Some("https://www.wikipedia.org/".to_string()),
                    wikidata_id: Some("Q52".to_string()),
                    official_api_documentation: Some("https://en.wikipedia.org/api/".to_string()),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Wikipedia {
    fn default() -> Self {
        Self::new()
    }
}

/// Title-case a string the way Python's `str.title()` does: the first letter of
/// each run of alphabetic characters is uppercased and the rest lowercased.
fn python_title(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_alpha = false;
    for ch in s.chars() {
        if ch.is_alphabetic() {
            if prev_alpha {
                out.extend(ch.to_lowercase());
            } else {
                out.extend(ch.to_uppercase());
            }
            prev_alpha = true;
        } else {
            out.push(ch);
            prev_alpha = false;
        }
    }
    out
}

/// Whether every cased character in `s` is lowercase (mirrors `str.islower()`:
/// there is at least one cased character and none are uppercase/title-case).
fn is_lower(s: &str) -> bool {
    let mut has_cased = false;
    for ch in s.chars() {
        if ch.is_uppercase() {
            return false;
        }
        if ch.is_lowercase() {
            has_cased = true;
        }
    }
    has_cased
}

/// Derive the Wikipedia netloc (`<lang>.wikipedia.org`) from a Upstream locale,
/// defaulting to `en.wikipedia.org`. This is the fallback used when bundled
/// engine traits are unavailable.
fn wiki_netloc(locale: &str) -> String {
    if locale.is_empty() || locale == "all" {
        return "en.wikipedia.org".to_string();
    }
    let lang = locale.split(['-', '_']).next().unwrap_or("en");
    let lang = if lang.is_empty() { "en" } else { lang };
    format!("{}.wikipedia.org", lang.to_lowercase())
}

/// Resolve the Wikipedia netloc for `locale` using bundled engine traits,
/// mirroring upstream's `get_wiki_params`: the engine tag is the trait region
/// (falling back to the trait language, then `"en"`), and the netloc is looked
/// up from the `wiki_netloc` custom map, defaulting to `en.wikipedia.org` for
/// tags without a dedicated Wikipedia (e.g. LanguageConverter variants).
fn resolve_wiki_netloc(traits: Option<&zoeken_data::EngineTraits>, locale: &str) -> String {
    let Some(traits) = traits else {
        return wiki_netloc(locale);
    };

    let lang_default = traits
        .get_language(locale, Some("en"))
        .unwrap_or_else(|| "en".to_string());
    let eng_tag = traits
        .get_region(locale, Some(lang_default.as_str()))
        .unwrap_or(lang_default);

    traits
        .custom
        .get("wiki_netloc")
        .and_then(|map| map.get(&eng_tag))
        .and_then(|v| v.as_str())
        .map(str::to_string)
        .unwrap_or_else(|| "en.wikipedia.org".to_string())
}

impl Engine for Wikipedia {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        let query = if is_lower(&q.query) {
            python_title(&q.query)
        } else {
            q.query.clone()
        };

        let netloc = resolve_wiki_netloc(zoeken_engine_core::engine_traits(NAME), &q.locale);
        let title = encode_path(&query);

        p.method = HttpMethod::Get;
        p.url = Some(format!("https://{netloc}/api/rest_v1/page/summary/{title}"));
        p.raise_for_httperror = false;
        p.soft_max_redirects = 2;
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        if resp.status == 404 {
            return Ok(res);
        }

        if resp.status == 400
            && let Ok(api) = serde_json::from_slice::<serde_json::Value>(&resp.body)
        {
            let bad_request = api.get("type").and_then(|t| t.as_str())
                == Some("https://mediawiki.org/wiki/HyperSwitch/errors/bad_request");
            let invalid_chars =
                api.get("detail").and_then(|d| d.as_str()) == Some("title-invalid-characters");
            if bad_request && invalid_chars {
                return Ok(res);
            }
        }

        let api: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Wikipedia JSON: {e}")))?;

        let raw_title = api
            .get("titles")
            .and_then(|t| t.get("display"))
            .and_then(|d| d.as_str())
            .or_else(|| api.get("title").and_then(|t| t.as_str()))
            .unwrap_or("");
        let title = zoeken_engine_core::html_to_text(raw_title);

        let wikipedia_link = api
            .get("content_urls")
            .and_then(|c| c.get("desktop"))
            .and_then(|d| d.get("page"))
            .and_then(|p| p.as_str())
            .unwrap_or("")
            .to_string();

        let page_type = api.get("type").and_then(|t| t.as_str());
        let description = api
            .get("description")
            .and_then(|d| d.as_str())
            .unwrap_or("")
            .to_string();

        // With display_type = ["infobox"], a list hit is added only for a
        // non-standard page type.
        if page_type != Some("standard") {
            res.add(Result_::Main(MainResult {
                url: wikipedia_link.clone(),
                normalized_url: wikipedia_link.clone(),
                title: title.clone(),
                content: description.clone(),
                engine: NAME.to_string(),
                ..MainResult::default()
            }));
        }

        if page_type == Some("standard") {
            let extract = api
                .get("extract")
                .and_then(|e| e.as_str())
                .unwrap_or("")
                .to_string();
            let img_src = api
                .get("thumbnail")
                .and_then(|t| t.get("source"))
                .and_then(|s| s.as_str())
                .map(str::to_string);
            let mut attributes = Vec::new();
            if !description.is_empty() {
                attributes.push(InfoboxAttribute {
                    label: "Description".to_string(),
                    value: description.clone(),
                    image: None,
                });
            }
            res.add(Result_::Answer(Answer {
                answer: if extract.is_empty() {
                    title.clone()
                } else {
                    format!("{title}: {extract}")
                },
                url: Some(wikipedia_link.clone()),
                engine: NAME.to_string(),
                interactive: Some(InteractiveAnswer::Wikipedia {
                    title: title.clone(),
                    extract: extract.clone(),
                    description: description.clone(),
                    img_src: img_src.clone().unwrap_or_default(),
                    url: wikipedia_link.clone(),
                }),
                ..Answer::default()
            }));
            res.add(Result_::Infobox(Infobox {
                infobox: title,
                id: Some(wikipedia_link.clone()),
                content: extract,
                img_src,
                urls: vec![InfoboxUrl {
                    title: "Wikipedia".to_string(),
                    url: wikipedia_link,
                }],
                attributes,
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

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: "https://en.wikipedia.org/".to_string(),
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

    const STANDARD_JSON: &str = r#"{
      "type": "standard",
      "title": "Rust (programming language)",
      "titles": {"display": "Rust (programming language)"},
      "description": "General-purpose programming language",
      "extract": "Rust is a multi-paradigm, general-purpose programming language.",
      "thumbnail": {"source": "https://upload.wikimedia.org/rust.png"},
      "content_urls": {"desktop": {"page": "https://en.wikipedia.org/wiki/Rust_(programming_language)"}}
    }"#;

    const DISAMBIGUATION_JSON: &str = r#"{
      "type": "disambiguation",
      "title": "Rust",
      "titles": {"display": "Rust"},
      "description": "Topic list",
      "extract": "Rust may refer to ...",
      "content_urls": {"desktop": {"page": "https://en.wikipedia.org/wiki/Rust"}}
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut infobox = EngineResults::new();
        infobox.add(Result_::Answer(Answer {
            answer: "Rust (programming language): Rust is a multi-paradigm, general-purpose programming language.".to_string(),
            url: Some("https://en.wikipedia.org/wiki/Rust_(programming_language)".to_string()),
            engine: NAME.to_string(),
            interactive: Some(InteractiveAnswer::Wikipedia {
                title: "Rust (programming language)".to_string(),
                extract: "Rust is a multi-paradigm, general-purpose programming language."
                    .to_string(),
                description: "General-purpose programming language".to_string(),
                img_src: "https://upload.wikimedia.org/rust.png".to_string(),
                url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".to_string(),
            }),
            ..Answer::default()
        }));
        infobox.add(Result_::Infobox(Infobox {
            infobox: "Rust (programming language)".to_string(),
            id: Some("https://en.wikipedia.org/wiki/Rust_(programming_language)".to_string()),
            content: "Rust is a multi-paradigm, general-purpose programming language.".to_string(),
            img_src: Some("https://upload.wikimedia.org/rust.png".to_string()),
            urls: vec![InfoboxUrl {
                title: "Wikipedia".to_string(),
                url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".to_string(),
            }],
            attributes: vec![InfoboxAttribute {
                label: "Description".to_string(),
                value: "General-purpose programming language".to_string(),
                image: None,
            }],
            related_topics: Vec::new(),
            engine: NAME.to_string(),
        }));
        Fixture::capture(
            NAME,
            query("Rust (programming language)", "all"),
            response(200, STANDARD_JSON),
            infobox,
        )
        .with_case("standard-infobox")
        .save(dir.join("standard-infobox.json"))
        .unwrap();

        let mut list = EngineResults::new();
        list.add(Result_::Main(MainResult {
            url: "https://en.wikipedia.org/wiki/Rust".to_string(),
            normalized_url: "https://en.wikipedia.org/wiki/Rust".to_string(),
            title: "Rust".to_string(),
            content: "Topic list".to_string(),
            engine: NAME.to_string(),
            ..MainResult::default()
        }));
        Fixture::capture(
            NAME,
            query("Rust", "all"),
            response(200, DISAMBIGUATION_JSON),
            list,
        )
        .with_case("disambiguation-list")
        .save(dir.join("disambiguation-list.json"))
        .unwrap();

        Fixture::capture(
            NAME,
            query("Nonexistent", "all"),
            response(404, ""),
            EngineResults::new(),
        )
        .with_case("status-404")
        .save(dir.join("status-404.json"))
        .unwrap();

        let q = query("rust language", "all");
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.raise_for_httperror = false;
        golden.soft_max_redirects = 2;
        golden.url =
            Some("https://en.wikipedia.org/api/rest_v1/page/summary/Rust%20Language".to_string());
        Fixture::capture(NAME, q.clone(), response(404, ""), EngineResults::new())
            .with_case("request-title-case")
            .with_golden_request(golden)
            .save(dir.join("request-title-case.json"))
            .unwrap();
    }

    #[test]
    fn wikipedia_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Wikipedia::new();
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
    fn title_cases_lowercase_query() {
        assert_eq!(python_title("rust language"), "Rust Language");
        assert!(is_lower("rust language"));
        assert!(!is_lower("Rust"));
    }

    #[test]
    fn derives_netloc_from_language() {
        assert_eq!(wiki_netloc("de-DE"), "de.wikipedia.org");
        assert_eq!(wiki_netloc("all"), "en.wikipedia.org");
    }

    /// Cross-checks `resolve_wiki_netloc` against bundled `engine_traits.json`
    /// (mirrors upstream's `get_wiki_params`/LanguageConverter handling: the
    /// `zh-*` variants all resolve through the `zh` engine tag rather than
    /// producing their own subdomain).
    #[test]
    fn resolve_wiki_netloc_uses_bundled_traits() {
        let traits = zoeken_engine_core::engine_traits(NAME);
        assert!(traits.is_some(), "wikipedia traits should be bundled");

        assert_eq!(resolve_wiki_netloc(traits, "de-DE"), "de.wikipedia.org");
        assert_eq!(resolve_wiki_netloc(traits, "all"), "en.wikipedia.org");
        assert_eq!(resolve_wiki_netloc(traits, "zh-CN"), "zh.wikipedia.org");
        assert_eq!(resolve_wiki_netloc(traits, "zh-TW"), "zh.wikipedia.org");
    }
}
