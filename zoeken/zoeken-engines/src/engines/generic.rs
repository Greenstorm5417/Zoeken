//! Generic HTML/XPath and JSON engines.

use std::collections::HashMap;

use quick_xml::de::from_str as xml_from_str;
use scraper::{ElementRef, Html, Selector};
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;
use url::Url;
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, MainResult,
    Processor, RequestParams, SafeSearch, SearchQueryView, Suggestion, TimeRange, extract_text,
    html_to_text, json_get, json_get_str, normalize_url, xpath_select_relative,
};
use zoeken_results::Result_;

use super::util::{encode_component, encode_query, looks_like_bot_wall};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GenericHtmlConfig {
    pub name: String,
    pub shortcut: String,
    #[serde(
        default = "default_categories",
        deserialize_with = "deserialize_categories"
    )]
    pub categories: Vec<String>,
    pub base_url: String,
    pub search_url: String,
    pub api_key: Option<String>,
    pub query_param: String,
    pub page_param: Option<String>,
    #[serde(alias = "first_page_num")]
    pub first_page: u32,
    pub page_size: u32,
    pub send_page_num_on_first_page: bool,
    pub paging: bool,
    pub max_page: u32,
    pub language: Option<String>,
    pub lang_all: String,
    pub time_range_support: bool,
    pub time_range_url: String,
    pub time_range_map: HashMap<String, String>,
    pub safesearch: bool,
    pub safe_search_map: HashMap<String, String>,
    pub method: String,
    pub request_body: String,
    pub headers: HashMap<String, String>,
    pub cookies: HashMap<String, String>,
    pub no_result_for_http_status: Vec<u16>,
    pub empty_result_error: Option<String>,
    #[serde(alias = "results_xpath", alias = "xpath_results")]
    pub result_xpath: Option<String>,
    #[serde(alias = "xpath_url")]
    pub url_xpath: String,
    #[serde(alias = "xpath_title")]
    pub title_xpath: String,
    #[serde(alias = "xpath_content")]
    pub content_xpath: Option<String>,
    #[serde(alias = "xpath_suggestion")]
    pub suggestion_xpath: Option<String>,
    pub result_css: Option<String>,
    pub url_css: Option<String>,
    pub url_attr: String,
    pub title_css: Option<String>,
    pub content_css: Option<String>,
    pub suggestion_css: Option<String>,
}

