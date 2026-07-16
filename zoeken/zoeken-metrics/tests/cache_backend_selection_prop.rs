// Cache backend selection is strict: build_kv returns the configured backend
// and never silently falls back. InProcess yields InProc; Valkey yields error
// without the feature or Valkey backend when available.

use std::path::PathBuf;

use proptest::prelude::*;
use zoeken_metrics::cache::{Kv, KvConfig, KvError, build_kv};

/// Test configuration choice.
#[derive(Debug, Clone)]
enum ConfigChoice {
    InProcessMemory,
    InProcessPersistent(PathBuf),
    Valkey(String),
}

/// Unique temp path for persistent case.
fn unique_temp_path(tag: u64) -> PathBuf {
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("zoeken-metrics-kv-sel-{tag}-{nanos}.sqlite"))
}

/// Strategy: memory-only, persistent, and Valkey configurations.
fn config_choice_strategy() -> impl Strategy<Value = ConfigChoice> {
    prop_oneof![
        3 => Just(ConfigChoice::InProcessMemory),
        1 => any::<u64>().prop_map(|tag| ConfigChoice::InProcessPersistent(unique_temp_path(tag))),
        3 => any::<String>().prop_map(ConfigChoice::Valkey),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn build_kv_selects_backend_strictly(choice in config_choice_strategy()) {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .build()
            .expect("build current-thread runtime");

        runtime.block_on(async {
            match choice {
                ConfigChoice::InProcessMemory => {
                    let config = KvConfig::InProcess { path: None };
                    let result = build_kv(&config).await;
                    prop_assert!(
                        matches!(result, Ok(Kv::InProc(_))),
                        "InProcess{{path: None}} must build Kv::InProc, got {:?}",
                        result.as_ref().map(|_| "Ok(Kv::InProc)"),
                    );
                }
                ConfigChoice::InProcessPersistent(path) => {
                    let config = KvConfig::InProcess { path: Some(path.clone()) };
                    let result = build_kv(&config).await;
                    let is_in_proc = matches!(result, Ok(Kv::InProc(_)));
                    let _ = std::fs::remove_file(&path);
                    prop_assert!(
                        is_in_proc,
                        "InProcess{{path: Some(..)}} must build Kv::InProc for {:?}",
                        path,
                    );
                }
                ConfigChoice::Valkey(url) => {
                    let config = KvConfig::Valkey { url: url.clone() };
                    let result = build_kv(&config).await;
                    prop_assert!(
                        matches!(result, Err(KvError::ValkeyFeatureDisabled)),
                        "Valkey{{url: {:?}}} must return Err(ValkeyFeatureDisabled) \
                         under default features, never an in-process fallback",
                        url,
                    );
                }
            }
            Ok(())
        })?;
    }
}
