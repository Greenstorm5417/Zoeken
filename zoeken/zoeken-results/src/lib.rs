//! Typed result-object family and normalization operations.

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Template {
    #[default]
    Default,
    Answer,
    Images,
    Videos,
    Paper,
    Code,
    File,
    KeyValue,
    Infobox,
    Suggestion,
    Correction,
}

impl Template {
    pub fn as_str(&self) -> &'static str {
        match self {
            Template::Default => "default.html",
            Template::Answer => "answer/legacy.html",
            Template::Images => "images.html",
            Template::Videos => "videos.html",
            Template::Paper => "paper.html",
            Template::Code => "code.html",
            Template::File => "file.html",
            Template::KeyValue => "keyvalue.html",
            Template::Infobox => "infobox.html",
            Template::Suggestion => "suggestion.html",
            Template::Correction => "correction.html",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResultKind {
    Main,
    Answer,
    Image,
    Paper,
    Code,
    File,
    KeyValue,
    Suggestion,
    Correction,
    Infobox,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResultError {
    #[error("result of kind {kind:?} is missing required field '{field}'")]
    MissingField {
        kind: ResultKind,
        field: &'static str,
    },
    #[error("invalid url: {0}")]
    InvalidUrl(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Result_ {
    Main(MainResult),
    Answer(Answer),
    Image(Image),
    Paper(Paper),
    Code(Code),
    File(FileResult),
    KeyValue(KeyValue),
    Suggestion(Suggestion),
    Correction(Correction),
    Infobox(Infobox),
}

pub type ResultItem = Result_;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct MainResult {
    pub url: String,
    pub normalized_url: String,
    pub title: String,
    pub content: String,
    pub engine: String,
    #[serde(default)]
    pub engines: Vec<String>,
    pub score: f64,
    pub positions: Vec<usize>,
    #[serde(default)]
    pub priority: String,
    pub template: Template,
    /// Preview image for video / rich results (SearXNG `thumbnail`).
    #[serde(default)]
    pub thumbnail: String,
    /// Embeddable player URL when available (SearXNG `iframe_src`).
    #[serde(default)]
    pub iframe_src: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Answer {
    pub answer: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub engine: String,
    #[serde(default)]
    pub template: Template,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Image {
    pub url: String,
    pub normalized_url: String,
    pub title: String,
    pub content: String,
    pub engine: String,
    pub img_src: String,
    pub thumbnail_src: String,
    pub resolution: String,
    #[serde(default)]
    pub img_format: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub filesize: String,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub positions: Vec<usize>,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub template: Template,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Paper {
    pub url: String,
    pub normalized_url: String,
    pub title: String,
    pub content: String,
    pub engine: String,
    pub authors: Vec<String>,
    pub doi: String,
    pub journal: String,
    pub published_date: Option<String>,
    #[serde(default)]
    pub publisher: String,
    #[serde(default)]
    pub editor: String,
    #[serde(default)]
    pub volume: String,
    #[serde(default)]
    pub pages: String,
    #[serde(default)]
    pub number: String,
    #[serde(default, rename = "type")]
    pub type_: String,
    #[serde(default)]
    pub tags: Vec<String>,
    #[serde(default)]
    pub issn: Vec<String>,
    #[serde(default)]
    pub isbn: Vec<String>,
    #[serde(default)]
    pub pdf_url: String,
    #[serde(default)]
    pub html_url: String,
    #[serde(default)]
    pub comments: String,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub positions: Vec<usize>,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub template: Template,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Code {
    pub url: String,
    pub normalized_url: String,
    pub title: String,
    pub content: String,
    pub engine: String,
    pub repository: Option<String>,
    pub codelines: Vec<(usize, String)>,
    #[serde(default)]
    pub hl_lines: Vec<usize>,
    #[serde(default = "guess_language")]
    pub code_language: String,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub positions: Vec<usize>,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub template: Template,
}

fn guess_language() -> String {
    "<guess>".to_string()
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct FileResult {
    pub url: String,
    pub normalized_url: String,
    pub title: String,
    pub content: String,
    pub engine: String,
    pub filename: String,
    #[serde(default)]
    pub size: String,
    #[serde(default)]
    pub time: String,
    #[serde(default)]
    pub mimetype: String,
    #[serde(default, rename = "abstract")]
    pub abstract_: String,
    #[serde(default)]
    pub author: String,
    #[serde(default)]
    pub embedded: String,
    #[serde(default)]
    pub mtype: String,
    #[serde(default)]
    pub subtype: String,
    #[serde(default)]
    pub filesize: Option<String>,
    #[serde(default)]
    pub seed: Option<i64>,
    #[serde(default)]
    pub leech: Option<i64>,
    #[serde(default)]
    pub magnetlink: Option<String>,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub positions: Vec<usize>,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub template: Template,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct KeyValue {
    #[serde(default)]
    pub url: String,
    #[serde(default)]
    pub normalized_url: String,
    #[serde(default)]
    pub title: String,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub engine: String,
    pub kvmap: Vec<(String, String)>,
    #[serde(default)]
    pub caption: String,
    #[serde(default)]
    pub key_title: String,
    #[serde(default)]
    pub value_title: String,
    #[serde(default)]
    pub score: f64,
    #[serde(default)]
    pub positions: Vec<usize>,
    #[serde(default)]
    pub priority: String,
    #[serde(default)]
    pub template: Template,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Suggestion {
    pub suggestion: String,
    #[serde(default)]
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct Correction {
    pub correction: String,
    #[serde(default)]
    pub url: Option<String>,
    #[serde(default)]
    pub engine: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InfoboxUrl {
    pub title: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InfoboxImage {
    pub src: String,
    #[serde(default)]
    pub alt: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct InfoboxAttribute {
    pub label: String,
    #[serde(default)]
    pub value: String,
    #[serde(default)]
    pub image: Option<InfoboxImage>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, Default)]
pub struct Infobox {
    pub infobox: String,
    #[serde(default)]
    pub id: Option<String>,
    #[serde(default)]
    pub content: String,
    #[serde(default)]
    pub img_src: Option<String>,
    #[serde(default)]
    pub urls: Vec<InfoboxUrl>,
    #[serde(default)]
    pub attributes: Vec<InfoboxAttribute>,
    #[serde(default)]
    pub related_topics: Vec<String>,
    #[serde(default)]
    pub engine: String,
}

/// Normalize a result URL.
/// Missing schemes default to `http`, and the output is idempotent.
pub fn normalize_url(raw: &str) -> Result<String, ResultError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(ResultError::InvalidUrl("empty url".to_string()));
    }

    let parsed = match url::Url::parse(trimmed) {
        Ok(u) => u,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            url::Url::parse(&format!("http://{trimmed}"))
                .map_err(|e| ResultError::InvalidUrl(format!("{trimmed}: {e}")))?
        }
        Err(e) => return Err(ResultError::InvalidUrl(format!("{trimmed}: {e}"))),
    };

    match parsed.host_str() {
        Some(host) if !host.is_empty() => {}
        _ => return Err(ResultError::InvalidUrl(format!("missing host: {trimmed}"))),
    }

    Ok(parsed.as_str().to_string())
}

pub fn assign_template(kind: ResultKind) -> Template {
    match kind {
        ResultKind::Main => Template::Default,
        ResultKind::Answer => Template::Answer,
        ResultKind::Image => Template::Images,
        ResultKind::Paper => Template::Paper,
        ResultKind::Code => Template::Code,
        ResultKind::File => Template::File,
        ResultKind::KeyValue => Template::KeyValue,
        ResultKind::Suggestion => Template::Suggestion,
        ResultKind::Correction => Template::Correction,
        ResultKind::Infobox => Template::Infobox,
    }
}

pub fn validate(result: &Result_) -> Result<(), ResultError> {
    fn require_str(kind: ResultKind, field: &'static str, value: &str) -> Result<(), ResultError> {
        if value.trim().is_empty() {
            Err(ResultError::MissingField { kind, field })
        } else {
            Ok(())
        }
    }

    match result {
        Result_::Main(r) => {
            require_str(ResultKind::Main, "url", &r.url)?;
        }
        Result_::Answer(r) => {
            require_str(ResultKind::Answer, "answer", &r.answer)?;
        }
        Result_::Image(r) => {
            require_str(ResultKind::Image, "url", &r.url)?;
            require_str(ResultKind::Image, "img_src", &r.img_src)?;
            require_str(ResultKind::Image, "thumbnail_src", &r.thumbnail_src)?;
            require_str(ResultKind::Image, "resolution", &r.resolution)?;
        }
        Result_::Paper(r) => {
            require_str(ResultKind::Paper, "url", &r.url)?;
            if r.authors.is_empty() {
                return Err(ResultError::MissingField {
                    kind: ResultKind::Paper,
                    field: "authors",
                });
            }
            require_str(ResultKind::Paper, "doi", &r.doi)?;
            require_str(ResultKind::Paper, "journal", &r.journal)?;
            match &r.published_date {
                Some(d) => require_str(ResultKind::Paper, "published_date", d)?,
                None => {
                    return Err(ResultError::MissingField {
                        kind: ResultKind::Paper,
                        field: "published_date",
                    });
                }
            }
        }
        Result_::Code(r) => {
            require_str(ResultKind::Code, "url", &r.url)?;
            match &r.repository {
                Some(repo) => require_str(ResultKind::Code, "repository", repo)?,
                None => {
                    return Err(ResultError::MissingField {
                        kind: ResultKind::Code,
                        field: "repository",
                    });
                }
            }
            if r.codelines.is_empty() {
                return Err(ResultError::MissingField {
                    kind: ResultKind::Code,
                    field: "codelines",
                });
            }
        }
        Result_::File(r) => {
            require_str(ResultKind::File, "url", &r.url)?;
            require_str(ResultKind::File, "filename", &r.filename)?;
        }
        Result_::KeyValue(r) => {
            if r.kvmap.is_empty() {
                return Err(ResultError::MissingField {
                    kind: ResultKind::KeyValue,
                    field: "kvmap",
                });
            }
        }
        Result_::Suggestion(r) => {
            require_str(ResultKind::Suggestion, "suggestion", &r.suggestion)?;
        }
        Result_::Correction(r) => {
            require_str(ResultKind::Correction, "correction", &r.correction)?;
        }
        Result_::Infobox(r) => {
            require_str(ResultKind::Infobox, "infobox", &r.infobox)?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalize_url_defaults_missing_scheme_to_http() {
        assert_eq!(
            normalize_url("example.com/path").unwrap(),
            "http://example.com/path"
        );
    }

    #[test]
    fn normalize_url_lowercases_scheme_and_host() {
        assert_eq!(
            normalize_url("HTTP://Example.COM/Path").unwrap(),
            "http://example.com/Path"
        );
    }

    #[test]
    fn normalize_url_removes_default_ports() {
        assert_eq!(
            normalize_url("http://example.com:80/").unwrap(),
            "http://example.com/"
        );
        assert_eq!(
            normalize_url("https://example.com:443/").unwrap(),
            "https://example.com/"
        );
    }

    #[test]
    fn normalize_url_resolves_dot_segments() {
        assert_eq!(
            normalize_url("http://example.com/a/../b").unwrap(),
            "http://example.com/b"
        );
    }

    #[test]
    fn normalize_url_adds_trailing_slash_for_empty_path() {
        assert_eq!(
            normalize_url("https://example.com").unwrap(),
            "https://example.com/"
        );
    }

    #[test]
    fn normalize_url_preserves_fragment_and_query() {
        assert_eq!(
            normalize_url("https://example.com/p?a=1#frag").unwrap(),
            "https://example.com/p?a=1#frag"
        );
    }

    #[test]
    fn normalize_url_is_idempotent() {
        for raw in [
            "example.com",
            "HTTP://Example.COM:80/a/../b?z=1#f",
            "https://example.com/path/",
            "ftp://host/dir",
        ] {
            let once = normalize_url(raw).unwrap();
            let twice = normalize_url(&once).unwrap();
            assert_eq!(once, twice, "normalize_url not idempotent for {raw:?}");
        }
    }

    #[test]
    fn normalize_url_rejects_empty_and_hostless() {
        assert!(normalize_url("").is_err());
        assert!(normalize_url("   ").is_err());
        assert!(normalize_url("mailto:a@b.com").is_err());
    }

    #[test]
    fn assign_template_maps_each_kind() {
        assert_eq!(assign_template(ResultKind::Main), Template::Default);
        assert_eq!(assign_template(ResultKind::Answer), Template::Answer);
        assert_eq!(assign_template(ResultKind::Image), Template::Images);
        assert_eq!(assign_template(ResultKind::Paper), Template::Paper);
        assert_eq!(assign_template(ResultKind::Code), Template::Code);
        assert_eq!(assign_template(ResultKind::File), Template::File);
        assert_eq!(assign_template(ResultKind::KeyValue), Template::KeyValue);
        assert_eq!(
            assign_template(ResultKind::Suggestion),
            Template::Suggestion
        );
        assert_eq!(
            assign_template(ResultKind::Correction),
            Template::Correction
        );
        assert_eq!(assign_template(ResultKind::Infobox), Template::Infobox);
    }

    #[test]
    fn validate_accepts_complete_main_result() {
        let r = Result_::Main(MainResult {
            url: "http://example.com/".to_string(),
            title: "t".to_string(),
            ..Default::default()
        });
        assert!(validate(&r).is_ok());
    }

    #[test]
    fn validate_rejects_main_without_url() {
        let r = Result_::Main(MainResult::default());
        assert_eq!(
            validate(&r),
            Err(ResultError::MissingField {
                kind: ResultKind::Main,
                field: "url"
            })
        );
    }

    #[test]
    fn validate_rejects_image_missing_required_field() {
        let r = Result_::Image(Image {
            url: "http://example.com/".to_string(),
            img_src: "http://example.com/i.png".to_string(),
            thumbnail_src: String::new(),
            resolution: "100x100".to_string(),
            ..Default::default()
        });
        assert_eq!(
            validate(&r),
            Err(ResultError::MissingField {
                kind: ResultKind::Image,
                field: "thumbnail_src"
            })
        );
    }

    #[test]
    fn validate_rejects_keyvalue_empty_map() {
        let r = Result_::KeyValue(KeyValue::default());
        assert_eq!(
            validate(&r),
            Err(ResultError::MissingField {
                kind: ResultKind::KeyValue,
                field: "kvmap"
            })
        );
    }

    #[test]
    fn validate_accepts_complete_answer() {
        let r = Result_::Answer(Answer {
            answer: "42".to_string(),
            ..Default::default()
        });
        assert!(validate(&r).is_ok());
    }
}

#[cfg(test)]
mod prop_tests {
    use super::*;
    use proptest::prelude::*;

    const ALL_KINDS: [ResultKind; 10] = [
        ResultKind::Main,
        ResultKind::Answer,
        ResultKind::Image,
        ResultKind::Paper,
        ResultKind::Code,
        ResultKind::File,
        ResultKind::KeyValue,
        ResultKind::Suggestion,
        ResultKind::Correction,
        ResultKind::Infobox,
    ];

    prop_compose! {
        fn arb_url_like()(
            scheme in prop::option::of(prop_oneof!["http", "https", "HTTP", "ftp"]),
            host in "[a-z][a-z0-9]{0,10}(\\.[a-z]{2,4}){0,2}",
            port in prop::option::of(prop_oneof![Just(80u16), Just(443), Just(8080), Just(21)]),
            segments in prop::collection::vec(
                prop_oneof![Just(".".to_string()), Just("..".to_string()), "[a-zA-Z0-9._-]{1,8}"],
                0..5,
            ),
            query in prop::option::of("[a-zA-Z0-9=&._-]{1,16}"),
            fragment in prop::option::of("[a-zA-Z0-9._-]{1,10}"),
        ) -> String {
            let mut s = String::new();
            if let Some(sc) = scheme {
                s.push_str(&sc);
                s.push_str("://");
            }
            s.push_str(&host);
            if let Some(p) = port {
                s.push(':');
                s.push_str(&p.to_string());
            }
            for seg in &segments {
                s.push('/');
                s.push_str(seg);
            }
            if let Some(q) = query {
                s.push('?');
                s.push_str(&q);
            }
            if let Some(f) = fragment {
                s.push('#');
                s.push_str(&f);
            }
            s
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(256))]

        #[test]
        fn normalize_url_is_idempotent(raw in arb_url_like()) {
            if let Ok(once) = normalize_url(&raw) {
                let twice = normalize_url(&once)
                    .expect("normalized url must itself normalize");
                prop_assert_eq!(&twice, &once);

                for kind in ALL_KINDS {
                    let _template = assign_template(kind);
                    let after = normalize_url(&raw)
                        .expect("normalization is deterministic and template-independent");
                    prop_assert_eq!(&after, &once);
                }
            }
        }

        #[test]
        fn normalize_url_is_idempotent_for_arbitrary_strings(raw in ".*") {
            if let Ok(once) = normalize_url(&raw) {
                let twice = normalize_url(&once)
                    .expect("normalized url must itself normalize");
                prop_assert_eq!(&twice, &once);

                for kind in ALL_KINDS {
                    let _template = assign_template(kind);
                    let again = normalize_url(&once)
                        .expect("normalized url must itself normalize");
                    prop_assert_eq!(&again, &once);
                }
            }
        }
    }
}
