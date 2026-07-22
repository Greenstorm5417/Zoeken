//! Build an [`EngineRegistry`] from [`Settings`]: default engine set, and the
//! `settings.engines[]` name -> built-in-engine factory.
//!
//! # Engine list policy
//!
//! - Empty `engines:` → built-in catalog (common engines enabled; others disabled).
//! - Non-empty + [`EngineListMode::Replace`] (default) → **only** listed engines
//!   are registered (historical Zoeken behavior).
//! - Non-empty + [`EngineListMode::Merge`] → listed entries overlay the built-in
//!   catalog by engine id / name; omitted defaults stay.

use std::time::Duration;

use serde::de::DeserializeOwned;
use zoeken_search::{EngineRegistry, RegisteredEngine};
use zoeken_settings::{EngineListMode, Settings};

use crate::engines::{
    AppleAppStore, Arxiv, Bandcamp, Bing, BingImages, Brave, Core, Crates, Crossref, Currency,
    Dailymotion, Dictionary, DockerHub, Dogpile, DogpileConfig, DuckDuckGo, Elasticsearch,
    ElasticsearchConfig, GenericEngineConfig, GenericHtmlConfig, GenericHtmlEngine,
    GenericJsonConfig, GenericJsonEngine, Genius, Github, GithubCode, Gitlab, Google, Hackernews,
    Imdb, Invidious, Lemmy, Marginalia, MarginaliaConfig, Mastodon, Meilisearch, MeilisearchConfig,
    Mojeek, NineGag, Nyaa, Openstreetmap, Openverse, Peertube, Photon, Piped, Piratebay, Pypi,
    Qwant, Reddit, SemanticScholar, SensCritique, SepiaSearch, SolidTorrents, Soundcloud, Sqlite,
    SqliteConfig, Stackexchange, Startpage, Swisscows, SwisscowsConfig, Tootfinder, Translate,
    Unsplash, Vimeo, Weather, Wikibooks, Wikidata, Wikipedia, Yacy, YacyConfig,
    builtin_generic_config,
};

/// Build a registry from settings using [`SearchSettings::engine_list_mode`](
/// zoeken_settings::SearchSettings::engine_list_mode).
pub fn registry_from_settings(settings: &Settings) -> EngineRegistry {
    if settings.engines.is_empty() {
        return EngineRegistry::from_engines(default_engines());
    }

    match settings.search.engine_list_mode {
        EngineListMode::Replace => registry_replace(settings),
        EngineListMode::Merge => registry_merge(settings),
    }
}

fn registry_replace(settings: &Settings) -> EngineRegistry {
    let mut registered = Vec::new();
    for cfg in &settings.engines {
        match engine_from_settings(cfg) {
            Some(re) => registered.push(apply_engine_settings(re, cfg)),
            None => {
                tracing::warn!(
                    engine = %cfg.name,
                    "settings.engines references an engine with no built-in implementation; ignoring"
                );
            }
        }
    }

    EngineRegistry::from_engines(registered)
}

fn registry_merge(settings: &Settings) -> EngineRegistry {
    let mut registered = default_engines();
    for cfg in &settings.engines {
        let key = cfg.engine.as_deref().unwrap_or(&cfg.name);
        let existing = registered
            .iter()
            .position(|re| re.name() == key || re.name() == cfg.name);

        match (existing, engine_from_settings(cfg)) {
            (Some(idx), Some(re)) => {
                registered[idx] = apply_engine_settings(re, cfg);
            }
            (Some(idx), None) => {
                registered[idx] = apply_engine_settings(registered[idx].clone(), cfg);
            }
            (None, Some(re)) => {
                registered.push(apply_engine_settings(re, cfg));
            }
            (None, None) => {
                tracing::warn!(
                    engine = %cfg.name,
                    "settings.engines references an engine with no built-in implementation; ignoring"
                );
            }
        }
    }

    EngineRegistry::from_engines(registered)
}

