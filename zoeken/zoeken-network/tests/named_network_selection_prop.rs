use proptest::prelude::*;
use zoeken_network::{DEFAULT_NETWORK, NetworkManager};
use zoeken_settings::{NetworkSettings, Settings};

fn is_reserved(name: &str) -> bool {
    matches!(name, "ipv4" | "ipv6" | "image_proxy") || name == DEFAULT_NETWORK
}

fn name_strategy() -> impl Strategy<Value = String> {
    "[a-z][a-z0-9_]{0,7}".prop_map(|s| s)
}

fn network_set() -> impl Strategy<Value = Vec<(String, u32)>> {
    prop::collection::vec((name_strategy(), 0u32..8), 0..6).prop_map(|entries| {
        let mut seen = std::collections::BTreeSet::new();
        entries
            .into_iter()
            .filter(|(name, _)| !is_reserved(name))
            .filter(|(name, _)| seen.insert(name.clone()))
            .collect()
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn named_network_selection_is_total(
        configured in network_set(),
        query in name_strategy(),
    ) {
        let mut outgoing = Settings::defaults().outgoing;
        outgoing.networks = configured
            .iter()
            .cloned()
            .map(|(name, retries)| {
                let ns = NetworkSettings {
                    retries: Some(retries),
                    ..NetworkSettings::default()
                };
                (name, ns)
            })
            .collect();

        let manager = NetworkManager::from_settings(&outgoing).expect("build manager");

        let selected = manager.get(&query);
        let selected_is_default = std::ptr::eq(selected, manager.default_network());
        prop_assert_eq!(selected_is_default, !manager.contains(&query));

        for (name, retries) in &configured {
            prop_assert!(manager.contains(name));
            let net = manager.get(name);
            prop_assert!(!std::ptr::eq(net, manager.default_network()));
            prop_assert_eq!(net.config().retries, *retries);
        }

        let absent = format!("{query}\u{0}\u{0}absent");
        prop_assert!(!manager.contains(&absent));
        prop_assert!(std::ptr::eq(manager.get(&absent), manager.default_network()));
    }
}
