// Property-based tests for per-engine attribute overlay.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use proptest::prelude::*;
use serde_yaml_ng::{Mapping, Number, Sequence, Value};
use zoeken_settings::{EnvMap, StringOrVec, load_settings};

const ENGINE_NAME: &str = "testengine";
#[derive(Debug, Clone)]
struct BaseEngine {
    shortcut: String,
    timeout: f64,
    categories: String,
    disabled: bool,
    weight: f64,
    tokens: Vec<String>,
}

/// Partial overlay: for each attribute, `Some(v)` means the overlay names it
/// (and it must replace the base value); `None` means the overlay omits it (and
/// the base value must be preserved).
#[derive(Debug, Clone)]
struct OverlayEngine {
    shortcut: Option<String>,
    timeout: Option<f64>,
    categories: Option<String>,
    disabled: Option<bool>,
    weight: Option<f64>,
    tokens: Option<Vec<String>>,
}

/// A short identifier-like string that round-trips cleanly through YAML.
fn ident() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,7}"
}

/// A finite, well-behaved float. `ryu`-based YAML float formatting guarantees an
/// exact round-trip for finite `f64`, so exact equality is valid on the way back.
fn finite_f64() -> impl Strategy<Value = f64> {
    0.0f64..1_000.0f64
}

/// 0..4 short tokens.
fn tokens() -> impl Strategy<Value = Vec<String>> {
    proptest::collection::vec(ident(), 0..4)
}

fn base_strategy() -> impl Strategy<Value = BaseEngine> {
    (
        ident(),
        finite_f64(),
        ident(),
        any::<bool>(),
        finite_f64(),
        tokens(),
    )
        .prop_map(
            |(shortcut, timeout, categories, disabled, weight, tokens)| BaseEngine {
                shortcut,
                timeout,
                categories,
                disabled,
                weight,
                tokens,
            },
        )
}

fn overlay_strategy() -> impl Strategy<Value = OverlayEngine> {
    (
        proptest::option::of(ident()),
        proptest::option::of(finite_f64()),
        proptest::option::of(ident()),
        proptest::option::of(any::<bool>()),
        proptest::option::of(finite_f64()),
        proptest::option::of(tokens()),
    )
        .prop_map(
            |(shortcut, timeout, categories, disabled, weight, tokens)| OverlayEngine {
                shortcut,
                timeout,
                categories,
                disabled,
                weight,
                tokens,
            },
        )
}

fn str_val(s: &str) -> Value {
    Value::String(s.to_string())
}

fn tokens_val(ts: &[String]) -> Value {
    Value::Sequence(ts.iter().map(|t| str_val(t)).collect::<Sequence>())
}

/// Build the YAML engine map for the base entry (all attributes present).
fn base_engine_map(base: &BaseEngine) -> Value {
    let mut m = Mapping::new();
    m.insert(str_val("name"), str_val(ENGINE_NAME));
    m.insert(str_val("shortcut"), str_val(&base.shortcut));
    m.insert(
        str_val("timeout"),
        Value::Number(Number::from(base.timeout)),
    );
    m.insert(str_val("categories"), str_val(&base.categories));
    m.insert(str_val("disabled"), Value::Bool(base.disabled));
    m.insert(str_val("weight"), Value::Number(Number::from(base.weight)));
    m.insert(str_val("tokens"), tokens_val(&base.tokens));
    Value::Mapping(m)
}

/// Build the YAML engine map for the overlay entry (same `name`, plus only the
/// attributes the overlay names).
fn overlay_engine_map(ov: &OverlayEngine) -> Value {
    let mut m = Mapping::new();
    m.insert(str_val("name"), str_val(ENGINE_NAME));
    if let Some(v) = &ov.shortcut {
        m.insert(str_val("shortcut"), str_val(v));
    }
    if let Some(v) = ov.timeout {
        m.insert(str_val("timeout"), Value::Number(Number::from(v)));
    }
    if let Some(v) = &ov.categories {
        m.insert(str_val("categories"), str_val(v));
    }
    if let Some(v) = ov.disabled {
        m.insert(str_val("disabled"), Value::Bool(v));
    }
    if let Some(v) = ov.weight {
        m.insert(str_val("weight"), Value::Number(Number::from(v)));
    }
    if let Some(v) = &ov.tokens {
        m.insert(str_val("tokens"), tokens_val(v));
    }
    Value::Mapping(m)
}

/// Build the full settings file: `use_default_settings: true` plus the base and
/// overlay engine entries (in that order) sharing one `name`.
fn build_file_yaml(base: &BaseEngine, ov: &OverlayEngine) -> String {
    let engines = Value::Sequence(vec![base_engine_map(base), overlay_engine_map(ov)]);
    let mut root = Mapping::new();
    root.insert(str_val("use_default_settings"), Value::Bool(true));
    root.insert(str_val("engines"), engines);
    serde_yaml_ng::to_string(&Value::Mapping(root)).expect("serialize settings file")
}

/// Write `contents` to a unique temp `.yml` file and return its path.
fn write_temp_yaml(contents: &str) -> PathBuf {
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let path = std::env::temp_dir().join(format!(
        "zoeken_settings_engine_overlay_prop_{}_{}.yml",
        std::process::id(),
        n
    ));
    std::fs::write(&path, contents).expect("write temp settings file");
    path
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn per_engine_overlay_replaces_only_named_attributes(
        base in base_strategy(),
        ov in overlay_strategy(),
    ) {
        let yaml = build_file_yaml(&base, &ov);
        let path = write_temp_yaml(&yaml);
        let loaded = load_settings(Some(&path), &EnvMap::new());
        std::fs::remove_file(&path).ok();

        let settings = loaded.expect("per-engine overlay must load successfully");

        // The overlay entry merges onto the base entry (same name), so exactly
        // one engine remains.
        prop_assert_eq!(settings.engines.len(), 1);
        let engine = &settings.engines[0];
        prop_assert_eq!(&engine.name, ENGINE_NAME);

        // shortcut: overlay value when named, else base value (both preserved
        // as `Some`).
        let expected_shortcut = ov.shortcut.clone().unwrap_or_else(|| base.shortcut.clone());
        prop_assert_eq!(engine.shortcut.as_deref(), Some(expected_shortcut.as_str()));

        // timeout
        let expected_timeout = ov.timeout.unwrap_or(base.timeout);
        prop_assert_eq!(engine.timeout, Some(expected_timeout));

        // categories (parsed as a single-string variant)
        let expected_categories = ov.categories.clone().unwrap_or_else(|| base.categories.clone());
        prop_assert_eq!(
            engine.categories.clone(),
            Some(StringOrVec::One(expected_categories))
        );

        // disabled
        let expected_disabled = ov.disabled.unwrap_or(base.disabled);
        prop_assert_eq!(engine.disabled, Some(expected_disabled));

        // weight
        let expected_weight = ov.weight.unwrap_or(base.weight);
        prop_assert_eq!(engine.weight, Some(expected_weight));

        // tokens
        let expected_tokens = ov.tokens.clone().unwrap_or_else(|| base.tokens.clone());
        prop_assert_eq!(engine.tokens.clone(), Some(expected_tokens));
    }
}