impl Default for GenericHtmlConfig {
    fn default() -> Self {
        Self {
            name: "generic_xpath".to_string(),
            shortcut: String::new(),
            categories: vec!["general".to_string()],
            base_url: String::new(),
            search_url: String::new(),
            api_key: None,
            query_param: "q".to_string(),
            page_param: None,
            first_page: 1,
            page_size: 1,
            send_page_num_on_first_page: true,
            paging: false,
            max_page: 0,
            language: None,
            lang_all: "en".to_string(),
            time_range_support: false,
            time_range_url: "&hours={time_range_val}".to_string(),
            time_range_map: default_time_range_map(),
            safesearch: false,
            safe_search_map: default_safe_search_map(),
            method: "GET".to_string(),
            request_body: String::new(),
            headers: HashMap::new(),
            cookies: HashMap::new(),
            no_result_for_http_status: Vec::new(),
            empty_result_error: None,
            result_xpath: None,
            url_xpath: ".//a/@href".to_string(),
            title_xpath: ".//a".to_string(),
            content_xpath: None,
            suggestion_xpath: None,
            result_css: None,
            url_css: None,
            url_attr: "href".to_string(),
            title_css: None,
            content_css: None,
            suggestion_css: None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenericHtmlEngine {
    meta: EngineMeta,
    config: GenericHtmlConfig,
}

impl GenericHtmlEngine {
    pub fn new(config: GenericHtmlConfig) -> Result<Self, EngineError> {
        validate_html_config(&config)?;
        let meta = generic_meta(GenericMetaInput {
            name: &config.name,
            shortcut: &config.shortcut,
            categories: config.categories.clone(),
            paging: config.paging,
            max_page: config.max_page,
            time_range_support: config.time_range_support,
            safesearch: config.safesearch,
            language_support: generic_language_support(
                &config.search_url,
                config.language.as_deref(),
            ),
            results: "HTML",
        });
        Ok(Self { meta, config })
    }
}

impl Engine for GenericHtmlEngine {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        apply_generic_request_fields(
            p,
            &self.config.method,
            &self.config.headers,
            &self.config.cookies,
            &self.config.request_body,
            SearchUrlParts {
                template: &self.config.search_url,
                api_key: self.config.api_key.as_deref(),
                query_param: &self.config.query_param,
                page_param: self.config.page_param.as_deref(),
                first_page: self.config.first_page,
                page_size: self.config.page_size,
                send_page_num_on_first_page: self.config.send_page_num_on_first_page,
                lang_all: &self.config.lang_all,
                time_range_support: self.config.time_range_support,
                time_range_url: &self.config.time_range_url,
                time_range_map: &self.config.time_range_map,
                safesearch: self.config.safesearch,
                safe_search_map: &self.config.safe_search_map,
                query: &q.query,
                pageno: q.pageno,
                locale: &q.locale,
                safe_search: q.safesearch,
                time_range: q.time_range,
            },
        );
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        if self.config.no_result_for_http_status.contains(&resp.status) {
            return Ok(EngineResults::new());
        }
        let body = resp.text();
        if let Some(error) = generic_response_error(resp.status, &body, &self.config.name) {
            return Err(error);
        }
        let mut out = EngineResults::new();
        if self.config.result_css.is_some() {
            parse_css_results(&self.config, &body, &mut out)?;
        } else {
            parse_xpath_results(&self.config, &body, &mut out)?;
        }
        if out.results.is_empty()
            && let Some(error) =
                configured_empty_result_error(&self.config.empty_result_error, &self.config.name)
        {
            return Err(error);
        }
        Ok(out)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct GenericJsonConfig {
    pub name: String,
    pub shortcut: String,
    #[serde(
        default = "default_categories",
        deserialize_with = "deserialize_categories"
    )]
    pub categories: Vec<String>,
    pub base_url: String,
    pub search_url: String,
    pub api_key: Option<String>,
    pub query_param: String,
    pub page_param: Option<String>,
    #[serde(alias = "first_page_num")]
    pub first_page: u32,
    pub page_size: u32,
    pub send_page_num_on_first_page: bool,
    pub paging: bool,
    pub max_page: u32,
    pub language: Option<String>,
    pub lang_all: String,
    pub time_range_support: bool,
    pub time_range_url: String,
    pub time_range_map: HashMap<String, String>,
    pub safesearch: bool,
    pub safe_search_map: HashMap<String, String>,
    pub method: String,
    pub request_body: String,
    pub headers: HashMap<String, String>,
    pub cookies: HashMap<String, String>,
    pub no_result_for_http_status: Vec<u16>,
    pub empty_result_error: Option<String>,
    #[serde(alias = "results_query")]
    pub results_path: String,
    #[serde(alias = "url_query")]
    pub url_path: String,
    pub url_prefix: String,
    #[serde(alias = "title_query")]
    pub title_path: String,
    #[serde(alias = "content_query")]
    pub content_path: Option<String>,
    #[serde(alias = "suggestion_query")]
    pub suggestion_path: Option<String>,
    pub title_html_to_text: bool,
    pub content_html_to_text: bool,
}

impl Default for GenericJsonConfig {
    fn default() -> Self {
        Self {
            name: "generic_json".to_string(),
            shortcut: String::new(),
            categories: vec!["general".to_string()],
            base_url: String::new(),
            search_url: String::new(),
            api_key: None,
            query_param: "q".to_string(),
            page_param: None,
            first_page: 1,
            page_size: 1,
            send_page_num_on_first_page: true,
            paging: false,
            max_page: 0,
            language: None,
            lang_all: "en".to_string(),
            time_range_support: false,
            time_range_url: "&hours={time_range_val}".to_string(),
            time_range_map: default_time_range_map(),
            safesearch: false,
            safe_search_map: default_safe_search_map(),
            method: "GET".to_string(),
            request_body: String::new(),
            headers: HashMap::new(),
            cookies: HashMap::new(),
            no_result_for_http_status: Vec::new(),
            empty_result_error: None,
            results_path: String::new(),
            url_path: "url".to_string(),
            url_prefix: String::new(),
            title_path: "title".to_string(),
            content_path: Some("content".to_string()),
            suggestion_path: None,
            title_html_to_text: false,
            content_html_to_text: false,
        }
    }
}

#[derive(Debug, Clone)]
pub struct GenericJsonEngine {
    meta: EngineMeta,
    config: GenericJsonConfig,
}

impl GenericJsonEngine {
    pub fn new(config: GenericJsonConfig) -> Result<Self, EngineError> {
        validate_json_config(&config)?;
        let meta = generic_meta(GenericMetaInput {
            name: &config.name,
            shortcut: &config.shortcut,
            categories: config.categories.clone(),
            paging: config.paging,
            max_page: config.max_page,
            time_range_support: config.time_range_support,
            safesearch: config.safesearch,
            language_support: generic_language_support(
                &config.search_url,
                config.language.as_deref(),
            ),
            results: "JSON",
        });
        Ok(Self { meta, config })
    }
}

impl Engine for GenericJsonEngine {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        apply_generic_request_fields(
            p,
            &self.config.method,
            &self.config.headers,
            &self.config.cookies,
            &self.config.request_body,
            SearchUrlParts {
                template: &self.config.search_url,
                api_key: self.config.api_key.as_deref(),
                query_param: &self.config.query_param,
                page_param: self.config.page_param.as_deref(),
                first_page: self.config.first_page,
                page_size: self.config.page_size,
                send_page_num_on_first_page: self.config.send_page_num_on_first_page,
                lang_all: &self.config.lang_all,
                time_range_support: self.config.time_range_support,
                time_range_url: &self.config.time_range_url,
                time_range_map: &self.config.time_range_map,
                safesearch: self.config.safesearch,
                safe_search_map: &self.config.safe_search_map,
                query: &q.query,
                pageno: q.pageno,
                locale: &q.locale,
                safe_search: q.safesearch,
                time_range: q.time_range,
            },
        );
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        if self.config.no_result_for_http_status.contains(&resp.status) {
            return Ok(EngineResults::new());
        }
        let body = resp.text();
        if let Some(error) = generic_response_error(resp.status, &body, &self.config.name) {
            return Err(error);
        }
        let value: Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid JSON response: {e}")))?;
        let items = json_result_items(&value, &self.config.results_path)?;
        let mut out = EngineResults::new();
        for item in items {
            let Some(title) = json_query_scalar(item, &self.config.title_path) else {
                continue;
            };
            let Some(raw_url) = json_query_scalar(item, &self.config.url_path) else {
                continue;
            };
            let raw_url = format!("{}{}", self.config.url_prefix, raw_url);
            let url = normalize_result_url(
                &raw_url,
                &result_base_url(&self.config.base_url, &self.config.search_url),
            )?;
            out.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title: filter_json_text(&title, self.config.title_html_to_text),
                content: self
                    .config
                    .content_path
                    .as_deref()
                    .and_then(|path| json_query_scalar(item, path))
                    .map(|value| filter_json_text(&value, self.config.content_html_to_text))
                    .unwrap_or_default(),
                engine: self.config.name.clone(),
                ..MainResult::default()
            }));
        }
        if let Some(path) = &self.config.suggestion_path {
            for item in json_query_values(&value, path) {
                if let Some(items) = item.as_array() {
                    for suggestion in items.iter().filter_map(scalar_value) {
                        out.add(Result_::Suggestion(Suggestion {
                            suggestion: extract_text(&suggestion),
                            engine: self.config.name.clone(),
                        }));
                    }
                } else if let Some(suggestion) = scalar_value(item) {
                    out.add(Result_::Suggestion(Suggestion {
                        suggestion: extract_text(&suggestion),
                        engine: self.config.name.clone(),
                    }));
                }
            }
        }
        if out.results.is_empty()
            && let Some(error) =
                configured_empty_result_error(&self.config.empty_result_error, &self.config.name)
        {
            return Err(error);
        }
        Ok(out)
    }
}

