//! Typed boot-time view of [`Settings`].
//!
//! Call [`resolve_settings`] (or [`Settings::resolve`]) after YAML + `APP_*`
//! env load. Hot paths and registry builders should prefer these fields over
//! re-parsing `ExtraMap` / raw plugin maps.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_yaml_ng::Value;

use crate::{EngineSettings, ExtraMap, Settings, StringOrVec, SuspendedTimes};

/// How a non-empty `engines:` list interacts with the built-in catalog.
///
/// Default is [`Replace`](EngineListMode::Replace) (historical Zoeken /
/// SearXNG-style): a non-empty list is the full registry. Use [`Merge`]
/// to overlay entries onto defaults without dropping engines you omit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum EngineListMode {
    /// Non-empty `engines:` fully replaces the built-in catalog.
    #[default]
    Replace,
    /// Overlay `engines:` onto the built-in catalog by engine id / name.
    Merge,
}

/// Hostname rewrite / priority rules for `/config` and SPA client-features.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostnameRules {
    pub replace: Vec<(String, String)>,
    pub remove: Vec<String>,
    pub high_priority: Vec<String>,
    pub low_priority: Vec<String>,
}

/// Where limiter TOML comes from (parsed later by the botdetect loader).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LimiterSource {
    /// Bundled `limiter.toml` from the data bundle.
    Bundled,
    /// Path from `limiter.file`.
    File(String),
    /// Inline TOML from `limiter.toml`.
    Inline(String),
}

/// Resolved limiter pointers (TOML source + link-token secret).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedLimiter {
    pub source: LimiterSource,
    pub link_token: String,
}

/// One `engines:` entry as a typed boot view (registry still builds the engine).
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedEngine {
    pub name: String,
    pub engine_key: String,
    pub disabled: bool,
    pub inactive: bool,
    pub shortcut: Option<String>,
    pub timeout: Option<f64>,
    pub weight: Option<f64>,
    pub tokens: Option<Vec<String>>,
    pub categories: Option<StringOrVec>,
    pub network: Option<String>,
}

impl From<&EngineSettings> for ResolvedEngine {
    fn from(cfg: &EngineSettings) -> Self {
        Self {
            name: cfg.name.clone(),
            engine_key: cfg.engine.clone().unwrap_or_else(|| cfg.name.clone()),
            disabled: cfg.disabled == Some(true),
            inactive: cfg.inactive == Some(true),
            shortcut: cfg.shortcut.clone(),
            timeout: cfg.timeout,
            weight: cfg.weight,
            tokens: cfg.tokens.clone(),
            categories: cfg.categories.clone(),
            network: cfg.network.clone(),
        }
    }
}

/// Preference defaults for SPA client-features (former plugin `active` flags).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientFeatureDefaults {
    pub active: BTreeMap<String, bool>,
}

/// Suspension / health durations from `search.suspended_times` (+ ban knobs).
///
/// Consumed by [`zoeken_search::SuspensionPolicy`] / the storage circuit —
/// the single engine-health authority after in-process suspend was removed.
#[derive(Debug, Clone, PartialEq)]
pub struct HealthDurations {
    pub ban_time_on_fail: f64,
    pub max_ban_time_on_fail: f64,
    pub access_denied: f64,
    pub captcha: f64,
    pub too_many_requests: f64,
    pub cf_captcha: f64,
    pub cf_access_denied: f64,
    pub recaptcha_captcha: f64,
}

impl From<&SuspendedTimes> for HealthDurations {
    fn from(times: &SuspendedTimes) -> Self {
        Self {
            ban_time_on_fail: 0.0,
            max_ban_time_on_fail: 0.0,
            access_denied: times.access_denied,
            captcha: times.captcha,
            too_many_requests: times.too_many_requests,
            cf_captcha: times.cf_captcha,
            cf_access_denied: times.cf_access_denied,
            recaptcha_captcha: times.recaptcha_captcha,
        }
    }
}

/// Typed settings view used at boot after YAML + env load.
#[derive(Debug, Clone, PartialEq)]
pub struct ResolvedSettings {
    pub raw: Settings,
    pub hostnames: HostnameRules,
    pub limiter: ResolvedLimiter,
    pub engines: Vec<ResolvedEngine>,
    pub engine_list_mode: EngineListMode,
    pub client_features: ClientFeatureDefaults,
    pub health: HealthDurations,
}

/// Build a [`ResolvedSettings`] from loaded [`Settings`] (post `APP_*` overrides).
#[must_use]
pub fn resolve_settings(settings: &Settings) -> ResolvedSettings {
    let mut health = HealthDurations::from(&settings.search.suspended_times);
    health.ban_time_on_fail = settings.search.ban_time_on_fail;
    health.max_ban_time_on_fail = settings.search.max_ban_time_on_fail;

    ResolvedSettings {
        raw: settings.clone(),
        hostnames: hostnames_from_extra(&settings.hostnames.extra),
        limiter: resolve_limiter(settings),
        engines: settings.engines.iter().map(ResolvedEngine::from).collect(),
        engine_list_mode: settings.search.engine_list_mode,
        client_features: ClientFeatureDefaults {
            active: settings
                .plugins
                .0
                .iter()
                .filter_map(|(id, entry)| entry.active.map(|active| (id.clone(), active)))
                .collect(),
        },
        health,
    }
}

impl Settings {
    /// Resolve typed boot fields; keeps `self` as the raw/compat document.
    #[must_use]
    pub fn resolve(&self) -> ResolvedSettings {
        resolve_settings(self)
    }
}

