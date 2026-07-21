//! zoeken-data: bundled static data assets (bangs, currencies, units, engine traits,
//! user-agents, locales). Default tables are precompiled at build time (PHF / static
//! slices); `APP_DATA_DIR` can still load JSON from disk. Provides user-agent
//! generation and language detection.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use rand::Rng;
use serde::Deserialize;
use thiserror::Error;

/// Errors produced while loading bundled data assets, with affected file identified.
#[derive(Debug, Error)]
pub enum DataError {
    #[error("failed to read bundled data file `{file}`: {source}")]
    Read {
        file: String,
        source: std::io::Error,
    },

    #[error("failed to parse bundled data file `{file}`: {source}")]
    Parse {
        file: String,
        source: serde_json::Error,
    },
}

/// Fully-loaded bundled static data assets.
#[derive(Debug, Default, Clone)]
pub struct DataBundle {
    pub bangs: BangTrie,
    pub currencies: CurrencyTable,
    pub units: UnitTable,
    pub engine_traits: EngineTraitsMap,
    pub useragents: UserAgentPool,
    pub locales: LocaleMap,
    pub tracker_patterns: TrackerPatterns,
    pub ahmia_blacklist: AhmiaBlacklist,
    pub doi_resolvers: DoiResolvers,
    pub autocomplete: AutocompleteMetadata,
    pub limiter_toml: String,
    pub info_pages: InfoPages,
    pub plugin_data: PluginData,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct DoiResolvers {
    pub default: String,
    pub resolvers: BTreeMap<String, String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct AutocompleteMetadata {
    pub backends: Vec<String>,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct InfoPage {
    pub title: String,
    pub content: String,
}

#[derive(Debug, Default, Clone, Deserialize)]
pub struct InfoPages {
    #[serde(default = "default_info_locale")]
    pub default_locale: String,
    #[serde(default)]
    pub locales: BTreeMap<String, BTreeMap<String, InfoPage>>,
}

fn default_info_locale() -> String {
    "en".to_string()
}

impl InfoPages {
    /// Resolve an information page using exact locale, base language, then the
    /// catalog's configured default locale.
    pub fn resolve<'a>(&'a self, requested: &str, page: &str) -> Option<(&'a str, &'a InfoPage)> {
        let requested = requested.trim();
        if let Some(resolved) = self.page_for_locale(requested, page) {
            return Some(resolved);
        }

        let base = requested.split(['-', '_']).next().unwrap_or(requested);
        if !base.eq_ignore_ascii_case(requested)
            && let Some(resolved) = self.page_for_locale(base, page)
        {
            return Some(resolved);
        }

        self.page_for_locale(&self.default_locale, page)
    }

    fn page_for_locale<'a>(&'a self, locale: &str, page: &str) -> Option<(&'a str, &'a InfoPage)> {
        self.locales.iter().find_map(|(candidate, pages)| {
            candidate
                .eq_ignore_ascii_case(locale)
                .then(|| pages.get(page).map(|info| (candidate.as_str(), info)))
                .flatten()
        })
    }
}

/// Host matcher for a ClearURLs-style provider (compiled at build time).
#[derive(Debug, Clone, Copy)]
pub enum HostMatch {
    Any,
    Suffix(&'static str),
    Label(&'static str),
    AnySuffix(&'static [&'static str]),
    Regex(&'static str),
}

/// One precompiled ClearURLs provider rule.
#[derive(Debug, Clone)]
pub struct CompiledTrackerRule {
    pub url_pattern: &'static str,
    pub host: HostMatch,
    pub path_prefix: Option<&'static str>,
    pub exact_params: &'static [&'static str],
    pub prefix_params: &'static [&'static str],
    pub regex_params: &'static [&'static str],
    pub exception_regexes: &'static [&'static str],
}

/// Runtime tracker rule (static embed or disk override).
#[derive(Debug)]
pub struct TrackerRule {
    pub url_pattern: String,
    pub exceptions: Vec<String>,
    pub rules: Vec<String>,
    host_suffixes: Vec<String>,
    host_label: Option<String>,
    match_any_host: bool,
    path_prefix: Option<String>,
    exact_params: HashSet<String>,
    prefix_params: Vec<String>,
    regex_params: Vec<String>,
    url_regex: Option<String>,
    /// Residual regexes compiled on first `clean_url` use (not at bundle load).
    compiled: OnceLock<CompiledMatchers>,
}

impl Clone for TrackerRule {
    fn clone(&self) -> Self {
        Self {
            url_pattern: self.url_pattern.clone(),
            exceptions: self.exceptions.clone(),
            rules: self.rules.clone(),
            host_suffixes: self.host_suffixes.clone(),
            host_label: self.host_label.clone(),
            match_any_host: self.match_any_host,
            path_prefix: self.path_prefix.clone(),
            exact_params: self.exact_params.clone(),
            prefix_params: self.prefix_params.clone(),
            regex_params: self.regex_params.clone(),
            url_regex: self.url_regex.clone(),
            compiled: OnceLock::new(),
        }
    }
}

#[derive(Debug)]
struct CompiledMatchers {
    url_re: Option<regex::Regex>,
    exception_res: Vec<regex::Regex>,
    rule_res: Vec<regex::Regex>,
}

// OnceLock is not Clone; TrackerRule clones reset the cache (recompile on next use).
impl Clone for CompiledMatchers {
    fn clone(&self) -> Self {
        Self {
            url_re: self.url_re.clone(),
            exception_res: self.exception_res.clone(),
            rule_res: self.rule_res.clone(),
        }
    }
}

/// Bundled ClearURLs tracker-parameter rules.
#[derive(Debug, Default)]
pub struct TrackerPatterns {
    /// Display / disk-override rules. Empty when using `static_rules` until first Lua export.
    pub rules: Vec<TrackerRule>,
    static_rules: Option<&'static [CompiledTrackerRule]>,
    /// Residual regexes for the static table, compiled on first `clean_url`.
    static_regexes: OnceLock<StaticRegexTables>,
}

#[derive(Debug)]
struct StaticRegexTables {
    /// Per-rule: (url_re, exception_res, param_res)
    per_rule: Vec<(Option<regex::Regex>, Vec<regex::Regex>, Vec<regex::Regex>)>,
}

impl Clone for TrackerPatterns {
    fn clone(&self) -> Self {
        Self {
            rules: self.rules.clone(),
            static_rules: self.static_rules,
            static_regexes: OnceLock::new(),
        }
    }
}

impl TrackerPatterns {
    fn from_static(rules: &'static [CompiledTrackerRule]) -> Self {
        // Near-zero startup: keep only the static slice; materialize owned `rules` for Lua later.
        Self {
            rules: Vec::new(),
            static_rules: Some(rules),
            static_regexes: OnceLock::new(),
        }
    }

    /// Number of provider rules (static or owned).
    pub fn rule_count(&self) -> usize {
        if let Some(rules) = self.static_rules {
            return rules.len();
        }
        self.rules.len()
    }

    /// Ensure `self.rules` is populated for introspection (Lua). Cheap relative to regex compile.
    pub fn materialize_rules_for_display(&mut self) {
        if !self.rules.is_empty() {
            return;
        }
        let Some(static_rules) = self.static_rules else {
            return;
        };
        self.rules = static_rules.iter().map(tracker_rule_from_static).collect();
    }