fn default_engines() -> Vec<RegisteredEngine> {
    // Keep the default set small and fast. Everything else stays available but
    // disabled until an operator/user enables it (SearXNG-style).
    const ENABLED_BY_DEFAULT: &[&str] = &[
        "duckduckgo",
        "brave",
        "bing",
        "mojeek",
        "dogpile",
        "wikipedia",
        "wikidata",
        "openverse",
        "unsplash",
        "bing_images",
        "peertube",
        "dailymotion",
        "sepiasearch",
        "openstreetmap",
        "weather",
        "currency",
        "dictionary",
        "translate",
        "swisscows news",
    ];
    let engines: Vec<std::sync::Arc<dyn zoeken_engine_core::Engine>> = vec![
        std::sync::Arc::new(Weather::new()),
        std::sync::Arc::new(Currency::new()),
        std::sync::Arc::new(Dictionary::new()),
        std::sync::Arc::new(Translate::new()),
        std::sync::Arc::new(
            Swisscows::new(SwisscowsConfig {
                base_url: "https://api.swisscows.com".to_string(),
                swisscows_category: "news".to_string(),
                results_per_page: 10,
            })
            .unwrap_or_default(),
        ),
        std::sync::Arc::new(DuckDuckGo::new()),
        std::sync::Arc::new(Google::new()),
        std::sync::Arc::new(Bing::new()),
        std::sync::Arc::new(Brave::new()),
        std::sync::Arc::new(Startpage::new()),
        std::sync::Arc::new(Mojeek::new()),
        std::sync::Arc::new(Qwant::new()),
        std::sync::Arc::new(Dogpile::default()),
        std::sync::Arc::new(Swisscows::default()),
        std::sync::Arc::new(Wikipedia::new()),
        std::sync::Arc::new(Wikidata::new()),
        std::sync::Arc::new(Wikibooks::new()),
        std::sync::Arc::new(Arxiv::new()),
        std::sync::Arc::new(Crates::new()),
        std::sync::Arc::new(DockerHub::new()),
        std::sync::Arc::new(Github::new()),
        std::sync::Arc::new(Gitlab::new()),
        std::sync::Arc::new(Pypi::new()),
        std::sync::Arc::new(Hackernews::new()),
        std::sync::Arc::new(Reddit::new()),
        std::sync::Arc::new(Lemmy::new()),
        std::sync::Arc::new(Mastodon::accounts()),
        std::sync::Arc::new(Stackexchange::stackoverflow()),
        std::sync::Arc::new(Bandcamp::new()),
        std::sync::Arc::new(Soundcloud::new()),
        std::sync::Arc::new(Openverse::new()),
        std::sync::Arc::new(BingImages::new()),
        std::sync::Arc::new(SepiaSearch::new()),
        std::sync::Arc::new(Openstreetmap::new()),
        std::sync::Arc::new(Peertube::new()),
        std::sync::Arc::new(Dailymotion::new()),
        std::sync::Arc::new(Unsplash::new()),
        std::sync::Arc::new(Genius::new()),
        std::sync::Arc::new(SemanticScholar::new()),
        std::sync::Arc::new(Crossref::new()),
        std::sync::Arc::new(Piratebay::new()),
        std::sync::Arc::new(Nyaa::new()),
        std::sync::Arc::new(SolidTorrents::new()),
        std::sync::Arc::new(Photon::new()),
        std::sync::Arc::new(Imdb::new()),
        std::sync::Arc::new(AppleAppStore::new()),
        std::sync::Arc::new(Tootfinder::new()),
        std::sync::Arc::new(SensCritique::new()),
        std::sync::Arc::new(NineGag::new()),
    ];
    engines
        .into_iter()
        .map(|engine| {
            let re = RegisteredEngine::new(engine);
            if ENABLED_BY_DEFAULT.contains(&re.name()) {
                re
            } else {
                re.disabled()
            }
        })
        .collect()
}

