//! Build-time compilation of bundled data into PHF maps, static slices, and
//! ClearURLs-style tracker rule tables (no JSON/regex plow at embedded startup).

use std::collections::BTreeMap;
use std::env;
use std::fs;
use std::path::PathBuf;

use serde_json::Value;

const BANG_LEAF_KEY: &str = "\u{10}";
const BANG_RANK_SEP: char = '\u{1}';

fn literal(value: &str) -> String {
    format!("{value:?}")
}

fn optional_literal(value: Option<&str>) -> String {
    value.map_or_else(
        || "None".to_string(),
        |value| format!("Some({})", literal(value)),
    )
}

fn string_slice(values: impl IntoIterator<Item = String>) -> String {
    let values = values
        .into_iter()
        .map(|value| literal(&value))
        .collect::<Vec<_>>()
        .join(", ");
    format!("&[{values}]")
}

fn flatten_bang_trie(node: &Value, prefix: &str, output: &mut BTreeMap<String, (String, i32)>) {
    match node {
        Value::String(definition) => {
            let mut parts = definition.splitn(2, BANG_RANK_SEP);
            let url = parts.next().unwrap_or_default().to_string();
            let rank = parts
                .next()
                .and_then(|rank| rank.parse().ok())
                .unwrap_or_default();
            output.insert(prefix.to_string(), (url, rank));
        }
        Value::Object(children) => {
            for (key, value) in children {
                if key == BANG_LEAF_KEY {
                    if let Value::String(definition) = value {
                        let mut parts = definition.splitn(2, BANG_RANK_SEP);
                        let url = parts.next().unwrap_or_default().to_string();
                        let rank = parts
                            .next()
                            .and_then(|rank| rank.parse().ok())
                            .unwrap_or_default();
                        output.insert(prefix.to_string(), (url, rank));
                    }
                } else {
                    flatten_bang_trie(value, &format!("{prefix}{key}"), output);
                }
            }
        }
        _ => {}
    }
}

/// Regex meta characters that make a ClearURLs param rule non-literal.
fn is_literal_pattern(s: &str) -> bool {
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
}

fn unescape_literal(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == '\\' {
            if let Some(next) = chars.next() {
                out.push(next);
            }
        } else {
            out.push(c);
        }
    }
    out
}

/// Classify a ClearURLs query-param rule into exact / prefix / residual regex.
fn classify_param(rule: &str) -> ParamClass {
    let stripped = rule
        .strip_prefix("(?:%3F)?")
        .or_else(|| rule.strip_prefix("(?:\\%3F)?"))
        .unwrap_or(rule);

    if is_literal_pattern(stripped) {
        return ParamClass::Exact(unescape_literal(stripped));
    }

    // utm(?:_[a-z_]*)?  /  mtm(?:_[a-z_]*)?  /  ga_[a-z_]+  style prefixes
    if let Some(prefix) = strip_optional_suffix_class(stripped) {
        return ParamClass::Prefix(prefix);
    }

    // name_? → name, name_
    if let Some(base) = stripped.strip_suffix('_').and_then(|s| s.strip_suffix('?')) {
        if is_literal_pattern(base) {
            let base = unescape_literal(base);
            return ParamClass::ExactVariants(vec![base.clone(), format!("{base}_")]);
        }
    }

    // srs? → sr, srs
    if let Some(base) = stripped.strip_suffix('?') {
        if !base.is_empty()
            && is_literal_pattern(base)
            && base
                .chars()
                .last()
                .is_some_and(|c| c.is_ascii_alphanumeric())
        {
            let base = unescape_literal(base);
            let mut short = base.clone();
            short.pop();
            if !short.is_empty() {
                return ParamClass::ExactVariants(vec![short, base]);
            }
        }
    }

    // colii?d → colid, coliid
    if let Some((head, tail)) = stripped.split_once('?') {
        if tail.chars().all(|c| c.is_ascii_alphanumeric())
            && let Some(optional) = head.chars().last()
            && optional.is_ascii_alphanumeric()
            && is_literal_pattern(&head[..head.len() - 1])
            && is_literal_pattern(tail)
        {
            let stem = unescape_literal(&head[..head.len() - 1]);
            return ParamClass::ExactVariants(vec![
                format!("{stem}{tail}"),
                format!("{stem}{optional}{tail}"),
            ]);
        }
    }

    ParamClass::Regex(rule.to_string())
}