    /// Strip tracker query args using ClearURLs rules (SearXNG `TRACKER_PATTERNS.clean_url`).
    pub fn clean_url(&self, raw: &str) -> String {
        let Ok(mut parsed) = url::Url::parse(raw) else {
            return raw.to_string();
        };
        if parsed.query().is_none() {
            return raw.to_string();
        }
        if let Some(static_rules) = self.static_rules {
            return clean_url_static(self, static_rules, raw, parsed);
        }
        let mut current = raw.to_string();
        for rule in &self.rules {
            if parsed.query().is_none() {
                break;
            }
            if !owned_rule_matches(rule, &parsed, &current) {
                continue;
            }
            if owned_rule_excepted(rule, &current) {
                continue;
            }
            let kept: Vec<(String, String)> = parsed
                .query_pairs()
                .filter(|(name, _)| !owned_param_matches(rule, name))
                .map(|(name, value)| (name.into_owned(), value.into_owned()))
                .collect();
            if kept.is_empty() {
                parsed.set_query(None);
            } else {
                parsed.query_pairs_mut().clear().extend_pairs(&kept);
            }
            current = parsed.to_string();
        }
        current
    }
}

fn tracker_rule_from_static(r: &CompiledTrackerRule) -> TrackerRule {
    let mut all: Vec<String> = r
        .exact_params
        .iter()
        .chain(r.prefix_params.iter())
        .chain(r.regex_params.iter())
        .map(|s| (*s).to_string())
        .collect();
    all.sort();
    all.dedup();
    TrackerRule {
        url_pattern: r.url_pattern.to_string(),
        exceptions: r
            .exception_regexes
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
        rules: all,
        host_suffixes: match &r.host {
            HostMatch::Suffix(s) => vec![(*s).to_string()],
            HostMatch::AnySuffix(parts) => parts.iter().map(|s| (*s).to_string()).collect(),
            _ => Vec::new(),
        },
        host_label: match &r.host {
            HostMatch::Label(s) => Some((*s).to_string()),
            _ => None,
        },
        match_any_host: matches!(r.host, HostMatch::Any),
        path_prefix: r.path_prefix.map(str::to_string),
        exact_params: r.exact_params.iter().map(|s| (*s).to_string()).collect(),
        prefix_params: r.prefix_params.iter().map(|s| (*s).to_string()).collect(),
        regex_params: r.regex_params.iter().map(|s| (*s).to_string()).collect(),
        url_regex: match &r.host {
            HostMatch::Regex(s) => Some((*s).to_string()),
            _ => None,
        },
        compiled: OnceLock::new(),
    }
}

fn clean_url_static(
    patterns: &TrackerPatterns,
    rules: &'static [CompiledTrackerRule],
    raw: &str,
    mut parsed: url::Url,
) -> String {
    let mut current = raw.to_string();
    let regexes = patterns.static_regexes.get_or_init(|| StaticRegexTables {
        per_rule: rules
            .iter()
            .map(|r| {
                let url_re = match &r.host {
                    HostMatch::Regex(pat) => regex::Regex::new(pat).ok(),
                    _ => None,
                };
                let exception_res = r
                    .exception_regexes
                    .iter()
                    .filter_map(|pat| regex::Regex::new(pat).ok())
                    .collect();
                let param_res = r
                    .regex_params
                    .iter()
                    .filter_map(|pat| regex::Regex::new(pat).ok())
                    .collect();
                (url_re, exception_res, param_res)
            })
            .collect(),
    });

    for (idx, rule) in rules.iter().enumerate() {
        if parsed.query().is_none() {
            break;
        }
        if !static_rule_matches(rule, &regexes.per_rule[idx].0, &parsed, &current) {
            continue;
        }
        if regexes.per_rule[idx]
            .1
            .iter()
            .any(|re| re.is_match(&current))
        {
            continue;
        }
        let kept: Vec<(String, String)> = parsed
            .query_pairs()
            .filter(|(name, _)| !static_param_matches(rule, &regexes.per_rule[idx].2, name))
            .map(|(name, value)| (name.into_owned(), value.into_owned()))
            .collect();
        if kept.is_empty() {
            parsed.set_query(None);
        } else {
            parsed.query_pairs_mut().clear().extend_pairs(&kept);
        }
        current = parsed.to_string();
    }
    current
}

fn static_rule_matches(
    rule: &CompiledTrackerRule,
    url_re: &Option<regex::Regex>,
    parsed: &url::Url,
    current: &str,
) -> bool {
    if let Some(prefix) = rule.path_prefix {
        if !parsed.path().starts_with(prefix) {
            return false;
        }
    }
    match &rule.host {
        HostMatch::Any => true,
        HostMatch::Suffix(suffix) => parsed
            .host_str()
            .is_some_and(|h| host_matches_suffix(&h.to_ascii_lowercase(), suffix)),
        HostMatch::Label(label) => parsed
            .host_str()
            .is_some_and(|h| host_has_label(&h.to_ascii_lowercase(), label)),
        HostMatch::AnySuffix(parts) => parsed.host_str().is_some_and(|h| {
            let host = h.to_ascii_lowercase();
            parts
                .iter()
                .any(|suffix| host_matches_suffix(&host, suffix))
        }),
        HostMatch::Regex(_) => url_re.as_ref().is_some_and(|re| re.is_match(current)),
    }
}

fn static_param_matches(
    rule: &CompiledTrackerRule,
    regex_params: &[regex::Regex],
    name: &str,
) -> bool {
    if rule.exact_params.contains(&name) {
        return true;
    }
    if rule
        .prefix_params
        .iter()
        .any(|prefix| name == *prefix || name.starts_with(&format!("{prefix}_")))
    {
        return true;
    }
    regex_params.iter().any(|re| re.is_match(name))
}

fn owned_rule_matches(rule: &TrackerRule, parsed: &url::Url, current: &str) -> bool {
    if let Some(prefix) = &rule.path_prefix {
        if !parsed.path().starts_with(prefix.as_str()) {
            return false;
        }
    }
    if rule.match_any_host {
        return true;
    }
    if let Some(host) = parsed.host_str() {
        let host = host.to_ascii_lowercase();
        if let Some(label) = &rule.host_label {
            if host_has_label(&host, label) {
                return true;
            }
        }
        for suffix in &rule.host_suffixes {
            if host_matches_suffix(&host, suffix) {
                return true;
            }
        }
    }
    if let Some(pat) = &rule.url_regex {
        let _ = pat;
        return rule
            .compiled
            .get_or_init(|| compile_matchers(rule))
            .url_re
            .as_ref()
            .is_some_and(|re| re.is_match(current));
    }
    false
}

fn owned_rule_excepted(rule: &TrackerRule, current: &str) -> bool {
    if rule.exceptions.is_empty() {
        return false;
    }
    rule.compiled
        .get_or_init(|| compile_matchers(rule))
        .exception_res
        .iter()
        .any(|re| re.is_match(current))
}

fn owned_param_matches(rule: &TrackerRule, name: &str) -> bool {
    if rule.exact_params.contains(name) {
        return true;
    }
    if rule
        .prefix_params
        .iter()
        .any(|prefix| name == prefix.as_str() || name.starts_with(&format!("{prefix}_")))
    {
        return true;
    }
    if rule.regex_params.is_empty() {
        return false;
    }
    rule.compiled
        .get_or_init(|| compile_matchers(rule))
        .rule_res
        .iter()
        .any(|re| re.is_match(name))
}

fn host_matches_suffix(host: &str, suffix: &str) -> bool {
    host == suffix || host.ends_with(&format!(".{suffix}"))
}

fn host_has_label(host: &str, label: &str) -> bool {
    host.split('.').any(|part| part == label)
}

fn compile_matchers(rule: &TrackerRule) -> CompiledMatchers {
    CompiledMatchers {
        url_re: rule
            .url_regex
            .as_deref()
            .and_then(|pat| regex::Regex::new(pat).ok()),
        exception_res: rule
            .exceptions
            .iter()
            .filter_map(|pat| regex::Regex::new(pat).ok())
            .collect(),
        rule_res: rule
            .regex_params
            .iter()
            .filter_map(|pat| regex::Regex::new(pat).ok())
            .collect(),
    }
}