struct GenericMetaInput<'a> {
    name: &'a str,
    shortcut: &'a str,
    categories: Vec<String>,
    paging: bool,
    max_page: u32,
    time_range_support: bool,
    safesearch: bool,
    language_support: bool,
    results: &'a str,
}

fn generic_meta(input: GenericMetaInput<'_>) -> EngineMeta {
    EngineMeta {
        name: input.name.to_string(),
        engine_type: Processor::Online,
        categories: input.categories,
        paging: input.paging,
        max_page: input.max_page,
        time_range_support: input.time_range_support,
        safesearch: input.safesearch,
        language_support: input.language_support,
        shortcut: input.shortcut.to_string(),
        about: About {
            results: input.results.to_string(),
            ..About::default()
        },
        ..EngineMeta::default()
    }
}

fn generic_response_error(status: u16, body: &str, name: &str) -> Option<EngineError> {
    if looks_like_bot_wall(status, body) {
        return Some(EngineError::Captcha(name.to_string()));
    }
    match status {
        0..=399 => None,
        401..=403 => Some(EngineError::AccessDenied(name.to_string())),
        429 | 503 => Some(EngineError::TooManyRequests(name.to_string())),
        _ => Some(EngineError::Unexpected(format!(
            "{name} returned HTTP {status}"
        ))),
    }
}

fn configured_empty_result_error(kind: &Option<String>, name: &str) -> Option<EngineError> {
    match kind.as_deref() {
        Some("access_denied") => Some(EngineError::AccessDenied(name.to_string())),
        Some("captcha") => Some(EngineError::Captcha(name.to_string())),
        Some("too_many_requests") => Some(EngineError::TooManyRequests(name.to_string())),
        Some(other) => Some(EngineError::Unexpected(format!(
            "{name} returned no results ({other})"
        ))),
        None => None,
    }
}

fn validate_html_config(config: &GenericHtmlConfig) -> Result<(), EngineError> {
    validate_common(&config.name, &config.search_url)?;
    if config.result_css.is_none() && (config.url_xpath.is_empty() || config.title_xpath.is_empty())
    {
        return Err(EngineError::Parse(
            "generic HTML engine requires url_xpath/title_xpath or result_css".to_string(),
        ));
    }
    Ok(())
}

fn validate_json_config(config: &GenericJsonConfig) -> Result<(), EngineError> {
    validate_common(&config.name, &config.search_url)?;
    if config.url_path.is_empty() || config.title_path.is_empty() {
        return Err(EngineError::Parse(
            "generic JSON engine requires url_path and title_path".to_string(),
        ));
    }
    Ok(())
}

fn validate_common(name: &str, search_url: &str) -> Result<(), EngineError> {
    if name.trim().is_empty() {
        return Err(EngineError::Parse(
            "generic engine name is empty".to_string(),
        ));
    }
    if search_url.trim().is_empty() {
        return Err(EngineError::Parse(format!(
            "generic engine `{name}` has empty search_url"
        )));
    }
    Ok(())
}

struct SearchUrlParts<'a> {
    template: &'a str,
    api_key: Option<&'a str>,
    query_param: &'a str,
    page_param: Option<&'a str>,
    first_page: u32,
    page_size: u32,
    send_page_num_on_first_page: bool,
    lang_all: &'a str,
    time_range_support: bool,
    time_range_url: &'a str,
    time_range_map: &'a HashMap<String, String>,
    safesearch: bool,
    safe_search_map: &'a HashMap<String, String>,
    query: &'a str,
    pageno: u32,
    locale: &'a str,
    safe_search: SafeSearch,
    time_range: Option<TimeRange>,
}

fn apply_generic_request_fields(
    params: &mut RequestParams,
    method: &str,
    headers: &HashMap<String, String>,
    cookies: &HashMap<String, String>,
    request_body: &str,
    parts: SearchUrlParts<'_>,
) {
    let api_key = parts.api_key;
    params.method = parse_http_method(method);
    params.url = Some(build_search_url(parts));
    params.headers.extend(render_headers(headers, api_key));
    params.cookies.extend(cookies.clone());
    if !request_body.is_empty() {
        params.content =
            render_template(request_body, api_key, &template_values_for_body(params)).into_bytes();
    }
    params.raise_for_httperror = false;
}

