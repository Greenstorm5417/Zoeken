//! Startup orchestration and fail-fast initialization.

use std::path::{Path, PathBuf};

use zoeken_data::{DataBundle, DataError, load_bundle, load_embedded_bundle};
use zoeken_network::{NetworkError, NetworkManager};
use zoeken_settings::{EnvMap, Settings, SettingsError, load_settings};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BootComponent {
    Settings,
    Data,
    Network,
}

impl BootComponent {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            BootComponent::Settings => "settings",
            BootComponent::Data => "data",
            BootComponent::Network => "network",
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum BootError {
    #[error("startup aborted: settings initialization failed: {source}")]
    Settings {
        #[source]
        source: SettingsError,
    },
    #[error("startup aborted: data initialization failed: {source}")]
    Data {
        #[source]
        source: DataError,
    },
    #[error("startup aborted: network initialization failed: {source}")]
    Network {
        #[source]
        source: NetworkError,
    },
}

impl BootError {
    #[must_use]
    pub fn component(&self) -> BootComponent {
        match self {
            BootError::Settings { .. } => BootComponent::Settings,
            BootError::Data { .. } => BootComponent::Data,
            BootError::Network { .. } => BootComponent::Network,
        }
    }
}

#[derive(Debug)]
pub struct Boot {
    pub settings: Settings,
    pub data: DataBundle,
    pub networks: NetworkManager,
}

#[derive(Debug, Clone, Default)]
pub struct BootConfig {
    pub settings_path: Option<PathBuf>,
    pub env: EnvMap,
    pub data_dir: Option<PathBuf>,
}

impl BootConfig {
    #[must_use]
    pub fn new(settings_path: Option<PathBuf>, data_dir: Option<PathBuf>) -> Self {
        Self {
            settings_path,
            env: EnvMap::from_env(),
            data_dir,
        }
    }
}

/// Run the startup sequence against the real init implementations.
pub fn boot(config: &BootConfig) -> Result<Boot, BootError> {
    let settings_path = config.settings_path.as_deref();
    let env = &config.env;
    let data_dir: Option<&Path> = config.data_dir.as_deref();

    boot_with(
        || load_settings(settings_path, env),
        |_settings| match data_dir {
            Some(dir) => load_bundle(dir),
            None => load_embedded_bundle(),
        },
        |settings| NetworkManager::from_settings(&settings.outgoing),
    )
}

/// Sequencing core of `boot`, parameterized for tests.
pub fn boot_with<LoadSettings, LoadData, SetupNetwork>(
    load_settings_step: LoadSettings,
    load_data_step: LoadData,
    setup_network_step: SetupNetwork,
) -> Result<Boot, BootError>
where
    LoadSettings: FnOnce() -> Result<Settings, SettingsError>,
    LoadData: FnOnce(&Settings) -> Result<DataBundle, DataError>,
    SetupNetwork: FnOnce(&Settings) -> Result<NetworkManager, NetworkError>,
{
    let settings = load_settings_step().map_err(|source| BootError::Settings { source })?;
    let data = load_data_step(&settings).map_err(|source| BootError::Data { source })?;
    let networks = setup_network_step(&settings).map_err(|source| BootError::Network { source })?;

    Ok(Boot {
        settings,
        data,
        networks,
    })
}

#[cfg(test)]
mod tests {

    use super::*;

    use std::cell::Cell;

    use zoeken_data::{
        BangTrie, CurrencyTable, DataBundle, EngineTraitsMap, LocaleMap, UnitTable, UserAgentPool,
    };
    use zoeken_settings::{NetworkSettings, Settings};

    fn empty_bundle() -> DataBundle {
        DataBundle {
            bangs: BangTrie::new(),
            currencies: CurrencyTable::default(),
            units: UnitTable::default(),
            engine_traits: EngineTraitsMap::default(),
            useragents: UserAgentPool::default(),
            locales: LocaleMap::default(),
            ..DataBundle::default()
        }
    }

    fn settings_with_broken_network() -> Settings {
        let mut settings = Settings::default();
        settings.outgoing.networks.insert(
            "broken".to_string(),
            NetworkSettings {
                network: Some("does-not-exist".to_string()),
                ..NetworkSettings::default()
            },
        );
        settings
    }

    #[test]
    fn settings_failure_aborts_naming_settings_and_skips_later_steps() {
        let data_ran = Cell::new(false);
        let network_ran = Cell::new(false);

        let result = boot_with(
            || Err(SettingsError::InvalidUseDefaultSettings),
            |_settings| {
                data_ran.set(true);
                Ok(empty_bundle())
            },
            |settings: &Settings| {
                network_ran.set(true);
                NetworkManager::from_settings(&settings.outgoing)
            },
        );

        let error = result.expect_err("settings failure should abort boot");
        assert_eq!(error.component(), BootComponent::Settings);
        // Later steps must not run once settings fail.
        assert!(
            !data_ran.get(),
            "data step must not run after settings fail"
        );
        assert!(
            !network_ran.get(),
            "network step must not run after settings fail"
        );
        // The underlying cause is preserved.
        assert!(matches!(
            error,
            BootError::Settings {
                source: SettingsError::InvalidUseDefaultSettings
            }
        ));
        // The report names the failing component.
        assert!(error.to_string().contains("settings"));
    }