#[derive(Debug, Default, Clone)]
pub struct PluginData {
    pub doi_resolver: Option<String>,
    pub hostnames: HostnamesRules,
    pub using_tor_proxy: bool,
}

#[derive(Debug, Default, Clone)]
pub struct HostnamesRules {
    pub replace: Vec<(String, String)>,
    pub remove: Vec<String>,
    pub high_priority: Vec<String>,
    pub low_priority: Vec<String>,
}

/// Placeholder for user query in bang URL template (U+0002).
pub const BANG_QUERY_PLACEHOLDER: char = '\u{2}';
const BANG_RANK_SEP: char = '\u{1}';
const BANG_LEAF_KEY: &str = "\u{10}";

/// Resolved external bang definition.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BangEntry {
    pub url_template: String,
    pub rank: i32,
}

#[derive(Debug, Default, Clone)]
struct BangNode {
    children: HashMap<char, BangNode>,
    entry: Option<BangEntry>,
}

/// Prefix trie mapping bang tokens to entries.
#[derive(Debug, Default, Clone)]
pub struct BangTrie {
    root: BangNode,
    len: usize,
    static_map: Option<&'static phf::Map<&'static str, (&'static str, i32)>>,
    static_tokens: Option<&'static [&'static str]>,
}

impl BangTrie {
    pub fn new() -> Self {
        Self::default()
    }

    fn from_static(
        map: &'static phf::Map<&'static str, (&'static str, i32)>,
        tokens: &'static [&'static str],
    ) -> Self {
        Self {
            root: BangNode::default(),
            len: map.len(),
            static_map: Some(map),
            static_tokens: Some(tokens),
        }
    }

    pub fn insert(&mut self, token: &str, entry: BangEntry) {
        let mut node = &mut self.root;
        for ch in token.chars() {
            node = node.children.entry(ch).or_default();
        }
        if node.entry.is_none() {
            self.len += 1;
        }
        node.entry = Some(entry);
    }

    pub fn resolve(&self, token: &str) -> Option<BangEntry> {
        if let Some(static_map) = self.static_map {
            let (url_template, rank) = static_map.get(token)?;
            return Some(BangEntry {
                url_template: (*url_template).to_string(),
                rank: *rank,
            });
        }
        let mut node = &self.root;
        for ch in token.chars() {
            node = node.children.get(&ch)?;
        }
        node.entry.clone()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Prefix / substring matches for bang discovery. Empty query → empty (type to search).
    pub fn suggest(&self, query: &str, limit: usize) -> Vec<(String, BangEntry)> {
        if limit == 0 {
            return Vec::new();
        }
        let q = query.trim().to_ascii_lowercase();
        // ponytail: refuse empty scan of the full bang map (~10k); UI requires a filter.
        if q.is_empty() {
            return Vec::new();
        }
        let mut matches: Vec<(String, BangEntry)> =
            if let (Some(static_map), Some(tokens)) = (self.static_map, self.static_tokens) {
                // Prefix hits via binary search over sorted tokens; substring via linear scan.
                let mut out = Vec::new();
                let start = tokens.partition_point(|t| *t < q.as_str());
                for token in &tokens[start..] {
                    if !token.starts_with(&q) {
                        break;
                    }
                    if let Some((url_template, rank)) = static_map.get(token) {
                        out.push((
                            (*token).to_string(),
                            BangEntry {
                                url_template: (*url_template).to_string(),
                                rank: *rank,
                            },
                        ));
                    }
                }
                // Substring matches that are not already prefixes.
                for token in tokens.iter() {
                    if token.starts_with(&q) {
                        continue;
                    }
                    if token.contains(&q) {
                        if let Some((url_template, rank)) = static_map.get(token) {
                            out.push((
                                (*token).to_string(),
                                BangEntry {
                                    url_template: (*url_template).to_string(),
                                    rank: *rank,
                                },
                            ));
                        }
                    }
                }
                out
            } else {
                let mut out = Vec::new();
                collect_bang_matches(&self.root, &mut String::new(), &q, &mut out);
                out
            };
        matches.sort_by(|a, b| {
            let score = |token: &str| -> u8 {
                if token == q {
                    2
                } else if token.starts_with(&q) {
                    1
                } else {
                    0
                }
            };
            score(&b.0)
                .cmp(&score(&a.0))
                .then_with(|| b.1.rank.cmp(&a.1.rank))
                .then_with(|| a.0.cmp(&b.0))
        });
        matches.truncate(limit);
        matches
    }
}

fn collect_bang_matches(
    node: &BangNode,
    prefix: &mut String,
    query: &str,
    out: &mut Vec<(String, BangEntry)>,
) {
    if let Some(entry) = &node.entry {
        let token = prefix.clone();
        if query.is_empty() || token.starts_with(query) || token.contains(query) {
            out.push((token, entry.clone()));
        }
    }
    for (ch, child) in &node.children {
        prefix.push(*ch);
        collect_bang_matches(child, prefix, query, out);
        prefix.pop();
    }
}

#[derive(Debug, Deserialize)]
struct ExternalBangsRaw {
    trie: serde_json::Value,
}

fn parse_bang_definition(def: &str) -> BangEntry {
    let mut parts = def.splitn(2, BANG_RANK_SEP);
    let url = parts.next().unwrap_or("");
    let rank_str = parts.next().unwrap_or("");
    let rank = if rank_str.is_empty() {
        0
    } else {
        rank_str.parse::<i32>().unwrap_or(0)
    };
    BangEntry {
        url_template: url.to_string(),
        rank,
    }
}

fn flatten_bang_trie(node: &serde_json::Value, prefix: &str, trie: &mut BangTrie) {
    match node {
        serde_json::Value::String(def) => {
            trie.insert(prefix, parse_bang_definition(def));
        }
        serde_json::Value::Object(map) => {
            for (key, value) in map {
                if key == BANG_LEAF_KEY {
                    if let serde_json::Value::String(def) = value {
                        trie.insert(prefix, parse_bang_definition(def));
                    }
                } else {
                    let mut next = String::with_capacity(prefix.len() + key.len());
                    next.push_str(prefix);
                    next.push_str(key);
                    flatten_bang_trie(value, &next, trie);
                }
            }
        }
        _ => {}
    }
}

#[derive(Debug, Deserialize)]
#[serde(untagged)]
enum StringOrVec {
    One(String),
    Many(Vec<String>),
}

impl StringOrVec {
    fn into_vec(self) -> Vec<String> {
        match self {
            StringOrVec::One(s) => vec![s],
            StringOrVec::Many(v) => v,
        }
    }
}

#[derive(Debug, Deserialize)]
struct CurrenciesRaw {
    iso4217: HashMap<String, HashMap<String, String>>,
    names: HashMap<String, StringOrVec>,
}

/// Currency name/symbol to ISO-4217 lookup.
#[derive(Debug, Default, Clone)]
pub struct CurrencyTable {
    /// Owned tables (disk override / tests). Empty when using precompiled PHF.
    pub names: HashMap<String, Vec<String>>,
    pub iso4217: HashMap<String, HashMap<String, String>>,
    static_names: Option<&'static phf::Map<&'static str, &'static [&'static str]>>,
    static_iso: Option<&'static phf::Map<&'static str, &'static [(&'static str, &'static str)]>>,
}

