//! Raw query parsing (bangs, shortcuts, language selectors) and form-parameter mapping.

use std::collections::{HashMap, HashSet};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use thiserror::Error;
use zoeken_data::{BANG_QUERY_PLACEHOLDER, BangTrie, DataBundle};

/// Errors produced while parsing a raw query or mapping form parameters.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QueryError {
    #[error("invalid parameter `{name}`: {value:?}")]
    InvalidParameter { name: String, value: String },
}

/// Language/locale tag (e.g. `en`, `en-US`, `all`, `auto`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Locale {
    tag: String,
}

impl Locale {
    pub const ALL: &'static str = "all";
    pub const AUTO: &'static str = "auto";

    pub fn new(tag: impl Into<String>) -> Self {
        Locale {
            tag: normalize_locale_tag(&tag.into()),
        }
    }

    pub fn all() -> Self {
        Locale {
            tag: Self::ALL.to_string(),
        }
    }

    pub fn auto() -> Self {
        Locale {
            tag: Self::AUTO.to_string(),
        }
    }

    pub fn as_str(&self) -> &str {
        &self.tag
    }

    pub fn is_auto(&self) -> bool {
        self.tag == Self::AUTO
    }

    pub fn is_all(&self) -> bool {
        self.tag == Self::ALL
    }
}

impl std::fmt::Display for Locale {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.tag)
    }
}

fn normalize_locale_tag(value: &str) -> String {
    let value = value.trim().to_lowercase().replace('_', "-");
    let mut parts = value.splitn(2, '-');
    let lang = parts.next().unwrap_or("");
    match parts.next() {
        Some(region) if !region.is_empty() => format!("{}-{}", lang, region.to_uppercase()),
        _ => lang.to_string(),
    }
}

