//! End-to-end tests for `load_settings`: merge behavior, validation, and
//! typed full-schema loading.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use zoeken_settings::{
    BoolOrString, EnvMap, IntOrString, Proxies, Settings, SettingsError, StringOrVec, load_settings,
};

/// Temporary YAML file removed when the guard is dropped.
struct TempYaml {
    path: PathBuf,
}

impl TempYaml {
    fn new(contents: &str) -> Self {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let n = COUNTER.fetch_add(1, Ordering::Relaxed);
        let path = std::env::temp_dir().join(format!(
            "zoeken_settings_unit_{}_{}.yml",
            std::process::id(),
            n
        ));
        std::fs::write(&path, contents).expect("write temp settings file");
        Self { path }
    }

    fn path(&self) -> &Path {
        &self.path
    }
}

impl Drop for TempYaml {
    fn drop(&mut self) {
        std::fs::remove_file(&self.path).ok();
    }
}

fn load_yaml(contents: &str) -> Result<Settings, SettingsError> {
    let file = TempYaml::new(contents);
    load_settings(Some(file.path()), &EnvMap::new())
}

#[test]
fn use_default_settings_true_merges_file_over_defaults() {
    let settings = load_yaml(
        "use_default_settings: true\n\
         general:\n\
         \x20 instance_name: My Instance\n\
         search:\n\
         \x20 autocomplete: duckduckgo\n",
    )
    .expect("merge over defaults");

    assert_eq!(settings.general.instance_name, "My Instance");
    assert_eq!(settings.search.autocomplete, "duckduckgo");
    assert!(!settings.general.debug);
    assert_eq!(settings.search.autocomplete_min, 1);
    assert_eq!(settings.search.ban_time_on_fail, 5.0);
    assert_eq!(settings.server.port, Some(IntOrString::Int(8888)));
    assert_eq!(settings.ui.default_theme, "simple");
}

#[test]
fn use_default_settings_true_deep_merges_nested_maps() {
    let settings = load_yaml(
        "use_default_settings: true\n\
         search:\n\
         \x20 suspended_times:\n\
         \x20   SearxEngineCaptcha: 10\n",
    )
    .expect("deep merge nested map");

    assert_eq!(settings.search.suspended_times.captcha, 10.0);
    assert_eq!(settings.search.suspended_times.access_denied, 86400.0);
    assert_eq!(settings.search.suspended_times.too_many_requests, 3600.0);
    assert_eq!(settings.search.suspended_times.recaptcha_captcha, 604800.0);
}

#[test]
fn use_default_settings_true_appends_file_engines() {
    let settings = load_yaml(
        "use_default_settings: true\n\
         engines:\n\
         \x20 - name: duckduckgo\n\
         \x20   engine: duckduckgo\n\
         \x20   shortcut: ddg\n",
    )
    .expect("merge engines");

    assert_eq!(settings.engines.len(), 1);
    assert_eq!(settings.engines[0].name, "duckduckgo");
    assert_eq!(settings.engines[0].shortcut.as_deref(), Some("ddg"));
}

#[test]
fn use_default_settings_mapping_form_is_honored_as_merge() {
    let settings = load_yaml(
        "use_default_settings:\n\
         \x20 engines:\n\
         \x20   keep_only:\n\
         \x20     - duckduckgo\n\
         general:\n\
         \x20 instance_name: Filtered\n",
    )
    .expect("mapping form merges");

    assert_eq!(settings.general.instance_name, "Filtered");
    assert_eq!(settings.ui.default_theme, "simple");
    assert_eq!(settings.server.method, "POST");
}

#[test]
fn absent_use_default_settings_replaces_but_backfills_omitted_sections() {
    let settings =
        load_yaml("general:\n  instance_name: Replaced\n").expect("replace mode backfills");

    assert_eq!(settings.general.instance_name, "Replaced");
    assert_eq!(settings.server.port, Some(IntOrString::Int(8888)));
    assert_eq!(settings.ui.default_theme, "simple");
    assert_eq!(settings.outgoing.request_timeout, 3.0);
}

#[test]
fn use_default_settings_false_replaces_defaults() {
    let settings = load_yaml(
        "use_default_settings: false\n\
         search:\n\
         \x20 autocomplete_min: 2\n",
    )
    .expect("false replaces");

    assert_eq!(settings.search.autocomplete_min, 2);
    assert_eq!(settings.search.ban_time_on_fail, 5.0);
    assert_eq!(settings.general.instance_name, "Search");
}