impl CurrencyTable {
    fn from_static(
        names: &'static phf::Map<&'static str, &'static [&'static str]>,
        iso: &'static phf::Map<&'static str, &'static [(&'static str, &'static str)]>,
    ) -> Self {
        Self {
            names: HashMap::new(),
            iso4217: HashMap::new(),
            static_names: Some(names),
            static_iso: Some(iso),
        }
    }

    pub fn name_to_iso4217(&self, name: &str) -> Option<&str> {
        if let Some(map) = self.static_names {
            return map.get(name).and_then(|codes| codes.last().copied());
        }
        self.names
            .get(name)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    pub fn iso4217_to_name(&self, iso4217: &str, language: &str) -> Option<&str> {
        if let Some(map) = self.static_iso {
            return map.get(iso4217).and_then(|langs| {
                langs
                    .iter()
                    .find(|(lang, _)| *lang == language)
                    .map(|(_, name)| *name)
            });
        }
        self.iso4217
            .get(iso4217)
            .and_then(|langs| langs.get(language))
            .map(String::as_str)
    }

    pub fn is_iso4217(&self, iso4217: &str) -> bool {
        if let Some(map) = self.static_iso {
            return map.contains_key(iso4217);
        }
        self.iso4217.contains_key(iso4217)
    }

    pub fn iso_len(&self) -> usize {
        if let Some(map) = self.static_iso {
            return map.len();
        }
        self.iso4217.len()
    }

    pub fn iter_iso(&self) -> CurrencyIsoIter<'_> {
        CurrencyIsoIter {
            static_iso: self.static_iso.map(|m| m.entries()),
            owned: self.iso4217.iter(),
        }
    }

    pub fn iter_names(&self) -> CurrencyNamesIter<'_> {
        CurrencyNamesIter {
            static_names: self.static_names.map(|m| m.entries()),
            owned: self.names.iter(),
        }
    }
}

pub struct CurrencyIsoIter<'a> {
    static_iso:
        Option<phf::map::Entries<'a, &'static str, &'static [(&'static str, &'static str)]>>,
    owned: std::collections::hash_map::Iter<'a, String, HashMap<String, String>>,
}

impl<'a> Iterator for CurrencyIsoIter<'a> {
    type Item = (&'a str, CurrencyLangIter<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(entries) = self.static_iso.as_mut() {
            let (code, langs) = entries.next()?;
            return Some((
                *code,
                CurrencyLangIter {
                    static_langs: Some(langs.iter()),
                    owned: None,
                },
            ));
        }
        let (code, langs) = self.owned.next()?;
        Some((
            code.as_str(),
            CurrencyLangIter {
                static_langs: None,
                owned: Some(langs.iter()),
            },
        ))
    }
}

pub struct CurrencyLangIter<'a> {
    static_langs: Option<std::slice::Iter<'a, (&'static str, &'static str)>>,
    owned: Option<std::collections::hash_map::Iter<'a, String, String>>,
}

impl<'a> Iterator for CurrencyLangIter<'a> {
    type Item = (&'a str, &'a str);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(iter) = self.static_langs.as_mut() {
            return iter.next().map(|(a, b)| (*a, *b));
        }
        self.owned
            .as_mut()?
            .next()
            .map(|(a, b)| (a.as_str(), b.as_str()))
    }
}

pub struct CurrencyNamesIter<'a> {
    static_names: Option<phf::map::Entries<'a, &'static str, &'static [&'static str]>>,
    owned: std::collections::hash_map::Iter<'a, String, Vec<String>>,
}

impl<'a> Iterator for CurrencyNamesIter<'a> {
    type Item = (&'a str, CurrencyCodesIter<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(entries) = self.static_names.as_mut() {
            let (name, codes) = entries.next()?;
            return Some((
                *name,
                CurrencyCodesIter {
                    static_codes: Some(codes.iter()),
                    owned: None,
                },
            ));
        }
        let (name, codes) = self.owned.next()?;
        Some((
            name.as_str(),
            CurrencyCodesIter {
                static_codes: None,
                owned: Some(codes.iter()),
            },
        ))
    }
}

pub struct CurrencyCodesIter<'a> {
    static_codes: Option<std::slice::Iter<'a, &'static str>>,
    owned: Option<std::slice::Iter<'a, String>>,
}

impl<'a> Iterator for CurrencyCodesIter<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(iter) = self.static_codes.as_mut() {
            return iter.next().copied();
        }
        self.owned.as_mut()?.next().map(String::as_str)
    }
}

impl From<CurrenciesRaw> for CurrencyTable {
    fn from(raw: CurrenciesRaw) -> Self {
        CurrencyTable {
            names: raw
                .names
                .into_iter()
                .map(|(k, v)| (k, v.into_vec()))
                .collect(),
            iso4217: raw.iso4217,
            static_names: None,
            static_iso: None,
        }
    }
}

/// Wikidata unit definition with SI conversion.
#[derive(Debug, Clone, Deserialize)]
pub struct UnitEntry {
    pub si_name: Option<String>,
    pub symbol: String,
    pub to_si_factor: Option<f64>,
}

/// Borrowed unit view (precompiled or owned path).
#[derive(Debug, Clone, Copy)]
pub struct UnitRef<'a> {
    pub si_name: Option<&'a str>,
    pub symbol: &'a str,
    pub to_si_factor: Option<f64>,
}

type StaticUnitFields = (Option<&'static str>, &'static str, Option<f64>);
type StaticUnitMap = phf::Map<&'static str, StaticUnitFields>;

/// Wikidata units keyed by Q-identifier.
#[derive(Debug, Default, Clone)]
pub struct UnitTable {
    pub units: HashMap<String, UnitEntry>,
    static_units: Option<&'static StaticUnitMap>,
}

impl UnitTable {
    fn from_static(units: &'static StaticUnitMap) -> Self {
        Self {
            units: HashMap::new(),
            static_units: Some(units),
        }
    }

    pub fn get(&self, id: &str) -> Option<UnitRef<'_>> {
        if let Some(map) = self.static_units {
            let (si_name, symbol, to_si_factor) = map.get(id)?;
            return Some(UnitRef {
                si_name: *si_name,
                symbol,
                to_si_factor: *to_si_factor,
            });
        }
        self.units.get(id).map(|entry| UnitRef {
            si_name: entry.si_name.as_deref(),
            // Lifetime tied to self via transmute-ish: store owned symbol as str through entry
            symbol: entry.symbol.as_str(),
            to_si_factor: entry.to_si_factor,
        })
    }

    pub fn len(&self) -> usize {
        if let Some(map) = self.static_units {
            return map.len();
        }
        self.units.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    pub fn iter(&self) -> UnitIter<'_> {
        UnitIter {
            static_units: self.static_units.map(|m| m.entries()),
            owned: self.units.iter(),
        }
    }
}

pub struct UnitIter<'a> {
    static_units: Option<phf::map::Entries<'a, &'static str, StaticUnitFields>>,
    owned: std::collections::hash_map::Iter<'a, String, UnitEntry>,
}

impl<'a> Iterator for UnitIter<'a> {
    type Item = (&'a str, UnitRef<'a>);

    fn next(&mut self) -> Option<Self::Item> {
        if let Some(entries) = self.static_units.as_mut() {
            let (id, (si_name, symbol, to_si_factor)) = entries.next()?;
            return Some((
                *id,
                UnitRef {
                    si_name: *si_name,
                    symbol,
                    to_si_factor: *to_si_factor,
                },
            ));
        }
        let (id, entry) = self.owned.next()?;
        Some((
            id.as_str(),
            UnitRef {
                si_name: entry.si_name.as_deref(),
                symbol: entry.symbol.as_str(),
                to_si_factor: entry.to_si_factor,
            },
        ))
    }
}