fn strip_optional_suffix_class(pat: &str) -> Option<String> {
    // foo(?:_[a-z_]*)?  or  foo_[a-z_]+  or  foo_[a-z]*
    const SUFFIXES: &[&str] = &[
        "(?:_[a-z_]*)?",
        "(?:_[a-z]*)?",
        "_[a-z_]+",
        "_[a-z]+",
        "_[a-z_]*",
        "_[a-z]*",
        "(?:_[a-z]*)+",
    ];
    for suffix in SUFFIXES {
        if let Some(prefix) = pat.strip_suffix(suffix) {
            if !prefix.is_empty() && is_literal_pattern(prefix) {
                return Some(unescape_literal(prefix));
            }
        }
    }
    None
}

enum ParamClass {
    Exact(String),
    ExactVariants(Vec<String>),
    Prefix(String),
    Regex(String),
}

#[derive(Clone)]
enum HostMatchGen {
    Any,
    /// host == needle or host.ends_with("."+needle)
    Suffix(String),
    /// any DNS label equals needle (amazon.*.*)
    Label(String),
    /// any of several suffix needles
    AnySuffix(Vec<String>),
    /// residual — kept as regex
    Regex(String),
}

fn compile_host_pattern(url: &str) -> (HostMatchGen, Option<String>) {
    if url == ".*" {
        return (HostMatchGen::Any, None);
    }

    // Normalize \/ to / for path extraction after host body parse.
    let common = url
        .strip_prefix("^https?:\\/\\/(?:[a-z0-9-]+\\.)*?")
        .or_else(|| url.strip_prefix("^https?:\\\\/\\\\/(?:[a-z0-9-]+\\\\.)*?"));

    if let Some(body) = common {
        return compile_host_body(body, url);
    }

    // Variants without the subdomain wildcard prefix.
    let alt_prefixes = ["^https?:\\/\\/", "^https?://", "https?:\\/\\/", "https?://"];
    for prefix in alt_prefixes {
        if let Some(body) = url.strip_prefix(prefix) {
            let body = body
                .strip_prefix("(?:[a-z0-9-]+\\.)*?")
                .or_else(|| body.strip_prefix("([a-z0-9-.]*\\.)"))
                .unwrap_or(body);
            return compile_host_body(body, url);
        }
    }

    (HostMatchGen::Regex(url.to_string()), None)
}