fn build_search_url(parts: SearchUrlParts<'_>) -> String {
    let SearchUrlParts {
        template,
        api_key,
        query_param,
        page_param,
        first_page,
        page_size,
        send_page_num_on_first_page,
        lang_all,
        time_range_support,
        time_range_url,
        time_range_map,
        safesearch,
        safe_search_map,
        query,
        pageno,
        locale,
        safe_search,
        time_range,
    } = parts;

    let pageno_value = if send_page_num_on_first_page || pageno != 1 {
        first_page.saturating_add(pageno.saturating_sub(1).saturating_mul(page_size))
    } else {
        0
    };
    let pageno_text = if send_page_num_on_first_page || pageno != 1 {
        pageno_value.to_string()
    } else {
        String::new()
    };
    let page = first_page.saturating_add(pageno.saturating_sub(1));
    let offset = pageno.saturating_sub(1).saturating_mul(page_size);
    let lang = request_language(locale, lang_all);
    let time_range = render_time_range(
        time_range,
        time_range_support,
        time_range_url,
        time_range_map,
    );
    let safe_search = render_safe_search(safe_search, safesearch, safe_search_map);

    if template.contains("{query}")
        || template.contains("{pageno}")
        || template.contains("{page}")
        || template.contains("{offset}")
        || template.contains("{lang}")
        || template.contains("{time_range}")
        || template.contains("{safe_search}")
        || template.contains("{api_key}")
    {
        return template
            .replace("{query}", &encode_component(query))
            .replace(
                "{api_key}",
                &api_key.map(encode_component).unwrap_or_default(),
            )
            .replace("{pageno}", &pageno_text)
            .replace("{page}", &page.to_string())
            .replace("{offset}", &offset.to_string())
            .replace("{lang}", &lang)
            .replace("{time_range}", &time_range)
            .replace("{safe_search}", &safe_search);
    }
    let mut pairs = vec![(query_param, query.to_string())];
    if let Some(page_param) = page_param {
        let page = first_page.saturating_add(pageno.saturating_sub(1));
        pairs.push((page_param, page.to_string()));
    }
    let separator = if template.contains('?') { '&' } else { '?' };
    format!("{template}{separator}{}", encode_query(&pairs))
}

fn render_template(
    template: &str,
    api_key: Option<&str>,
    values: &HashMap<&'static str, String>,
) -> String {
    values.iter().fold(
        template.replace("{api_key}", api_key.unwrap_or_default()),
        |acc, (key, value)| acc.replace(&format!("{{{key}}}"), value),
    )
}

fn template_values_for_body(params: &RequestParams) -> HashMap<&'static str, String> {
    let mut values = HashMap::new();
    values.insert("query", params.query.clone());
    values.insert("pageno", params.pageno.to_string());
    values.insert("page", params.pageno.to_string());
    values.insert("offset", params.pageno.saturating_sub(1).to_string());
    values.insert("lang", request_language(&params.locale_key, "en"));
    values.insert("time_range", String::new());
    values.insert("safe_search", String::new());
    values
}

fn parse_http_method(method: &str) -> HttpMethod {
    if method.eq_ignore_ascii_case("POST") {
        HttpMethod::Post
    } else {
        HttpMethod::Get
    }
}

fn request_language(locale: &str, lang_all: &str) -> String {
    if locale.is_empty() || locale.eq_ignore_ascii_case("all") {
        return lang_all.to_string();
    }
    locale
        .split(['-', '_'])
        .next()
        .filter(|value| !value.is_empty())
        .unwrap_or(lang_all)
        .to_string()
}

fn render_time_range(
    time_range: Option<TimeRange>,
    supported: bool,
    template: &str,
    map: &HashMap<String, String>,
) -> String {
    if !supported {
        return String::new();
    }
    let Some(time_range) = time_range else {
        return String::new();
    };
    let value = map.get(time_range.as_str()).cloned().unwrap_or_default();
    template.replace("{time_range_val}", &value)
}

fn render_safe_search(
    safe_search: SafeSearch,
    supported: bool,
    map: &HashMap<String, String>,
) -> String {
    if !supported {
        return String::new();
    }
    map.get(&safe_search.level().to_string())
        .cloned()
        .unwrap_or_default()
}

fn render_headers(
    headers: &HashMap<String, String>,
    api_key: Option<&str>,
) -> HashMap<String, String> {
    headers
        .iter()
        .map(|(key, value)| {
            (
                key.clone(),
                value.replace("{api_key}", api_key.unwrap_or_default()),
            )
        })
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum GenericEngineConfig {
    Html(GenericHtmlConfig),
    Json(GenericJsonConfig),
}

#[derive(Debug, Clone, Deserialize)]
struct CatalogEntry {
    engine: String,
    #[serde(flatten)]
    value: Value,
}

pub fn builtin_generic_config(key: &str) -> Option<GenericEngineConfig> {
    let value = GENERIC_CATALOG
        .iter()
        .find(|entry| entry.value.get("name").and_then(Value::as_str) == Some(key))?;
    match value.engine.as_str() {
        "xpath" => serde_json::from_value::<GenericHtmlConfig>(value.value.clone())
            .ok()
            .map(GenericEngineConfig::Html),
        "json_engine" => serde_json::from_value::<GenericJsonConfig>(value.value.clone())
            .ok()
            .map(GenericEngineConfig::Json),
        _ => None,
    }
}

pub fn builtin_generic_html_config(key: &str) -> Option<GenericHtmlConfig> {
    match builtin_generic_config(key)? {
        GenericEngineConfig::Html(config) => Some(config),
        GenericEngineConfig::Json(_) => None,
    }
}

/// Ids of catalog engines that are usable as-is (excludes `inactive` entries:
/// engines whose upstream needs tokens/keys/multi-step requests, or templates
/// that require per-instance configuration).
pub fn builtin_generic_ids() -> impl Iterator<Item = &'static str> {
    GENERIC_CATALOG
        .iter()
        .filter(|entry| entry.value.get("inactive").and_then(Value::as_bool) != Some(true))
        .filter_map(|entry| entry.value.get("name").and_then(Value::as_str))
}

/// All catalog ids, including inactive entries (which can still be
/// instantiated explicitly via [`builtin_generic_config`]).
pub fn all_generic_ids() -> impl Iterator<Item = &'static str> {
    GENERIC_CATALOG
        .iter()
        .filter_map(|entry| entry.value.get("name").and_then(Value::as_str))
}

static GENERIC_CATALOG_JSON: &str = include_str!("generic_catalog.json");