/// Per-engine language and region traits.
#[derive(Debug, Clone, Deserialize)]
pub struct EngineTraits {
    #[serde(default)]
    pub all_locale: Option<String>,
    #[serde(default)]
    pub data_type: Option<String>,
    #[serde(default)]
    pub languages: HashMap<String, String>,
    #[serde(default)]
    pub regions: HashMap<String, String>,
    #[serde(default)]
    pub custom: serde_json::Value,
}

/// Engine traits keyed by engine name.
#[derive(Debug, Default)]
pub struct EngineTraitsMap {
    pub engines: HashMap<String, EngineTraits>,
    static_traits: Option<&'static phf::Map<&'static str, StaticEngineTraits>>,
    /// Materialized on first lookup (deferred past bundle load / process startup).
    cache: OnceLock<HashMap<&'static str, EngineTraits>>,
}

impl Clone for EngineTraitsMap {
    fn clone(&self) -> Self {
        Self {
            engines: self.engines.clone(),
            static_traits: self.static_traits,
            // ponytail: drop cache on clone; next get() rebuilds
            cache: OnceLock::new(),
        }
    }
}

type StaticEngineTraits = (
    Option<&'static str>,
    Option<&'static str>,
    &'static [(&'static str, &'static str)],
    &'static [(&'static str, &'static str)],
    &'static str,
);

impl EngineTraitsMap {
    /// Owned engine-trait table (disk load / tests).
    pub fn from_engines(engines: HashMap<String, EngineTraits>) -> Self {
        Self {
            engines,
            static_traits: None,
            cache: OnceLock::new(),
        }
    }

    fn from_static(map: &'static phf::Map<&'static str, StaticEngineTraits>) -> Self {
        Self {
            engines: HashMap::new(),
            static_traits: Some(map),
            cache: OnceLock::new(),
        }
    }

    pub fn get(&self, engine: &str) -> Option<&EngineTraits> {
        if let Some(owned) = self.engines.get(engine) {
            return Some(owned);
        }
        let static_map = self.static_traits?;
        let cache = self.cache.get_or_init(|| {
            static_map
                .entries()
                .map(|(key, value)| (*key, materialize_engine_traits(value)))
                .collect()
        });
        cache.get(engine)
    }
}

fn materialize_engine_traits(value: &StaticEngineTraits) -> EngineTraits {
    let (all_locale, data_type, languages, regions, custom_json) = *value;
    EngineTraits {
        all_locale: all_locale.map(str::to_string),
        data_type: data_type.map(str::to_string),
        languages: languages
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        regions: regions
            .iter()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
        custom: serde_json::from_str(custom_json).unwrap_or(serde_json::Value::Null),
    }
}

/// User-agent pool for generating request user-agent strings with OS and version substitution.
#[derive(Debug, Default, Clone)]
pub struct UserAgentPool {
    pub os: Vec<String>,
    pub ua_template: String,
    pub versions: Vec<String>,
    pub gsa: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct UserAgentsRaw {
    os: Vec<String>,
    ua: String,
    versions: Vec<String>,
}

pub const GSA_USERAGENT_SUFFIX: &str = " NSTNWV";

fn format_useragent(template: &str, os: &str, version: &str) -> String {
    template.replace("{os}", os).replace("{version}", version)
}

impl UserAgentPool {
    fn from_static(
        os: &'static [&'static str],
        template: &'static str,
        versions: &'static [&'static str],
        gsa: &'static [&'static str],
    ) -> Self {
        // Tiny pools — copy into Vec so existing APIs stay unchanged.
        Self {
            os: os.iter().map(|s| (*s).to_string()).collect(),
            ua_template: template.to_string(),
            versions: versions.iter().map(|s| (*s).to_string()).collect(),
            gsa: gsa.iter().map(|s| (*s).to_string()).collect(),
        }
    }

    pub fn os_count(&self) -> usize {
        self.os.len()
    }

    pub fn version_count(&self) -> usize {
        self.versions.len()
    }

    pub fn gsa_count(&self) -> usize {
        self.gsa.len()
    }

    pub fn generate_at(&self, os_index: usize, version_index: usize) -> Option<String> {
        let os = self.os.get(os_index)?;
        let version = self.versions.get(version_index)?;
        Some(format_useragent(&self.ua_template, os, version))
    }

    pub fn generate(&self) -> Option<String> {
        if self.os.is_empty() || self.versions.is_empty() {
            return None;
        }
        let mut rng = rand::rng();
        let os_index = rng.random_range(0..self.os.len());
        let version_index = rng.random_range(0..self.versions.len());
        self.generate_at(os_index, version_index)
    }

    pub fn generate_gsa_at(&self, index: usize) -> Option<String> {
        let base = self.gsa.get(index)?;
        Some(format!("{base}{GSA_USERAGENT_SUFFIX}"))
    }

    pub fn generate_gsa(&self) -> Option<String> {
        if self.gsa.is_empty() {
            return None;
        }
        let index = rand::rng().random_range(0..self.gsa.len());
        self.generate_gsa_at(index)
    }

    pub fn is_member(&self, ua: &str) -> bool {
        self.os.iter().any(|os| {
            self.versions
                .iter()
                .any(|version| format_useragent(&self.ua_template, os, version) == ua)
        })
    }
}

/// Language and region parsed from a locale code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocaleInfo {
    pub language: String,
    pub region: Option<String>,
    pub display_name: String,
}

/// Locale mapping to display names and RTL status.
#[derive(Debug, Default, Clone)]
pub struct LocaleMap {
    pub locale_names: HashMap<String, String>,
    pub rtl_locales: Vec<String>,
    static_names: Option<&'static phf::Map<&'static str, &'static str>>,
    static_rtl: Option<&'static [&'static str]>,
}

#[derive(Debug, Deserialize)]
struct LocalesRaw {
    #[serde(rename = "LOCALE_NAMES")]
    locale_names: HashMap<String, String>,
    #[serde(rename = "RTL_LOCALES")]
    rtl_locales: Vec<String>,
}

fn parse_locale(code: &str, display_name: String) -> LocaleInfo {
    let mut parts = code.split('-');
    let language = parts.next().unwrap_or("").to_string();
    let mut region = None;
    for part in parts {
        if part.len() == 2 && part.chars().all(|c| c.is_ascii_uppercase()) {
            region = Some(part.to_string());
        }
    }
    LocaleInfo {
        language,
        region,
        display_name,
    }
}

impl LocaleMap {
    /// Owned locale tables (disk load / tests).
    pub fn from_owned(locale_names: HashMap<String, String>, rtl_locales: Vec<String>) -> Self {
        Self {
            locale_names,
            rtl_locales,
            static_names: None,
            static_rtl: None,
        }
    }

    fn from_static(
        names: &'static phf::Map<&'static str, &'static str>,
        rtl: &'static [&'static str],
    ) -> Self {
        Self {
            // Materialize small locale table so existing `.locale_names` consumers keep working.
            locale_names: names
                .entries()
                .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
                .collect(),
            rtl_locales: rtl.iter().map(|s| (*s).to_string()).collect(),
            static_names: Some(names),
            static_rtl: Some(rtl),
        }
    }

    pub fn resolve(&self, locale: &str) -> Option<LocaleInfo> {
        if let Some(map) = self.static_names {
            if let Some(name) = map.get(locale) {
                return Some(parse_locale(locale, (*name).to_string()));
            }
        }
        self.locale_names
            .get(locale)
            .map(|name| parse_locale(locale, name.clone()))
    }

    pub fn contains(&self, locale: &str) -> bool {
        if let Some(map) = self.static_names {
            if map.contains_key(locale) {
                return true;
            }
        }
        self.locale_names.contains_key(locale)
    }

    pub fn normalize_supported(&self, locale: &str) -> Option<String> {
        let normalized = normalize_locale_code(locale);
        if normalized == "all" || normalized == "auto" {
            return Some(normalized);
        }
        if self.contains(&normalized) {
            return Some(normalized);
        }
        let language = normalized.split('-').next().unwrap_or("");
        if self.contains(language) {
            return Some(language.to_string());
        }
        None
    }

    pub fn is_rtl(&self, locale: &str) -> bool {
        if let Some(rtl) = self.static_rtl {
            if rtl.contains(&locale) {
                return true;
            }
        }
        self.rtl_locales.iter().any(|l| l == locale)
    }
}

