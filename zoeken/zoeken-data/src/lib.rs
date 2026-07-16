//! zoeken-data: bundled static data assets (bangs, currencies, units, engine traits,
//! user-agents, locales). Default tables are precompiled at build time (PHF / static
//! slices); `APP_DATA_DIR` can still load JSON from disk. Provides user-agent
//! generation and language detection.

use std::collections::{HashMap, HashSet};
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
    pub ahmia_blacklist: HashSet<String>,
    pub plugin_data: PluginData,
}

/// One ClearURLs-style provider rule (url match → delete matching query args).
#[derive(Debug, Clone)]
pub struct TrackerRule {
    pub url_pattern: String,
    pub exceptions: Vec<String>,
    pub rules: Vec<String>,
    url_re: regex::Regex,
    exception_res: Vec<regex::Regex>,
    rule_res: Vec<regex::Regex>,
}

/// Bundled ClearURLs tracker-parameter rules.
#[derive(Debug, Default, Clone)]
pub struct TrackerPatterns {
    pub rules: Vec<TrackerRule>,
}

impl TrackerPatterns {
    /// Strip tracker query args using ClearURLs rules (SearXNG `TRACKER_PATTERNS.clean_url`).
    pub fn clean_url(&self, raw: &str) -> String {
        let Ok(mut parsed) = url::Url::parse(raw) else {
            return raw.to_string();
        };
        // Most result URLs have no query string — skip the rule scan entirely.
        if parsed.query().is_none() {
            return raw.to_string();
        }
        let mut current = raw.to_string();
        for rule in &self.rules {
            if parsed.query().is_none() {
                break;
            }
            if !rule.url_re.is_match(&current) {
                continue;
            }
            if rule
                .exception_res
                .iter()
                .any(|exception| exception.is_match(&current))
            {
                continue;
            }
            let kept: Vec<(String, String)> = parsed
                .query_pairs()
                .filter(|(name, _)| !rule.rule_res.iter().any(|pat| pat.is_match(name)))
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
    static_entries: OnceLock<HashMap<&'static str, BangEntry>>,
}

impl BangTrie {
    pub fn new() -> Self {
        Self::default()
    }

    fn from_static(map: &'static phf::Map<&'static str, (&'static str, i32)>) -> Self {
        Self {
            root: BangNode::default(),
            len: map.len(),
            static_map: Some(map),
            static_entries: OnceLock::new(),
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

    pub fn resolve(&self, token: &str) -> Option<&BangEntry> {
        if let Some(static_map) = self.static_map {
            if !static_map.contains_key(token) {
                return None;
            }
            let entries = self.static_entries.get_or_init(|| {
                static_map
                    .entries()
                    .map(|(token, (url_template, rank))| {
                        (
                            *token,
                            BangEntry {
                                url_template: (*url_template).to_string(),
                                rank: *rank,
                            },
                        )
                    })
                    .collect()
            });
            return entries.get(token);
        }
        let mut node = &self.root;
        for ch in token.chars() {
            node = node.children.get(&ch)?;
        }
        node.entry.as_ref()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
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
    pub names: HashMap<String, Vec<String>>,
    pub iso4217: HashMap<String, HashMap<String, String>>,
}

impl CurrencyTable {
    pub fn name_to_iso4217(&self, name: &str) -> Option<&str> {
        self.names
            .get(name)
            .and_then(|v| v.last())
            .map(String::as_str)
    }

    pub fn iso4217_to_name(&self, iso4217: &str, language: &str) -> Option<&str> {
        self.iso4217
            .get(iso4217)
            .and_then(|langs| langs.get(language))
            .map(String::as_str)
    }

    pub fn is_iso4217(&self, iso4217: &str) -> bool {
        self.iso4217.contains_key(iso4217)
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

/// Wikidata units keyed by Q-identifier.
#[derive(Debug, Default, Clone)]
pub struct UnitTable {
    pub units: HashMap<String, UnitEntry>,
}

impl UnitTable {
    pub fn get(&self, id: &str) -> Option<&UnitEntry> {
        self.units.get(id)
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
#[derive(Debug, Default, Clone)]
pub struct EngineTraitsMap {
    pub engines: HashMap<String, EngineTraits>,
}

impl EngineTraitsMap {
    pub fn get(&self, engine: &str) -> Option<&EngineTraits> {
        self.engines.get(engine)
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
    pub fn resolve(&self, locale: &str) -> Option<LocaleInfo> {
        self.locale_names
            .get(locale)
            .map(|name| parse_locale(locale, name.clone()))
    }

    pub fn contains(&self, locale: &str) -> bool {
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
        list if !list.is_empty() => list.into_iter().collect(),
        _ => load_optional_line_set(source, "ahmia_blacklist.txt")?,
    };

    Ok(DataBundle {
        bangs,
        currencies,
        units,
        engine_traits,
        useragents,
        locales,
        tracker_patterns,
        ahmia_blacklist,
        plugin_data: PluginData::default(),
    })
}

fn load_precompiled_bundle() -> DataBundle {
    let currencies = CurrencyTable {
        names: PRECOMPILED_CURRENCY_NAMES
            .iter()
            .map(|(name, codes)| {
                (
                    (*name).to_string(),
                    codes.iter().map(|code| (*code).to_string()).collect(),
                )
            })
            .collect(),
        iso4217: PRECOMPILED_CURRENCY_ISO
            .iter()
            .map(|(code, languages)| {
                (
                    (*code).to_string(),
                    languages
                        .iter()
                        .map(|(language, name)| ((*language).to_string(), (*name).to_string()))
                        .collect(),
                )
            })
            .collect(),
    };
    let units = UnitTable {
        units: PRECOMPILED_UNITS
            .iter()
            .map(|(id, si_name, symbol, to_si_factor)| {
                (
                    (*id).to_string(),
                    UnitEntry {
                        si_name: si_name.map(str::to_string),
                        symbol: (*symbol).to_string(),
                        to_si_factor: *to_si_factor,
                    },
                )
            })
            .collect(),
    };
    let engine_traits = EngineTraitsMap {
        engines: PRECOMPILED_ENGINE_TRAITS
            .iter()
            .zip(precompiled_engine_trait_custom())
            .map(
                |((engine, all_locale, data_type, languages, regions), custom)| {
                    (
                        (*engine).to_string(),
                        EngineTraits {
                            all_locale: all_locale.map(str::to_string),
                            data_type: data_type.map(str::to_string),
                            languages: languages
                                .iter()
                                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                                .collect(),
                            regions: regions
                                .iter()
                                .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
                                .collect(),
                            custom,
                        },
                    )
                },
            )
            .collect(),
    };
    let useragents = UserAgentPool {
        os: PRECOMPILED_USERAGENT_OS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        ua_template: PRECOMPILED_USERAGENT_TEMPLATE.to_string(),
        versions: PRECOMPILED_USERAGENT_VERSIONS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
        gsa: PRECOMPILED_GSA_USERAGENTS
            .iter()
            .map(|value| (*value).to_string())
            .collect(),
    };
    let locales = LocaleMap {
        locale_names: PRECOMPILED_LOCALE_NAMES
            .iter()
            .map(|(locale, name)| ((*locale).to_string(), (*name).to_string()))
            .collect(),
        rtl_locales: PRECOMPILED_RTL_LOCALES
            .iter()
            .map(|locale| (*locale).to_string())
            .collect(),
    };
    let tracker_patterns = tracker_patterns_from_entries(PRECOMPILED_TRACKER_PATTERNS.iter().map(
        |(url, exceptions, rules)| {
            (
                (*url).to_string(),
                exceptions
                    .iter()
                    .map(|value| (*value).to_string())
                    .collect(),
                rules.iter().map(|value| (*value).to_string()).collect(),
            )
        },
    ));
    let ahmia_blacklist = include_str!("../data/ahmia_blacklist.txt")
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect();

    DataBundle {
        bangs: BangTrie::from_static(&PRECOMPILED_BANGS),
        currencies,
        units,
        engine_traits,
        useragents,
        locales,
        tracker_patterns,
        ahmia_blacklist,
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
    Ok(tracker_patterns_from_entries(
        raw.into_iter()
            .map(|entry| (entry.url, entry.exceptions, entry.rules)),
    ))
}

fn tracker_patterns_from_entries(
    entries: impl IntoIterator<Item = (String, Vec<String>, Vec<String>)>,
) -> TrackerPatterns {
    let mut rules = Vec::new();
    for (url_pattern, exceptions, rule_patterns) in entries {
        let Ok(url_re) = regex::Regex::new(&url_pattern) else {
            tracing::warn!(pattern = %url_pattern, "skipping invalid tracker url pattern");
            continue;
        };
        let exception_res: Vec<regex::Regex> = exceptions
            .iter()
            .filter_map(|pat| regex::Regex::new(pat).ok())
            .collect();
        let rule_res: Vec<regex::Regex> = rule_patterns
            .iter()
            .filter_map(|pat| regex::Regex::new(pat).ok())
            .collect();
        if rule_res.is_empty() {
            continue;
        }
        rules.push(TrackerRule {
            url_pattern,
            exceptions,
            rules: rule_patterns,
            url_re,
            exception_res,
            rule_res,
        });
    }
    TrackerPatterns { rules }
}

fn load_optional_line_set(
    source: &dyn DataSource,
    file: &str,
) -> Result<HashSet<String>, DataError> {
    let Some(contents) = source.read_optional(file)? else {
        return Ok(HashSet::new());
    };
    Ok(contents
        .lines()
        .map(str::trim)
        .filter(|line| !line.is_empty() && !line.starts_with('#'))
        .map(str::to_string)
        .collect())
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
    Ok(UnitTable { units })
}

fn load_engine_traits(source: &dyn DataSource) -> Result<EngineTraitsMap, DataError> {
    const FILE: &str = "engine_traits.json";
    let contents = source.read(FILE)?;
    let engines: HashMap<String, EngineTraits> = parse_json(&contents, FILE)?;
    Ok(EngineTraitsMap { engines })
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
    Ok(LocaleMap {
        locale_names: raw.locale_names,
        rtl_locales: raw.rtl_locales,
    })
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
        assert!(!bundle.tracker_patterns.rules.is_empty());
        let cleaned = bundle
            .tracker_patterns
            .clean_url("https://example.com/a?utm_source=x&q=rust");
        assert_eq!(cleaned, "https://example.com/a?q=rust");
    }

    #[test]
    fn embedded_bangs_are_available() {
        let bundle = load_embedded_bundle().expect("precompiled data");
        assert!(!bundle.bangs.is_empty());
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
            // A `HashMap` models a *set* of bang entries keyed by token: it
            // dedupes tokens so the final inserted entry is well-defined.
            entries in prop::collection::hash_map(
                "[a-zA-Z0-9!:._-]{1,10}",
                arb_entry(),
                0..24,
            ),
            // Arbitrary probe tokens; those absent from the set must not resolve.
            probes in prop::collection::vec("[a-zA-Z0-9!:._-]{0,10}", 0..24),
        ) {
            let mut trie = BangTrie::new();
            for (token, entry) in &entries {
                trie.insert(token, entry.clone());
            }

            prop_assert_eq!(trie.len(), entries.len());
            prop_assert_eq!(trie.is_empty(), entries.is_empty());

            for (token, entry) in &entries {
                prop_assert_eq!(trie.resolve(token), Some(entry));
            }

            for probe in &probes {
                if !entries.contains_key(probe.as_str()) {
                    prop_assert_eq!(trie.resolve(probe), None);
                }
            }
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
            prop_assert_eq!(trie.resolve(&token), Some(&second));
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
            assert_eq!(trie.resolve(t), Some(e));
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
