//! Property test for plugin gating.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use proptest::prelude::*;
use zoeken_plugins::{
    Plugin, PluginCtx, PluginGating, PluginInfo, PluginRegistry, ResultContainerMut,
    SimpleResultContainer,
};
use zoeken_query::SearchQuery;
use zoeken_results::{MainResult, Result_};

struct RecordingPlugin {
    id: String,
    default_enabled: bool,
    pre: AtomicUsize,
    on_result: AtomicUsize,
    post: AtomicUsize,
}

impl RecordingPlugin {
    fn new(id: &str, default_enabled: bool) -> Arc<Self> {
        Arc::new(RecordingPlugin {
            id: id.to_string(),
            default_enabled,
            pre: AtomicUsize::new(0),
            on_result: AtomicUsize::new(0),
            post: AtomicUsize::new(0),
        })
    }

    fn total_runs(&self) -> usize {
        self.pre.load(Ordering::SeqCst)
            + self.on_result.load(Ordering::SeqCst)
            + self.post.load(Ordering::SeqCst)
    }
}

impl Plugin for RecordingPlugin {
    fn id(&self) -> &str {
        &self.id
    }
    fn info(&self) -> PluginInfo {
        PluginInfo {
            default_enabled: self.default_enabled,
            ..PluginInfo::simple(&self.id)
        }
    }
    fn on_pre_search(&self, _query: &mut SearchQuery, _ctx: &PluginCtx) -> bool {
        self.pre.fetch_add(1, Ordering::SeqCst);
        true
    }
    fn on_result(&self, _result: &mut Result_, _query: &SearchQuery, _ctx: &PluginCtx) -> bool {
        self.on_result.fetch_add(1, Ordering::SeqCst);
        true
    }
    fn on_post_search(&self, _c: &mut dyn ResultContainerMut, _ctx: &PluginCtx) {
        self.post.fetch_add(1, Ordering::SeqCst);
    }
}

fn run_all_phases(registry: &PluginRegistry, ctx: &PluginCtx) {
    let mut query = SearchQuery::default();
    assert!(registry.run_pre_search(&mut query, ctx));

    let mut result = Result_::Main(MainResult::default());
    registry.run_on_result(&mut result, &query, ctx);

    let mut container = SimpleResultContainer::default();
    registry.run_post_search(&mut container, &query, ctx);
}

fn reference_should_run(
    globally_enabled: bool,
    default_enabled: bool,
    per_plugin: &HashMap<String, bool>,
    id: &str,
) -> bool {
    if !globally_enabled {
        return false;
    }
    match per_plugin.get(id) {
        Some(flag) => *flag,
        None => default_enabled,
    }
}

fn id_strategy() -> impl Strategy<Value = String> {
    (0u8..6).prop_map(|n| format!("p{n}"))
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    #[test]
    fn plugin_runs_iff_globally_and_individually_enabled(
        plugin_ids in prop::collection::vec(id_strategy(), 0..8),
        per_plugin in prop::collection::hash_map(id_strategy(), any::<bool>(), 0..8),
        globally_enabled in any::<bool>(),
        default_enabled in any::<bool>(),
    ) {
        let recorders: Vec<Arc<RecordingPlugin>> = plugin_ids
            .iter()
            .map(|id| RecordingPlugin::new(id, default_enabled))
            .collect();
        let registry = PluginRegistry::from_plugins(
            recorders.iter().map(|p| p.clone() as Arc<dyn Plugin>),
        );

        let gating = PluginGating {
            globally_enabled,
            per_plugin: per_plugin.clone(),
            // Ignored for enablement; each plugin's own default_enabled wins.
            default_enabled: true,
        };
        let ctx = PluginCtx::new(gating);

        let actual_enabled_ids: Vec<&str> =
            registry.enabled_plugins(None, &ctx).map(|p| p.id()).collect();
        let expected_enabled_ids: Vec<&str> = plugin_ids
            .iter()
            .filter(|id| {
                reference_should_run(globally_enabled, default_enabled, &per_plugin, id)
            })
            .map(String::as_str)
            .collect();
        prop_assert_eq!(
            &actual_enabled_ids,
            &expected_enabled_ids,
            "enabled_plugins disagreed with reference model (globally_enabled={}, default_enabled={})",
            globally_enabled,
            default_enabled
        );

        if !globally_enabled {
            prop_assert!(
                expected_enabled_ids.is_empty(),
                "global disable should leave no plugin enabled"
            );
        }

        run_all_phases(&registry, &ctx);

        for recorder in &recorders {
            let expect_run = reference_should_run(
                globally_enabled,
                default_enabled,
                &per_plugin,
                &recorder.id,
            );
            let (want_pre, want_res, want_post, want_total) =
                if expect_run { (1, 1, 1, 3) } else { (0, 0, 0, 0) };

            prop_assert_eq!(
                recorder.pre.load(Ordering::SeqCst),
                want_pre,
                "pre-search runs for plugin {} (expect_run={})",
                recorder.id,
                expect_run
            );
            prop_assert_eq!(
                recorder.on_result.load(Ordering::SeqCst),
                want_res,
                "on-result runs for plugin {} (expect_run={})",
                recorder.id,
                expect_run
            );
            prop_assert_eq!(
                recorder.post.load(Ordering::SeqCst),
                want_post,
                "post-search runs for plugin {} (expect_run={})",
                recorder.id,
                expect_run
            );
            prop_assert_eq!(
                recorder.total_runs(),
                want_total,
                "total lifecycle runs for plugin {} (expect_run={})",
                recorder.id,
                expect_run
            );
        }
    }
}