fn normalize_locale_code(locale: &str) -> String {
    let value = locale.trim().replace('_', "-");
    let mut parts = value.split('-');
    let language = parts.next().unwrap_or("").to_ascii_lowercase();
    let rest: Vec<String> = parts
        .map(|part| {
            if part.len() == 2 {
                part.to_ascii_uppercase()
            } else {
                part.to_ascii_lowercase()
            }
        })
        .collect();
    if rest.is_empty() {
        language
    } else {
        format!("{}-{}", language, rest.join("-"))
    }
}

/// Ahmia onion blacklist (MD5 hex hashes). Embedded path uses packed binary search.
#[derive(Debug, Default, Clone)]
pub struct AhmiaBlacklist {
    owned: HashSet<String>,
    static_hashes: Option<&'static [u8]>,
    static_count: usize,
}

impl AhmiaBlacklist {
    fn from_static(bytes: &'static [u8]) -> Self {
        assert_eq!(bytes.len() % 32, 0);
        Self {
            owned: HashSet::new(),
            static_hashes: Some(bytes),
            static_count: bytes.len() / 32,
        }
    }

    pub fn insert(&mut self, hash: String) -> bool {
        self.owned.insert(hash)
    }

    pub fn contains(&self, hash: &str) -> bool {
        if self.owned.contains(hash) {
            return true;
        }
        let Some(bytes) = self.static_hashes else {
            return false;
        };
        if hash.len() != 32 {
            return false;
        }
        let target = hash.as_bytes();
        let mut lo = 0usize;
        let mut hi = self.static_count;
        while lo < hi {
            let mid = (lo + hi) / 2;
            let start = mid * 32;
            let entry = &bytes[start..start + 32];
            match entry.cmp(target) {
                std::cmp::Ordering::Less => lo = mid + 1,
                std::cmp::Ordering::Greater => hi = mid,
                std::cmp::Ordering::Equal => return true,
            }
        }
        false
    }

    pub fn len(&self) -> usize {
        self.static_count + self.owned.len()
    }

    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// Detected language code (opaque wrapper; detector swappable).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct LangCode(String);

impl LangCode {
    pub fn new(code: impl Into<String>) -> Self {
        LangCode(code.into())
    }

    pub fn as_str(&self) -> &str {
        &self.0
    }

    pub fn into_string(self) -> String {
        self.0
    }
}

impl std::fmt::Display for LangCode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

pub fn detect_language(text: &str) -> Option<LangCode> {
    whatlang::detect_lang(text).map(|lang| LangCode::new(lang.code()))
}

#[allow(clippy::approx_constant, clippy::type_complexity)]
mod generated_data {
    use super::{CompiledTrackerRule, HostMatch};
    include!(concat!(env!("OUT_DIR"), "/generated_data.rs"));
}
pub use generated_data::*;

trait DataSource {
    fn read(&self, file: &str) -> Result<String, DataError>;

    fn read_optional(&self, file: &str) -> Result<Option<String>, DataError> {
        match self.read(file) {
            Ok(contents) => Ok(Some(contents)),
            Err(DataError::Read { source, .. })
                if source.kind() == std::io::ErrorKind::NotFound =>
            {
                Ok(None)
            }
            Err(error) => Err(error),
        }
    }
}

struct DirSource<'a> {
    dir: &'a Path,
}

impl DataSource for DirSource<'_> {
    fn read(&self, file: &str) -> Result<String, DataError> {
        read_required(&data_path(self.dir, file), file)
    }
}

fn read_required(path: &Path, file: &str) -> Result<String, DataError> {
    std::fs::read_to_string(path).map_err(|source| DataError::Read {
        file: file.to_string(),
        source,
    })
}

fn parse_json<T: for<'de> Deserialize<'de>>(contents: &str, file: &str) -> Result<T, DataError> {
    serde_json::from_str(contents).map_err(|source| DataError::Parse {
        file: file.to_string(),
        source,
    })
}

fn data_path(data_dir: &Path, file: &str) -> PathBuf {
    data_dir.join(file)
}

pub fn load_embedded_bundle() -> Result<DataBundle, DataError> {
    tracing::debug!("loading precompiled bundled data");
    Ok(load_precompiled_bundle())
}

pub fn load_bundle(data_dir: &Path) -> Result<DataBundle, DataError> {
    tracing::debug!(dir = %data_dir.display(), "loading bundled data from directory");
    load_from_source(&DirSource { dir: data_dir })
}

fn load_from_source(source: &dyn DataSource) -> Result<DataBundle, DataError> {
    let bangs = load_bangs(source)?;
    let currencies = load_currencies(source)?;
    let units = load_units(source)?;
    let engine_traits = load_engine_traits(source)?;
    let useragents = load_useragents(source)?;
    let locales = load_locales(source)?;
    let tracker_patterns = load_tracker_patterns(source)?;
    // ponytail: prefer json list; fall back to SearXNG's line-oriented txt
    let ahmia_blacklist = match load_optional_string_list(source, "ahmia_blacklist.json")? {
        list if !list.is_empty() => {
            let mut set = AhmiaBlacklist::default();
            for hash in list {
                set.insert(hash);
            }
            set
        }
        _ => {
            let mut set = AhmiaBlacklist::default();
            if let Some(contents) = source.read_optional("ahmia_blacklist.txt")? {
                for line in contents.lines() {
                    let hash = line.trim();
                    if !hash.is_empty() && !hash.starts_with('#') {
                        set.insert(hash.to_string());
                    }
                }
            }
            set
        }
    };
    let doi_resolvers = parse_json(&source.read("doi_resolvers.json")?, "doi_resolvers.json")?;
    let autocomplete = parse_json(
        &source.read("autocomplete_backends.json")?,
        "autocomplete_backends.json",
    )?;
    let limiter_toml = source.read("limiter.toml")?;
    let info_pages = parse_json(&source.read("info_pages.json")?, "info_pages.json")?;

    Ok(DataBundle {
        bangs,
        currencies,
        units,
        engine_traits,
        useragents,
        locales,
        tracker_patterns,
        ahmia_blacklist,
        doi_resolvers,
        autocomplete,
        limiter_toml,
        info_pages,
        plugin_data: PluginData::default(),
    })
}