fn engine_from_settings(cfg: &zoeken_settings::EngineSettings) -> Option<RegisteredEngine> {
    let key = cfg.engine.as_deref().unwrap_or(&cfg.name);
    let base_url = engine_extra_string(cfg, "base_url");
    let engine: std::sync::Arc<dyn zoeken_engine_core::Engine> = match key {
        "duckduckgo" => std::sync::Arc::new(DuckDuckGo::new()),
        "google" => std::sync::Arc::new(Google::new()),
        "bing" => std::sync::Arc::new(Bing::new()),
        "bing_images" | "bing images" => std::sync::Arc::new(BingImages::new()),
        "brave" => std::sync::Arc::new(Brave::new()),
        "startpage" => std::sync::Arc::new(Startpage::new()),
        "mojeek" => std::sync::Arc::new(Mojeek::new()),
        "qwant" => std::sync::Arc::new(Qwant::new()),
        "dogpile" => {
            let engine = engine_config_from_settings::<DogpileConfig>(cfg)
                .and_then(|config| Dogpile::new(config).ok())
                .unwrap_or_default();
            std::sync::Arc::new(engine)
        }
        "swisscows" | "swisscows_news" | "swisscows news" => {
            let mut config =
                engine_config_from_settings::<SwisscowsConfig>(cfg).unwrap_or_else(|| {
                    SwisscowsConfig {
                        base_url: "https://api.swisscows.com".to_string(),
                        swisscows_category: "web".to_string(),
                        results_per_page: 20,
                    }
                });
            if matches!(key, "swisscows_news" | "swisscows news") {
                config.swisscows_category = "news".to_string();
            }
            let engine = Swisscows::new(config).unwrap_or_default();
            std::sync::Arc::new(engine)
        }
        "marginalia" => {
            let config = engine_config_from_settings::<MarginaliaConfig>(cfg)?;
            std::sync::Arc::new(Marginalia::new(config).ok()?)
        }
        "wikipedia" => std::sync::Arc::new(Wikipedia::new()),
        "wikidata" => std::sync::Arc::new(Wikidata::new()),
        "wikibooks" => std::sync::Arc::new(Wikibooks::new()),
        "arxiv" => std::sync::Arc::new(Arxiv::new()),
        "crates" => std::sync::Arc::new(Crates::new()),
        "docker_hub" | "docker hub" => std::sync::Arc::new(DockerHub::new()),
        "github" => std::sync::Arc::new(Github::new()),
        "gitlab" => {
            let engine = base_url
                .map(|url| Gitlab::new().with_base_url(url))
                .unwrap_or_default();
            std::sync::Arc::new(engine)
        }
        "pypi" => std::sync::Arc::new(Pypi::new()),
        "hackernews" | "hacker news" => std::sync::Arc::new(Hackernews::new()),
        "reddit" => std::sync::Arc::new(Reddit::new()),
        "lemmy" => {
            let engine = base_url
                .map(|url| Lemmy::new().with_base_url(url))
                .unwrap_or_default();
            std::sync::Arc::new(engine)
        }
        "mastodon" | "mastodon users" => std::sync::Arc::new(Mastodon::accounts()),
        "mastodon hashtags" => std::sync::Arc::new(Mastodon::hashtags()),
        "stackoverflow" | "stackexchange" => std::sync::Arc::new(Stackexchange::stackoverflow()),
        "askubuntu" => std::sync::Arc::new(Stackexchange::askubuntu()),
        "superuser" => std::sync::Arc::new(Stackexchange::superuser()),
        "bandcamp" => std::sync::Arc::new(Bandcamp::new()),
        "soundcloud" => std::sync::Arc::new(Soundcloud::new()),
        "openverse" => std::sync::Arc::new(Openverse::new()),
        "sepiasearch" | "sepia search" => std::sync::Arc::new(SepiaSearch::new()),
        "openstreetmap" | "openstreetmap search" => std::sync::Arc::new(Openstreetmap::new()),
        "piped" => std::sync::Arc::new(Piped::videos()),
        "piped.music" | "piped music" => std::sync::Arc::new(Piped::music()),
        "invidious" => {
            let engine = base_url
                .map(|url| Invidious::new().with_base_url(url))
                .unwrap_or_default();
            std::sync::Arc::new(engine)
        }
        "peertube" => std::sync::Arc::new(Peertube::new()),
        "dailymotion" => std::sync::Arc::new(Dailymotion::new()),
        "vimeo" => std::sync::Arc::new(Vimeo::new()),
        "weather" | "wttr.in" | "wttr" => std::sync::Arc::new(Weather::new()),
        "currency" | "currency_convert" => std::sync::Arc::new(Currency::new()),
        "dictionary" | "wiktionary_define" => std::sync::Arc::new(Dictionary::new()),
        "translate" | "mymemory" | "mymemory translated" => std::sync::Arc::new(Translate::new()),
        "unsplash" => std::sync::Arc::new(Unsplash::new()),
        "genius" => std::sync::Arc::new(Genius::new()),
        "semantic scholar" | "semantic_scholar" => std::sync::Arc::new(SemanticScholar::new()),
        "crossref" => std::sync::Arc::new(Crossref::new()),
        "core.ac.uk" | "core" => {
            let engine = engine_extra_string(cfg, "api_key")
                .map(Core::with_api_key)
                .unwrap_or_default();
            std::sync::Arc::new(engine)
        }
        "github code" | "github_code" => std::sync::Arc::new(GithubCode::new()),
        "piratebay" => std::sync::Arc::new(Piratebay::new()),
        "nyaa" => std::sync::Arc::new(Nyaa::new()),
        "solidtorrents" => {
            let engine = base_url
                .map(SolidTorrents::with_base_url)
                .unwrap_or_default();
            std::sync::Arc::new(engine)
        }
        "photon" => std::sync::Arc::new(Photon::new()),
        "imdb" => std::sync::Arc::new(Imdb::new()),
        "apple_app_store" | "apple app store" => std::sync::Arc::new(AppleAppStore::new()),
        "tootfinder" => std::sync::Arc::new(Tootfinder::new()),
        "senscritique" => std::sync::Arc::new(SensCritique::new()),
        "9gag" => std::sync::Arc::new(NineGag::new()),
        "yacy" => {
            let config = engine_config_from_settings::<YacyConfig>(cfg)?;
            std::sync::Arc::new(Yacy::new(config).ok()?)
        }
        "elasticsearch" => {
            let config = engine_config_from_settings::<ElasticsearchConfig>(cfg)?;
            std::sync::Arc::new(Elasticsearch::new(config).ok()?)
        }
        "meilisearch" => {
            let config = engine_config_from_settings::<MeilisearchConfig>(cfg)?;
            std::sync::Arc::new(Meilisearch::new(config).ok()?)
        }
        "sqlite" => {
            let config = engine_config_from_settings::<SqliteConfig>(cfg)?;
            std::sync::Arc::new(Sqlite::new(config).ok()?)
        }
        "xpath" | "html" | "generic_xpath" => {
            let config = generic_html_config_from_settings(cfg)?;
            std::sync::Arc::new(GenericHtmlEngine::new(config).ok()?)
        }
        "json_engine" | "json" | "generic_json" => {
            let config = generic_json_config_from_settings(cfg)?;
            std::sync::Arc::new(GenericJsonEngine::new(config).ok()?)
        }
        _ => match builtin_generic_config(key)? {
            GenericEngineConfig::Html(config) => {
                std::sync::Arc::new(GenericHtmlEngine::new(config).ok()?)
            }
            GenericEngineConfig::Json(config) => {
                std::sync::Arc::new(GenericJsonEngine::new(config).ok()?)
            }
        },
    };
    Some(RegisteredEngine::new(engine))
}

