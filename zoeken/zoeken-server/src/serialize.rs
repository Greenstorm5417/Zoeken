//! `/search` response serialization.

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use zoeken_results::{Answer, Infobox, Result_, Template};
use zoeken_search::{ResultContainer, UnresponsiveCause};

#[derive(Debug, Clone, Copy)]
pub struct ProxySettings<'a> {
    pub secret_key: &'a str,
    pub image_proxy: bool,
    pub favicon_proxy: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum OutputFormat {
    #[default]
    Html,
    Json,
    Csv,
    Rss,
}

impl OutputFormat {
    pub fn from_param(value: Option<&str>) -> Result<OutputFormat, UnsupportedFormat> {
        match value.map(str::trim) {
            None | Some("") => Ok(OutputFormat::Html),
            Some(raw) if raw.eq_ignore_ascii_case("html") => Ok(OutputFormat::Html),
            Some(raw) if raw.eq_ignore_ascii_case("json") => Ok(OutputFormat::Json),
            Some(raw) if raw.eq_ignore_ascii_case("csv") => Ok(OutputFormat::Csv),
            Some(raw) if raw.eq_ignore_ascii_case("rss") => Ok(OutputFormat::Rss),
            Some(raw) => Err(UnsupportedFormat {
                requested: raw.to_string(),
            }),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            OutputFormat::Html => "html",
            OutputFormat::Json => "json",
            OutputFormat::Csv => "csv",
            OutputFormat::Rss => "rss",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsupportedFormat {
    pub requested: String,
}

impl std::fmt::Display for UnsupportedFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "unsupported output format: '{}'", self.requested)
    }
}

impl std::error::Error for UnsupportedFormat {}

pub fn format_json(container: &ResultContainer) -> String {
    format_json_for_query("", container)
}

pub fn format_json_for_query(query: &str, container: &ResultContainer) -> String {
    let response = JsonResponse::from_query_and_container(query, container);
    serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string())
}

pub fn format_json_for_query_with_proxies(
    query: &str,
    container: &ResultContainer,
    proxies: ProxySettings<'_>,
) -> String {
    let mut response = JsonResponse::from_query_and_container(query, container);
    for result in &mut response.results {
        let Some(obj) = result.as_object_mut() else {
            continue;
        };
        if proxies.favicon_proxy
            && let Some(url) = obj.get("url").and_then(Value::as_str)
            && let Some(authority) = url::Url::parse(url)
                .ok()
                .and_then(|url| url.host_str().map(str::to_string))
            && zoeken_favicons::validate_proxy_authority(&authority).is_ok()
        {
            obj.insert(
                "favicon".to_string(),
                Value::String(signed_proxy_url(
                    "/favicon_proxy",
                    "authority",
                    &authority,
                    proxies.secret_key,
                )),
            );
        }
        if proxies.image_proxy {
            for key in ["img_src", "thumbnail_src", "thumbnail"] {
                let original = obj.get(key).and_then(Value::as_str).map(str::to_string);
                if let Some(original) = original
                    && zoeken_favicons::validate_proxy_url(&original).is_ok()
                {
                    obj.insert(
                        key.to_string(),
                        Value::String(signed_proxy_url(
                            "/image_proxy",
                            "url",
                            &original,
                            proxies.secret_key,
                        )),
                    );
                } else {
                    obj.remove(key);
                }
            }
        }
    }
    if proxies.image_proxy {
        for infobox in &mut response.infoboxes {
            if let Some(original) = infobox.img_src.as_deref()
                && zoeken_favicons::validate_proxy_url(original).is_ok()
            {
                infobox.img_src = Some(signed_proxy_url(
                    "/image_proxy",
                    "url",
                    original,
                    proxies.secret_key,
                ));
            } else {
                infobox.img_src = None;
            }
        }
    }
    serde_json::to_string(&response).unwrap_or_else(|_| "{}".to_string())
}

