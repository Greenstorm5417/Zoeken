//! Production startup sequence for `zoeken-server`.

use std::path::PathBuf;
use std::sync::Arc;

use metrics_exporter_prometheus::PrometheusBuilder;
use tracing_subscriber::filter::LevelFilter;
use zoeken_server::boot::{BootConfig, boot};
use zoeken_server::middleware::{level_filter, resolve_limiter_gate};
use zoeken_server::readiness::ReadinessState;
use zoeken_server::serve::{ServeConfig, bind_listener, serve};
use zoeken_server::static_assets::{AssetSource, DirAssets, startup_asset_check};
use zoeken_server::{AppState, app};
use zoeken_settings::{
    EnvMap, SecretKeyDecision, load_settings, resolve_bind, secret_key_decision, secret_key_is_weak,
};
use zoeken_storage::{BackendConfig, StorageConfig};

const SETTINGS_PATH_ENV: &str = "APP_SETTINGS_PATH";
const DATA_DIR_ENV: &str = "APP_DATA_DIR";
const ASSETS_DIR_ENV: &str = "APP_ASSETS_DIR";

fn resolve_assets_dir() -> PathBuf {
    if let Some(dir) = std::env::var_os(ASSETS_DIR_ENV) {
        return PathBuf::from(dir);
    }
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(|dir| dir.join("assets")))
        .unwrap_or_else(|| PathBuf::from("assets"))
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let settings_path = std::env::var_os(SETTINGS_PATH_ENV).map(PathBuf::from);
    let data_dir = std::env::var_os(DATA_DIR_ENV).map(PathBuf::from);

    let level = load_settings(settings_path.as_deref(), &EnvMap::from_env())
        .map(|settings| level_filter(&settings.deployment))
        .unwrap_or(LevelFilter::INFO);
    tracing_subscriber::fmt().with_max_level(level).init();

    let metrics_handle = {
        let recorder = PrometheusBuilder::new().build_recorder();
        let handle = recorder.handle();
        match metrics::set_global_recorder(recorder) {
            Ok(()) => Some(handle),
            Err(error) => {
                tracing::warn!("metrics recorder already installed: {error}");
                None
            }
        }
    };

    let mut boot = boot(&BootConfig::new(settings_path, data_dir))?;
    let settings = boot.settings.clone();

    let storage_config = match settings.storage.backend.as_str() {
        "sqlite" => StorageConfig {
            backend: BackendConfig::Sqlite {
                path: settings.storage.sqlite.path.clone().into(),
                busy_timeout: std::time::Duration::from_millis(
                    settings.storage.sqlite.busy_timeout_ms,
                ),
                max_connections: 4,
            },
        },
        "postgres" => StorageConfig {
            backend: BackendConfig::Postgres {
                url: settings
                    .storage
                    .postgres
                    .url
                    .clone()
                    .expect("validated PostgreSQL URL"),
                max_connections: settings.storage.postgres.max_connections,
                acquire_timeout: std::time::Duration::from_secs(
                    settings.storage.postgres.acquire_timeout_seconds,
                ),
            },
        },
        _ => unreachable!("storage backend was validated while loading settings"),
    };
    let storage = zoeken_storage::connect(&storage_config).await?;
    boot.networks = boot.networks.with_coordinator(Arc::clone(&storage));

    let bind = resolve_bind(&settings.server)?;
    let is_loopback = bind.ip().is_loopback();

    match secret_key_decision(is_loopback, settings.server.secret_key.is_empty()) {
        SecretKeyDecision::Abort => {
            return Err(format!(
                "startup aborted: a non-empty server.secret_key is required for a public \
                 (non-loopback) deployment on {bind} (Req 11.2)"
            )
            .into());
        }
        SecretKeyDecision::StartWithWarning => {
            tracing::warn!(
                bind = %bind,
                "no server.secret_key configured; continuing because the bind is loopback (Req 11.3)"
            );
        }
        SecretKeyDecision::Start => {
            if !is_loopback && secret_key_is_weak(&settings.server.secret_key) {
                return Err(
                    "startup aborted: server.secret_key is too short or a known placeholder; \
                     use at least 16 random characters for public binds"
                        .into(),
                );
            }
            if is_loopback && secret_key_is_weak(&settings.server.secret_key) {
                tracing::warn!(
                    "server.secret_key looks weak; fine for loopback, not for public binds"
                );
            }
        }
    }

    let gate = resolve_limiter_gate(
        is_loopback,
        Some(settings.server.limiter),
        settings.server.public_instance,
    );
    if settings.server.public_instance && !is_loopback && !settings.server.limiter {
        tracing::warn!(
            bind = %bind,
            "server.public_instance=true force-enabled the inbound rate limiter despite \
             server.limiter=false"
        );
    }
    if gate.warn_public_unprotected {
        tracing::warn!(
            bind = %bind,
            "instance is publicly exposed without inbound protection: the rate limiter is \
             explicitly disabled on a non-loopback bind (Req 13.4)"
        );
    }
    if gate.enabled {
        tracing::info!(bind = %bind, "inbound rate limiter enabled (Req 13.1)");
    } else {
        tracing::info!(
            bind = %bind,
            "inbound rate limiter disabled (loopback default / explicitly off)"
        );
    }

    let assets_dir = resolve_assets_dir();
    let assets: Arc<dyn AssetSource> = Arc::new(DirAssets::new(&assets_dir));
    startup_asset_check(assets.as_ref(), &assets_dir.display().to_string())?;
    tracing::info!(dir = %assets_dir.display(), "serving frontend assets from directory");

    let readiness = ReadinessState::new_not_ready();
    let storage_monitor = Arc::clone(&storage);
    let storage_readiness = readiness.clone();
    let favicon_max_total_bytes = settings.cache.favicons.max_total_bytes;
    tokio::spawn(async move {
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(5));
        loop {
            interval.tick().await;
            let healthy = storage_monitor.healthcheck().await.is_ok();
            storage_readiness.set_storage_healthy(healthy);
            metrics::gauge!("storage_healthy").set(if healthy { 1.0 } else { 0.0 });
            if healthy
                && storage_monitor
                    .maintenance(favicon_max_total_bytes)
                    .await
                    .is_err()
            {
                metrics::counter!("storage_operations_total", "operation" => "maintenance", "outcome" => "error")
                    .increment(1);
            }
        }
    });
    let mut state = AppState::from_boot(boot)?
        .with_assets(assets)
        .with_readiness(readiness.clone())
        .with_limiter_enabled(gate.enabled);
    if let Some(handle) = metrics_handle {
        state = state.with_metrics_handle(handle);
    }
    let router = app(state);

    let listener = bind_listener(bind).await?;
    tracing::info!("server listening on http://{bind}");

    readiness.set_ready();
    let serve_config = ServeConfig::from_deployment(bind, &settings.deployment);
    serve(listener, router, &serve_config, readiness).await?;
    Ok(())
}