fn generic_html_config_from_settings(
    cfg: &zoeken_settings::EngineSettings,
) -> Option<GenericHtmlConfig> {
    let value = generic_config_value(cfg)?;
    serde_json::from_value(value).ok()
}

fn generic_json_config_from_settings(
    cfg: &zoeken_settings::EngineSettings,
) -> Option<GenericJsonConfig> {
    let value = generic_config_value(cfg)?;
    serde_json::from_value(value).ok()
}

fn engine_config_from_settings<T: DeserializeOwned>(
    cfg: &zoeken_settings::EngineSettings,
) -> Option<T> {
    let value = serde_json::to_value(&cfg.extra).ok()?;
    serde_json::from_value(value).ok()
}

fn generic_config_value(cfg: &zoeken_settings::EngineSettings) -> Option<serde_json::Value> {
    let mut value = serde_json::to_value(&cfg.extra).ok()?;
    let serde_json::Value::Object(ref mut map) = value else {
        return None;
    };
    map.entry("name".to_string())
        .or_insert_with(|| serde_json::Value::String(cfg.name.clone()));
    if let Some(shortcut) = &cfg.shortcut {
        map.entry("shortcut".to_string())
            .or_insert_with(|| serde_json::Value::String(shortcut.clone()));
    }
    if let Some(categories) = &cfg.categories {
        map.entry("categories".to_string())
            .or_insert_with(|| serde_json::json!(categories_to_vec(categories)));
    }
    normalize_generic_aliases(map);
    Some(value)
}