static GENERIC_CATALOG: std::sync::LazyLock<Vec<CatalogEntry>> = std::sync::LazyLock::new(|| {
    serde_json::from_str(GENERIC_CATALOG_JSON).expect("valid generic engine catalog")
});

fn parse_xpath_results(
    config: &GenericHtmlConfig,
    body: &str,
    out: &mut EngineResults,
) -> Result<(), EngineError> {
    let Some(item_xpath) = config.result_xpath.as_deref() else {
        return parse_global_xpath_results(config, body, out);
    };
    let titles = xpath_select_relative(body, item_xpath, &config.title_xpath)?;
    let urls = xpath_select_relative(body, item_xpath, &config.url_xpath)?;
    let contents = match &config.content_xpath {
        Some(expr) => xpath_select_relative(body, item_xpath, expr)?,
        None => Vec::new(),
    };
    let base_url = result_base_url(&config.base_url, &config.search_url);
    for (idx, (title, raw_url)) in titles.into_iter().zip(urls).enumerate() {
        if title.trim().is_empty() || raw_url.trim().is_empty() {
            continue;
        }
        let url = normalize_result_url(&raw_url, &base_url)?;
        out.add(Result_::Main(MainResult {
            url: url.clone(),
            normalized_url: url,
            title: extract_text(&title),
            content: contents
                .get(idx)
                .map(|value| html_to_text(value))
                .unwrap_or_default(),
            engine: config.name.clone(),
            ..MainResult::default()
        }));
    }
    if let Some(expr) = &config.suggestion_xpath {
        for suggestion in xpath_select_relative(body, "/*", expr)? {
            let suggestion = extract_text(&suggestion);
            if !suggestion.is_empty() {
                out.add(Result_::Suggestion(Suggestion {
                    suggestion,
                    engine: config.name.clone(),
                }));
            }
        }
    }
    if out.results.is_empty() {
        parse_rss_item_results(config, body, out)?;
    }
    Ok(())
}

#[derive(Debug, Deserialize)]
struct RssFeed {
    channel: RssChannel,
}

#[derive(Debug, Deserialize)]
struct RssChannel {
    #[serde(default, rename = "item")]
    items: Vec<RssItem>,
}

#[derive(Debug, Deserialize)]
struct RssItem {
    title: Option<String>,
    link: Option<String>,
    description: Option<String>,
}

fn parse_rss_item_results(
    config: &GenericHtmlConfig,
    body: &str,
    out: &mut EngineResults,
) -> Result<(), EngineError> {
    if config.result_xpath.as_deref() != Some("//item")
        || config.title_xpath != "./title"
        || config.url_xpath != "./link"
    {
        return Ok(());
    }
    let Ok(feed) = xml_from_str::<RssFeed>(body) else {
        return Ok(());
    };
    let base_url = result_base_url(&config.base_url, &config.search_url);
    for item in feed.channel.items {
        let (Some(title), Some(raw_url)) = (item.title, item.link) else {
            continue;
        };
        if title.trim().is_empty() || raw_url.trim().is_empty() {
            continue;
        }
        let url = normalize_result_url(&raw_url, &base_url)?;
        out.add(Result_::Main(MainResult {
            url: url.clone(),
            normalized_url: url,
            title: extract_text(&title),
            content: item
                .description
                .as_deref()
                .map(html_to_text)
                .unwrap_or_default(),
            engine: config.name.clone(),
            ..MainResult::default()
        }));
    }
    Ok(())
}

fn parse_global_xpath_results(
    config: &GenericHtmlConfig,
    body: &str,
    out: &mut EngineResults,
) -> Result<(), EngineError> {
    let document = zoeken_engine_core::HtmlDocument::parse(body);
    let titles = document.eval_xpath_list(&config.title_xpath, None)?;
    let urls = document.eval_xpath_list(&config.url_xpath, None)?;
    let contents = match &config.content_xpath {
        Some(expr) => document.eval_xpath_list(expr, None)?,
        None => Vec::new(),
    };
    let base_url = result_base_url(&config.base_url, &config.search_url);
    for (idx, (title, raw_url)) in titles.into_iter().zip(urls).enumerate() {
        if title.trim().is_empty() || raw_url.trim().is_empty() {
            continue;
        }
        let url = normalize_result_url(&raw_url, &base_url)?;
        out.add(Result_::Main(MainResult {
            url: url.clone(),
            normalized_url: url,
            title: extract_text(&title),
            content: contents
                .get(idx)
                .map(|value| html_to_text(value))
                .unwrap_or_default(),
            engine: config.name.clone(),
            ..MainResult::default()
        }));
    }
    Ok(())
}

fn parse_css_results(
    config: &GenericHtmlConfig,
    body: &str,
    out: &mut EngineResults,
) -> Result<(), EngineError> {
    let document = Html::parse_document(body);
    let result_sel = selector(config.result_css.as_deref().unwrap_or_default())?;
    let title_sel = selector(config.title_css.as_deref().unwrap_or("a"))?;
    let url_sel = selector(config.url_css.as_deref().unwrap_or("a"))?;
    let content_sel = match &config.content_css {
        Some(value) => Some(selector(value)?),
        None => None,
    };
    let base_url = result_base_url(&config.base_url, &config.search_url);
    for result in document.select(&result_sel) {
        let Some(title_el) = result.select(&title_sel).next() else {
            continue;
        };
        let Some(url_el) = result.select(&url_sel).next() else {
            continue;
        };
        let Some(raw_url) = url_el.value().attr(&config.url_attr) else {
            continue;
        };
        let url = normalize_result_url(raw_url, &base_url)?;
        out.add(Result_::Main(MainResult {
            url: url.clone(),
            normalized_url: url,
            title: element_text(title_el),
            content: content_sel
                .as_ref()
                .and_then(|sel| result.select(sel).next())
                .map(element_text)
                .unwrap_or_default(),
            engine: config.name.clone(),
            ..MainResult::default()
        }));
    }
    if let Some(selector_raw) = &config.suggestion_css {
        let suggestion_sel = selector(selector_raw)?;
        for suggestion in document.select(&suggestion_sel).map(element_text) {
            if !suggestion.is_empty() {
                out.add(Result_::Suggestion(Suggestion {
                    suggestion,
                    engine: config.name.clone(),
                }));
            }
        }
    }
    Ok(())
}