fn load_precompiled_bundle() -> DataBundle {
    let doi_resolvers = DoiResolvers {
        default: PRECOMPILED_DOI_DEFAULT.to_string(),
        resolvers: PRECOMPILED_DOI_RESOLVERS
            .entries()
            .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
            .collect(),
    };
    let autocomplete = AutocompleteMetadata {
        backends: PRECOMPILED_AUTOCOMPLETE_BACKENDS
            .iter()
            .map(|s| (*s).to_string())
            .collect(),
    };
    let mut info_locales = BTreeMap::new();
    for (locale, pages) in PRECOMPILED_INFO_PAGES {
        let mut page_map = BTreeMap::new();
        for (page, title, content) in *pages {
            page_map.insert(
                (*page).to_string(),
                InfoPage {
                    title: (*title).to_string(),
                    content: (*content).to_string(),
                },
            );
        }
        info_locales.insert((*locale).to_string(), page_map);
    }
    let info_pages = InfoPages {
        default_locale: PRECOMPILED_INFO_DEFAULT_LOCALE.to_string(),
        locales: info_locales,
    };

    DataBundle {
        bangs: BangTrie::from_static(&PRECOMPILED_BANGS, PRECOMPILED_BANG_TOKENS),
        currencies: CurrencyTable::from_static(
            &PRECOMPILED_CURRENCY_NAMES,
            &PRECOMPILED_CURRENCY_ISO,
        ),
        units: UnitTable::from_static(&PRECOMPILED_UNITS),
        engine_traits: EngineTraitsMap::from_static(&PRECOMPILED_ENGINE_TRAITS),
        useragents: UserAgentPool::from_static(
            PRECOMPILED_USERAGENT_OS,
            PRECOMPILED_USERAGENT_TEMPLATE,
            PRECOMPILED_USERAGENT_VERSIONS,
            PRECOMPILED_GSA_USERAGENTS,
        ),
        locales: LocaleMap::from_static(&PRECOMPILED_LOCALE_NAMES, PRECOMPILED_RTL_LOCALES),
        tracker_patterns: TrackerPatterns::from_static(PRECOMPILED_TRACKER_RULES),
        ahmia_blacklist: AhmiaBlacklist::from_static(PRECOMPILED_AHMIA_HASHES),
        doi_resolvers,
        autocomplete,
        // Config: still a string; parsed by consumers at load.
        limiter_toml: include_str!("../data/limiter.toml").to_string(),
        info_pages,
        plugin_data: PluginData::default(),
    }
}

fn load_optional_string_list(
    source: &dyn DataSource,
    file: &str,
) -> Result<Vec<String>, DataError> {
    let Some(contents) = source.read_optional(file)? else {
        return Ok(Vec::new());
    };
    parse_json(&contents, file)
}

#[derive(Debug, Deserialize)]
struct TrackerRuleRaw {
    url: String,
    #[serde(default)]
    exceptions: Vec<String>,
    #[serde(default)]
    rules: Vec<String>,
}

fn load_tracker_patterns(source: &dyn DataSource) -> Result<TrackerPatterns, DataError> {
    const FILE: &str = "tracker_patterns.json";
    let Some(contents) = source.read_optional(FILE)? else {
        return Ok(TrackerPatterns::default());
    };
    let raw: Vec<TrackerRuleRaw> = parse_json(&contents, FILE)?;
    Ok(tracker_patterns_from_disk(raw))
}

fn tracker_patterns_from_disk(raw: Vec<TrackerRuleRaw>) -> TrackerPatterns {
    // Disk override: keep regex URL matching (same ClearURLs patterns) but classify
    // param rules the same way as build.rs where possible.
    let mut rules = Vec::new();
    for entry in raw {
        let mut exact = HashSet::new();
        let mut prefixes = Vec::new();
        let mut regex_params = Vec::new();
        for rule in &entry.rules {
            classify_param_runtime(rule, &mut exact, &mut prefixes, &mut regex_params);
        }
        if exact.is_empty() && prefixes.is_empty() && regex_params.is_empty() {
            continue;
        }
        rules.push(TrackerRule {
            url_pattern: entry.url.clone(),
            exceptions: entry.exceptions,
            rules: entry.rules,
            host_suffixes: Vec::new(),
            host_label: None,
            match_any_host: entry.url == ".*",
            path_prefix: None,
            exact_params: exact,
            prefix_params: prefixes,
            regex_params,
            url_regex: if entry.url == ".*" {
                None
            } else {
                Some(entry.url)
            },
            compiled: OnceLock::new(),
        });
    }
    TrackerPatterns {
        rules,
        static_rules: None,
        static_regexes: OnceLock::new(),
    }
}

fn classify_param_runtime(
    rule: &str,
    exact: &mut HashSet<String>,
    prefixes: &mut Vec<String>,
    regex_params: &mut Vec<String>,
) {
    let stripped = rule.strip_prefix("(?:%3F)?").unwrap_or(rule);
    let is_literal = |s: &str| {
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                let _ = chars.next();
                continue;
            }
            if matches!(
                c,
                '.' | '*' | '+' | '?' | '^' | '$' | '{' | '}' | '[' | ']' | '(' | ')' | '|'
            ) {
                return false;
            }
        }
        true
    };
    let unescape = |s: &str| {
        let mut out = String::new();
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '\\' {
                if let Some(n) = chars.next() {
                    out.push(n);
                }
            } else {
                out.push(c);
            }
        }
        out
    };
    if is_literal(stripped) {
        exact.insert(unescape(stripped));
        return;
    }
    for suffix in [
        "(?:_[a-z_]*)?",
        "(?:_[a-z]*)?",
        "_[a-z_]+",
        "_[a-z]+",
        "_[a-z_]*",
        "_[a-z]*",
        "(?:_[a-z]*)+",
    ] {
        if let Some(prefix) = stripped.strip_suffix(suffix) {
            if !prefix.is_empty() && is_literal(prefix) {
                prefixes.push(unescape(prefix));
                return;
            }
        }
    }
    regex_params.push(rule.to_string());
}

fn load_bangs(source: &dyn DataSource) -> Result<BangTrie, DataError> {
    const FILE: &str = "external_bangs.json";
    let contents = source.read(FILE)?;
    let raw: ExternalBangsRaw = parse_json(&contents, FILE)?;
    let mut trie = BangTrie::new();
    flatten_bang_trie(&raw.trie, "", &mut trie);
    Ok(trie)
}

fn load_currencies(source: &dyn DataSource) -> Result<CurrencyTable, DataError> {
    const FILE: &str = "currencies.json";
    let contents = source.read(FILE)?;
    let raw: CurrenciesRaw = parse_json(&contents, FILE)?;
    Ok(CurrencyTable::from(raw))
}

fn load_units(source: &dyn DataSource) -> Result<UnitTable, DataError> {
    const FILE: &str = "wikidata_units.json";
    let contents = source.read(FILE)?;
    let units: HashMap<String, UnitEntry> = parse_json(&contents, FILE)?;
    Ok(UnitTable {
        units,
        static_units: None,
    })
}

fn load_engine_traits(source: &dyn DataSource) -> Result<EngineTraitsMap, DataError> {
    const FILE: &str = "engine_traits.json";
    let contents = source.read(FILE)?;
    let engines: HashMap<String, EngineTraits> = parse_json(&contents, FILE)?;
    Ok(EngineTraitsMap::from_engines(engines))
}

fn load_useragents(source: &dyn DataSource) -> Result<UserAgentPool, DataError> {
    const UA_FILE: &str = "useragents.json";
    const GSA_FILE: &str = "gsa_useragents.txt";

    let ua_contents = source.read(UA_FILE)?;
    let raw: UserAgentsRaw = parse_json(&ua_contents, UA_FILE)?;

    let gsa_contents = source.read(GSA_FILE)?;
    let gsa: Vec<String> = gsa_contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty())
        .map(str::to_string)
        .collect();

    Ok(UserAgentPool {
        os: raw.os,
        ua_template: raw.ua,
        versions: raw.versions,
        gsa,
    })
}

