use zoeken_network::{EmulationProfile, Network, NetworkConfig, TOR_CHECK_URL};
#[test]
fn network_is_constructed_with_configured_emulation_profile() {
    let firefox_cfg = NetworkConfig {
        emulation: EmulationProfile::firefox(),
        ..NetworkConfig::default()
    };
    let firefox_net =
        Network::build("firefox-net", firefox_cfg).expect("build network with firefox profile");
    assert_eq!(
        firefox_net.config().emulation,
        EmulationProfile::firefox(),
        "network should carry the Firefox emulation profile it was configured with",
    );
    assert!(
        !firefox_net.clients().is_empty(),
        "a client should be built with the emulation profile applied",
    );

    let chrome_cfg = NetworkConfig {
        emulation: EmulationProfile::chrome(),
        ..NetworkConfig::default()
    };
    let chrome_net =
        Network::build("chrome-net", chrome_cfg).expect("build network with chrome profile");
    assert_eq!(chrome_net.config().emulation, EmulationProfile::chrome());

    assert_ne!(
        firefox_net.config().emulation,
        chrome_net.config().emulation,
        "distinct networks retain their own emulation profiles",
    );
}

#[test]
fn default_network_config_uses_weighted_random_emulation() {
    let cfg = NetworkConfig::default();
    assert_eq!(
        cfg.emulation,
        EmulationProfile::default(),
        "default config uses the default emulation profile",
    );
    assert_eq!(
        cfg.emulation,
        EmulationProfile::Random,
        "the default emulation profile is weighted-random",
    );

    let net = Network::build("default-net", cfg).expect("build network with default profile");
    assert_eq!(net.config().emulation, EmulationProfile::Random);
}

#[tokio::test]
async fn tor_disabled_network_reports_unhealthy_without_io() {
    let cfg = NetworkConfig::default();
    assert!(
        !cfg.using_tor_proxy,
        "the default network is not a Tor network",
    );
    let net = Network::build("plain-net", cfg).expect("build non-tor network");

    assert!(
        !net.check_tor("plain-net")
            .await
            .expect("check_tor is infallible when gated off"),
        "a non-Tor network reports Tor health as false without contacting the network",
    );
    net.ensure_tor_routing("plain-net")
        .await
        .expect("ensure_tor_routing passes trivially for a non-Tor network");
}

#[test]
fn tor_enabled_network_is_configured_to_route_and_exposes_health_api() {
    let cfg = NetworkConfig {
        using_tor_proxy: true,
        ..NetworkConfig::default()
    };
    let net = Network::build("tor-net", cfg).expect("build tor network");

    assert!(
        net.config().using_tor_proxy,
        "a Tor network is configured to route its requests through the Tor proxy",
    );

    assert_eq!(
        TOR_CHECK_URL, "https://check.torproject.org/api/ip",
        "the Tor health probe uses the reference Tor check endpoint",
    );
}
