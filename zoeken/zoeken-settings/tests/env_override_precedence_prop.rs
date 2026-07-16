// Property: environment overrides win over file values and defaults.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};

use proptest::prelude::*;
use serde_yaml_ng::{Mapping, Value};
use zoeken_settings::{BoolOrString, EnvMap, IntOrString, load_settings};

#[derive(Debug, Clone)]
enum Port {
    Int(i64),
    Str(String),
}

#[derive(Debug, Clone)]
struct Vals {
    debug: bool,
    bind_address: String,
    limiter: bool,
    public_instance: bool,
    secret: String,
    base_url: String,
    image_proxy: bool,
    method: String,
    redis_url: String,
    valkey_url: String,
    port: Port,
}

fn scalar() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9._:/@-]{0,24}".prop_map(|s| s.to_string())
}

fn port() -> impl Strategy<Value = Port> {
    prop_oneof![
        any::<i64>().prop_map(Port::Int),
        "[a-zA-Z_][a-zA-Z0-9_]{0,10}".prop_map(|s| Port::Str(s.to_string())),
    ]
}

fn vals() -> impl Strategy<Value = Vals> {
    (
        any::<bool>(),
        scalar(),
        any::<bool>(),
        any::<bool>(),
        scalar(),
        scalar(),
        any::<bool>(),
        prop_oneof![Just("POST".to_string()), Just("GET".to_string())],
        scalar(),
        scalar(),
        port(),
    )
        .prop_map(
            |(
                debug,
                bind_address,
                limiter,
                public_instance,
                secret,
                base_url,
                image_proxy,
                method,
                redis_url,
                valkey_url,
                port,
            )| Vals {
                debug,
                bind_address,
                limiter,
                public_instance,
                secret,
                base_url,
                image_proxy,
                method,
                redis_url,
                valkey_url,
                port,
            },
        )
}

fn str_val(s: &str) -> Value {
    Value::String(s.to_string())
}

fn port_val(p: &Port) -> Value {
    match p {
        Port::Int(n) => Value::Number((*n).into()),
        Port::Str(s) => str_val(s),
    }
}

fn port_env_raw(p: &Port) -> String {
    match p {
        Port::Int(n) => n.to_string(),
        Port::Str(s) => s.clone(),
    }
}

fn expected_port(p: &Port) -> IntOrString {
    match p {
        Port::Int(n) => IntOrString::Int(*n),
        Port::Str(s) => IntOrString::Str(s.clone()),
    }
}

fn file_yaml(v: &Vals) -> String {
    let mut general = Mapping::new();
    general.insert(str_val("debug"), Value::Bool(v.debug));

    let mut server = Mapping::new();
    server.insert(str_val("bind_address"), str_val(&v.bind_address));
    server.insert(str_val("limiter"), Value::Bool(v.limiter));
    server.insert(str_val("public_instance"), Value::Bool(v.public_instance));
    server.insert(str_val("secret_key"), str_val(&v.secret));
    server.insert(str_val("base_url"), str_val(&v.base_url));
    server.insert(str_val("image_proxy"), Value::Bool(v.image_proxy));
    server.insert(str_val("method"), str_val(&v.method));
    server.insert(str_val("port"), port_val(&v.port));

    let mut redis = Mapping::new();
    redis.insert(str_val("url"), str_val(&v.redis_url));
    let mut valkey = Mapping::new();
    valkey.insert(str_val("url"), str_val(&v.valkey_url));

    let mut top = Mapping::new();
    top.insert(str_val("use_default_settings"), Value::Bool(true));
    top.insert(str_val("general"), Value::Mapping(general));
    top.insert(str_val("server"), Value::Mapping(server));
    top.insert(str_val("redis"), Value::Mapping(redis));
    top.insert(str_val("valkey"), Value::Mapping(valkey));

    serde_yaml_ng::to_string(&Value::Mapping(top)).expect("serialize file overlay")
}

fn env_map(v: &Vals) -> EnvMap {
    EnvMap::new()
        .with("APP_DEBUG", if v.debug { "true" } else { "false" })
        .with("APP_BIND_ADDRESS", v.bind_address.clone())
        .with("APP_LIMITER", if v.limiter { "true" } else { "false" })
        .with(
            "APP_PUBLIC_INSTANCE",
            if v.public_instance { "true" } else { "false" },
        )
        .with("APP_SECRET_KEY", v.secret.clone())
        .with("APP_BASE_URL", v.base_url.clone())
        .with(
            "APP_IMAGE_PROXY",
            if v.image_proxy { "true" } else { "false" },
        )
        .with("APP_METHOD", v.method.clone())
        .with("APP_REDIS_URL", v.redis_url.clone())
        .with("APP_VALKEY_URL", v.valkey_url.clone())
        .with("APP_PORT", port_env_raw(&v.port))
}

static COUNTER: AtomicU64 = AtomicU64::new(0);

fn unique_temp_path() -> PathBuf {
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!(
        "zoeken_settings_env_prop_{}_{}_{}.yml",
        std::process::id(),
        nanos,
        n
    ))
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 200, ..ProptestConfig::default() })]

    #[test]
    fn environment_overrides_win(env_vals in vals(), file_vals in vals()) {
        let path = unique_temp_path();
        std::fs::write(&path, file_yaml(&file_vals)).expect("write temp settings file");

        let result = load_settings(Some(&path), &env_map(&env_vals));
        let _ = std::fs::remove_file(&path);

        let settings = match result {
            Ok(s) => s,
            Err(e) => return Err(TestCaseError::fail(format!("load_settings failed: {e}"))),
        };

        prop_assert_eq!(settings.general.debug, env_vals.debug);
        prop_assert_eq!(&settings.server.bind_address, &env_vals.bind_address);
        prop_assert_eq!(settings.server.limiter, env_vals.limiter);
        prop_assert_eq!(settings.server.public_instance, env_vals.public_instance);
        prop_assert_eq!(&settings.server.secret_key, &env_vals.secret);
        prop_assert_eq!(
            settings.server.base_url,
            Some(BoolOrString::Str(env_vals.base_url.clone()))
        );
        prop_assert_eq!(settings.server.image_proxy, env_vals.image_proxy);
        prop_assert_eq!(&settings.server.method, &env_vals.method);
        prop_assert_eq!(
            settings.redis.url,
            Some(BoolOrString::Str(env_vals.redis_url.clone()))
        );
        prop_assert_eq!(
            settings.valkey.url,
            Some(BoolOrString::Str(env_vals.valkey_url.clone()))
        );
        prop_assert_eq!(settings.server.port, Some(expected_port(&env_vals.port)));
    }
}