fn normalize_generic_aliases(map: &mut serde_json::Map<String, serde_json::Value>) {
    for (from, to) in [
        ("url", "search_url"),
        ("search_url_get", "search_url"),
        ("results_query", "result_css"),
        ("url_query", "url_css"),
        ("title_query", "title_css"),
        ("content_query", "content_css"),
    ] {
        if let Some(value) = map.get(from).cloned() {
            map.entry(to.to_string()).or_insert(value);
        }
    }
}

/// Apply a single `settings.engines` entry's overrides onto a built-in engine.
fn apply_engine_settings(
    mut re: RegisteredEngine,
    cfg: &zoeken_settings::EngineSettings,
) -> RegisteredEngine {
    if cfg.disabled == Some(true) || cfg.inactive == Some(true) {
        re = re.disabled();
    }
    if let Some(shortcut) = &cfg.shortcut {
        re = re.with_shortcut(shortcut.clone());
    }
    if let Some(tokens) = &cfg.tokens {
        re = re.with_tokens(tokens.clone());
    }
    if let Some(timeout) = cfg.timeout {
        if timeout.is_finite() && timeout > 0.0 {
            re = re.with_timeout(Duration::from_secs_f64(timeout));
        }
    }
    if let Some(weight) = cfg.weight {
        re = re.with_weight(weight);
    }
    if let Some(categories) = &cfg.categories {
        re = re.with_categories(categories_to_vec(categories));
    }
    re
}

fn engine_extra_string(cfg: &zoeken_settings::EngineSettings, key: &str) -> Option<String> {
    cfg.extra
        .get(key)
        .and_then(|value| value.as_str())
        .map(str::to_string)
}