pub(crate) fn signed_proxy_url(
    path: &str,
    parameter: &str,
    value: &str,
    secret_key: &str,
) -> String {
    let h = zoeken_favicons::new_hmac(secret_key, value.as_bytes());
    let query = url::form_urlencoded::Serializer::new(String::new())
        .append_pair(parameter, value)
        .append_pair("h", &h)
        .finish();
    format!("{path}?{query}")
}

const CSV_HEADER: [&str; 7] = ["title", "url", "content", "host", "engine", "score", "type"];

pub fn format_csv(container: &ResultContainer) -> String {
    let mut writer = csv::Writer::from_writer(Vec::new());

    let _ = writer.write_record(CSV_HEADER);
    for result in &container.results {
        let row = csv_result_row(result);
        let _ = writer.write_record(row);
    }
    for answer in &container.answers {
        let score = String::new();
        let row = [
            answer.answer.clone(),
            answer.url.clone().unwrap_or_default(),
            String::new(),
            answer.url.as_deref().map(host).unwrap_or_default(),
            answer.engine.clone(),
            score,
            "answer".to_string(),
        ];
        let _ = writer.write_record(row);
    }
    for suggestion in &container.suggestions {
        let row = [
            suggestion.suggestion.clone(),
            String::new(),
            String::new(),
            String::new(),
            suggestion.engine.clone(),
            String::new(),
            "suggestion".to_string(),
        ];
        let _ = writer.write_record(row);
    }
    for correction in &container.corrections {
        let row = [
            correction.correction.clone(),
            correction.url.clone().unwrap_or_default(),
            String::new(),
            correction.url.as_deref().map(host).unwrap_or_default(),
            correction.engine.clone(),
            String::new(),
            "correction".to_string(),
        ];
        let _ = writer.write_record(row);
    }

    let bytes = writer.into_inner().unwrap_or_default();
    String::from_utf8(bytes).unwrap_or_default()
}

fn csv_result_row(result: &Result_) -> [String; 7] {
    let value = result_json(result);
    let obj = value.as_object();
    let get = |key: &str| {
        obj.and_then(|m| m.get(key))
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string()
    };
    let url = get("url");
    [
        get("title"),
        url.clone(),
        get("content"),
        host(&url),
        get("engine"),
        obj.and_then(|m| m.get("score"))
            .and_then(Value::as_f64)
            .map(|score| score.to_string())
            .unwrap_or_default(),
        "result".to_string(),
    ]
}

fn host(raw: &str) -> String {
    url::Url::parse(raw)
        .ok()
        .and_then(|url| url.host_str().map(ToString::to_string))
        .unwrap_or_default()
}

