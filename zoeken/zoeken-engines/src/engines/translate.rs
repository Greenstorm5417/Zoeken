//! Translation engine backed by the MyMemory API (no key required).
//!
//! Only fires on `translate <text> to <language>` queries; the target
//! language rides in the request URL's `langpair` so the response step can
//! recover it.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Answer, InteractiveAnswer, Result_};

/// Engine name / identifier.
pub const NAME: &str = "translate";

const BASE_URL: &str = "https://api.mymemory.translated.net/get";

/// `(language name, ISO 639-1 code)`.
const LANGUAGES: &[(&str, &str)] = &[
    ("english", "en"),
    ("spanish", "es"),
    ("french", "fr"),
    ("german", "de"),
    ("dutch", "nl"),
    ("italian", "it"),
    ("portuguese", "pt"),
    ("russian", "ru"),
    ("japanese", "ja"),
    ("chinese", "zh"),
    ("korean", "ko"),
    ("arabic", "ar"),
    ("hindi", "hi"),
    ("turkish", "tr"),
    ("polish", "pl"),
    ("swedish", "sv"),
    ("norwegian", "no"),
    ("danish", "da"),
    ("finnish", "fi"),
    ("greek", "el"),
    ("czech", "cs"),
    ("ukrainian", "uk"),
    ("romanian", "ro"),
    ("hungarian", "hu"),
    ("hebrew", "he"),
    ("thai", "th"),
    ("vietnamese", "vi"),
    ("indonesian", "id"),
];

fn language_code(name: &str) -> Option<&'static str> {
    let needle = name.trim().to_ascii_lowercase();
    LANGUAGES
        .iter()
        .find(|(lang, code)| *lang == needle || *code == needle)
        .map(|(_, code)| *code)
}

fn language_name(code: &str) -> &str {
    LANGUAGES
        .iter()
        .find(|(_, c)| *c == code)
        .map(|(name, _)| *name)
        .unwrap_or(code)
}

/// Parse `translate <text> to <language>`.
pub fn parse_translate_query(query: &str) -> Option<(String, &'static str)> {
    let lower = query.trim().to_ascii_lowercase();
    let rest = lower.strip_prefix("translate ")?;
    // Split on the last ` to ` so the text itself may contain "to".
    let (text, lang) = rest
        .rsplit_once(" to ")
        .or_else(|| rest.rsplit_once(" into "))?;
    let code = language_code(lang)?;
    let text = text.trim().trim_matches('"').trim();
    (!text.is_empty() && text.len() <= 200).then(|| (text.to_string(), code))
}

fn marker_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

/// The MyMemory translation engine.
#[derive(Debug, Clone)]
pub struct Translate {
    meta: EngineMeta,
}

impl Translate {
    pub fn new() -> Self {
        Translate {
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
                shortcut: "tr".to_string(),
                about: About {
                    website: Some("https://mymemory.translated.net/".to_string()),
                    wikidata_id: None,
                    official_api_documentation: Some(
                        "https://mymemory.translated.net/doc/spec.php".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Translate {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for Translate {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        let Some((text, target)) = parse_translate_query(&q.query) else {
            return;
        };
        if q.pageno > 1 {
            return;
        }
        p.method = HttpMethod::Get;
        let query = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("q", &text)
            .append_pair("langpair", &format!("Autodetect|{target}"))
            .finish();
        p.url = Some(format!("{BASE_URL}?{query}"));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid MyMemory JSON: {e}")))?;

        let translated = value
            .pointer("/responseData/translatedText")
            .and_then(|v| v.as_str())
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .ok_or_else(|| EngineError::Parse("no translation in response".to_string()))?;

        let source = marker_param(&resp.url, "q").unwrap_or_default();
        let target = marker_param(&resp.url, "langpair")
            .and_then(|pair| pair.split('|').next_back().map(str::to_string))
            .unwrap_or_default();
        let target_label = {
            let mut label = language_name(&target).to_string();
            if let Some(first) = label.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            label
        };

        res.add(Result_::Answer(Answer {
            answer: format!("“{source}” in {target_label}: {translated}"),
            url: Some(format!(
                "https://mymemory.translated.net/en/Autodetect/{target}/{}",
                url::form_urlencoded::byte_serialize(source.as_bytes()).collect::<String>()
            )),
            engine: NAME.to_string(),
            interactive: Some(InteractiveAnswer::Translate {
                source: source.clone(),
                target_lang: target.clone(),
                translated: translated.to_string(),
            }),
            ..Answer::default()
        }));

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_translation_queries() {
        assert_eq!(
            parse_translate_query("translate hello to spanish"),
            Some(("hello".to_string(), "es"))
        );
        assert_eq!(
            parse_translate_query("translate good morning to french"),
            Some(("good morning".to_string(), "fr"))
        );
        assert_eq!(
            parse_translate_query("translate welcome to the jungle to german"),
            Some(("welcome to the jungle".to_string(), "de"))
        );
        assert_eq!(
            parse_translate_query("translate hallo into english"),
            Some(("hallo".to_string(), "en"))
        );
        assert_eq!(parse_translate_query("translate hello to klingon"), None);
        assert_eq!(parse_translate_query("hello world"), None);
        assert_eq!(parse_translate_query("translate to spanish"), None);
    }

    #[test]
    fn non_translation_query_builds_no_request() {
        let engine = Translate::new();
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
    fn builds_mymemory_url() {
        let engine = Translate::new();
        let q = SearchQueryView {
            query: "translate hello to spanish".to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        };
        let mut p = RequestParams::default();
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://api.mymemory.translated.net/get?q=hello&langpair=Autodetect%7Ces")
        );
    }

    #[test]
    fn parses_translation_into_an_answer() {
        let engine = Translate::new();
        let resp = EngineResponse {
            status: 200,
            url: format!("{BASE_URL}?q=hello&langpair=Autodetect%7Ces"),
            body: br#"{"responseData":{"translatedText":"hola"},"responseStatus":200}"#.to_vec(),
            ..EngineResponse::default()
        };
        let results = engine.response(&resp).unwrap();
        assert_eq!(results.answers.len(), 1);
        assert_eq!(results.answers[0].answer, "“hello” in Spanish: hola");
        assert_eq!(results.answers[0].engine, NAME);
        assert_eq!(
            results.answers[0].interactive,
            Some(InteractiveAnswer::Translate {
                source: "hello".to_string(),
                target_lang: "es".to_string(),
                translated: "hola".to_string(),
            })
        );
    }

    #[test]
    fn empty_translation_is_a_parse_error() {
        let engine = Translate::new();
        let resp = EngineResponse {
            status: 200,
            url: format!("{BASE_URL}?q=x&langpair=Autodetect%7Ces"),
            body: br#"{"responseData":{"translatedText":""}}"#.to_vec(),
            ..EngineResponse::default()
        };
        assert!(engine.response(&resp).is_err());
    }
}
