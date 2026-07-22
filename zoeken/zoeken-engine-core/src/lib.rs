//! Engine trait, processors, suspend/penalty state machine, and parsing helpers.

use std::borrow::Cow;
use std::collections::HashMap;
use std::time::{Duration, Instant};

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use thiserror::Error;

pub mod challenge;
pub mod error_category;

pub use challenge::{ChallengeKind, classify_challenge, looks_like_bot_wall};
pub use error_category::ErrorCategory;
pub use zoeken_results::{
    Answer, Code, Correction, FileResult, Image, Infobox, KeyValue, MainResult, Paper, Result_,
    ResultItem, ResultKind, Suggestion, Template,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Processor {
    #[default]
    Online,
    Offline,
}

impl Processor {
    pub fn as_str(&self) -> &'static str {
        match self {
            Processor::Online => "online",
            Processor::Offline => "offline",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum SafeSearch {
    #[default]
    Off,
    Moderate,
    Strict,
}

impl SafeSearch {
    pub fn level(&self) -> u8 {
        match self {
            SafeSearch::Off => 0,
            SafeSearch::Moderate => 1,
            SafeSearch::Strict => 2,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeRange {
    Day,
    Week,
    Month,
    Year,
}

impl TimeRange {
    pub fn as_str(&self) -> &'static str {
        match self {
            TimeRange::Day => "day",
            TimeRange::Week => "week",
            TimeRange::Month => "month",
            TimeRange::Year => "year",
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SearchQueryView {
    pub query: String,
    pub pageno: u32,
    pub safesearch: SafeSearch,
    pub time_range: Option<TimeRange>,
    pub locale: String,
    pub categories: Vec<String>,
    pub engines: Vec<String>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub engine_data: HashMap<String, String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct About {
    #[serde(default)]
    pub website: Option<String>,
    #[serde(default)]
    pub wikidata_id: Option<String>,
    #[serde(default)]
    pub official_api_documentation: Option<String>,
    #[serde(default)]
    pub use_official_api: bool,
    #[serde(default)]
    pub require_api_key: bool,
    #[serde(default)]
    pub results: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EngineMeta {
    pub name: String,
    pub engine_type: Processor,
    pub categories: Vec<String>,
    pub paging: bool,
    pub max_page: u32,
    pub time_range_support: bool,
    pub safesearch: bool,
    pub language_support: bool,
    pub weight: u32,
    pub shortcut: String,
    pub about: About,
}

impl Default for EngineMeta {
    fn default() -> Self {
        EngineMeta {
            name: String::new(),
            engine_type: Processor::Online,
            categories: Vec::new(),
            paging: false,
            max_page: 0,
            time_range_support: false,
            safesearch: false,
            language_support: false,
            weight: 1,
            shortcut: String::new(),
            about: About::default(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum HttpMethod {
    #[default]
    Get,
    Post,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum TlsVerify {
    #[default]
    Default,
    Disabled,
    CaFile(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct RequestParams {
    pub query: String,
    pub category: String,
    pub pageno: u32,
    pub safesearch: SafeSearch,
    pub time_range: Option<TimeRange>,
    pub locale_key: String,
    pub engine_data: HashMap<String, String>,
    pub method: HttpMethod,
    pub url: Option<String>,
    /// Insertion-ordered: some engines (notably DuckDuckGo) fingerprint header order.
    pub headers: IndexMap<String, String>,
    pub cookies: HashMap<String, String>,
    /// Insertion-ordered form fields (POST body order can matter for bot checks).
    pub data: IndexMap<String, String>,
    pub json: Option<serde_json::Value>,
    #[serde(with = "bytes_repr")]
    pub content: Vec<u8>,
    pub allow_redirects: bool,
    pub max_redirects: u32,
    pub soft_max_redirects: u32,
    pub auth: Option<String>,
    pub verify: TlsVerify,
    pub raise_for_httperror: bool,
    pub network: Option<String>,
}

impl Default for RequestParams {
    fn default() -> Self {
        RequestParams {
            query: String::new(),
            category: String::new(),
            pageno: 1,
            safesearch: SafeSearch::Off,
            time_range: None,
            locale_key: String::new(),
            engine_data: HashMap::new(),
            method: HttpMethod::Get,
            url: None,
            headers: IndexMap::new(),
            cookies: HashMap::new(),
            data: IndexMap::new(),
            json: None,
            content: Vec::new(),
            allow_redirects: false,
            max_redirects: 0,
            soft_max_redirects: 0,
            auth: None,
            verify: TlsVerify::Default,
            raise_for_httperror: true,
            network: None,
        }
    }
}

mod bytes_repr {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};

    pub fn serialize<S: Serializer>(bytes: &[u8], serializer: S) -> Result<S::Ok, S::Error> {
        match std::str::from_utf8(bytes) {
            Ok(text) => serializer.serialize_str(text),
            Err(_) => bytes.serialize(serializer),
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<u8>, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Repr {
            Text(String),
            Bytes(Vec<u8>),
        }
        Ok(match Repr::deserialize(deserializer)? {
            Repr::Text(text) => text.into_bytes(),
            Repr::Bytes(bytes) => bytes,
        })
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EngineResponse {
    pub status: u16,
    pub url: String,
    pub headers: HashMap<String, String>,
    #[serde(with = "bytes_repr")]
    pub body: Vec<u8>,
}

impl EngineResponse {
    pub fn text(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.body)
    }

    pub fn is_success(&self) -> bool {
        (200..=299).contains(&self.status)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
pub struct EngineResults {
    pub results: Vec<Result_>,
    pub answers: Vec<Answer>,
    pub suggestions: Vec<Suggestion>,
    pub corrections: Vec<Correction>,
    pub infoboxes: Vec<Infobox>,
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub engine_data: HashMap<String, String>,
}

impl EngineResults {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn add(&mut self, result: Result_) {
        match result {
            Result_::Answer(a) => self.answers.push(a),
            Result_::Suggestion(s) => self.suggestions.push(s),
            Result_::Correction(c) => self.corrections.push(c),
            Result_::Infobox(i) => self.infoboxes.push(i),
            other => self.results.push(other),
        }
    }

    pub fn is_empty(&self) -> bool {
        self.results.is_empty()
            && self.answers.is_empty()
            && self.suggestions.is_empty()
            && self.corrections.is_empty()
            && self.infoboxes.is_empty()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EngineError {
    #[error("access denied: {0}")]
    AccessDenied(String),
    #[error("cloudflare access denied: {0}")]
    CloudflareAccessDenied(String),
    #[error("captcha: {0}")]
    Captcha(String),
    #[error("cloudflare captcha: {0}")]
    CloudflareCaptcha(String),
    #[error("recaptcha captcha: {0}")]
    RecaptchaCaptcha(String),
    #[error("too many requests: {0}")]
    TooManyRequests(String),
    #[error("failed to parse engine response: {0}")]
    Parse(String),
    #[error("engine request timed out")]
    Timeout,
    #[error("outbound request expired while waiting for its origin permit")]
    QueueExpired,
    #[error("unexpected engine error: {0}")]
    Unexpected(String),
}

pub trait Engine: Send + Sync {
    fn metadata(&self) -> &EngineMeta;
    fn request(&self, q: &SearchQueryView, p: &mut RequestParams);
    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError>;
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SuspendConfig {
    pub threshold: u32,
    pub base: Duration,
    pub max: Duration,
}

impl SuspendConfig {
    pub fn new(threshold: u32, base: Duration, max: Duration) -> Self {
        SuspendConfig {
            threshold,
            base,
            max,
        }
    }
}

impl Default for SuspendConfig {
    fn default() -> Self {
        SuspendConfig {
            threshold: 1,
            base: Duration::from_secs(5),
            max: Duration::from_secs(120),
        }
    }
}

pub fn suspend_duration(penalty_count: u32, base: Duration, max: Duration) -> Duration {
    let base_nanos = base.as_nanos();
    if base_nanos == 0 {
        return Duration::ZERO;
    }
    let max_nanos = max.as_nanos();
    let scaled = if penalty_count >= 128 || base_nanos.leading_zeros() < penalty_count {
        u128::MAX
    } else {
        base_nanos << penalty_count
    };
    let capped = scaled.min(max_nanos);
    let secs = (capped / 1_000_000_000) as u64;
    let nanos = (capped % 1_000_000_000) as u32;
    Duration::new(secs, nanos)
}

#[derive(Debug, Clone, Default)]
pub struct EngineState {
    pub continuous_errors: u32,
    pub suspend_end: Option<Instant>,
    pub suspend_reason: Option<String>,
    /// Same [`ErrorCategory`] vocabulary as metrics/storage health, so
    /// suspend reasons don't drift into a fourth ad-hoc taxonomy.
    pub suspend_category: Option<ErrorCategory>,
}

impl EngineState {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn is_suspended(&self, now: Instant) -> bool {
        match self.suspend_end {
            Some(end) => now < end,
            None => false,
        }
    }

    pub fn on_success(&mut self) {
        self.continuous_errors = 0;
    }

    pub fn on_error(
        &mut self,
        now: Instant,
        cfg: &SuspendConfig,
        reason: &str,
        category: ErrorCategory,
        explicit: Option<Duration>,
    ) {
        if self.is_suspended(now) {
            return;
        }
        self.continuous_errors = self.continuous_errors.saturating_add(1);
        match explicit {
            Some(duration) => self.apply_suspension(now, duration, reason, category),
            None => {
                if cfg.threshold != 0 && self.continuous_errors >= cfg.threshold {
                    let penalty = self.continuous_errors - cfg.threshold;
                    let duration = suspend_duration(penalty, cfg.base, cfg.max);
                    self.apply_suspension(now, duration, reason, category);
                }
            }
        }
    }

    fn apply_suspension(
        &mut self,
        now: Instant,
        duration: Duration,
        reason: &str,
        category: ErrorCategory,
    ) {
        self.suspend_end = Some(now + duration);
        self.suspend_reason = Some(reason.to_string());
        self.suspend_category = Some(category);
    }
}

use std::collections::HashMap as StdHashMap;

use chrono::{DateTime, NaiveDate, NaiveDateTime};
use ego_tree::NodeRef;
use scraper::node::Node as DomNode;
use scraper::{Html, Selector};
use url::Url;

pub fn normalize_whitespace(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

pub fn extract_text(raw: &str) -> String {
    normalize_whitespace(&raw.replace('\n', " "))
}

fn is_serializable_xml_name(name: &str) -> bool {
    let mut chars = name.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
}

fn push_escaped_text(text: &str, out: &mut String) {
    for ch in text.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '\t' | '\n' | '\r' => out.push(ch),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
}

fn push_escaped_attr(value: &str, out: &mut String) {
    for ch in value.chars() {
        match ch {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\t' | '\n' | '\r' => out.push(ch),
            c if (c as u32) < 0x20 => {}
            c => out.push(c),
        }
    }
}

fn serialize_node_xml(node: NodeRef<'_, DomNode>, out: &mut String) {
    match node.value() {
        DomNode::Text(text) => push_escaped_text(text, out),
        DomNode::Element(el) => {
            let name = el.name();
            if !is_serializable_xml_name(name) {
                for child in node.children() {
                    serialize_node_xml(child, out);
                }
                return;
            }
            out.push('<');
            out.push_str(name);
            for (attr_name, attr_value) in el.attrs() {
                if attr_name.eq_ignore_ascii_case("xmlns")
                    || attr_name.contains(':')
                    || !is_serializable_xml_name(attr_name)
                {
                    continue;
                }
                out.push(' ');
                out.push_str(attr_name);
                out.push_str("=\"");
                push_escaped_attr(attr_value, out);
                out.push('"');
            }
            out.push('>');
            for child in node.children() {
                serialize_node_xml(child, out);
            }
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
        _ => {
            for child in node.children() {
                serialize_node_xml(child, out);
            }
        }
    }
}

fn html_to_xml(html: &str) -> String {
    let document = Html::parse_document(html);
    let mut out = String::with_capacity(html.len() + 16);
    serialize_node_xml(document.tree.root(), &mut out);
    out
}

pub struct HtmlDocument {
    xml: String,
}

impl HtmlDocument {
    pub fn parse(html: &str) -> Self {
        HtmlDocument {
            xml: html_to_xml(html),
        }
    }

    pub fn eval_xpath(&self, expr: &str) -> Result<Vec<String>, EngineError> {
        let package = sxd_document::parser::parse(&self.xml)
            .map_err(|e| EngineError::Parse(format!("invalid XML for XPath: {e:?}")))?;
        let document = package.as_document();
        let factory = sxd_xpath::Factory::new();
        let xpath = factory
            .build(expr)
            .map_err(|e| EngineError::Parse(format!("invalid XPath `{expr}`: {e}")))?
            .ok_or_else(|| EngineError::Parse(format!("empty XPath `{expr}`")))?;
        let context = sxd_xpath::Context::new();
        let value = xpath
            .evaluate(&context, document.root())
            .map_err(|e| EngineError::Parse(format!("XPath `{expr}` evaluation failed: {e}")))?;
        Ok(match value {
            sxd_xpath::Value::Nodeset(nodeset) => nodeset
                .document_order()
                .into_iter()
                .map(|node| node.string_value())
                .collect(),
            other => vec![other.string()],
        })
    }

    pub fn eval_xpath_list(
        &self,
        expr: &str,
        min_len: Option<usize>,
    ) -> Result<Vec<String>, EngineError> {
        let results = self.eval_xpath(expr)?;
        if let Some(min) = min_len
            && results.len() < min
        {
            return Err(EngineError::Parse(format!(
                "XPath `{expr}` returned {} results, expected at least {min}",
                results.len()
            )));
        }
        Ok(results)
    }

    pub fn eval_xpath_getindex(
        &self,
        expr: &str,
        index: usize,
        default: Option<&str>,
    ) -> Result<String, EngineError> {
        let results = self.eval_xpath(expr)?;
        match results.into_iter().nth(index) {
            Some(value) => Ok(value),
            None => match default {
                Some(d) => Ok(d.to_string()),
                None => Err(EngineError::Parse(format!(
                    "XPath `{expr}` has no result at index {index}"
                ))),
            },
        }
    }
}

pub fn xpath_select_relative(
    html: &str,
    item_expr: &str,
    field_expr: &str,
) -> Result<Vec<String>, EngineError> {
    let xml = html_to_xml(html);
    let package = sxd_document::parser::parse(&xml)
        .map_err(|e| EngineError::Parse(format!("invalid XML for XPath: {e:?}")))?;
    let document = package.as_document();
    let factory = sxd_xpath::Factory::new();
    let item_xpath = factory
        .build(item_expr)
        .map_err(|e| EngineError::Parse(format!("invalid XPath `{item_expr}`: {e}")))?
        .ok_or_else(|| EngineError::Parse(format!("empty XPath `{item_expr}`")))?;
    let field_xpath = factory
        .build(field_expr)
        .map_err(|e| EngineError::Parse(format!("invalid XPath `{field_expr}`: {e}")))?
        .ok_or_else(|| EngineError::Parse(format!("empty XPath `{field_expr}`")))?;
    let context = sxd_xpath::Context::new();
    let items = item_xpath
        .evaluate(&context, document.root())
        .map_err(|e| EngineError::Parse(format!("XPath `{item_expr}` evaluation failed: {e}")))?;
    let sxd_xpath::Value::Nodeset(nodeset) = items else {
        return Err(EngineError::Parse(format!(
            "XPath `{item_expr}` did not return nodes"
        )));
    };
    nodeset
        .document_order()
        .into_iter()
        .map(|node| {
            field_xpath
                .evaluate(&context, node)
                .map(xpath_value_string)
                .map_err(|e| {
                    EngineError::Parse(format!("XPath `{field_expr}` evaluation failed: {e}"))
                })
        })
        .collect()
}

fn xpath_value_string(value: sxd_xpath::Value<'_>) -> String {
    match value {
        sxd_xpath::Value::Nodeset(nodeset) => nodeset
            .document_order()
            .into_iter()
            .next()
            .map(|node| node.string_value())
            .unwrap_or_default(),
        other => other.string(),
    }
}

fn compile_selector(selector: &str) -> Result<Selector, EngineError> {
    Selector::parse(selector)
        .map_err(|e| EngineError::Parse(format!("invalid CSS selector `{selector}`: {e:?}")))
}

pub fn css_select_text(html: &str, selector: &str) -> Result<Vec<String>, EngineError> {
    let document = Html::parse_document(html);
    let sel = compile_selector(selector)?;
    Ok(document
        .select(&sel)
        .map(|el| normalize_whitespace(&el.text().collect::<String>()))
        .collect())
}

pub fn css_select_attr(html: &str, selector: &str, attr: &str) -> Result<Vec<String>, EngineError> {
    let document = Html::parse_document(html);
    let sel = compile_selector(selector)?;
    Ok(document
        .select(&sel)
        .filter_map(|el| el.value().attr(attr))
        .map(str::to_string)
        .collect())
}

pub fn normalize_url(url: &str, base_url: &str) -> Result<String, EngineError> {
    let base = Url::parse(base_url)
        .map_err(|e| EngineError::Parse(format!("invalid base URL `{base_url}`: {e}")))?;
    let resolved = base
        .join(url)
        .map_err(|e| EngineError::Parse(format!("cannot resolve URL `{url}`: {e}")))?;
    if resolved.host_str().is_none() {
        return Err(EngineError::Parse("Cannot parse url".to_string()));
    }
    Ok(resolved.to_string())
}

pub fn extract_url(raw: &str, base_url: &str) -> Result<String, EngineError> {
    let url = extract_text(raw);
    if url.is_empty() {
        return Err(EngineError::Parse("URL not found".to_string()));
    }
    normalize_url(&url, base_url)
}

fn collect_visible_text(node: NodeRef<'_, DomNode>, out: &mut String) {
    match node.value() {
        DomNode::Text(text) => out.push_str(text),
        DomNode::Element(el) => {
            let name = el.name();
            if name.eq_ignore_ascii_case("script") || name.eq_ignore_ascii_case("style") {
                return;
            }
            for child in node.children() {
                collect_visible_text(child, out);
            }
        }
        _ => {
            for child in node.children() {
                collect_visible_text(child, out);
            }
        }
    }
}

pub fn html_to_text(html_str: &str) -> String {
    if html_str.is_empty() {
        return String::new();
    }
    let collapsed = normalize_whitespace(&html_str.replace(['\n', '\r'], " "));
    let fragment = Html::parse_fragment(&collapsed);
    let mut text = String::new();
    collect_visible_text(fragment.tree.root(), &mut text);
    normalize_whitespace(&text)
}

const DATETIME_FORMATS: &[&str] = &[
    "%Y-%m-%dT%H:%M:%S%.f",
    "%Y-%m-%dT%H:%M:%S",
    "%Y-%m-%d %H:%M:%S",
    "%Y-%m-%d %H:%M",
    "%d.%m.%Y %H:%M:%S",
    "%m/%d/%Y %H:%M:%S",
];

const DATE_FORMATS: &[&str] = &[
    "%Y-%m-%d",
    "%d.%m.%Y",
    "%d/%m/%Y",
    "%m/%d/%Y",
    "%Y/%m/%d",
    "%b %d, %Y",
    "%B %d, %Y",
    "%d %b %Y",
    "%d %B %Y",
];

pub fn parse_date(input: &str) -> Option<NaiveDateTime> {
    let s = input.trim();
    if s.is_empty() {
        return None;
    }
    for fmt in DATETIME_FORMATS {
        if let Ok(dt) = NaiveDateTime::parse_from_str(s, fmt) {
            return Some(dt);
        }
    }
    if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
        return Some(dt.naive_utc());
    }
    if let Ok(dt) = DateTime::parse_from_rfc2822(s) {
        return Some(dt.naive_utc());
    }
    for fmt in DATE_FORMATS {
        if let Ok(date) = NaiveDate::parse_from_str(s, fmt) {
            return date.and_hms_opt(0, 0, 0);
        }
    }
    None
}

pub fn json_get<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a serde_json::Value> {
    let mut current = value;
    for segment in path.split('.') {
        if segment.is_empty() {
            continue;
        }
        current = match current {
            serde_json::Value::Object(map) => map.get(segment)?,
            serde_json::Value::Array(items) => {
                let index: usize = segment.parse().ok()?;
                items.get(index)?
            }
            _ => return None,
        };
    }
    Some(current)
}

pub fn json_get_str<'a>(value: &'a serde_json::Value, path: &str) -> Option<&'a str> {
    json_get(value, path).and_then(serde_json::Value::as_str)
}

fn locale_language(tag: &str) -> &str {
    let end = tag.find(['-', '_']).unwrap_or(tag.len());
    &tag[..end]
}

fn locale_territory(tag: &str) -> Option<&str> {
    tag.split(['-', '_'])
        .find(|part| part.len() == 2 && part.chars().all(|c| c.is_ascii_uppercase()))
}

pub fn get_engine_locale(
    locale_key: &str,
    engine_locales: &StdHashMap<String, String>,
    default: Option<&str>,
) -> Option<String> {
    if let Some(value) = engine_locales.get(locale_key) {
        return Some(value.clone());
    }

    let normalized = locale_key.replace('_', "-");
    let language = locale_language(&normalized);
    let territory = locale_territory(&normalized);

    if !language.is_empty()
        && let Some(value) = engine_locales.get(language)
    {
        return Some(value.clone());
    }

    if let Some(terr) = territory {
        let mut keys: Vec<&String> = engine_locales
            .keys()
            .filter(|k| locale_territory(k) == Some(terr))
            .collect();
        keys.sort();
        if let Some(key) = keys.first() {
            return engine_locales.get(*key).cloned();
        }
    }

    if !language.is_empty() {
        let preferred_territory = if language.eq_ignore_ascii_case("en") {
            "US".to_string()
        } else {
            language.to_ascii_uppercase()
        };
        let preferred_key = format!("{language}-{preferred_territory}");
        if let Some(value) = engine_locales.get(&preferred_key) {
            return Some(value.clone());
        }
        let mut keys: Vec<&String> = engine_locales
            .keys()
            .filter(|k| locale_language(k) == language)
            .collect();
        keys.sort();
        if let Some(key) = keys.first() {
            return engine_locales.get(*key).cloned();
        }
    }

    default.map(str::to_string)
}

pub trait LocaleTranslate {
    fn get_language(&self, locale_key: &str, default: Option<&str>) -> Option<String>;
    fn get_region(&self, locale_key: &str, default: Option<&str>) -> Option<String>;
}

/// Bundled per-engine language/region traits (`engine_traits.json`), loaded
/// once from the embedded data asset. Engines consult this via
/// [`engine_traits`] to translate a resolved SearXNG locale into the
/// engine-specific language/region parameter, mirroring upstream's
/// `EngineTraits.get_language`/`get_region`.
static ENGINE_TRAITS: std::sync::OnceLock<zoeken_data::EngineTraitsMap> =
    std::sync::OnceLock::new();

fn engine_traits_bundle() -> &'static zoeken_data::EngineTraitsMap {
    ENGINE_TRAITS.get_or_init(|| {
        zoeken_data::load_embedded_bundle()
            .map(|bundle| bundle.engine_traits)
            .unwrap_or_else(|err| {
                tracing::warn!(error = %err, "failed to load bundled engine traits");
                zoeken_data::EngineTraitsMap::default()
            })
    })
}

/// Bundled language/region traits for a single engine, keyed by engine name
/// (e.g. `"wikipedia"`, `"google"`, `"bing"`), or `None` if the engine has no
/// entry in `engine_traits.json`.
pub fn engine_traits(name: &str) -> Option<&'static zoeken_data::EngineTraits> {
    engine_traits_bundle().get(name)
}

impl LocaleTranslate for zoeken_data::EngineTraits {
    fn get_language(&self, locale_key: &str, default: Option<&str>) -> Option<String> {
        if locale_key == "all"
            && let Some(all_locale) = &self.all_locale
        {
            return Some(all_locale.clone());
        }
        get_engine_locale(locale_key, &self.languages, default)
    }

    fn get_region(&self, locale_key: &str, default: Option<&str>) -> Option<String> {
        if locale_key == "all"
            && let Some(all_locale) = &self.all_locale
        {
            return Some(all_locale.clone());
        }
        get_engine_locale(locale_key, &self.regions, default)
    }
}

#[cfg(test)]
mod helper_tests {
    use super::*;

    #[test]
    fn eval_xpath_extracts_text_and_attrs_from_malformed_html() {
        let html = r#"<html><body><p class="a">Hello<br>world<a href="/x">link</a></body></html>"#;
        let doc = HtmlDocument::parse(html);

        let links = doc.eval_xpath("//a/@href").unwrap();
        assert_eq!(links, vec!["/x".to_string()]);

        let para = doc.eval_xpath_getindex("//p", 0, None).unwrap();
        assert_eq!(normalize_whitespace(&para), "Helloworldlink");
    }

    #[test]
    fn eval_xpath_list_enforces_min_len() {
        let doc = HtmlDocument::parse("<ul><li>1</li><li>2</li></ul>");
        assert!(doc.eval_xpath_list("//li", Some(2)).is_ok());
        assert!(doc.eval_xpath_list("//li", Some(3)).is_err());
    }

    #[test]
    fn eval_xpath_getindex_uses_default_when_missing() {
        let doc = HtmlDocument::parse("<div></div>");
        let value = doc
            .eval_xpath_getindex("//span", 0, Some("fallback"))
            .unwrap();
        assert_eq!(value, "fallback");
    }

    #[test]
    fn css_helpers_extract_text_and_attr() {
        let html = r#"<a class="r" href="https://example.com/p">  Title   here </a>"#;
        assert_eq!(
            css_select_text(html, "a.r").unwrap(),
            vec!["Title here".to_string()]
        );
        assert_eq!(
            css_select_attr(html, "a.r", "href").unwrap(),
            vec!["https://example.com/p".to_string()]
        );
    }

    #[test]
    fn normalize_url_handles_scheme_relative_and_relative() {
        assert_eq!(
            normalize_url("//example.com", "https://example.com/").unwrap(),
            "https://example.com/"
        );
        assert_eq!(
            normalize_url("/path?a=1", "https://example.com").unwrap(),
            "https://example.com/path?a=1"
        );
        assert_eq!(
            normalize_url("", "https://example.com").unwrap(),
            "https://example.com/"
        );
    }

    #[test]
    fn extract_url_errors_on_empty_text() {
        assert!(extract_url("   ", "https://example.com").is_err());
    }

    #[test]
    fn html_to_text_strips_tags_and_style() {
        assert_eq!(
            html_to_text(r#"Example <span id="42">#2</span>"#),
            "Example #2"
        );
        assert_eq!(
            html_to_text("<style>.span { color: red; }</style><span>Example</span>"),
            "Example"
        );
    }

    #[test]
    fn parse_date_accepts_common_formats() {
        assert!(parse_date("2024-01-02").is_some());
        assert!(parse_date("2024-01-02T15:04:05").is_some());
        assert!(parse_date("2024-01-02T15:04:05Z").is_some());
        assert!(parse_date("02.01.2024").is_some());
        assert!(parse_date("not a date").is_none());
    }

    #[test]
    fn json_get_navigates_objects_and_arrays() {
        let value = serde_json::json!({
            "results": [
                {"title": "first"},
                {"title": "second"}
            ]
        });
        assert_eq!(json_get_str(&value, "results.1.title"), Some("second"));
        assert_eq!(json_get(&value, "results.5"), None);
        assert_eq!(json_get(&value, "missing"), None);
    }

    fn sample_locales() -> StdHashMap<String, String> {
        let mut map = StdHashMap::new();
        map.insert("fr".to_string(), "fr_FR".to_string());
        map.insert("fr-BE".to_string(), "fr_BE".to_string());
        map.insert("en-US".to_string(), "en_US".to_string());
        map.insert("zh".to_string(), "zh".to_string());
        map
    }

    #[test]
    fn get_engine_locale_prefers_exact_then_narrows() {
        let map = sample_locales();
        assert_eq!(
            get_engine_locale("fr-BE", &map, None).as_deref(),
            Some("fr_BE")
        );
        assert_eq!(
            get_engine_locale("fr", &map, None).as_deref(),
            Some("fr_FR")
        );
        assert_eq!(
            get_engine_locale("en", &map, None).as_deref(),
            Some("en_US")
        );
        assert_eq!(
            get_engine_locale("de-DE", &map, Some("fallback")).as_deref(),
            Some("fallback")
        );
    }

    #[test]
    fn get_engine_locale_result_is_always_supported_or_default() {
        let map = sample_locales();
        let supported: std::collections::HashSet<&String> = map.values().collect();
        for locale in ["fr", "fr-BE", "fr-CA", "en", "en-GB", "zh-HK", "xx-YY"] {
            if let Some(result) = get_engine_locale(locale, &map, None) {
                assert!(
                    supported.contains(&result),
                    "locale {locale} resolved to unsupported {result}"
                );
            }
        }
    }

    #[test]
    fn engine_traits_translate_language_and_region() {
        let mut languages = StdHashMap::new();
        languages.insert("fr".to_string(), "fr_FR".to_string());
        let mut regions = StdHashMap::new();
        regions.insert("fr-BE".to_string(), "fr_BE".to_string());
        let traits = zoeken_data::EngineTraits {
            all_locale: Some("xx-all".to_string()),
            data_type: Some("traits_v1".to_string()),
            languages,
            regions,
            custom: serde_json::Value::Null,
        };

        assert_eq!(traits.get_language("fr", None).as_deref(), Some("fr_FR"));
        assert_eq!(traits.get_region("fr-BE", None).as_deref(), Some("fr_BE"));
        assert_eq!(traits.get_language("all", None).as_deref(), Some("xx-all"));
        assert_eq!(traits.get_region("all", None).as_deref(), Some("xx-all"));
    }

    #[test]
    fn engine_traits_bundle_serves_known_engines() {
        let wikipedia = engine_traits("wikipedia").expect("wikipedia traits are bundled");
        assert_eq!(wikipedia.get_language("de-DE", None).as_deref(), Some("de"));

        let google = engine_traits("google").expect("google traits are bundled");
        assert_eq!(google.get_region("en-US", None).as_deref(), Some("US"));

        assert!(engine_traits("not-a-real-engine").is_none());
    }
}