fn result_json(result: &Result_) -> Value {
    match result {
        Result_::Main(result) => {
            let mut obj = base_result_json(
                &result.url,
                &result.title,
                &result.content,
                &result.engine,
                result.score,
                result.template,
            );
            insert_engines(&mut obj, &result.engine, &result.engines);
            insert_array(
                &mut obj,
                "positions",
                result.positions.iter().copied().map(|n| json!(n)),
            );
            insert_str(&mut obj, "priority", &result.priority);
            insert_str(&mut obj, "thumbnail", &result.thumbnail);
            insert_str(&mut obj, "iframe_src", &result.iframe_src);
            Value::Object(obj)
        }
        Result_::Image(result) => {
            let mut obj = base_result_json(
                &result.url,
                &result.title,
                &result.content,
                &result.engine,
                result.score,
                result.template,
            );
            insert_engines(&mut obj, &result.engine, &[]);
            insert_str(&mut obj, "img_src", &result.img_src);
            insert_str(&mut obj, "thumbnail_src", &result.thumbnail_src);
            insert_str(&mut obj, "thumbnail", &result.thumbnail_src);
            insert_str(&mut obj, "resolution", &result.resolution);
            insert_str(&mut obj, "img_format", &result.img_format);
            insert_str(&mut obj, "source", &result.source);
            insert_str(&mut obj, "filesize", &result.filesize);
            insert_str(&mut obj, "priority", &result.priority);
            Value::Object(obj)
        }
        Result_::Paper(result) => {
            let mut obj = base_result_json(
                &result.url,
                &result.title,
                &result.content,
                &result.engine,
                result.score,
                result.template,
            );
            insert_array(
                &mut obj,
                "authors",
                result.authors.iter().cloned().map(Value::String),
            );
            insert_str(&mut obj, "doi", &result.doi);
            insert_str(&mut obj, "journal", &result.journal);
            if let Some(date) = &result.published_date {
                insert_str(&mut obj, "publishedDate", date);
                insert_str(&mut obj, "pubdate", date);
            }
            insert_str(&mut obj, "publisher", &result.publisher);
            insert_str(&mut obj, "type", &result.type_);
            insert_str(&mut obj, "priority", &result.priority);
            insert_array(
                &mut obj,
                "tags",
                result.tags.iter().cloned().map(Value::String),
            );
            insert_str(&mut obj, "pdf_url", &result.pdf_url);
            insert_str(&mut obj, "html_url", &result.html_url);
            Value::Object(obj)
        }
        Result_::Code(result) => {
            let mut obj = base_result_json(
                &result.url,
                &result.title,
                &result.content,
                &result.engine,
                result.score,
                result.template,
            );
            if let Some(repository) = &result.repository {
                insert_str(&mut obj, "repository", repository);
            }
            obj.insert("codelines".to_string(), json!(result.codelines));
            insert_array(
                &mut obj,
                "hl_lines",
                result.hl_lines.iter().copied().map(|n| json!(n)),
            );
            insert_str(&mut obj, "code_language", &result.code_language);
            if let Some(filename) = &result.filename {
                insert_str(&mut obj, "filename", filename);
            }
            insert_str(&mut obj, "priority", &result.priority);
            Value::Object(obj)
        }
        Result_::File(result) => {
            let mut obj = base_result_json(
                &result.url,
                &result.title,
                &result.content,
                &result.engine,
                result.score,
                result.template,
            );
            insert_str(&mut obj, "filename", &result.filename);
            insert_str(&mut obj, "size", &result.size);
            insert_str(&mut obj, "mimetype", &result.mimetype);
            insert_str(&mut obj, "abstract", &result.abstract_);
            insert_str(&mut obj, "author", &result.author);
            insert_str(&mut obj, "embedded", &result.embedded);
            if let Some(filesize) = &result.filesize {
                insert_str(&mut obj, "filesize", filesize);
            }
            if let Some(seed) = result.seed {
                obj.insert("seed".to_string(), json!(seed));
            }
            if let Some(leech) = result.leech {
                obj.insert("leech".to_string(), json!(leech));
            }
            if let Some(magnetlink) = &result.magnetlink {
                insert_str(&mut obj, "magnetlink", magnetlink);
            }
            insert_str(&mut obj, "priority", &result.priority);
            Value::Object(obj)
        }
        Result_::KeyValue(result) => {
            let mut obj = base_result_json(
                &result.url,
                &result.title,
                &result.content,
                &result.engine,
                result.score,
                result.template,
            );
            obj.insert("kvmap".to_string(), json!(result.kvmap));
            insert_str(&mut obj, "caption", &result.caption);
            insert_str(&mut obj, "key_title", &result.key_title);
            insert_str(&mut obj, "value_title", &result.value_title);
            insert_str(&mut obj, "priority", &result.priority);
            Value::Object(obj)
        }
        Result_::Answer(answer) => answer_json(answer),
        Result_::Suggestion(suggestion) => {
            json!({"suggestion": suggestion.suggestion, "engine": suggestion.engine})
        }
        Result_::Correction(correction) => {
            json!({"correction": correction.correction, "url": correction.url, "engine": correction.engine})
        }
        Result_::Infobox(infobox) => serde_json::to_value(infobox).unwrap_or(Value::Null),
    }
}