fn resolve_limiter(settings: &Settings) -> ResolvedLimiter {
    let link_token = settings
        .limiter
        .get("link_token")
        .and_then(Value::as_str)
        .unwrap_or("")
        .to_string();
    let source = if let Some(text) = settings.limiter.get("toml").and_then(Value::as_str) {
        LimiterSource::Inline(text.to_string())
    } else if let Some(path) = settings.limiter.get("file").and_then(Value::as_str) {
        LimiterSource::File(path.to_string())
    } else {
        LimiterSource::Bundled
    };
    ResolvedLimiter { source, link_token }
}

pub(crate) fn hostnames_from_extra(extra: &ExtraMap) -> HostnameRules {
    HostnameRules {
        replace: hostnames_replace(extra.get("replace")),
        remove: yaml_string_list(extra.get("remove")),
        high_priority: yaml_string_list(extra.get("high_priority")),
        low_priority: yaml_string_list(extra.get("low_priority")),
    }
}

fn hostnames_replace(value: Option<&Value>) -> Vec<(String, String)> {
    let Some(value) = value else {
        return Vec::new();
    };
    match value {
        Value::Mapping(map) => map
            .iter()
            .filter_map(|(key, value)| Some((yaml_string(key)?, yaml_string(value)?)))
            .collect(),
        Value::Sequence(seq) => seq
            .iter()
            .filter_map(|item| match item {
                Value::Mapping(map) => {
                    let pattern = map
                        .get(Value::String("pattern".to_string()))
                        .and_then(yaml_string)?;
                    let replacement = map
                        .get(Value::String("replacement".to_string()))
                        .and_then(yaml_string)?;
                    Some((pattern, replacement))
                }
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

fn yaml_string_list(value: Option<&Value>) -> Vec<String> {
    match value {
        Some(Value::Sequence(seq)) => seq.iter().filter_map(yaml_string).collect(),
        Some(value) => yaml_string(value).into_iter().collect(),
        None => Vec::new(),
    }
}

fn yaml_string(value: &Value) -> Option<String> {
    match value {
        Value::String(value) => Some(value.clone()),
        Value::Number(value) => Some(value.to_string()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{HostnamesSettings, PluginEntry, PluginSettings, SearchSettings};

    #[test]
    fn resolve_defaults_are_empty_catalog_replace() {
        let resolved = Settings::default().resolve();
        assert!(resolved.engines.is_empty());
        assert_eq!(resolved.engine_list_mode, EngineListMode::Replace);
        assert!(matches!(resolved.limiter.source, LimiterSource::Bundled));
        assert!(resolved.hostnames.replace.is_empty());
        assert_eq!(resolved.health.captcha, 86400.0);
        assert_eq!(resolved.health.ban_time_on_fail, 5.0);
    }

    #[test]
    fn resolve_parses_hostnames_and_client_features() {
        let settings = Settings {
            hostnames: HostnamesSettings {
                extra: BTreeMap::from([
                    (
                        "remove".to_string(),
                        Value::Sequence(vec![Value::String("spam.example".to_string())]),
                    ),
                    (
                        "high_priority".to_string(),
                        Value::Sequence(vec![Value::String("docs.example".to_string())]),
                    ),
                    (
                        "replace".to_string(),
                        Value::Mapping(
                            [(
                                Value::String("(www\\.)?old\\.test".to_string()),
                                Value::String("new.test".to_string()),
                            )]
                            .into_iter()
                            .collect(),
                        ),
                    ),
                ]),
            },
            plugins: PluginSettings(BTreeMap::from([
                (
                    "calculator".to_string(),
                    PluginEntry {
                        active: Some(true),
                        ..Default::default()
                    },
                ),
                (
                    "hash".to_string(),
                    PluginEntry {
                        active: Some(false),
                        ..Default::default()
                    },
                ),
            ])),
            search: SearchSettings {
                engine_list_mode: EngineListMode::Merge,
                ..Default::default()
            },
            engines: vec![EngineSettings {
                name: "duckduckgo".to_string(),
                disabled: Some(true),
                ..Default::default()
            }],
            limiter: BTreeMap::from([
                (
                    "file".to_string(),
                    Value::String("/etc/zoeken/limiter.toml".to_string()),
                ),
                (
                    "link_token".to_string(),
                    Value::String("secret".to_string()),
                ),
            ]),
            ..Default::default()
        };

        let resolved = settings.resolve();
        assert_eq!(resolved.hostnames.remove, vec!["spam.example".to_string()]);
        assert_eq!(
            resolved.hostnames.high_priority,
            vec!["docs.example".to_string()]
        );
        assert_eq!(
            resolved.hostnames.replace,
            vec![("(www\\.)?old\\.test".to_string(), "new.test".to_string())]
        );
        assert_eq!(
            resolved.client_features.active.get("calculator"),
            Some(&true)
        );
        assert_eq!(resolved.client_features.active.get("hash"), Some(&false));
        assert_eq!(resolved.engine_list_mode, EngineListMode::Merge);
        assert_eq!(resolved.engines.len(), 1);
        assert!(resolved.engines[0].disabled);
        assert_eq!(
            resolved.limiter,
            ResolvedLimiter {
                source: LimiterSource::File("/etc/zoeken/limiter.toml".to_string()),
                link_token: "secret".to_string(),
            }
        );
    }

    #[test]
    fn engine_list_mode_deserializes_from_yaml() {
        let settings: Settings =
            serde_yaml_ng::from_str("search:\n  engine_list_mode: merge\n").expect("parse");
        assert_eq!(settings.search.engine_list_mode, EngineListMode::Merge);
    }
}