    #[test]
    fn real_settings_load_failure_surfaces_underlying_cause() {
        // A malformed settings file makes the real loader fail while parsing;
        // boot must tag it as the settings component, abort before touching
        // data/network, and keep the underlying (file-naming) cause.
        let bad_file = std::env::temp_dir().join("zoeken-boot-malformed-settings-xyz.yml");
        // Unterminated YAML flow sequence => parse error.
        std::fs::write(&bad_file, "general: [debug: true").expect("write temp settings file");

        let config = BootConfig {
            settings_path: Some(bad_file.clone()),
            env: EnvMap::new(),
            // Deliberately non-existent: the settings stage must fail first, so
            // this is never reached.
            data_dir: Some(std::env::temp_dir().join("zoeken-boot-unused-data-dir-xyz")),
        };

        let result = boot(&config);
        let _ = std::fs::remove_file(&bad_file);

        let error = result.expect_err("malformed settings file should abort boot");
        assert_eq!(error.component(), BootComponent::Settings);
        assert!(matches!(error, BootError::Settings { .. }));
        // The underlying cause names the offending file.
        let cause = error.to_string();
        assert!(
            cause.contains(&bad_file.display().to_string()),
            "underlying cause should name the settings file, got: {cause}"
        );
    }

    // --- Data-stage failure ----------------------------------------------

    #[test]
    fn data_failure_aborts_naming_data_and_skips_network() {
        // Point the data loader at a directory with no bundled assets; boot must
        // tag the failure as the data component and not run network.
        let missing_data_dir = std::env::temp_dir().join("zoeken-boot-nonexistent-data-dir-xyz");
        let network_ran = Cell::new(false);

        let result = boot_with(
            || Ok(Settings::default()),
            |_settings| load_bundle(&missing_data_dir),
            |settings: &Settings| {
                network_ran.set(true);
                NetworkManager::from_settings(&settings.outgoing)
            },
        );

        let error = result.expect_err("missing data files should abort boot");
        assert_eq!(error.component(), BootComponent::Data);
        assert!(
            !network_ran.get(),
            "network step must not run after data fails"
        );
        assert!(matches!(error, BootError::Data { .. }));
        // The underlying data error identifies the affected file (Req 2.6).
        let cause = error.to_string();
        assert!(
            cause.contains("external_bangs.json"),
            "underlying cause should name the affected data file, got: {cause}"
        );
        assert!(error.to_string().contains("data"));
    }

    #[test]
    fn network_failure_aborts_naming_network_with_cause() {
        // Settings and data succeed; the real network setup fails because a
        // named network references an unknown network.
        let settings = settings_with_broken_network();

        let result = boot_with(
            || Ok(settings.clone()),
            |_settings| Ok(empty_bundle()),
            |settings: &Settings| NetworkManager::from_settings(&settings.outgoing),
        );

        let error = result.expect_err("bad network reference should abort boot");
        assert_eq!(error.component(), BootComponent::Network);
        assert!(matches!(
            error,
            BootError::Network {
                source: NetworkError::UnknownReference { .. }
            }
        ));
        // The underlying cause identifies the offending network reference.
        let cause = error.to_string();
        assert!(
            cause.contains("does-not-exist") && cause.contains("broken"),
            "underlying cause should name the offending reference, got: {cause}"
        );
        assert!(error.to_string().contains("network"));
    }

    // --- Success path ------------------------------------------------------

    #[test]
    fn success_path_runs_all_steps_in_order() {
        let settings_ran = Cell::new(false);
        let data_ran = Cell::new(false);
        let network_ran = Cell::new(false);

        let boot = boot_with(
            || {
                settings_ran.set(true);
                Ok(Settings::default())
            },
            |_settings| {
                // Data runs only after settings succeeded.
                assert!(settings_ran.get());
                data_ran.set(true);
                Ok(empty_bundle())
            },
            |settings: &Settings| {
                // Network runs only after data succeeded.
                assert!(data_ran.get());
                network_ran.set(true);
                NetworkManager::from_settings(&settings.outgoing)
            },
        )
        .expect("default settings/data/network should boot successfully");

        assert!(settings_ran.get() && data_ran.get() && network_ran.get());
        assert_eq!(boot.settings, Settings::default());
        // The default network is always present and selectable.
        let _ = boot.networks.get("__DEFAULT__");
    }
}