/// Flatten a `StringOrVec` engine `categories` setting into a plain list.
fn categories_to_vec(categories: &zoeken_settings::StringOrVec) -> Vec<String> {
    match categories {
        zoeken_settings::StringOrVec::One(one) => vec![one.clone()],
        zoeken_settings::StringOrVec::Many(many) => many.clone(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use zoeken_settings::EngineListMode;

    fn find<'a>(reg: &'a EngineRegistry, name: &str) -> &'a RegisteredEngine {
        reg.engines()
            .iter()
            .find(|re| re.name() == name)
            .unwrap_or_else(|| panic!("engine `{name}` present in registry"))
    }

    #[test]
    fn build_registry_defaults_when_no_engine_settings() {
        let reg = registry_from_settings(&Settings::default());
        assert_eq!(reg.engines().len(), default_engines().len());
        let enabled: Vec<_> = reg
            .engines()
            .iter()
            .filter(|re| !re.disabled)
            .map(|re| re.name())
            .collect();
        assert_eq!(
            enabled,
            vec![
                "weather",
                "currency",
                "dictionary",
                "translate",
                "swisscows news",
                "duckduckgo",
                "bing",
                "brave",
                "mojeek",
                "dogpile",
                "wikipedia",
                "wikidata",
                "openverse",
                "bing_images",
                "sepiasearch",
                "openstreetmap",
                "peertube",
                "dailymotion",
                "unsplash",
            ]
        );
    }

    #[test]
    fn build_registry_from_settings_applies_overrides() {
        let settings = Settings {
            engines: vec![
                zoeken_settings::EngineSettings {
                    name: "google".to_string(),
                    disabled: Some(true),
                    ..Default::default()
                },
                zoeken_settings::EngineSettings {
                    name: "bing".to_string(),
                    tokens: Some(vec!["secret".to_string()]),
                    timeout: Some(2.5),
                    weight: Some(3.0),
                    categories: Some(zoeken_settings::StringOrVec::One("images".to_string())),
                    shortcut: Some("bi".to_string()),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), 2);

        let google = find(&reg, "google");
        assert!(google.disabled, "google disabled by settings");

        let bing = find(&reg, "bing");
        assert_eq!(bing.tokens, vec!["secret".to_string()]);
        assert_eq!(bing.timeout, Some(Duration::from_secs_f64(2.5)));
        assert_eq!(bing.weight, Some(3.0));
        assert_eq!(bing.categories, Some(vec!["images".to_string()]));
        assert_eq!(bing.shortcuts, vec!["bi".to_string()]);
    }

    #[test]
    fn build_registry_replace_mode_is_default_for_nonempty_engines() {
        let settings = Settings {
            engines: vec![zoeken_settings::EngineSettings {
                name: "gitlab".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        assert_eq!(settings.search.engine_list_mode, EngineListMode::Replace);
        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), 1);
        assert_eq!(reg.engines()[0].name(), "gitlab");
        assert!(
            reg.engines().iter().all(|re| re.name() != "duckduckgo"),
            "replace mode must drop defaults not listed in engines:"
        );
    }

    #[test]
    fn build_registry_merge_mode_overlays_defaults() {
        let mut marginalia = zoeken_settings::EngineSettings {
            name: "marginalia".to_string(),
            engine: Some("marginalia".to_string()),
            ..Default::default()
        };
        marginalia.extra.insert(
            "api_key".to_string(),
            serde_yaml_ng::Value::String("test-key".to_string()),
        );

        let settings = Settings {
            search: zoeken_settings::SearchSettings {
                engine_list_mode: EngineListMode::Merge,
                ..Default::default()
            },
            engines: vec![
                zoeken_settings::EngineSettings {
                    name: "duckduckgo".to_string(),
                    disabled: Some(true),
                    shortcut: Some("ddg".to_string()),
                    ..Default::default()
                },
                marginalia,
            ],
            ..Default::default()
        };

        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), default_engines().len() + 1);

        let ddg = find(&reg, "duckduckgo");
        assert!(ddg.disabled);
        assert_eq!(ddg.shortcuts, vec!["ddg".to_string()]);

        assert!(
            reg.engines().iter().any(|re| re.name() == "brave"),
            "merge keeps defaults not mentioned in engines:"
        );
        assert!(reg.engines().iter().any(|re| re.name() == "marginalia"));
    }

    #[test]
    fn build_registry_merge_mode_can_disable_without_dropping_catalog() {
        let settings = Settings {
            search: zoeken_settings::SearchSettings {
                engine_list_mode: EngineListMode::Merge,
                ..Default::default()
            },
            engines: vec![zoeken_settings::EngineSettings {
                name: "bing".to_string(),
                disabled: Some(true),
                ..Default::default()
            }],
            ..Default::default()
        };
        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), default_engines().len());
        assert!(find(&reg, "bing").disabled);
        assert!(!find(&reg, "duckduckgo").disabled);
    }

    #[test]
    fn build_registry_skips_unknown_settings_engines() {
        let settings = Settings {
            engines: vec![
                zoeken_settings::EngineSettings {
                    name: "unknown".to_string(),
                    ..Default::default()
                },
                zoeken_settings::EngineSettings {
                    name: "gitlab".to_string(),
                    ..Default::default()
                },
            ],
            ..Default::default()
        };

        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), 1);
        assert_eq!(reg.engines()[0].name(), "gitlab");
    }

    #[test]
    fn build_registry_uses_engine_key_for_settings_named_entry() {
        let settings = Settings {
            engines: vec![zoeken_settings::EngineSettings {
                name: "private gitlab".to_string(),
                engine: Some("gitlab".to_string()),
                ..Default::default()
            }],
            ..Default::default()
        };

        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), 1);
        assert_eq!(reg.engines()[0].name(), "gitlab");
    }

    #[test]
    fn build_registry_instantiates_generic_xpath_engine_from_settings() {
        let settings: Settings = serde_yaml_ng::from_str(
            r#"
engines:
  - name: example html
    engine: xpath
    shortcut: ex
    categories: general
    base_url: https://example.test/
    search_url: https://example.test/search
    page_param: page
    paging: true
    results_xpath: //article
    xpath_title: .//a
    xpath_url: .//a/@href
    xpath_content: .//p
"#,
        )
        .expect("settings parse");
        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), 1);
        let engine = find(&reg, "example html");
        assert_eq!(engine.shortcuts, vec!["ex".to_string()]);
        assert_eq!(engine.categories, Some(vec!["general".to_string()]));
        assert!(engine.engine.metadata().paging);
    }

    #[test]
    fn build_registry_instantiates_generic_json_engine_from_settings() {
        let settings: Settings = serde_yaml_ng::from_str(
            r#"
engines:
  - name: example api
    engine: json_engine
    shortcut: ea
    categories: it
    base_url: https://api.example.test/
    search_url: https://api.example.test/search?q={query}&page={page}
    results_path: data.items
    title_path: name
    url_path: link
    content_path: summary
"#,
        )
        .expect("settings parse");
        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), 1);
        let engine = find(&reg, "example api");
        assert_eq!(engine.shortcuts, vec!["ea".to_string()]);
        assert_eq!(engine.categories, Some(vec!["it".to_string()]));
    }

    #[test]
    fn build_registry_instantiates_builtin_generic_catalog_engine() {
        let settings = Settings {
            engines: vec![zoeken_settings::EngineSettings {
                name: "abcnyheter".to_string(),
                ..Default::default()
            }],
            ..Default::default()
        };
        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), 1);
        let engine = find(&reg, "abcnyheter");
        assert!(engine.engine.metadata().paging);
        assert_eq!(
            engine.engine.metadata().categories,
            vec!["general".to_string()]
        );
    }

    #[test]
    fn build_registry_instantiates_database_engines_from_settings() {
        let settings: Settings = serde_yaml_ng::from_str(
            r#"
engines:
  - name: elastic local
    engine: elasticsearch
    index: docs
  - name: meili local
    engine: meilisearch
    index: docs
  - name: sqlite local
    engine: sqlite
    database: /tmp/zoeken-test.db
    query_str: SELECT title, url FROM docs WHERE title LIKE :wildcard
"#,
        )
        .expect("settings parse");

        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), 3);
        assert_eq!(reg.engines()[0].name(), "elasticsearch");
        assert_eq!(reg.engines()[1].name(), "meilisearch");
        assert_eq!(reg.engines()[2].name(), "sqlite");
    }

    #[test]
    fn build_registry_instantiates_yacy_from_settings() {
        let settings: Settings = serde_yaml_ng::from_str(
            r#"
engines:
  - name: yacy
    engine: yacy
    base_url: https://search.example.test
"#,
        )
        .expect("settings parse");

        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), 1);
        assert_eq!(reg.engines()[0].name(), "yacy");
    }

    #[test]
    fn build_registry_instantiates_phase_6_general_engines_from_settings() {
        let settings: Settings = serde_yaml_ng::from_str(
            r#"
engines:
  - name: dogpile
    engine: dogpile
    dogpile_categ: search
  - name: swisscows
    engine: swisscows
    swisscows_category: web
  - name: swisscows news
    engine: swisscows_news
  - name: marginalia
    engine: marginalia
    api_key: test-key
"#,
        )
        .expect("settings parse");

        let reg = registry_from_settings(&settings);
        assert_eq!(reg.engines().len(), 4);
        assert_eq!(reg.engines()[0].name(), "dogpile");
        assert_eq!(reg.engines()[1].name(), "swisscows");
        assert_eq!(reg.engines()[2].name(), "swisscows news");
        assert_eq!(reg.engines()[3].name(), "marginalia");
    }
}