fn compile_host_body(body: &str, original: &str) -> (HostMatchGen, Option<String>) {
    // Split host vs path at first \/ or / or \?
    let (host_part, path_part) = if let Some(idx) = body.find("\\/") {
        (&body[..idx], Some(&body[idx + 2..]))
    } else if let Some(idx) = body.find('/') {
        (&body[..idx], Some(&body[idx + 1..]))
    } else if let Some(idx) = body.find("\\?") {
        (&body[..idx], None)
    } else {
        (body, None)
    };

    let path_prefix = path_part.and_then(|p| {
        let p = p.strip_suffix("\\?").unwrap_or(p);
        let p = p.strip_suffix('?').unwrap_or(p);
        if p.is_empty() || !is_literal_pattern(p) {
            return None;
        }
        Some(format!("/{}", unescape_literal(p)))
    });

    // amazon(?:\.[a-z]{2,}){1,}
    if let Some(label) = host_part.strip_suffix("(?:\\.[a-z]{2,}){1,}") {
        if is_literal_pattern(label) {
            return (HostMatchGen::Label(unescape_literal(label)), path_prefix);
        }
    }

    // (youtube\.com|youtu\.be)
    if host_part.starts_with('(') && host_part.ends_with(')') && host_part.contains('|') {
        let inner = &host_part[1..host_part.len() - 1];
        let raw_parts: Vec<&str> = inner.split('|').collect();
        if raw_parts.iter().all(|p| is_literal_pattern(p)) {
            let parts: Vec<String> = raw_parts.into_iter().map(unescape_literal).collect();
            if !parts.is_empty() {
                return (HostMatchGen::AnySuffix(parts), path_prefix);
            }
        }
    }

    if is_literal_pattern(host_part) {
        return (
            HostMatchGen::Suffix(unescape_literal(host_part)),
            path_prefix,
        );
    }

    // Optional subdomain groups: (?:accounts\.)?firefox\.com
    if let Some(rest) = host_part.strip_prefix("(?:") {
        if let Some(idx) = rest.find(")?") {
            let after = &rest[idx + 2..];
            if is_literal_pattern(after) {
                return (HostMatchGen::Suffix(unescape_literal(after)), path_prefix);
            }
        }
    }

    (HostMatchGen::Regex(original.to_string()), path_prefix)
}

fn emit_host_match(host: &HostMatchGen) -> String {
    match host {
        HostMatchGen::Any => "HostMatch::Any".to_string(),
        HostMatchGen::Suffix(s) => format!("HostMatch::Suffix({})", literal(s)),
        HostMatchGen::Label(s) => format!("HostMatch::Label({})", literal(s)),
        HostMatchGen::AnySuffix(parts) => {
            format!("HostMatch::AnySuffix({})", string_slice(parts.clone()))
        }
        HostMatchGen::Regex(s) => format!("HostMatch::Regex({})", literal(s)),
    }
}