fn load_locales(source: &dyn DataSource) -> Result<LocaleMap, DataError> {
    const FILE: &str = "locales.json";
    let contents = source.read(FILE)?;
    let raw: LocalesRaw = parse_json(&contents, FILE)?;
    Ok(LocaleMap::from_owned(raw.locale_names, raw.rtl_locales))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_pool() -> UserAgentPool {
        UserAgentPool {
            os: vec![
                "Windows NT 10.0; Win64; x64".into(),
                "X11; Linux x86_64".into(),
            ],
            ua_template: "Mozilla/5.0 ({os}; rv:{version}) Gecko/20100101 Firefox/{version}".into(),
            versions: vec!["140.0".into(), "141.0".into()],
            gsa: vec!["GSA/123.0 Mobile".into()],
        }
    }

    #[test]
    fn generate_at_instantiates_template() {
        let pool = sample_pool();
        let ua = pool.generate_at(0, 1).unwrap();
        assert_eq!(
            ua,
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64; rv:141.0) Gecko/20100101 Firefox/141.0"
        );
    }

    #[test]
    fn generate_at_out_of_range_is_none() {
        let pool = sample_pool();
        assert!(pool.generate_at(9, 0).is_none());
        assert!(pool.generate_at(0, 9).is_none());
    }

    #[test]
    fn random_generation_is_always_a_pool_member() {
        let pool = sample_pool();
        for _ in 0..50 {
            let ua = pool.generate().expect("non-empty pool generates");
            assert!(pool.is_member(&ua), "generated UA not a pool member: {ua}");
        }
    }

    #[test]
    fn embedded_tracker_patterns_strip_utm() {
        let bundle = load_embedded_bundle().expect("embedded data");
        assert!(bundle.tracker_patterns.rule_count() > 0);
        let cleaned = bundle
            .tracker_patterns
            .clean_url("https://example.com/a?utm_source=x&q=rust");
        assert_eq!(cleaned, "https://example.com/a?q=rust");
    }

    #[test]
    fn embedded_bangs_are_available() {
        let bundle = load_embedded_bundle().expect("precompiled data");
        assert!(!bundle.bangs.is_empty());
        assert!(bundle.bangs.resolve("g").is_some() || bundle.bangs.len() > 100);
    }

    #[test]
    fn embedded_ahmia_binary_search() {
        let bundle = load_embedded_bundle().expect("embedded data");
        assert!(bundle.ahmia_blacklist.len() > 1000);
        // First hash from the bundled list.
        assert!(
            bundle
                .ahmia_blacklist
                .contains("0000412c901989287c281fb4416d39dd")
        );
        assert!(
            !bundle
                .ahmia_blacklist
                .contains("zzzzzzzzzzzzzzzzzzzzzzzzzzzzzzzz")
        );
    }

    #[test]
    fn empty_pool_does_not_generate() {
        let pool = UserAgentPool::default();
        assert!(pool.generate().is_none());
        assert!(pool.generate_gsa().is_none());
    }

    #[test]
    fn gsa_generation_appends_suffix() {
        let pool = sample_pool();
        let ua = pool.generate_gsa_at(0).unwrap();
        assert_eq!(ua, "GSA/123.0 Mobile NSTNWV");
        assert!(pool.generate_gsa().unwrap().ends_with(GSA_USERAGENT_SUFFIX));
    }

    #[test]
    fn detect_language_identifies_english() {
        let code = detect_language(
            "This is a reasonably long sentence written plainly in the English language.",
        )
        .expect("a language should be detected");
        assert_eq!(code.as_str(), "eng");
    }

    #[test]
    fn detect_language_empty_text_is_none() {
        assert!(detect_language("").is_none());
    }
}

#[cfg(test)]
mod bang_trie_properties {
    use super::*;
    use proptest::prelude::*;
    use std::collections::HashMap;

    prop_compose! {
        fn arb_entry()(
            url in "[a-z0-9:/._\u{2}-]{0,24}",
            rank in -10i32..10_000,
        ) -> BangEntry {
            BangEntry { url_template: url, rank }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        #[test]
        fn insert_lookup_round_trip(
            entries in prop::collection::hash_map(
                "[a-zA-Z0-9!:._-]{1,10}",
                arb_entry(),
                0..24,
            ),
            probes in prop::collection::vec("[a-zA-Z0-9!:._-]{0,10}", 0..24),
        ) {
            let mut trie = BangTrie::new();
            for (token, entry) in &entries {
                trie.insert(token, entry.clone());
            }

            prop_assert_eq!(trie.len(), entries.len());
            prop_assert_eq!(trie.is_empty(), entries.is_empty());

            for (token, entry) in &entries {
                prop_assert_eq!(trie.resolve(token), Some(entry.clone()));
            }

            for probe in &probes {
                if !entries.contains_key(probe.as_str()) {
                    prop_assert_eq!(trie.resolve(probe), None);
                }
            }
        }

        #[test]
        fn suggest_returns_prefix_matches(
            entries in prop::collection::hash_map(
                "[a-z]{2,6}",
                arb_entry(),
                1..16,
            ),
        ) {
            let mut trie = BangTrie::new();
            for (token, entry) in &entries {
                trie.insert(token, entry.clone());
            }
            let Some(sample) = entries.keys().next() else {
                return Ok(());
            };
            let prefix: String = sample.chars().take(1).collect();
            let suggested = trie.suggest(&prefix, 32);
            prop_assert!(suggested.iter().all(|(t, _)| t.contains(&prefix)));
            prop_assert!(suggested.iter().any(|(t, _)| t.starts_with(&prefix) || t.contains(&prefix)));
            prop_assert!(suggested.iter().any(|(t, _)| entries.contains_key(t)));
        }

        #[test]
        fn reinserting_a_token_overwrites_without_changing_len(
            token in "[a-zA-Z0-9!]{1,10}",
            first in arb_entry(),
            second in arb_entry(),
        ) {
            let mut trie = BangTrie::new();
            trie.insert(&token, first);
            trie.insert(&token, second.clone());

            prop_assert_eq!(trie.len(), 1);
            prop_assert_eq!(trie.resolve(&token), Some(second));
        }
    }

    #[test]
    fn shared_prefixes_resolve_independently() {
        let mut map: HashMap<&str, BangEntry> = HashMap::new();
        map.insert(
            "g",
            BangEntry {
                url_template: "g".into(),
                rank: 1,
            },
        );
        map.insert(
            "go",
            BangEntry {
                url_template: "go".into(),
                rank: 2,
            },
        );
        map.insert(
            "goo",
            BangEntry {
                url_template: "goo".into(),
                rank: 3,
            },
        );

        let mut trie = BangTrie::new();
        for (t, e) in &map {
            trie.insert(t, e.clone());
        }

        for (t, e) in &map {
            assert_eq!(trie.resolve(t), Some(e.clone()));
        }
        assert_eq!(trie.resolve("goog"), None);
        assert_eq!(trie.resolve(""), None);
    }
}

#[cfg(test)]
mod useragent_properties {
    use super::*;
    use proptest::prelude::*;

    prop_compose! {
        fn arb_pool()(
            os in prop::collection::vec("[A-Za-z0-9 ;:._()x-]{1,32}", 1..8),
            versions in prop::collection::vec("[0-9]{1,3}\\.[0-9]{1,3}", 1..8),
            template in "[A-Za-z0-9/ ():;.-]{0,16}(\\{os\\})?[A-Za-z0-9/ ():;.-]{0,8}(\\{version\\})?[A-Za-z0-9/ ():;.-]{0,8}",
        ) -> UserAgentPool {
            UserAgentPool {
                os,
                ua_template: template,
                versions,
                gsa: Vec::new(),
            }
        }
    }

    proptest! {
        #![proptest_config(ProptestConfig::with_cases(200))]

        #[test]
        fn every_generated_useragent_is_a_pool_member(
            pool in arb_pool(),
            draws in 1usize..32,
        ) {
            for _ in 0..draws {
                let ua = pool
                    .generate()
                    .expect("a loaded pool with non-empty os/versions always generates");
                prop_assert!(
                    pool.is_member(&ua),
                    "generated user-agent is not a member of the pool: {ua}"
                );
            }
        }
    }
}