pub fn is_valid_language_code(value: &str) -> bool {
    if value == Locale::AUTO {
        return true;
    }
    let mut parts = value.splitn(2, '-');
    let lang = parts.next().unwrap_or("");
    if !(2..=3).contains(&lang.len()) || !lang.chars().all(|c| c.is_ascii_lowercase()) {
        return false;
    }
    match parts.next() {
        None => true,
        Some(region) => region.len() == 2 && region.chars().all(|c| c.is_ascii_alphabetic()),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum SafeSearch {
    #[default]
    Off = 0,
    Moderate = 1,
    Strict = 2,
}

impl SafeSearch {
    pub fn as_u8(self) -> u8 {
        self as u8
    }

    pub fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(SafeSearch::Off),
            1 => Some(SafeSearch::Moderate),
            2 => Some(SafeSearch::Strict),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TimeRange {
    Day,
    Week,
    Month,
    Year,
}

impl TimeRange {
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "day" => Some(TimeRange::Day),
            "week" => Some(TimeRange::Week),
            "month" => Some(TimeRange::Month),
            "year" => Some(TimeRange::Year),
            _ => None,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            TimeRange::Day => "day",
            TimeRange::Week => "week",
            TimeRange::Month => "month",
            TimeRange::Year => "year",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExternalBang {
    pub target_url: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SearchQuery {
    pub query: String,
    pub categories: Vec<String>,
    pub engines: Vec<String>,
    pub locale: Locale,
    pub pageno: u32,
    pub time_range: Option<TimeRange>,
    pub safesearch: SafeSearch,
    pub timeout: Option<Duration>,
    pub external_bang: Option<ExternalBang>,
    pub redirect: Option<String>,
    pub engine_data: HashMap<String, String>,
}

impl Default for SearchQuery {
    fn default() -> Self {
        SearchQuery {
            query: String::new(),
            categories: Vec::new(),
            engines: Vec::new(),
            locale: Locale::all(),
            pageno: 1,
            time_range: None,
            safesearch: SafeSearch::Off,
            timeout: None,
            external_bang: None,
            redirect: None,
            engine_data: HashMap::new(),
        }
    }
}

pub const REDIRECT_FIRST_RESULT: &str = "first_result";

#[derive(Debug, Clone, PartialEq)]
pub enum ParseOutcome {
    Query(SearchQuery),
    ExternalRedirect(String),
    AnswererRoute {
        answerer: String,
        query: SearchQuery,
    },
}

impl ParseOutcome {
    pub fn resolve(
        raw: &RawTextQuery,
        query: SearchQuery,
        answerer: Option<String>,
    ) -> ParseOutcome {
        if let Some(url) = &raw.external_redirect {
            return ParseOutcome::ExternalRedirect(url.clone());
        }
        match answerer {
            Some(answerer) => ParseOutcome::AnswererRoute { answerer, query },
            None => ParseOutcome::Query(query),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QueryFeedback {
    UnknownBang { bang: String },
}

const KNOWN_CATEGORIES: &[&str] = &[
    "general",
    "images",
    "videos",
    "news",
    "map",
    "music",
    "it",
    "science",
    "files",
    "social media",
    "onions",
    "repos",
    "software wikis",
    "web",
    "q&a",
    "scientific publications",
    "packages",
    "lyrics",
    "movies",
    "radio",
    "tv",
    "weather",
    "apps",
    "dictionaries",
    "translate",
];

#[derive(Debug, Clone, PartialEq)]
pub struct RawTextQuery {
    pub raw: String,
    pub engines: Vec<String>,
    pub categories: Vec<String>,
    pub languages: Vec<String>,
    pub timeout: Option<Duration>,
    pub external_bang: Option<String>,
    pub external_redirect: Option<String>,
    pub specific: bool,
    pub redirect_to_first_result: bool,
    pub feedback: Vec<QueryFeedback>,
    query_parts: Vec<String>,
    user_query_parts: Vec<String>,
}

impl RawTextQuery {
    pub fn query(&self) -> String {
        self.user_query_parts.join(" ")
    }

    pub fn full_query(&self) -> String {
        let parts = self.query_parts.join(" ");
        let user = self.query();
        format!("{} {}", parts, user).trim().to_string()
    }
}

pub fn parse_raw(text: &str, data: &DataBundle) -> Result<RawTextQuery, QueryError> {
    let mut raw = RawTextQuery {
        raw: text.to_string(),
        engines: Vec::new(),
        categories: Vec::new(),
        languages: Vec::new(),
        timeout: None,
        external_bang: None,
        external_redirect: None,
        specific: false,
        redirect_to_first_result: false,
        feedback: Vec::new(),
        query_parts: Vec::new(),
        user_query_parts: Vec::new(),
    };

    for part in text.split_whitespace() {
        let parsed = parse_query_part(&mut raw, part, &data.engine_traits, &data.bangs);
        if parsed {
            raw.query_parts.push(part.to_string());
        } else {
            raw.user_query_parts.push(part.to_string());
        }
    }

    resolve_external_bang(&mut raw, &data.bangs);
    Ok(raw)
}

fn parse_query_part(
    raw: &mut RawTextQuery,
    part: &str,
    engine_traits: &zoeken_data::EngineTraitsMap,
    bangs: &BangTrie,
) -> bool {
    if let Some(rest) = part.strip_prefix('<') {
        return parse_timeout_token(raw, rest);
    }
    if let Some(rest) = part.strip_prefix(':') {
        return parse_language_token(raw, rest);
    }
    if part == "!!" {
        raw.redirect_to_first_result = true;
        return true;
    }
    if let Some(rest) = part.strip_prefix("!!") {
        return parse_external_bang_token(raw, rest, bangs);
    }
    if let Some(rest) = part.strip_prefix('!').or_else(|| part.strip_prefix('?')) {
        return parse_engine_or_category_token(raw, rest, engine_traits);
    }
    false
}

fn parse_timeout_token(raw: &mut RawTextQuery, value: &str) -> bool {
    if value.is_empty() || !value.chars().all(|c| c.is_ascii_digit()) {
        return false;
    }
    let Ok(amount) = value.parse::<u64>() else {
        return false;
    };
    let duration = if amount < 100 {
        Duration::from_secs(amount)
    } else {
        Duration::from_millis(amount)
    };
    raw.timeout = Some(duration);
    true
}

fn parse_language_token(raw: &mut RawTextQuery, value: &str) -> bool {
    if value.is_empty() {
        return false;
    }
    let normalized = normalize_locale_tag(value);
    if !is_valid_language_code(&normalized) {
        return false;
    }
    if !raw.languages.contains(&normalized) {
        raw.languages.push(normalized);
    }
    true
}

fn parse_external_bang_token(raw: &mut RawTextQuery, value: &str, bangs: &BangTrie) -> bool {
    if value.is_empty() {
        return false;
    }
    let token = value.to_lowercase();
    if bangs.resolve(&token).is_some() {
        raw.external_bang = Some(token);
        true
    } else {
        raw.feedback
            .push(QueryFeedback::UnknownBang { bang: token });
        false
    }
}

fn parse_engine_or_category_token(
    raw: &mut RawTextQuery,
    value: &str,
    engine_traits: &zoeken_data::EngineTraitsMap,
) -> bool {
    if value.is_empty() {
        return false;
    }
    let normalized = value.replace(['-', '_'], " ").to_lowercase();

    if engine_traits.get(&normalized).is_some() {
        if !raw.engines.contains(&normalized) {
            raw.engines.push(normalized);
        }
        raw.specific = true;
        return true;
    }

    if KNOWN_CATEGORIES.contains(&normalized.as_str()) {
        if !raw.categories.contains(&normalized) {
            raw.categories.push(normalized);
        }
        raw.specific = true;
        return true;
    }

    false
}

fn resolve_external_bang(raw: &mut RawTextQuery, bangs: &BangTrie) {
    let Some(token) = raw.external_bang.clone() else {
        return;
    };
    if let Some(entry) = bangs.resolve(&token) {
        let url = resolve_bang_url(&entry.url_template, &raw.query());
        raw.external_redirect = Some(url);
    }
}

fn resolve_bang_url(url_template: &str, query: &str) -> String {
    let mut url = url_template.to_string();
    if let Some(rest) = url.strip_prefix("//") {
        url = format!("https://{rest}");
    }
    if query.is_empty() {
        if let Ok(parsed) = url::Url::parse(&url)
            && let Some(host) = parsed.host_str()
        {
            return format!("{}://{}", parsed.scheme(), host);
        }
        url.replace(BANG_QUERY_PLACEHOLDER, "")
    } else {
        url.replace(BANG_QUERY_PLACEHOLDER, &quote_plus(query))
    }
}

fn quote_plus(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(byte as char);
            }
            b' ' => out.push('+'),
            other => {
                out.push('%');
                out.push_str(&format!("{other:02X}"));
            }
        }
    }
    out
}

#[derive(Debug, Clone, Default)]
pub struct FormParams {
    entries: Vec<(String, String)>,
}

impl FormParams {
    pub fn from_pairs(entries: impl IntoIterator<Item = (String, String)>) -> Self {
        FormParams {
            entries: entries.into_iter().collect(),
        }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.entries
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }

    pub fn contains(&self, name: &str) -> bool {
        self.entries.iter().any(|(k, _)| k == name)
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.entries.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

pub trait PreferencesView {
    fn is_locked(&self, key: &str) -> bool;
    fn default_language(&self) -> String;
    fn default_safesearch(&self) -> u8;
    fn default_categories(&self) -> Vec<String>;
}

#[derive(Debug, Clone)]
pub struct StaticPreferences {
    pub locked: HashSet<String>,
    pub language: String,
    pub safesearch: u8,
    pub categories: Vec<String>,
}

impl Default for StaticPreferences {
    fn default() -> Self {
        StaticPreferences {
            locked: HashSet::new(),
            language: Locale::ALL.to_string(),
            safesearch: 0,
            categories: vec!["general".to_string()],
        }
    }
}

impl PreferencesView for StaticPreferences {
    fn is_locked(&self, key: &str) -> bool {
        self.locked.contains(key)
    }
    fn default_language(&self) -> String {
        self.language.clone()
    }
    fn default_safesearch(&self) -> u8 {
        self.safesearch
    }
    fn default_categories(&self) -> Vec<String> {
        self.categories.clone()
    }
}

pub fn from_params<P: PreferencesView + ?Sized>(
    params: &FormParams,
    prefs: &P,
    data: &DataBundle,
) -> Result<SearchQuery, QueryError> {
    let raw_text = match params.get("q") {
        Some(q) if !q.is_empty() => q.to_string(),
        _ => {
            return Err(QueryError::InvalidParameter {
                name: "q".to_string(),
                value: params.get("q").unwrap_or("").to_string(),
            });
        }
    };

    let raw = parse_raw(&raw_text, data)?;

    let pageno = parse_pageno(params)?;
    let safesearch = parse_safesearch(params, prefs)?;
    let time_range = parse_time_range(params)?;
    let timeout = raw.timeout.or(parse_timeout(params)?);

    let locale = if raw.languages.is_empty() {
        parse_lang(params, prefs)?
    } else if prefs.is_locked("language") {
        Locale::new(prefs.default_language())
    } else {
        let tag = normalize_locale_tag(&raw.languages[0]);
        if !is_valid_language_code(&tag) {
            return Err(QueryError::InvalidParameter {
                name: "language".to_string(),
                value: raw.languages[0].clone(),
            });
        }
        Locale::new(tag)
    };

    let categories = if raw.categories.is_empty() {
        parse_categories(params, prefs)
    } else {
        raw.categories.clone()
    };

    let engines = if raw.engines.is_empty() {
        parse_engines(params)
    } else {
        raw.engines.clone()
    };
    let engine_data = parse_engine_data(params)?;

    let external_bang = raw
        .external_bang
        .as_ref()
        .zip(raw.external_redirect.as_ref())
        .map(|(_, url)| ExternalBang {
            target_url: url.clone(),
        });

    let redirect = raw.redirect_to_first_result.then(|| "first".to_string());

    Ok(SearchQuery {
        query: raw.query(),
        categories,
        engines,
        locale,
        pageno,
        time_range,
        safesearch,
        timeout,
        external_bang,
        redirect,
        engine_data,
    })
}

fn parse_engine_data(params: &FormParams) -> Result<HashMap<String, String>, QueryError> {
    let mut out = HashMap::new();
    if let Some(raw) = params.get("engine_data").filter(|v| !v.trim().is_empty()) {
        let parsed: serde_json::Value =
            serde_json::from_str(raw).map_err(|_| QueryError::InvalidParameter {
                name: "engine_data".to_string(),
                value: raw.to_string(),
            })?;
        let Some(object) = parsed.as_object() else {
            return Err(QueryError::InvalidParameter {
                name: "engine_data".to_string(),
                value: raw.to_string(),
            });
        };
        for (key, value) in object {
            if let Some(value) = value.as_str() {
                out.insert(key.clone(), value.to_string());
            }
        }
    }
    for (name, value) in params.iter() {
        if let Some(key) = name.strip_prefix("engine_data.") {
            if !key.is_empty() {
                out.insert(key.to_string(), value.to_string());
            }
        } else if let Some(key) = name
            .strip_prefix("engine_data[")
            .and_then(|rest| rest.strip_suffix(']'))
        {
            if !key.is_empty() {
                out.insert(key.to_string(), value.to_string());
            }
        }
    }
    Ok(out)
}

fn parse_pageno(params: &FormParams) -> Result<u32, QueryError> {
    let value = params.get("pageno").unwrap_or("1");
    let invalid = || QueryError::InvalidParameter {
        name: "pageno".to_string(),
        value: value.to_string(),
    };
    if !value.chars().all(|c| c.is_ascii_digit()) || value.is_empty() {
        return Err(invalid());
    }
    let pageno = value.parse::<u32>().map_err(|_| invalid())?;
    if pageno < 1 {
        return Err(invalid());
    }
    Ok(pageno)
}

fn parse_safesearch<P: PreferencesView + ?Sized>(
    params: &FormParams,
    prefs: &P,
) -> Result<SafeSearch, QueryError> {
    if prefs.is_locked("safesearch") {
        return Ok(SafeSearch::from_u8(prefs.default_safesearch()).unwrap_or_default());
    }
    let level = match params.get("safesearch") {
        Some(value) => {
            let invalid = || QueryError::InvalidParameter {
                name: "safesearch".to_string(),
                value: value.to_string(),
            };
            if value.is_empty() || !value.chars().all(|c| c.is_ascii_digit()) {
                return Err(invalid());
            }
            value.parse::<u8>().map_err(|_| invalid())?
        }
        None => prefs.default_safesearch(),
    };
    SafeSearch::from_u8(level).ok_or_else(|| QueryError::InvalidParameter {
        name: "safesearch".to_string(),
        value: level.to_string(),
    })
}

fn parse_time_range(params: &FormParams) -> Result<Option<TimeRange>, QueryError> {
    match params.get("time_range") {
        None => Ok(None),
        Some(value) if value.is_empty() || value == "None" => Ok(None),
        Some(value) => {
            TimeRange::parse(value)
                .map(Some)
                .ok_or_else(|| QueryError::InvalidParameter {
                    name: "time_range".to_string(),
                    value: value.to_string(),
                })
        }
    }
}

fn parse_timeout(params: &FormParams) -> Result<Option<Duration>, QueryError> {
    match params.get("timeout_limit") {
        None => Ok(None),
        Some(value) if value.is_empty() || value == "None" => Ok(None),
        Some(value) => {
            let seconds = value
                .parse::<f64>()
                .map_err(|_| QueryError::InvalidParameter {
                    name: "timeout_limit".to_string(),
                    value: value.to_string(),
                })?;
            if !seconds.is_finite() || seconds < 0.0 {
                return Err(QueryError::InvalidParameter {
                    name: "timeout_limit".to_string(),
                    value: value.to_string(),
                });
            }
            Ok(Some(Duration::from_secs_f64(seconds)))
        }
    }
}

fn parse_lang<P: PreferencesView + ?Sized>(
    params: &FormParams,
    prefs: &P,
) -> Result<Locale, QueryError> {
    if prefs.is_locked("language") {
        return Ok(Locale::new(prefs.default_language()));
    }
    let raw = params
        .get("language")
        .map(str::to_string)
        .unwrap_or_else(|| prefs.default_language());
    let normalized = normalize_locale_tag(&raw);
    if normalized == Locale::ALL {
        return Ok(Locale::all());
    }
    if !is_valid_language_code(&normalized) {
        return Err(QueryError::InvalidParameter {
            name: "language".to_string(),
            value: raw,
        });
    }
    Ok(Locale::new(normalized))
}

fn parse_categories<P: PreferencesView + ?Sized>(params: &FormParams, prefs: &P) -> Vec<String> {
    let mut categories: Vec<String> = Vec::new();
    if !prefs.is_locked("categories") {
        for (name, value) in params.iter() {
            apply_category_form(&mut categories, name, value);
        }
    }
    if categories.is_empty() {
        categories = prefs.default_categories();
    }
    if categories.is_empty() {
        categories.push("general".to_string());
    }
    categories
}

fn apply_category_form(categories: &mut Vec<String>, name: &str, value: &str) {
    if name == "categories" {
        for categ in value.split(',').map(str::trim).filter(|c| !c.is_empty()) {
            if !categories.iter().any(|c| c == categ) {
                categories.push(categ.to_string());
            }
        }
    } else if let Some(category) = name.strip_prefix("category_") {
        if value == "off" {
            categories.retain(|c| c != category);
        } else if !categories.iter().any(|c| c == category) {
            categories.push(category.to_string());
        }
    }
}

fn parse_engines(params: &FormParams) -> Vec<String> {
    match params.get("engines") {
        Some(value) => value
            .split(',')
            .map(str::trim)
            .filter(|e| !e.is_empty())
            .map(str::to_string)
            .collect(),
        None => Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;
    use zoeken_data::{
        BangEntry, BangTrie, CurrencyTable, EngineTraits, EngineTraitsMap, LocaleMap, UnitTable,
        UserAgentPool,
    };

    fn engine_traits(names: &[&str]) -> EngineTraitsMap {
        let mut engines = HashMap::new();
        for name in names {
            engines.insert(
                name.to_string(),
                EngineTraits {
                    all_locale: None,
                    data_type: None,
                    languages: HashMap::new(),
                    regions: HashMap::new(),
                    custom: Default::default(),
                },
            );
        }
        EngineTraitsMap { engines }
    }

    fn bundle(engines: &[&str], bangs: BangTrie) -> DataBundle {
        DataBundle {
            bangs,
            currencies: CurrencyTable::default(),
            units: UnitTable::default(),
            engine_traits: engine_traits(engines),
            useragents: UserAgentPool::default(),
            locales: LocaleMap::default(),
            ..DataBundle::default()
        }
    }

    #[test]
    fn engine_selector_sets_engine_and_strips_token() {
        let data = bundle(&["brave"], BangTrie::new());
        let raw = parse_raw("hello !brave world", &data).unwrap();
        assert_eq!(raw.engines, vec!["brave".to_string()]);
        assert!(raw.specific);
        assert_eq!(raw.query(), "hello world");
    }

    #[test]
    fn category_selector_sets_category_and_strips_token() {
        let data = bundle(&[], BangTrie::new());
        let raw = parse_raw("!images cats", &data).unwrap();
        assert_eq!(raw.categories, vec!["images".to_string()]);
        assert_eq!(raw.query(), "cats");
    }

    #[test]
    fn question_mark_prefix_selects_like_bang() {
        let data = bundle(&["brave"], BangTrie::new());
        let raw = parse_raw("?brave foo", &data).unwrap();
        assert_eq!(raw.engines, vec!["brave".to_string()]);
        assert_eq!(raw.query(), "foo");
    }

    #[test]
    fn language_selector_sets_language_and_strips_token() {
        let data = bundle(&[], BangTrie::new());
        let raw = parse_raw(":en_us weather", &data).unwrap();
        assert_eq!(raw.languages, vec!["en-US".to_string()]);
        assert_eq!(raw.query(), "weather");
    }

    #[test]
    fn timeout_selector_parses_seconds_and_millis() {
        let data = bundle(&[], BangTrie::new());
        let raw = parse_raw("<3 quick", &data).unwrap();
        assert_eq!(raw.timeout, Some(Duration::from_secs(3)));
        let raw = parse_raw("<850 quick", &data).unwrap();
        assert_eq!(raw.timeout, Some(Duration::from_millis(850)));
    }

    #[test]
    fn known_external_bang_resolves_redirect() {
        let mut bangs = BangTrie::new();
        bangs.insert(
            "gh",
            BangEntry {
                url_template: format!("https://github.com/search?q={BANG_QUERY_PLACEHOLDER}"),
                rank: 0,
            },
        );
        let data = bundle(&[], bangs);
        let raw = parse_raw("!!gh rust lang", &data).unwrap();
        assert_eq!(raw.external_bang, Some("gh".to_string()));
        assert_eq!(
            raw.external_redirect,
            Some("https://github.com/search?q=rust+lang".to_string())
        );
    }

    #[test]
    fn scheme_relative_bang_gets_https() {
        let mut bangs = BangTrie::new();
        bangs.insert(
            "x",
            BangEntry {
                url_template: format!("//example.com/?q={BANG_QUERY_PLACEHOLDER}"),
                rank: 0,
            },
        );
        let data = bundle(&[], bangs);
        let raw = parse_raw("!!x foo", &data).unwrap();
        assert_eq!(
            raw.external_redirect,
            Some("https://example.com/?q=foo".to_string())
        );
    }

    #[test]
    fn empty_query_bang_reduces_to_host() {
        let mut bangs = BangTrie::new();
        bangs.insert(
            "gh",
            BangEntry {
                url_template: format!("https://github.com/search?q={BANG_QUERY_PLACEHOLDER}"),
                rank: 0,
            },
        );
        let data = bundle(&[], bangs);
        let raw = parse_raw("!!gh", &data).unwrap();
        assert_eq!(
            raw.external_redirect,
            Some("https://github.com".to_string())
        );
    }

    #[test]
    fn unknown_bang_leaves_terms_and_reports_feedback() {
        let data = bundle(&[], BangTrie::new());
        let raw = parse_raw("!!nope find me", &data).unwrap();
        assert!(raw.external_bang.is_none());
        assert!(raw.external_redirect.is_none());
        assert_eq!(raw.query(), "!!nope find me");
        assert_eq!(
            raw.feedback,
            vec![QueryFeedback::UnknownBang {
                bang: "nope".to_string()
            }]
        );
    }

    #[test]
    fn feeling_lucky_double_bang() {
        let data = bundle(&[], BangTrie::new());
        let raw = parse_raw("!! lucky search", &data).unwrap();
        assert!(raw.redirect_to_first_result);
        assert_eq!(raw.query(), "lucky search");
    }

    #[test]
    fn parse_outcome_prefers_external_redirect() {
        let mut bangs = BangTrie::new();
        bangs.insert(
            "gh",
            BangEntry {
                url_template: format!("https://github.com/?q={BANG_QUERY_PLACEHOLDER}"),
                rank: 0,
            },
        );
        let data = bundle(&[], bangs);
        let raw = parse_raw("!!gh foo", &data).unwrap();
        let outcome = ParseOutcome::resolve(&raw, SearchQuery::default(), Some("calc".to_string()));
        assert!(matches!(outcome, ParseOutcome::ExternalRedirect(_)));
    }

    #[test]
    fn parse_outcome_routes_to_answerer() {
        let data = bundle(&[], BangTrie::new());
        let raw = parse_raw("avg 1 2 3", &data).unwrap();
        let outcome =
            ParseOutcome::resolve(&raw, SearchQuery::default(), Some("statistics".to_string()));
        match outcome {
            ParseOutcome::AnswererRoute { answerer, .. } => assert_eq!(answerer, "statistics"),
            other => panic!("expected AnswererRoute, got {other:?}"),
        }
    }

    #[test]
    fn from_params_maps_all_fields() {
        let params = FormParams::from_pairs([
            ("q".to_string(), "cats".to_string()),
            ("pageno".to_string(), "2".to_string()),
            ("safesearch".to_string(), "1".to_string()),
            ("time_range".to_string(), "week".to_string()),
            ("language".to_string(), "en-US".to_string()),
            ("categories".to_string(), "images,news".to_string()),
            ("engines".to_string(), "brave, mojeek".to_string()),
            ("timeout_limit".to_string(), "3.5".to_string()),
        ]);
        let prefs = StaticPreferences::default();
        let sq = from_params(&params, &prefs, &DataBundle::default()).unwrap();
        assert_eq!(sq.query, "cats");
        assert_eq!(sq.pageno, 2);
        assert_eq!(sq.safesearch, SafeSearch::Moderate);
        assert_eq!(sq.time_range, Some(TimeRange::Week));
        assert_eq!(sq.locale.as_str(), "en-US");
        assert_eq!(
            sq.categories,
            vec!["images".to_string(), "news".to_string()]
        );
        assert_eq!(sq.engines, vec!["brave".to_string(), "mojeek".to_string()]);
        assert_eq!(sq.timeout, Some(Duration::from_secs_f64(3.5)));
    }

    #[test]
    fn from_params_rejects_invalid_pageno() {
        let params = FormParams::from_pairs([
            ("q".to_string(), "x".to_string()),
            ("pageno".to_string(), "0".to_string()),
        ]);
        let err = from_params(
            &params,
            &StaticPreferences::default(),
            &DataBundle::default(),
        )
        .unwrap_err();
        assert_eq!(
            err,
            QueryError::InvalidParameter {
                name: "pageno".to_string(),
                value: "0".to_string()
            }
        );
    }

    #[test]
    fn from_params_rejects_invalid_time_range() {
        let params = FormParams::from_pairs([
            ("q".to_string(), "x".to_string()),
            ("time_range".to_string(), "decade".to_string()),
        ]);
        let err = from_params(
            &params,
            &StaticPreferences::default(),
            &DataBundle::default(),
        )
        .unwrap_err();
        assert!(matches!(err, QueryError::InvalidParameter { name, .. } if name == "time_range"));
    }

    #[test]
    fn from_params_requires_q() {
        let params = FormParams::from_pairs([("pageno".to_string(), "1".to_string())]);
        let err = from_params(
            &params,
            &StaticPreferences::default(),
            &DataBundle::default(),
        )
        .unwrap_err();
        assert!(matches!(err, QueryError::InvalidParameter { name, .. } if name == "q"));
    }

    #[test]
    fn from_params_defaults_to_general_category() {
        let params = FormParams::from_pairs([("q".to_string(), "x".to_string())]);
        let prefs = StaticPreferences {
            categories: Vec::new(),
            ..StaticPreferences::default()
        };
        let sq = from_params(&params, &prefs, &DataBundle::default()).unwrap();
        assert_eq!(sq.categories, vec!["general".to_string()]);
    }

    #[test]
    fn locked_language_overrides_form() {
        let params = FormParams::from_pairs([
            ("q".to_string(), "x".to_string()),
            ("language".to_string(), "fr".to_string()),
        ]);
        let mut locked = std::collections::HashSet::new();
        locked.insert("language".to_string());
        let prefs = StaticPreferences {
            locked,
            language: "de".to_string(),
            ..StaticPreferences::default()
        };
        let sq = from_params(&params, &prefs, &DataBundle::default()).unwrap();
        assert_eq!(sq.locale.as_str(), "de");
    }
}