fn assert_validation_setting(yaml: &str, expected_setting: &str) {
    let err = load_yaml(yaml).expect_err("expected a validation error");
    match err {
        SettingsError::Validation { setting, .. } => assert_eq!(
            setting, expected_setting,
            "validation error named the wrong setting"
        ),
        other => panic!("expected SettingsError::Validation, got {other:?}"),
    }
}

#[test]
fn validation_names_invalid_safe_search() {
    assert_validation_setting("search:\n  safe_search: 3\n", "search.safe_search");
}

#[test]
fn validation_names_invalid_http_protocol_version() {
    assert_validation_setting(
        "server:\n  http_protocol_version: \"2.0\"\n",
        "server.http_protocol_version",
    );
}

#[test]
fn validation_names_invalid_method() {
    assert_validation_setting("server:\n  method: PUT\n", "server.method");
}

#[test]
fn validation_names_invalid_simple_style() {
    assert_validation_setting(
        "ui:\n  theme_args:\n    simple_style: neon\n",
        "ui.theme_args.simple_style",
    );
}

#[test]
fn validation_names_invalid_hotkeys() {
    assert_validation_setting("ui:\n  hotkeys: emacs\n", "ui.hotkeys");
}

#[test]
fn validation_names_invalid_url_formatting() {
    assert_validation_setting("ui:\n  url_formatting: raw\n", "ui.url_formatting");
}

#[test]
fn validation_names_unsupported_output_format() {
    assert_validation_setting("search:\n  formats:\n    - xml\n", "search.formats");
}

#[test]
fn validation_error_message_mentions_offending_value() {
    let err = load_yaml("server:\n  method: PATCH\n").expect_err("invalid method");
    let SettingsError::Validation { setting, message } = err else {
        panic!("expected a validation error");
    };
    assert_eq!(setting, "server.method");
    assert!(
        message.contains("PATCH"),
        "message should mention the offending value, got: {message}"
    );
}

const FULL_SCHEMA_YAML: &str = r#"
general:
  debug: true
  instance_name: "Full Instance"
  enable_metrics: false
brand:
  issue_url: "https://example.test/issues"
  docs_url: "https://example.test/docs"
  new_issue_url: "https://example.test/new"
search:
  safe_search: 1
  autocomplete: "google"
  autocomplete_min: 3
  default_lang: "en"
  formats:
    - html
    - json
  max_page: 5
server:
  port: 9999
  bind_address: "0.0.0.0"
  limiter: true
  secret_key: "s3cr3t"
  http_protocol_version: "1.1"
  method: "GET"
ui:
  default_theme: "simple"
  hotkeys: "vim"
  url_formatting: "full"
  theme_args:
    simple_style: "dark"
outgoing:
  request_timeout: 4.5
  enable_http2: false
  pool_connections: 42
  proxies: "http://proxy.test:8080"
  networks:
    tor:
      request_timeout: 10.0
      using_tor_proxy: true
engines:
  - name: duckduckgo
    engine: duckduckgo
    shortcut: ddg
    timeout: 5.0
    categories:
      - general
      - web
    disabled: false
    base_url: "https://duckduckgo.test"
plugins:
  hostnames.plugin:
    active: true
    only_show_green_hosts: true
categories_as_tabs:
  general: {}
  images: {}
  custom: {}
preferences:
  lock:
    - language
"#;

#[test]
fn full_schema_load_populates_general_brand_search_server_ui() {
    let s = load_yaml(FULL_SCHEMA_YAML).expect("full-schema load succeeds");

    assert!(s.general.debug);
    assert_eq!(s.general.instance_name, "Full Instance");
    assert!(!s.general.enable_metrics);

    assert_eq!(s.brand.issue_url, "https://example.test/issues");
    assert_eq!(s.brand.docs_url, "https://example.test/docs");
    assert_eq!(s.brand.new_issue_url, "https://example.test/new");

    assert_eq!(s.search.safe_search, 1);
    assert_eq!(s.search.autocomplete, "google");
    assert_eq!(s.search.autocomplete_min, 3);
    assert_eq!(s.search.default_lang, "en");
    assert_eq!(
        s.search.formats,
        vec!["html".to_string(), "json".to_string()]
    );
    assert_eq!(s.search.max_page, 5);

    assert_eq!(s.server.port, Some(IntOrString::Int(9999)));
    assert_eq!(s.server.bind_address, "0.0.0.0");
    assert!(s.server.limiter);
    assert_eq!(s.server.secret_key, "s3cr3t");
    assert_eq!(s.server.http_protocol_version, "1.1");
    assert_eq!(s.server.method, "GET");

    assert_eq!(s.ui.default_theme, "simple");
    assert_eq!(s.ui.hotkeys, "vim");
    assert_eq!(s.ui.url_formatting, "full");
    assert_eq!(s.ui.theme_args.simple_style, "dark");
}