fn selector(raw: &str) -> Result<Selector, EngineError> {
    Selector::parse(raw)
        .map_err(|e| EngineError::Parse(format!("invalid CSS selector `{raw}`: {e:?}")))
}

fn element_text(el: ElementRef<'_>) -> String {
    extract_text(&el.text().collect::<String>())
}

fn normalize_result_url(raw: &str, base_url: &str) -> Result<String, EngineError> {
    normalize_url(&extract_text(raw), base_url)
}

fn json_query_scalar(value: &Value, path: &str) -> Option<String> {
    if path.contains('/') {
        return json_query_values(value, path)
            .into_iter()
            .find_map(scalar_value);
    }
    json_get_str(value, path)
        .map(str::to_string)
        .or_else(|| json_get(value, path).and_then(scalar_value))
}

fn json_result_items<'a>(value: &'a Value, path: &str) -> Result<Vec<&'a Value>, EngineError> {
    if path.is_empty() {
        return value
            .as_array()
            .map(|items| items.iter().collect())
            .ok_or_else(|| EngineError::Parse("JSON root is not an array".to_string()));
    }
    let found = if path.contains('/') {
        json_query_values(value, path)
    } else {
        json_get(value, path).into_iter().collect()
    };
    let Some(first) = found.first() else {
        return Ok(Vec::new());
    };
    first
        .as_array()
        .map(|items| items.iter().collect())
        .ok_or_else(|| {
            EngineError::Parse(format!("JSON path `{path}` did not resolve to an array"))
        })
}

fn json_query_values<'a>(value: &'a Value, path: &str) -> Vec<&'a Value> {
    let parts: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();
    let mut out = Vec::new();
    json_query_inner(value, &parts, &mut out);
    out
}

fn json_query_inner<'a>(value: &'a Value, parts: &[&str], out: &mut Vec<&'a Value>) {
    let Some((needle, rest)) = parts.split_first() else {
        return;
    };
    match value {
        Value::Object(map) => {
            for (key, child) in map {
                if rest.is_empty() {
                    if key == needle {
                        out.push(child);
                    } else if child.is_object() || child.is_array() {
                        json_query_inner(child, parts, out);
                    }
                } else if key == needle {
                    json_query_inner(child, rest, out);
                } else if child.is_object() || child.is_array() {
                    json_query_inner(child, parts, out);
                }
            }
        }
        Value::Array(items) => {
            for (idx, child) in items.iter().enumerate() {
                if idx.to_string() == *needle {
                    if rest.is_empty() {
                        out.push(child);
                    } else {
                        json_query_inner(child, rest, out);
                    }
                } else if child.is_object() || child.is_array() {
                    json_query_inner(child, parts, out);
                }
            }
        }
        _ => {}
    }
}

fn scalar_value(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        Value::Bool(value) => Some(value.to_string()),
        _ => None,
    }
}

fn filter_json_text(value: &str, html: bool) -> String {
    if html {
        html_to_text(value)
    } else {
        extract_text(value)
    }
}

fn result_base_url(base_url: &str, search_url: &str) -> String {
    if !base_url.trim().is_empty() {
        return base_url.to_string();
    }
    Url::parse(search_url)
        .ok()
        .and_then(|url| url.origin().ascii_serialization().parse::<Url>().ok())
        .map(|mut url| {
            url.set_path("/");
            url.to_string()
        })
        .unwrap_or_else(|| search_url.to_string())
}

fn generic_language_support(search_url: &str, language: Option<&str>) -> bool {
    search_url.contains("{lang}") || language.is_some()
}

fn default_categories() -> Vec<String> {
    vec!["general".to_string()]
}

fn deserialize_categories<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Raw {
        One(String),
        Many(Vec<String>),
    }

    Ok(match Option::<Raw>::deserialize(deserializer)? {
        Some(Raw::One(value)) if value.trim().is_empty() => Vec::new(),
        Some(Raw::One(value)) => vec![value],
        Some(Raw::Many(values)) => values,
        None => default_categories(),
    })
}

fn default_time_range_map() -> HashMap<String, String> {
    [
        ("day".to_string(), "24".to_string()),
        ("week".to_string(), (24 * 7).to_string()),
        ("month".to_string(), (24 * 30).to_string()),
        ("year".to_string(), (24 * 365).to_string()),
    ]
    .into_iter()
    .collect()
}