fn main() {
    let manifest = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("manifest dir"));
    let data_dir = manifest.join("data");
    let out_dir = PathBuf::from(env::var("OUT_DIR").expect("out dir"));

    for file in [
        "external_bangs.json",
        "currencies.json",
        "wikidata_units.json",
        "engine_traits.json",
        "locales.json",
        "useragents.json",
        "gsa_useragents.txt",
        "tracker_patterns.json",
        "ahmia_blacklist.txt",
        "doi_resolvers.json",
        "autocomplete_backends.json",
        "limiter.toml",
        "info_pages.json",
    ] {
        println!("cargo:rerun-if-changed={}", data_dir.join(file).display());
    }

    let read_json = |file: &str| -> Value {
        serde_json::from_str(
            &fs::read_to_string(data_dir.join(file)).expect("read bundled data file"),
        )
        .expect("parse bundled data JSON")
    };

    let mut generated = String::from("// @generated by build.rs; do not edit.\n");

    // --- bangs: PHF exact + sorted token list for suggest ---
    let mut bangs = BTreeMap::new();
    flatten_bang_trie(&read_json("external_bangs.json")["trie"], "", &mut bangs);
    let mut bang_map = phf_codegen::Map::new();
    let bang_entries: Vec<(String, String)> = bangs
        .iter()
        .map(|(token, (url, rank))| (token.clone(), format!("({}, {})", literal(url), rank)))
        .collect();
    for (token, entry) in &bang_entries {
        bang_map.entry(token, entry);
    }
    generated.push_str(&format!(
        "pub static PRECOMPILED_BANGS: phf::Map<&'static str, (&'static str, i32)> = {};\n",
        bang_map.build()
    ));
    generated.push_str("pub static PRECOMPILED_BANG_TOKENS: &[&str] = &[\n");
    for token in bangs.keys() {
        generated.push_str(&format!("{},\n", literal(token)));
    }
    generated.push_str("];\n");

    // --- currencies: PHF ---
    let currencies = read_json("currencies.json");
    let mut currency_names = phf_codegen::Map::new();
    let currency_name_entries: Vec<(String, String)> = currencies["names"]
        .as_object()
        .expect("currency names")
        .iter()
        .map(|(name, codes)| {
            let values = match codes {
                Value::String(code) => vec![code.clone()],
                Value::Array(codes) => codes
                    .iter()
                    .map(|code| code.as_str().expect("currency code").to_string())
                    .collect(),
                _ => panic!("invalid currency name"),
            };
            (name.clone(), string_slice(values))
        })
        .collect();
    for (name, value) in &currency_name_entries {
        currency_names.entry(name, value);
    }
    generated.push_str(&format!(
        "pub static PRECOMPILED_CURRENCY_NAMES: phf::Map<&'static str, &'static [&'static str]> = {};\n",
        currency_names.build()
    ));
    let mut currency_iso = phf_codegen::Map::new();
    let currency_iso_entries: Vec<(String, String)> = currencies["iso4217"]
        .as_object()
        .expect("currency iso")
        .iter()
        .map(|(code, languages)| {
            let entries = languages
                .as_object()
                .expect("currency languages")
                .iter()
                .map(|(language, name)| {
                    format!(
                        "({}, {})",
                        literal(language),
                        literal(name.as_str().expect("currency display name"))
                    )
                })
                .collect::<Vec<_>>()
                .join(", ");
            (code.clone(), format!("&[{entries}]"))
        })
        .collect();
    for (code, value) in &currency_iso_entries {
        currency_iso.entry(code, value);
    }
    generated.push_str(&format!(
        "pub static PRECOMPILED_CURRENCY_ISO: phf::Map<&'static str, &'static [(&'static str, &'static str)]> = {};\n",
        currency_iso.build()
    ));

    // --- units: PHF ---
    let units = read_json("wikidata_units.json");
    let mut unit_map = phf_codegen::Map::new();
    let unit_entries: Vec<(String, String)> = units
        .as_object()
        .expect("units")
        .iter()
        .map(|(id, unit)| {
            (
                id.clone(),
                format!(
                    "({}, {}, {:?})",
                    optional_literal(unit["si_name"].as_str()),
                    literal(unit["symbol"].as_str().expect("unit symbol")),
                    unit["to_si_factor"].as_f64()
                ),
            )
        })
        .collect();
    for (id, value) in &unit_entries {
        unit_map.entry(id, value);
    }
    generated.push_str(&format!(
        "pub static PRECOMPILED_UNITS: phf::Map<&'static str, (Option<&'static str>, &'static str, Option<f64>)> = {};\n",
        unit_map.build()
    ));

    // --- engine traits: PHF + custom JSON string (parsed lazily per engine) ---
    let traits = read_json("engine_traits.json");
    let mut trait_map = phf_codegen::Map::new();
    let trait_entries: Vec<(String, String)> = traits
        .as_object()
        .expect("engine traits")
        .iter()
        .map(|(engine, trait_value)| {
            let pairs = |name: &str| {
                trait_value[name]
                    .as_object()
                    .map(|map| {
                        map.iter()
                            .map(|(key, value)| {
                                format!(
                                    "({}, {})",
                                    literal(key),
                                    literal(value.as_str().expect("trait value"))
                                )
                            })
                            .collect::<Vec<_>>()
                            .join(", ")
                    })
                    .unwrap_or_default()
            };
            let custom = serde_json::to_string(&trait_value["custom"]).expect("custom json");
            (
                engine.clone(),
                format!(
                    "({}, {}, &[{}], &[{}], {})",
                    optional_literal(trait_value["all_locale"].as_str()),
                    optional_literal(trait_value["data_type"].as_str()),
                    pairs("languages"),
                    pairs("regions"),
                    literal(&custom)
                ),
            )
        })
        .collect();
    for (engine, value) in &trait_entries {
        trait_map.entry(engine, value);
    }
    generated.push_str(&format!(
        "pub static PRECOMPILED_ENGINE_TRAITS: phf::Map<&'static str, (Option<&'static str>, Option<&'static str>, &'static [(&'static str, &'static str)], &'static [(&'static str, &'static str)], &'static str)> = {};\n",
        trait_map.build()
    ));

    // --- locales ---
    let locales = read_json("locales.json");
    let mut locale_map = phf_codegen::Map::new();
    let locale_entries: Vec<(String, String)> = locales["LOCALE_NAMES"]
        .as_object()
        .expect("locale names")
        .iter()
        .map(|(locale, name)| (locale.clone(), literal(name.as_str().expect("locale name"))))
        .collect();
    for (locale, name) in &locale_entries {
        locale_map.entry(locale, name);
    }
    generated.push_str(&format!(
        "pub static PRECOMPILED_LOCALE_NAMES: phf::Map<&'static str, &'static str> = {};\n",
        locale_map.build()
    ));
    generated.push_str(&format!(
        "pub static PRECOMPILED_RTL_LOCALES: &[&str] = {};\n",
        string_slice(
            locales["RTL_LOCALES"]
                .as_array()
                .expect("rtl locales")
                .iter()
                .map(|locale| locale.as_str().expect("rtl locale").to_string())
        )
    ));

    // --- user agents ---
    let useragents = read_json("useragents.json");
    generated.push_str(&format!(
        "pub static PRECOMPILED_USERAGENT_OS: &[&str] = {};\npub static PRECOMPILED_USERAGENT_TEMPLATE: &str = {};\npub static PRECOMPILED_USERAGENT_VERSIONS: &[&str] = {};\n",
        string_slice(useragents["os"].as_array().expect("useragent os").iter().map(|value| value.as_str().expect("os").to_string())),
        literal(useragents["ua"].as_str().expect("useragent template")),
        string_slice(useragents["versions"].as_array().expect("useragent versions").iter().map(|value| value.as_str().expect("version").to_string())),
    ));
    let gsa = fs::read_to_string(data_dir.join("gsa_useragents.txt")).expect("read gsa useragents");
    generated.push_str(&format!(
        "pub static PRECOMPILED_GSA_USERAGENTS: &[&str] = {};\n",
        string_slice(
            gsa.lines()
                .map(str::trim)
                .filter(|line| !line.is_empty())
                .map(str::to_string)
        )
    ));

    // --- tracker patterns: structured host/param tables ---
    let trackers = read_json("tracker_patterns.json");
    let mut regex_url_count = 0usize;
    let mut regex_param_count = 0usize;
    generated.push_str("pub static PRECOMPILED_TRACKER_RULES: &[CompiledTrackerRule] = &[\n");
    for tracker in trackers.as_array().expect("tracker patterns") {
        let url = tracker["url"].as_str().expect("tracker url");
        let (host, path_prefix) = compile_host_pattern(url);
        if matches!(host, HostMatchGen::Regex(_)) {
            regex_url_count += 1;
        }

        let mut exact: Vec<String> = Vec::new();
        let mut prefixes: Vec<String> = Vec::new();
        let mut regex_params: Vec<String> = Vec::new();
        for rule in tracker["rules"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str())
        {
            match classify_param(rule) {
                ParamClass::Exact(s) => exact.push(s),
                ParamClass::ExactVariants(vs) => exact.extend(vs),
                ParamClass::Prefix(s) => prefixes.push(s),
                ParamClass::Regex(s) => {
                    regex_param_count += 1;
                    regex_params.push(s);
                }
            }
        }
        exact.sort();
        exact.dedup();
        prefixes.sort();
        prefixes.dedup();

        let exceptions = tracker["exceptions"]
            .as_array()
            .into_iter()
            .flatten()
            .filter_map(|v| v.as_str().map(str::to_string))
            .collect::<Vec<_>>();

        // Keep original url string for Lua introspection / disk parity display.
        generated.push_str(&format!(
            "CompiledTrackerRule {{ url_pattern: {}, host: {}, path_prefix: {}, exact_params: {}, prefix_params: {}, regex_params: {}, exception_regexes: {} }},\n",
            literal(url),
            emit_host_match(&host),
            optional_literal(path_prefix.as_deref()),
            string_slice(exact),
            string_slice(prefixes),
            string_slice(regex_params),
            string_slice(exceptions),
        ));
    }
    generated.push_str("];\n");
    println!(
        "cargo:warning=tracker compile: {regex_url_count} url-regex fallbacks, {regex_param_count} param-regex leftovers"
    );

    // --- ahmia: packed 32-byte hex hashes for binary search ---
    let ahmia = fs::read_to_string(data_dir.join("ahmia_blacklist.txt")).expect("ahmia");
    let mut packed = Vec::new();
    for line in ahmia.lines() {
        let hash = line.trim();
        if hash.is_empty() || hash.starts_with('#') {
            continue;
        }
        assert_eq!(hash.len(), 32, "ahmia hash must be 32 hex chars");
        packed.extend_from_slice(hash.as_bytes());
    }
    fs::write(out_dir.join("ahmia_hashes.bin"), &packed).expect("write ahmia bin");
    generated.push_str(&format!(
        "pub static PRECOMPILED_AHMIA_HASHES: &[u8] = include_bytes!(\"ahmia_hashes.bin\");\npub const PRECOMPILED_AHMIA_COUNT: usize = {};\n",
        packed.len() / 32
    ));

    // --- doi resolvers ---
    let doi = read_json("doi_resolvers.json");
    generated.push_str(&format!(
        "pub static PRECOMPILED_DOI_DEFAULT: &str = {};\n",
        literal(doi["default"].as_str().expect("doi default"))
    ));
    let mut doi_map = phf_codegen::Map::new();
    let doi_entries: Vec<(String, String)> = doi["resolvers"]
        .as_object()
        .expect("doi resolvers")
        .iter()
        .map(|(id, url)| (id.clone(), literal(url.as_str().expect("doi url"))))
        .collect();
    for (id, url) in &doi_entries {
        doi_map.entry(id, url);
    }
    generated.push_str(&format!(
        "pub static PRECOMPILED_DOI_RESOLVERS: phf::Map<&'static str, &'static str> = {};\n",
        doi_map.build()
    ));

    // --- autocomplete backends ---
    let autocomplete = read_json("autocomplete_backends.json");
    generated.push_str(&format!(
        "pub static PRECOMPILED_AUTOCOMPLETE_BACKENDS: &[&str] = {};\n",
        string_slice(
            autocomplete["backends"]
                .as_array()
                .expect("backends")
                .iter()
                .map(|v| v.as_str().expect("backend").to_string())
        )
    ));

    // --- info pages ---
    let info = read_json("info_pages.json");
    generated.push_str(&format!(
        "pub static PRECOMPILED_INFO_DEFAULT_LOCALE: &str = {};\n",
        literal(info["default_locale"].as_str().unwrap_or("en"))
    ));
    generated
        .push_str("pub static PRECOMPILED_INFO_PAGES: &[(&str, &[(&str, &str, &str)])] = &[\n");
    let locales_obj = info["locales"].as_object().expect("info locales");
    let mut locale_keys: Vec<_> = locales_obj.keys().collect();
    locale_keys.sort();
    for locale in locale_keys {
        let pages = locales_obj[locale].as_object().expect("info pages");
        let mut page_keys: Vec<_> = pages.keys().collect();
        page_keys.sort();
        generated.push_str(&format!("({}, &[\n", literal(locale)));
        for page in page_keys {
            let entry = &pages[page];
            generated.push_str(&format!(
                "({}, {}, {}),\n",
                literal(page),
                literal(entry["title"].as_str().expect("title")),
                literal(entry["content"].as_str().expect("content"))
            ));
        }
        generated.push_str("]),\n");
    }
    generated.push_str("];\n");

    fs::write(out_dir.join("generated_data.rs"), generated).expect("write generated data");
}