fn base_result_json(
    url: &str,
    title: &str,
    content: &str,
    engine: &str,
    score: f64,
    template: Template,
) -> Map<String, Value> {
    let mut obj = Map::new();
    insert_str(&mut obj, "url", url);
    insert_str(&mut obj, "title", title);
    insert_str(&mut obj, "content", content);
    insert_str(&mut obj, "engine", engine);
    obj.insert("score".to_string(), json!(score));
    obj.insert("template".to_string(), json!(template.as_str()));
    obj
}

fn answer_json(answer: &Answer) -> Value {
    let mut obj = Map::new();
    insert_str(&mut obj, "answer", &answer.answer);
    if let Some(url) = &answer.url {
        insert_str(&mut obj, "url", url);
    }
    insert_str(&mut obj, "engine", &answer.engine);
    obj.insert("template".to_string(), json!(answer.template.as_str()));
    if let Some(interactive) = &answer.interactive {
        obj.insert("interactive".to_string(), json!(interactive));
    }
    Value::Object(obj)
}

fn insert_str(obj: &mut Map<String, Value>, key: &str, value: &str) {
    if !value.is_empty() {
        obj.insert(key.to_string(), Value::String(value.to_string()));
    }
}

fn insert_engines(obj: &mut Map<String, Value>, primary: &str, engines: &[String]) {
    let mut names: Vec<String> = if engines.is_empty() {
        if primary.is_empty() {
            Vec::new()
        } else {
            vec![primary.to_string()]
        }
    } else {
        engines.to_vec()
    };
    names.sort();
    names.dedup();
    insert_array(obj, "engines", names.into_iter().map(Value::String));
}

fn insert_array<I>(obj: &mut Map<String, Value>, key: &str, values: I)
where
    I: IntoIterator<Item = Value>,
{
    let values: Vec<Value> = values.into_iter().collect();
    if !values.is_empty() {
        obj.insert(key.to_string(), Value::Array(values));
    }
}

const RSS_CHANNEL_TITLE: &str = "Search";

const RSS_CHANNEL_DESCRIPTION: &str = "Search results";