fn default_safe_search_map() -> HashMap<String, String> {
    [
        ("0".to_string(), "&filter=none".to_string()),
        ("1".to_string(), "&filter=moderate".to_string()),
        ("2".to_string(), "&filter=strict".to_string()),
    ]
    .into_iter()
    .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::Fixture;
    use std::path::PathBuf;

    fn query() -> SearchQueryView {
        SearchQueryView {
            query: "rust search".to_string(),
            pageno: 2,
            ..SearchQueryView::default()
        }
    }

    #[test]
    fn generic_xpath_builds_request_and_parses_relative_results() {
        let engine = GenericHtmlEngine::new(GenericHtmlConfig {
            name: "example_xpath".to_string(),
            shortcut: "ex".to_string(),
            base_url: "https://example.test/".to_string(),
            search_url: "https://example.test/search".to_string(),
            page_param: Some("page".to_string()),
            paging: true,
            result_xpath: Some("//article".to_string()),
            title_xpath: ".//a".to_string(),
            url_xpath: ".//a/@href".to_string(),
            content_xpath: Some(".//p".to_string()),
            ..GenericHtmlConfig::default()
        })
        .expect("generic xpath engine");
        let mut params = RequestParams::default();
        engine.request(&query(), &mut params);
        assert_eq!(
            params.url.as_deref(),
            Some("https://example.test/search?q=rust+search&page=2")
        );

        let response = EngineResponse {
            status: 200,
            url: "https://example.test/search".to_string(),
            body: br#"
              <html><body>
                <article><a href="/one">First</a><p>Alpha result</p></article>
                <article><a href="https://other.test/two">Second</a><p>Beta result</p></article>
              </body></html>
            "#
            .to_vec(),
            ..EngineResponse::default()
        };
        let results = engine.response(&response).expect("parse response");
        assert_eq!(results.results.len(), 2);
        assert!(matches!(
            &results.results[0],
            Result_::Main(result)
                if result.title == "First"
                    && result.url == "https://example.test/one"
                    && result.content == "Alpha result"
        ));
    }

    #[test]
    fn generic_css_parses_results() {
        let engine = GenericHtmlEngine::new(GenericHtmlConfig {
            name: "example_css".to_string(),
            base_url: "https://example.test/".to_string(),
            search_url: "https://example.test/search?q={query}&p={page}".to_string(),
            result_css: Some(".result".to_string()),
            title_css: Some(".title".to_string()),
            url_css: Some(".title".to_string()),
            content_css: Some(".snippet".to_string()),
            ..GenericHtmlConfig::default()
        })
        .expect("generic css engine");
        let mut params = RequestParams::default();
        engine.request(&query(), &mut params);
        assert_eq!(
            params.url.as_deref(),
            Some("https://example.test/search?q=rust+search&p=2")
        );
        let response = EngineResponse {
            status: 200,
            url: "https://example.test/search".to_string(),
            body: br#"<div class="result"><a class="title" href="/one">First</a><p class="snippet">Text</p></div>"#.to_vec(),
            ..EngineResponse::default()
        };
        let results = engine.response(&response).expect("parse response");
        assert_eq!(results.results.len(), 1);
    }

    #[test]
    fn generic_xpath_parses_rss_items() {
        let engine = GenericHtmlEngine::new(GenericHtmlConfig {
            name: "example_rss".to_string(),
            base_url: "https://example.test/".to_string(),
            search_url: "https://example.test/rss?q={query}".to_string(),
            result_xpath: Some("//item".to_string()),
            title_xpath: "./title".to_string(),
            url_xpath: "./link".to_string(),
            content_xpath: Some("./description".to_string()),
            ..GenericHtmlConfig::default()
        })
        .expect("generic rss engine");
        let response = EngineResponse {
            status: 200,
            url: "https://example.test/rss?q=rust".to_string(),
            body: br#"<?xml version="1.0" encoding="UTF-8"?>
              <rss version="2.0">
                <channel>
                  <item>
                    <title><![CDATA[First item]]></title>
                    <link>https://example.test/one</link>
                    <description><![CDATA[<p>Alpha <b>result</b></p>]]></description>
                  </item>
                </channel>
              </rss>"#
                .to_vec(),
            ..EngineResponse::default()
        };
        let results = engine.response(&response).expect("parse rss");
        assert_eq!(results.results.len(), 1);
        assert!(matches!(
            &results.results[0],
            Result_::Main(result)
                if result.title == "First item"
                    && result.url == "https://example.test/one"
                    && result.content == "Alpha result"
        ));
    }

    #[test]
    fn generic_html_maps_access_denied_status_to_error() {
        let engine = GenericHtmlEngine::new(GenericHtmlConfig {
            name: "blocked_html".to_string(),
            base_url: "https://example.test/".to_string(),
            search_url: "https://example.test/search?q={query}".to_string(),
            result_xpath: Some("//article".to_string()),
            title_xpath: ".//a".to_string(),
            url_xpath: ".//a/@href".to_string(),
            ..GenericHtmlConfig::default()
        })
        .expect("generic html engine");
        let response = EngineResponse {
            status: 401,
            url: "https://example.test/search?q=rust".to_string(),
            ..EngineResponse::default()
        };
        assert!(matches!(
            engine.response(&response),
            Err(EngineError::AccessDenied(name)) if name == "blocked_html"
        ));
    }

    #[test]
    fn generic_html_can_map_empty_parse_to_configured_error() {
        let engine = GenericHtmlEngine::new(GenericHtmlConfig {
            name: "empty_blocked_html".to_string(),
            base_url: "https://example.test/".to_string(),
            search_url: "https://example.test/search?q={query}".to_string(),
            empty_result_error: Some("access_denied".to_string()),
            result_xpath: Some("//article".to_string()),
            title_xpath: ".//a".to_string(),
            url_xpath: ".//a/@href".to_string(),
            ..GenericHtmlConfig::default()
        })
        .expect("generic html engine");
        let response = EngineResponse {
            status: 200,
            url: "https://example.test/search?q=rust".to_string(),
            body: b"<html><body></body></html>".to_vec(),
            ..EngineResponse::default()
        };
        assert!(matches!(
            engine.response(&response),
            Err(EngineError::AccessDenied(name)) if name == "empty_blocked_html"
        ));
    }

    #[test]
    fn generic_json_builds_request_and_parses_results() {
        let engine = GenericJsonEngine::new(GenericJsonConfig {
            name: "example_json".to_string(),
            base_url: "https://api.example.test/".to_string(),
            search_url: "https://api.example.test/search".to_string(),
            page_param: Some("page".to_string()),
            results_path: "data.items".to_string(),
            url_path: "link".to_string(),
            title_path: "name".to_string(),
            content_path: Some("summary".to_string()),
            suggestion_path: Some("suggestions".to_string()),
            ..GenericJsonConfig::default()
        })
        .expect("generic json engine");
        let mut params = RequestParams::default();
        engine.request(&query(), &mut params);
        assert_eq!(
            params.url.as_deref(),
            Some("https://api.example.test/search?q=rust+search&page=2")
        );
        let response = EngineResponse {
            status: 200,
            url: "https://api.example.test/search".to_string(),
            body: serde_json::json!({
                "data": {"items": [{"name": "First", "link": "/one", "summary": "<b>Text</b>"}]},
                "suggestions": ["rust book"]
            })
            .to_string()
            .into_bytes(),
            ..EngineResponse::default()
        };
        let results = engine.response(&response).expect("parse response");
        assert_eq!(results.results.len(), 1);
        assert_eq!(results.suggestions.len(), 1);
    }

    #[test]
    fn generic_json_renders_api_key_in_url_and_headers() {
        let engine = GenericJsonEngine::new(GenericJsonConfig {
            name: "keyed_json".to_string(),
            base_url: "https://api.example.test/".to_string(),
            search_url: "https://api.example.test/search?token={api_key}&q={query}".to_string(),
            api_key: Some("secret token".to_string()),
            headers: [("Authorization".to_string(), "Bearer {api_key}".to_string())]
                .into_iter()
                .collect(),
            results_path: "items".to_string(),
            url_path: "url".to_string(),
            title_path: "title".to_string(),
            ..GenericJsonConfig::default()
        })
        .expect("generic json engine");
        let mut params = RequestParams::default();
        engine.request(&query(), &mut params);
        assert_eq!(
            params.url.as_deref(),
            Some("https://api.example.test/search?token=secret+token&q=rust+search")
        );
        assert_eq!(
            params.headers.get("Authorization").map(String::as_str),
            Some("Bearer secret token")
        );
    }

    #[test]
    fn builtin_generic_catalog_builds_converted_engines() {
        let all_ids: Vec<_> = all_generic_ids().collect();
        assert!(
            all_ids.len() >= 195,
            "generic catalog should include bulk-ported engines"
        );
        let active: Vec<_> = builtin_generic_ids().collect();
        assert!(
            active.len() >= 120,
            "most catalog engines should be active (got {})",
            active.len()
        );
        for id in all_ids {
            match builtin_generic_config(id).expect("catalog config") {
                GenericEngineConfig::Html(config) => {
                    let engine = GenericHtmlEngine::new(config).expect("catalog engine");
                    let mut params = RequestParams::default();
                    engine.request(&query(), &mut params);
                    let url = params.url.expect("request url");
                    assert!(
                        url.contains("rust+search"),
                        "{id} request should include encoded query, got {url}"
                    );
                }
                GenericEngineConfig::Json(config) => {
                    let engine = GenericJsonEngine::new(config).expect("catalog engine");
                    let mut params = RequestParams::default();
                    engine.request(&query(), &mut params);
                    let url = params.url.expect("request url");
                    assert!(
                        url.contains("rust+search"),
                        "{id} request should include encoded query, got {url}"
                    );
                }
            }
        }
    }

    #[test]
    fn builtin_abcnyheter_config_parses_sample_result() {
        let engine = GenericHtmlEngine::new(
            builtin_generic_html_config("abcnyheter").expect("abcnyheter config"),
        )
        .expect("abcnyheter engine");
        let response = EngineResponse {
            status: 200,
            url: "https://startsiden.abcnyheter.no/sok/?q=rust".to_string(),
            body: br#"
              <ul class="results__list">
                <li class="result">
                  <a href="/nyhet"><h3>Rust Site</h3></a>
                  <div>A small site about Rust.</div>
                </li>
              </ul>
            "#
            .to_vec(),
            ..EngineResponse::default()
        };
        let results = engine.response(&response).expect("parse abcnyheter");
        assert_eq!(results.results.len(), 1);
        assert!(matches!(
            &results.results[0],
            Result_::Main(result)
                if result.title == "Rust Site"
                    && result.url == "https://startsiden.abcnyheter.no/nyhet"
                    && result.content == "A small site about Rust."
        ));
    }

    #[test]
    #[ignore = "regenerates generic catalog conformance fixtures"]
    fn generate_generic_catalog_fixtures() {
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join("generic");
        for id in builtin_generic_ids() {
            let config = builtin_generic_config(id).expect("catalog config");
            let query = query();
            let mut request = RequestParams {
                query: query.query.clone(),
                pageno: query.pageno,
                safesearch: query.safesearch,
                time_range: query.time_range,
                locale_key: query.locale.clone(),
                ..RequestParams::default()
            };
            let response_body = match &config {
                GenericEngineConfig::Html(config) => {
                    GenericHtmlEngine::new(config.clone())
                        .expect("html engine")
                        .request(&query, &mut request);
                    "<html><body></body></html>".as_bytes().to_vec()
                }
                GenericEngineConfig::Json(config) => {
                    GenericJsonEngine::new(config.clone())
                        .expect("json engine")
                        .request(&query, &mut request);
                    if config.results_path.is_empty() {
                        b"[]".to_vec()
                    } else {
                        b"{}".to_vec()
                    }
                }
            };
            let fixture = Fixture::capture(
                id,
                query,
                EngineResponse {
                    status: 200,
                    url: request.url.clone().unwrap_or_default(),
                    body: response_body,
                    ..EngineResponse::default()
                },
                EngineResults::new(),
            )
            .with_case("generic-empty")
            .with_golden_request(request);
            fixture
                .save(root.join(format!("{}.json", fixture_name(id))))
                .expect("save fixture");
        }
    }

    fn fixture_name(value: &str) -> String {
        value
            .chars()
            .map(|ch| if ch.is_ascii_alphanumeric() { ch } else { '_' })
            .collect()
    }
}
