// File overlay property: `use_default_settings: true` deep-merges onto defaults.
// The generated YAML only varies unvalidated scalars so the property stays
// focused on merge semantics.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use proptest::prelude::*;
use serde_yaml_ng::{Mapping, Number, Value};
use zoeken_settings::{EnvMap, Settings, load_settings};

/// Optional overrides: `Some(v)` writes the key, `None` omits it.
#[derive(Debug, Clone)]
struct Overrides {
    debug: Option<bool>,
    instance_name: Option<String>,
    enable_metrics: Option<bool>,
    autocomplete_min: Option<u32>,
    max_page: Option<u32>,
    ban_time_on_fail: Option<f64>,
    bind_address: Option<String>,
    limiter: Option<bool>,
    public_instance: Option<bool>,
    image_proxy: Option<bool>,
    secret_key: Option<String>,
    default_theme: Option<String>,
    center_alignment: Option<bool>,
    results_on_new_tab: Option<bool>,
    request_timeout: Option<f64>,
    pool_connections: Option<u32>,
    max_redirects: Option<u32>,
    retries: Option<u32>,
    enable_http2: Option<bool>,
}

/// Printable strings that round-trip cleanly through YAML.
fn safe_string() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9 ._:/-]{0,24}"
}

/// Finite `f64` values that serialize and deserialize exactly.
fn finite_f64() -> impl Strategy<Value = f64> {
    0.0f64..1_000_000.0f64
}

fn overrides_strategy() -> impl Strategy<Value = Overrides> {
    (
        (
            proptest::option::of(any::<bool>()),
            proptest::option::of(safe_string()),
            proptest::option::of(any::<bool>()),
            proptest::option::of(0u32..=100_000),
            proptest::option::of(0u32..=100_000),
            proptest::option::of(finite_f64()),
        ),
        (
            proptest::option::of(safe_string()),
            proptest::option::of(any::<bool>()),
            proptest::option::of(any::<bool>()),
            proptest::option::of(any::<bool>()),
            proptest::option::of(safe_string()),
        ),
        (
            proptest::option::of(safe_string()),
            proptest::option::of(any::<bool>()),
            proptest::option::of(any::<bool>()),
            proptest::option::of(finite_f64()),
            proptest::option::of(0u32..=100_000),
            proptest::option::of(0u32..=100_000),
            proptest::option::of(0u32..=100_000),
            proptest::option::of(any::<bool>()),
        ),
    )
        .prop_map(
            |(
                (
                    debug,
                    instance_name,
                    enable_metrics,
                    autocomplete_min,
                    max_page,
                    ban_time_on_fail,
                ),
                (bind_address, limiter, public_instance, image_proxy, secret_key),
                (
                    default_theme,
                    center_alignment,
                    results_on_new_tab,
                    request_timeout,
                    pool_connections,
                    max_redirects,
                    retries,
                    enable_http2,
                ),
            )| Overrides {
                debug,
                instance_name,
                enable_metrics,
                autocomplete_min,
                max_page,
                ban_time_on_fail,
                bind_address,
                limiter,
                public_instance,
                image_proxy,
                secret_key,
                default_theme,
                center_alignment,
                results_on_new_tab,
                request_timeout,
                pool_connections,
                max_redirects,
                retries,
                enable_http2,
            },
        )
}

fn bool_val(b: bool) -> Value {
    Value::Bool(b)
}
fn u32_val(n: u32) -> Value {
    Value::Number(Number::from(n))
}
fn f64_val(x: f64) -> Value {
    Value::Number(Number::from(x))
}
fn str_val(s: &str) -> Value {
    Value::String(s.to_string())
}

fn put<T>(section: &mut Mapping, key: &str, opt: &Option<T>, to_value: impl Fn(&T) -> Value) {
    if let Some(v) = opt {
        section.insert(str_val(key), to_value(v));
    }
}