pub fn format_rss(container: &ResultContainer) -> String {
    let feed = RssFeed::from(container);
    let body = quick_xml::se::to_string(&feed).unwrap_or_default();
    format!("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n{body}")
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename = "rss")]
pub struct RssFeed {
    #[serde(rename = "@version")]
    pub version: String,
    pub channel: RssChannel,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RssChannel {
    pub title: String,
    pub description: String,
    #[serde(default, rename = "item")]
    pub items: Vec<RssItem>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RssItem {
    pub title: String,
    pub link: String,
    pub description: String,
    pub author: String,
}

impl From<&ResultContainer> for RssFeed {
    fn from(container: &ResultContainer) -> Self {
        let items = container
            .results
            .iter()
            .filter_map(|result| match result {
                Result_::Main(main) => Some(RssItem {
                    title: main.title.clone(),
                    link: main.url.clone(),
                    description: main.content.clone(),
                    author: main.engine.clone(),
                }),
                _ => None,
            })
            .collect();
        RssFeed {
            version: "2.0".to_string(),
            channel: RssChannel {
                title: RSS_CHANNEL_TITLE.to_string(),
                description: RSS_CHANNEL_DESCRIPTION.to_string(),
                items,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct JsonResponse {
    pub query: String,
    pub results: Vec<Value>,
    pub answers: Vec<Value>,
    pub corrections: Vec<String>,
    pub infoboxes: Vec<Infobox>,
    pub suggestions: Vec<String>,
    pub unresponsive_engines: Vec<(String, String)>,
    #[serde(default, skip_serializing_if = "std::collections::HashMap::is_empty")]
    pub engine_data: std::collections::HashMap<String, String>,
}

impl JsonResponse {
    pub fn from_query_and_container(query: &str, container: &ResultContainer) -> Self {
        JsonResponse {
            query: query.to_string(),
            results: container.results.iter().map(result_json).collect(),
            answers: container.answers.iter().map(answer_json).collect(),
            corrections: container
                .corrections
                .iter()
                .map(|correction| correction.correction.clone())
                .collect(),
            infoboxes: container.infoboxes.clone(),
            suggestions: container
                .suggestions
                .iter()
                .map(|suggestion| suggestion.suggestion.clone())
                .collect(),
            unresponsive_engines: container
                .unresponsive_engines
                .iter()
                .map(|engine| {
                    (
                        engine.engine.clone(),
                        translated_cause(&engine.cause).to_string(),
                    )
                })
                .collect(),
            engine_data: container.engine_data.clone(),
        }
    }
}

fn translated_cause(cause: &UnresponsiveCause) -> &'static str {
    match cause {
        UnresponsiveCause::Error { category, .. } => category.user_label(),
        UnresponsiveCause::Timeout | UnresponsiveCause::DeadlineExceeded => "timeout",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoeken_results::{MainResult, Result_};
    use zoeken_search::UnresponsiveEngine;

    fn container_with_one_result() -> ResultContainer {
        ResultContainer {
            results: vec![Result_::Main(MainResult {
                url: "https://example.test/".to_string(),
                normalized_url: "https://example.test/".to_string(),
                title: "Example".to_string(),
                content: "An example result.".to_string(),
                engine: "duckduckgo".to_string(),
                score: 1.0,
                positions: vec![1],
                engines: vec!["duckduckgo".to_string()],
                ..MainResult::default()
            })],
            number_of_results: 1,
            ..ResultContainer::default()
        }
    }

    #[test]
    fn json_includes_results() {
        let body = format_json_for_query("rust", &container_with_one_result());
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();

        assert_eq!(value["query"], "rust");
        assert!(value.get("number_of_results").is_none());
        assert_eq!(value["results"][0]["title"], "Example");
        assert_eq!(value["results"][0]["url"], "https://example.test/");
        assert_eq!(value["results"][0]["template"], "default.html");
        assert_eq!(value["results"][0]["engine"], "duckduckgo");
        assert_eq!(
            value["results"][0]["engines"],
            serde_json::json!(["duckduckgo"])
        );
        assert!(value["answers"].is_array());
        assert!(value["suggestions"].is_array());
        assert!(value["corrections"].is_array());
        assert!(value["infoboxes"].is_array());
        assert!(value["unresponsive_engines"].is_array());
    }

    #[test]
    fn json_is_total_for_empty_container() {
        let body = format_json(&ResultContainer::default());
        let value: serde_json::Value = serde_json::from_str(&body).unwrap();
        assert_eq!(value["query"], "");
        assert_eq!(value["results"].as_array().unwrap().len(), 0);
        assert_eq!(value["unresponsive_engines"].as_array().unwrap().len(), 0);
    }

    #[test]
    fn json_serializes_unresponsive_engines_as_pairs() {
        let container = ResultContainer {
            unresponsive_engines: vec![
                UnresponsiveEngine {
                    engine: "boom".to_string(),
                    cause: UnresponsiveCause::Error {
                        category: zoeken_engine_core::ErrorCategory::Unexpected,
                        message: "nope".to_string(),
                    },
                },
                UnresponsiveEngine {
                    engine: "slow".to_string(),
                    cause: UnresponsiveCause::Timeout,
                },
                UnresponsiveEngine {
                    engine: "late".to_string(),
                    cause: UnresponsiveCause::DeadlineExceeded,
                },
            ],
            ..ResultContainer::default()
        };
        let value: serde_json::Value = serde_json::from_str(&format_json(&container)).unwrap();
        let engines = value["unresponsive_engines"].as_array().unwrap();
        assert_eq!(engines.len(), 3);
        assert_eq!(engines[0], serde_json::json!(["boom", "error"]));
        assert_eq!(engines[1], serde_json::json!(["slow", "timeout"]));
        assert_eq!(engines[2], serde_json::json!(["late", "timeout"]));
    }

    #[test]
    fn from_param_defaults_to_html_and_accepts_json() {
        assert_eq!(OutputFormat::from_param(None), Ok(OutputFormat::Html));
        assert_eq!(OutputFormat::from_param(Some("")), Ok(OutputFormat::Html));
        assert_eq!(OutputFormat::from_param(Some("  ")), Ok(OutputFormat::Html));
        assert_eq!(
            OutputFormat::from_param(Some("html")),
            Ok(OutputFormat::Html)
        );
        assert_eq!(
            OutputFormat::from_param(Some("json")),
            Ok(OutputFormat::Json)
        );
        // Case-insensitive and whitespace-tolerant.
        assert_eq!(
            OutputFormat::from_param(Some(" JSON ")),
            Ok(OutputFormat::Json)
        );
    }

    #[test]
    fn from_param_accepts_csv() {
        assert_eq!(OutputFormat::from_param(Some("csv")), Ok(OutputFormat::Csv));
        assert_eq!(OutputFormat::from_param(Some("CSV")), Ok(OutputFormat::Csv));
        assert_eq!(
            OutputFormat::from_param(Some(" Csv ")),
            Ok(OutputFormat::Csv)
        );
    }

    #[test]
    fn from_param_accepts_rss() {
        assert_eq!(OutputFormat::from_param(Some("rss")), Ok(OutputFormat::Rss));
        assert_eq!(OutputFormat::from_param(Some("RSS")), Ok(OutputFormat::Rss));
        assert_eq!(
            OutputFormat::from_param(Some(" Rss ")),
            Ok(OutputFormat::Rss)
        );
    }

    #[test]
    fn from_param_rejects_unsupported_formats() {
        for raw in ["xml", "yaml", "totally-bogus"] {
            let error = OutputFormat::from_param(Some(raw)).unwrap_err();
            assert_eq!(error.requested, raw);
            assert!(
                error.to_string().contains(raw),
                "error should name the offending format: {error}"
            );
        }
    }

    #[test]
    fn csv_has_stable_header_and_row_per_main_result() {
        let body = format_csv(&container_with_one_result());
        let mut lines = body.lines();
        // The header is fixed and comes first.
        assert_eq!(
            lines.next().unwrap(),
            "title,url,content,host,engine,score,type"
        );
        // One data row for the single main result.
        let row = lines.next().unwrap();
        assert!(row.starts_with("Example,https://example.test/,An example result.,example.test,"));
        assert!(lines.next().is_none(), "no extra rows expected");

        // Parse the row back with the csv crate and confirm the fields round-trip.
        let mut reader = csv::Reader::from_reader(body.as_bytes());
        let record = reader.records().next().unwrap().unwrap();
        assert_eq!(&record[0], "Example");
        assert_eq!(&record[1], "https://example.test/");
        assert_eq!(&record[2], "An example result.");
        assert_eq!(&record[3], "example.test");
        assert_eq!(&record[4], "duckduckgo");
        assert_eq!(record[5].parse::<f64>().unwrap(), 1.0);
        assert_eq!(&record[6], "result");
    }

    #[test]
    fn csv_is_header_only_for_empty_container() {
        // An empty container still produces a well-formed table: just the header
        // row and no data rows (terminator-agnostic via `lines`).
        let body = format_csv(&ResultContainer::default());
        let mut lines = body.lines();
        assert_eq!(
            lines.next().unwrap(),
            "title,url,content,host,engine,score,type"
        );
        assert!(
            lines.next().is_none(),
            "empty container yields no data rows"
        );
    }

    #[test]
    fn csv_quotes_fields_containing_delimiters() {
        // Commas, quotes, and newlines in a field are quoted/escaped by the csv
        // crate, so the value survives a parse unchanged.
        let container = ResultContainer {
            results: vec![Result_::Main(MainResult {
                url: "https://example.test/".to_string(),
                normalized_url: "https://example.test/".to_string(),
                title: "Comma, \"quote\" and\nnewline".to_string(),
                content: "body".to_string(),
                engine: "e".to_string(),
                score: 2.5,
                engines: vec!["e".to_string()],
                positions: vec![3],
                ..MainResult::default()
            })],
            number_of_results: 1,
            ..ResultContainer::default()
        };
        let body = format_csv(&container);
        let mut reader = csv::Reader::from_reader(body.as_bytes());
        let record = reader.records().next().unwrap().unwrap();
        assert_eq!(&record[0], "Comma, \"quote\" and\nnewline");
    }

    #[test]
    fn rss_is_well_formed_with_item_per_main_result() {
        let body = format_rss(&container_with_one_result());

        // Starts with an XML declaration and the RSS 2.0 root element.
        assert!(body.starts_with("<?xml version=\"1.0\" encoding=\"UTF-8\"?>"));
        assert!(body.contains("<rss version=\"2.0\">"));
        assert!(body.contains("<channel>"));

        // The document is well-formed and parses back through the mirror type.
        let feed: RssFeed = quick_xml::de::from_str(&body).unwrap();
        assert_eq!(feed.version, "2.0");
        assert_eq!(feed.channel.items.len(), 1);
        let item = &feed.channel.items[0];
        assert_eq!(item.title, "Example");
        assert_eq!(item.link, "https://example.test/");
        assert_eq!(item.description, "An example result.");
        assert_eq!(item.author, "duckduckgo");
    }

    #[test]
    fn rss_is_total_for_empty_container() {
        // An empty container still produces a well-formed feed: a channel with
        // no items.
        let body = format_rss(&ResultContainer::default());
        let feed: RssFeed = quick_xml::de::from_str(&body).unwrap();
        assert_eq!(feed.version, "2.0");
        assert!(feed.channel.items.is_empty());
    }

    #[test]
    fn rss_emits_one_item_per_main_result_in_order() {
        let container = ResultContainer {
            results: vec![
                Result_::Main(MainResult {
                    url: "https://a.test/".to_string(),
                    title: "First".to_string(),
                    content: "one".to_string(),
                    engine: "ddg".to_string(),
                    ..MainResult::default()
                }),
                Result_::Main(MainResult {
                    url: "https://b.test/".to_string(),
                    title: "Second".to_string(),
                    content: "two".to_string(),
                    engine: "bing".to_string(),
                    ..MainResult::default()
                }),
            ],
            number_of_results: 2,
            ..ResultContainer::default()
        };
        let feed: RssFeed = quick_xml::de::from_str(&format_rss(&container)).unwrap();
        let titles: Vec<&str> = feed
            .channel
            .items
            .iter()
            .map(|item| item.title.as_str())
            .collect();
        assert_eq!(titles, vec!["First", "Second"]);
    }

    #[test]
    fn rss_escapes_special_characters() {
        // Angle brackets, ampersands, and quotes in a field are XML-escaped so
        // the document stays well-formed and the value survives a parse.
        let container = ResultContainer {
            results: vec![Result_::Main(MainResult {
                url: "https://example.test/?a=1&b=2".to_string(),
                title: "Tom & Jerry <\"quoted\">".to_string(),
                content: "1 < 2 && 3 > 2".to_string(),
                engine: "e".to_string(),
                ..MainResult::default()
            })],
            number_of_results: 1,
            ..ResultContainer::default()
        };
        let body = format_rss(&container);
        // The raw body must not contain an unescaped, un-quoted ampersand-space
        // (a bare `&` is not well-formed XML).
        assert!(!body.contains("Jerry & Jerry"));
        assert!(body.contains("&amp;"));

        // And it round-trips back to the original, unescaped strings.
        let feed: RssFeed = quick_xml::de::from_str(&body).unwrap();
        let item = &feed.channel.items[0];
        assert_eq!(item.title, "Tom & Jerry <\"quoted\">");
        assert_eq!(item.link, "https://example.test/?a=1&b=2");
        assert_eq!(item.description, "1 < 2 && 3 > 2");
    }
}