#[test]
fn full_schema_load_populates_outgoing_and_networks() {
    let s = load_yaml(FULL_SCHEMA_YAML).expect("full-schema load succeeds");

    assert_eq!(s.outgoing.request_timeout, 4.5);
    assert!(!s.outgoing.enable_http2);
    assert_eq!(s.outgoing.pool_connections, 42);
    assert_eq!(
        s.outgoing.proxies,
        Some(Proxies::Single("http://proxy.test:8080".to_string()))
    );

    let tor = s.outgoing.networks.get("tor").expect("tor network present");
    assert_eq!(tor.request_timeout, Some(10.0));
    assert_eq!(tor.using_tor_proxy, Some(true));
}

#[test]
fn full_schema_load_populates_engines() {
    let s = load_yaml(FULL_SCHEMA_YAML).expect("full-schema load succeeds");

    assert_eq!(s.engines.len(), 1);
    let engine = &s.engines[0];
    assert_eq!(engine.name, "duckduckgo");
    assert_eq!(engine.engine.as_deref(), Some("duckduckgo"));
    assert_eq!(engine.shortcut.as_deref(), Some("ddg"));
    assert_eq!(engine.timeout, Some(5.0));
    assert_eq!(engine.disabled, Some(false));
    assert_eq!(
        engine.categories,
        Some(StringOrVec::Many(vec![
            "general".to_string(),
            "web".to_string()
        ]))
    );
    assert!(engine.extra.contains_key("base_url"));
}

#[test]
fn full_schema_load_populates_plugins_and_categories() {
    let s = load_yaml(FULL_SCHEMA_YAML).expect("full-schema load succeeds");

    let plugin = s
        .plugins
        .0
        .get("hostnames.plugin")
        .expect("plugin entry present");
    assert_eq!(plugin.active, Some(true));
    assert!(plugin.extra.contains_key("only_show_green_hosts"));

    assert!(s.categories.0.contains_key("general"));
    assert!(s.categories.0.contains_key("images"));
    assert!(s.categories.0.contains_key("custom"));

    assert_eq!(s.preferences.lock, vec!["language".to_string()]);
}

#[test]
fn full_schema_load_covers_every_top_level_section() {
    let s = load_yaml(FULL_SCHEMA_YAML).expect("full-schema load succeeds");

    assert_eq!(s.general.instance_name, "Full Instance");
    assert_eq!(s.brand.docs_url, "https://example.test/docs");
    assert_eq!(s.search.autocomplete, "google");
    assert_eq!(s.server.bind_address, "0.0.0.0");
    assert_eq!(s.ui.default_theme, "simple");
    assert_eq!(s.outgoing.request_timeout, 4.5);
    assert!(!s.outgoing.networks.is_empty());
    assert!(!s.engines.is_empty());
    assert!(!s.plugins.0.is_empty());
    assert!(!s.categories.0.is_empty());

    assert_eq!(s.server.base_url, Some(BoolOrString::Bool(false)));
}

#[test]
fn packaged_debian_settings_yml_loads() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../packaging/debian/zoeken.settings.yml");
    let settings = load_settings(Some(&path), &EnvMap::new())
        .expect("packaging/debian/zoeken.settings.yml must parse");

    assert_eq!(settings.general.instance_name, "Zoeken");
    assert_eq!(settings.server.bind_address, "127.0.0.1");
    assert_eq!(settings.server.port, Some(IntOrString::Int(8888)));
    assert_eq!(
        settings.limiter.get("file").and_then(|v| v.as_str()),
        Some("/etc/zoeken/limiter.toml")
    );
    assert_eq!(
        settings.lua_plugins.directory.as_deref(),
        Some("/usr/share/zoeken/plugins")
    );
    assert!(
        settings.engines.is_empty(),
        "empty engines → built-in catalog"
    );
    assert!(settings.plugins.0.contains_key("calculator"));
    assert_eq!(settings.search.safe_search, 0);
    assert_eq!(settings.outgoing.request_timeout, 3.0);
    assert!(settings.deployment.metrics_enabled);
}