/// Build the partial settings file as YAML text.
fn build_file_yaml(ov: &Overrides) -> String {
    let mut general = Mapping::new();
    put(&mut general, "debug", &ov.debug, |v| bool_val(*v));
    put(&mut general, "instance_name", &ov.instance_name, |v| {
        str_val(v)
    });
    put(&mut general, "enable_metrics", &ov.enable_metrics, |v| {
        bool_val(*v)
    });

    let mut search = Mapping::new();
    put(&mut search, "autocomplete_min", &ov.autocomplete_min, |v| {
        u32_val(*v)
    });
    put(&mut search, "max_page", &ov.max_page, |v| u32_val(*v));
    put(&mut search, "ban_time_on_fail", &ov.ban_time_on_fail, |v| {
        f64_val(*v)
    });

    let mut server = Mapping::new();
    put(&mut server, "bind_address", &ov.bind_address, |v| {
        str_val(v)
    });
    put(&mut server, "limiter", &ov.limiter, |v| bool_val(*v));
    put(&mut server, "public_instance", &ov.public_instance, |v| {
        bool_val(*v)
    });
    put(&mut server, "image_proxy", &ov.image_proxy, |v| {
        bool_val(*v)
    });
    put(&mut server, "secret_key", &ov.secret_key, |v| str_val(v));

    let mut ui = Mapping::new();
    put(&mut ui, "default_theme", &ov.default_theme, |v| str_val(v));
    put(&mut ui, "center_alignment", &ov.center_alignment, |v| {
        bool_val(*v)
    });
    put(&mut ui, "results_on_new_tab", &ov.results_on_new_tab, |v| {
        bool_val(*v)
    });

    let mut outgoing = Mapping::new();
    put(&mut outgoing, "request_timeout", &ov.request_timeout, |v| {
        f64_val(*v)
    });
    put(
        &mut outgoing,
        "pool_connections",
        &ov.pool_connections,
        |v| u32_val(*v),
    );
    put(&mut outgoing, "max_redirects", &ov.max_redirects, |v| {
        u32_val(*v)
    });
    put(&mut outgoing, "retries", &ov.retries, |v| u32_val(*v));
    put(&mut outgoing, "enable_http2", &ov.enable_http2, |v| {
        bool_val(*v)
    });

    let mut root = Mapping::new();
    root.insert(str_val("use_default_settings"), Value::Bool(true));
    if !general.is_empty() {
        root.insert(str_val("general"), Value::Mapping(general));
    }
    if !search.is_empty() {
        root.insert(str_val("search"), Value::Mapping(search));
    }
    if !server.is_empty() {
        root.insert(str_val("server"), Value::Mapping(server));
    }
    if !ui.is_empty() {
        root.insert(str_val("ui"), Value::Mapping(ui));
    }
    if !outgoing.is_empty() {
        root.insert(str_val("outgoing"), Value::Mapping(outgoing));
    }

    serde_yaml_ng::to_string(&Value::Mapping(root)).expect("serialize partial settings file")
}

fn write_temp_yaml(contents: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "zoeken_settings_overlay_prop_{}_{}.yml",
        std::process::id(),
        n
    ));
    std::fs::write(&path, contents).expect("write temp settings file");
    path
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn settings_file_overlays_defaults(ov in overrides_strategy()) {
        let defaults = Settings::defaults();

        let yaml = build_file_yaml(&ov);
        let path = write_temp_yaml(&yaml);
        let loaded = load_settings(Some(&path), &EnvMap::new());
        std::fs::remove_file(&path).ok();

        let merged = loaded.expect("partial overlay must load successfully");

        prop_assert_eq!(merged.general.debug, ov.debug.unwrap_or(defaults.general.debug));
        prop_assert_eq!(
            &merged.general.instance_name,
            ov.instance_name.as_ref().unwrap_or(&defaults.general.instance_name)
        );
        prop_assert_eq!(
            merged.general.enable_metrics,
            ov.enable_metrics.unwrap_or(defaults.general.enable_metrics)
        );

        prop_assert_eq!(
            merged.search.autocomplete_min,
            ov.autocomplete_min.unwrap_or(defaults.search.autocomplete_min)
        );
        prop_assert_eq!(merged.search.max_page, ov.max_page.unwrap_or(defaults.search.max_page));
        prop_assert_eq!(
            merged.search.ban_time_on_fail,
            ov.ban_time_on_fail.unwrap_or(defaults.search.ban_time_on_fail)
        );

        prop_assert_eq!(
            &merged.server.bind_address,
            ov.bind_address.as_ref().unwrap_or(&defaults.server.bind_address)
        );
        prop_assert_eq!(merged.server.limiter, ov.limiter.unwrap_or(defaults.server.limiter));
        prop_assert_eq!(
            merged.server.public_instance,
            ov.public_instance.unwrap_or(defaults.server.public_instance)
        );
        prop_assert_eq!(
            merged.server.image_proxy,
            ov.image_proxy.unwrap_or(defaults.server.image_proxy)
        );
        prop_assert_eq!(
            &merged.server.secret_key,
            ov.secret_key.as_ref().unwrap_or(&defaults.server.secret_key)
        );

        prop_assert_eq!(
            &merged.ui.default_theme,
            ov.default_theme.as_ref().unwrap_or(&defaults.ui.default_theme)
        );
        prop_assert_eq!(
            merged.ui.center_alignment,
            ov.center_alignment.unwrap_or(defaults.ui.center_alignment)
        );
        prop_assert_eq!(
            merged.ui.results_on_new_tab,
            ov.results_on_new_tab.unwrap_or(defaults.ui.results_on_new_tab)
        );

        prop_assert_eq!(
            merged.outgoing.request_timeout,
            ov.request_timeout.unwrap_or(defaults.outgoing.request_timeout)
        );
        prop_assert_eq!(
            merged.outgoing.pool_connections,
            ov.pool_connections.unwrap_or(defaults.outgoing.pool_connections)
        );
        prop_assert_eq!(
            merged.outgoing.max_redirects,
            ov.max_redirects.unwrap_or(defaults.outgoing.max_redirects)
        );
        prop_assert_eq!(merged.outgoing.retries, ov.retries.unwrap_or(defaults.outgoing.retries));
        prop_assert_eq!(
            merged.outgoing.enable_http2,
            ov.enable_http2.unwrap_or(defaults.outgoing.enable_http2)
        );
    }
}
