//! Shared plugin traits and registry.

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use zoeken_query::SearchQuery;
use zoeken_results::{Answer, Infobox, Result_};

pub mod lua;

pub const STANDARD_PLUGIN_IDS: [&str; 10] = [
    "calculator",
    "unit_converter",
    "self_info",
    "time_zone",
    "tracker_url_remover",
    "hostnames",
    "oa_doi_rewrite",
    "ahmia_filter",
    "tor_check",
    "infiniteScroll",
];

pub trait ResultContainerMut {
    fn main_results_mut(&mut self) -> &mut Vec<Result_>;
    fn answers_mut(&mut self) -> &mut Vec<Answer>;
    fn infoboxes_mut(&mut self) -> &mut Vec<Infobox>;

    fn append_result(&mut self, result: Result_) {
        self.main_results_mut().push(result);
    }

    fn drop_results_by<F>(&mut self, mut keep: F)
    where
        Self: Sized,
        F: FnMut(&Result_) -> bool,
    {
        self.main_results_mut().retain(|result| keep(result));
    }
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct SimpleResultContainer {
    pub results: Vec<Result_>,
    pub answers: Vec<Answer>,
    pub infoboxes: Vec<Infobox>,
}

impl SimpleResultContainer {
    pub fn new(results: Vec<Result_>) -> Self {
        SimpleResultContainer {
            results,
            answers: Vec::new(),
            infoboxes: Vec::new(),
        }
    }
}

impl ResultContainerMut for SimpleResultContainer {
    fn main_results_mut(&mut self) -> &mut Vec<Result_> {
        &mut self.results
    }

    fn answers_mut(&mut self) -> &mut Vec<Answer> {
        &mut self.answers
    }

    fn infoboxes_mut(&mut self) -> &mut Vec<Infobox> {
        &mut self.infoboxes
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PluginKind {
    ResultPlugin,
    Answerer,
    Both,
}

impl PluginKind {
    pub fn runs_result_hooks(self) -> bool {
        matches!(self, PluginKind::ResultPlugin | PluginKind::Both)
    }

    pub fn runs_answer_hooks(self) -> bool {
        matches!(self, PluginKind::Answerer | PluginKind::Both)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginInfo {
    pub id: String,
    pub name: String,
    pub description: String,
    pub examples: Vec<String>,
    pub version: String,
    pub api_version: u32,
    pub kind: PluginKind,
    pub default_enabled: bool,
    pub keywords: Vec<String>,
    pub preference_section: String,
    pub order: i32,
    pub after: Vec<String>,
    pub before: Vec<String>,
    pub capabilities: Vec<String>,
}

impl PluginInfo {
    pub fn simple(id: impl Into<String>) -> Self {
        let id = id.into();
        PluginInfo {
            name: id.clone(),
            id,
            description: String::new(),
            examples: Vec::new(),
            version: String::new(),
            api_version: 1,
            kind: PluginKind::Both,
            default_enabled: true,
            keywords: Vec::new(),
            preference_section: "plugins".to_string(),
            order: 0,
            after: Vec::new(),
            before: Vec::new(),
            capabilities: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PluginGating {
    pub globally_enabled: bool,
    pub per_plugin: HashMap<String, bool>,
    pub default_enabled: bool,
}

impl PluginGating {
    pub fn all_enabled() -> Self {
        PluginGating {
            globally_enabled: true,
            per_plugin: HashMap::new(),
            default_enabled: true,
        }
    }

    pub fn globally_disabled() -> Self {
        PluginGating {
            globally_enabled: false,
            per_plugin: HashMap::new(),
            default_enabled: true,
        }
    }

    pub fn with_plugin(mut self, id: impl Into<String>, enabled: bool) -> Self {
        self.per_plugin.insert(id.into(), enabled);
        self
    }

    pub fn is_enabled(&self, id: &str) -> bool {
        self.is_enabled_with_default(id, self.default_enabled)
    }

    /// Prefer an explicit preference, otherwise the plugin's own `default_enabled`.
    pub fn is_enabled_with_default(&self, id: &str, plugin_default: bool) -> bool {
        if !self.globally_enabled {
            return false;
        }
        self.per_plugin.get(id).copied().unwrap_or(plugin_default)
    }
}

impl Default for PluginGating {
    fn default() -> Self {
        PluginGating::all_enabled()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, serde::Serialize)]
pub struct PluginMetricsSnapshot {
    pub id: String,
    pub hook_failures: usize,
    pub load_failures: usize,
    pub init_failures: usize,
    pub timeouts: usize,
    pub dropped_results: usize,
    pub appended_results: usize,
}

#[derive(Debug, Clone, Default)]
pub struct PluginCtx {
    pub gating: PluginGating,
    pub client_ip: Option<String>,
    pub user_agent: Option<String>,
    pub headers: HashMap<String, String>,
    pub method: Option<String>,
    pub locale: Option<String>,
    pub image_proxy: Option<bool>,
}

impl PluginCtx {
    pub fn new(gating: PluginGating) -> Self {
        PluginCtx {
            gating,
            client_ip: None,
            user_agent: None,
            headers: HashMap::new(),
            method: None,
            locale: None,
            image_proxy: None,
        }
    }

    pub fn all_enabled() -> Self {
        PluginCtx::new(PluginGating::all_enabled())
    }

    pub fn with_client_ip(mut self, ip: impl Into<String>) -> Self {
        self.client_ip = Some(ip.into());
        self
    }

    pub fn with_user_agent(mut self, ua: impl Into<String>) -> Self {
        self.user_agent = Some(ua.into());
        self
    }

    pub fn with_header(mut self, name: impl Into<String>, value: impl Into<String>) -> Self {
        self.headers.insert(name.into(), value.into());
        self
    }

    pub fn with_method(mut self, method: impl Into<String>) -> Self {
        self.method = Some(method.into());
        self
    }

    pub fn with_locale(mut self, locale: impl Into<String>) -> Self {
        self.locale = Some(locale.into());
        self
    }

    pub fn with_image_proxy(mut self, enabled: bool) -> Self {
        self.image_proxy = Some(enabled);
        self
    }
}

pub trait Plugin: Send + Sync {
    fn id(&self) -> &str;
    fn info(&self) -> PluginInfo {
        PluginInfo::simple(self.id())
    }
    /// Optional runtime counters (Lua plugins). Used by `/stats`.
    fn metrics_snapshot(&self) -> Option<PluginMetricsSnapshot> {
        None
    }
    fn on_pre_search(&self, _query: &mut SearchQuery, _ctx: &PluginCtx) -> bool {
        true
    }
    fn on_pre_search_answers(&self, _query: &SearchQuery, _ctx: &PluginCtx) -> Vec<Answer> {
        Vec::new()
    }
    fn on_result(&self, _result: &mut Result_, _query: &SearchQuery, _ctx: &PluginCtx) -> bool {
        true
    }
    fn on_results(
        &self,
        _container: &mut dyn ResultContainerMut,
        _query: &SearchQuery,
        _ctx: &PluginCtx,
    ) {
    }
    fn on_post_search(&self, _container: &mut dyn ResultContainerMut, _ctx: &PluginCtx) {}
}

#[derive(Clone, Default)]
pub struct PluginRegistry {
    plugins: Vec<Arc<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        PluginRegistry {
            plugins: Vec::new(),
        }
    }

    pub fn register(&mut self, plugin: Arc<dyn Plugin>) -> &mut Self {
        self.plugins.push(plugin);
        self
    }

    pub fn from_plugins(plugins: impl IntoIterator<Item = Arc<dyn Plugin>>) -> Self {
        PluginRegistry {
            plugins: plugins.into_iter().collect(),
        }
    }

    pub fn plugins(&self) -> &[Arc<dyn Plugin>] {
        &self.plugins
    }

    pub fn metrics_snapshots(&self) -> Vec<PluginMetricsSnapshot> {
        self.plugins
            .iter()
            .filter_map(|plugin| plugin.metrics_snapshot())
            .collect()
    }

    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }

    pub fn enabled_plugins<'a>(
        &'a self,
        query: Option<&'a SearchQuery>,
        ctx: &'a PluginCtx,
    ) -> impl Iterator<Item = &'a Arc<dyn Plugin>> {
        self.plugins.iter().filter(move |p| {
            ctx.gating
                .is_enabled_with_default(p.id(), p.info().default_enabled)
                && query.is_none_or(|query| keyword_match(&p.info(), query))
        })
    }

    pub fn run_pre_search(&self, query: &mut SearchQuery, ctx: &PluginCtx) -> bool {
        let plugins: Vec<Arc<dyn Plugin>> =
            self.enabled_plugins(Some(query), ctx).cloned().collect();
        for plugin in plugins {
            if !plugin.on_pre_search(query, ctx) {
                return false;
            }
        }
        true
    }

    pub fn run_pre_search_answers(&self, query: &SearchQuery, ctx: &PluginCtx) -> Vec<Answer> {
        let mut answers = Vec::new();
        for plugin in self.enabled_plugins(Some(query), ctx) {
            if plugin.info().kind.runs_answer_hooks() {
                answers.extend(plugin.on_pre_search_answers(query, ctx));
            }
        }
        answers
    }

    pub fn run_on_result(
        &self,
        result: &mut Result_,
        query: &SearchQuery,
        ctx: &PluginCtx,
    ) -> bool {
        for plugin in self.enabled_plugins(Some(query), ctx) {
            if plugin.info().kind.runs_result_hooks() && !plugin.on_result(result, query, ctx) {
                return false;
            }
        }
        true
    }

    pub fn run_on_results(
        &self,
        container: &mut dyn ResultContainerMut,
        query: &SearchQuery,
        ctx: &PluginCtx,
    ) {
        for plugin in self.enabled_plugins(Some(query), ctx) {
            if plugin.info().kind.runs_result_hooks() {
                plugin.on_results(container, query, ctx);
            }
        }
    }

    pub fn run_post_search(
        &self,
        container: &mut dyn ResultContainerMut,
        query: &SearchQuery,
        ctx: &PluginCtx,
    ) {
        for plugin in self.enabled_plugins(Some(query), ctx) {
            if plugin.info().kind.runs_result_hooks() {
                plugin.on_post_search(container, ctx);
            }
        }
    }

    pub fn infos(&self) -> Vec<PluginInfo> {
        self.plugins.iter().map(|plugin| plugin.info()).collect()
    }
}

#[derive(Debug, thiserror::Error)]
#[error("plugin ordering cycle: {0}")]
pub struct PluginOrderError(String);

pub fn sort_plugins(
    plugins: &mut Vec<Arc<dyn Plugin>>,
    explicit_order: &[String],
) -> Result<(), PluginOrderError> {
    let infos: Vec<_> = plugins.iter().map(|plugin| plugin.info()).collect();
    let id_to_idx: HashMap<_, _> = infos
        .iter()
        .enumerate()
        .map(|(idx, info)| (info.id.clone(), idx))
        .collect();
    let explicit_rank: HashMap<_, _> = explicit_order
        .iter()
        .enumerate()
        .map(|(idx, id)| (id.as_str(), idx))
        .collect();
    let mut edges: Vec<HashSet<usize>> = vec![HashSet::new(); infos.len()];
    let mut indegree = vec![0usize; infos.len()];
    for (idx, info) in infos.iter().enumerate() {
        for target in &info.after {
            if let Some(&target_idx) = id_to_idx.get(target)
                && edges[target_idx].insert(idx)
            {
                indegree[idx] += 1;
            }
        }
        for target in &info.before {
            if let Some(&target_idx) = id_to_idx.get(target)
                && edges[idx].insert(target_idx)
            {
                indegree[target_idx] += 1;
            }
        }
    }

    let mut remaining: HashSet<usize> = (0..infos.len()).collect();
    let mut ordered = Vec::with_capacity(infos.len());
    while !remaining.is_empty() {
        let Some(&next) = remaining
            .iter()
            .filter(|&&idx| indegree[idx] == 0)
            .min_by(|&&a, &&b| {
                plugin_sort_key(&infos[a], &explicit_rank)
                    .cmp(&plugin_sort_key(&infos[b], &explicit_rank))
            })
        else {
            let mut cycle: Vec<_> = remaining.iter().map(|&idx| infos[idx].id.clone()).collect();
            cycle.sort();
            return Err(PluginOrderError(cycle.join(", ")));
        };
        remaining.remove(&next);
        ordered.push(Arc::clone(&plugins[next]));
        for &target in &edges[next] {
            indegree[target] -= 1;
        }
    }
    *plugins = ordered;
    Ok(())
}

fn plugin_sort_key<'a>(
    info: &'a PluginInfo,
    explicit_rank: &HashMap<&'a str, usize>,
) -> (usize, i32, &'a str) {
    (
        explicit_rank
            .get(info.id.as_str())
            .copied()
            .unwrap_or(usize::MAX),
        info.order,
        info.id.as_str(),
    )
}

fn keyword_match(info: &PluginInfo, query: &SearchQuery) -> bool {
    if info.keywords.is_empty() {
        return true;
    }
    let Some(first) = query.query.split_whitespace().next() else {
        return false;
    };
    info.keywords.iter().any(|keyword| keyword == first)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use zoeken_results::MainResult;

    #[derive(Default)]
    struct RecordingPlugin {
        id: String,
        info: Option<PluginInfo>,
        pre: AtomicUsize,
        on_result: AtomicUsize,
        post: AtomicUsize,
    }

    impl RecordingPlugin {
        fn new(id: &str) -> Arc<Self> {
            Arc::new(RecordingPlugin {
                id: id.to_string(),
                ..RecordingPlugin::default()
            })
        }

        fn with_default_enabled(id: &str, default_enabled: bool) -> Arc<Self> {
            Arc::new(RecordingPlugin {
                id: id.to_string(),
                info: Some(PluginInfo {
                    default_enabled,
                    ..PluginInfo::simple(id)
                }),
                ..RecordingPlugin::default()
            })
        }
    }

    impl Plugin for RecordingPlugin {
        fn id(&self) -> &str {
            &self.id
        }
        fn info(&self) -> PluginInfo {
            self.info
                .clone()
                .unwrap_or_else(|| PluginInfo::simple(&self.id))
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

    fn ordered_plugin(id: &str, order: i32, after: &[&str], before: &[&str]) -> Arc<dyn Plugin> {
        Arc::new(RecordingPlugin {
            id: id.to_string(),
            info: Some(PluginInfo {
                id: id.to_string(),
                name: id.to_string(),
                order,
                after: after.iter().map(|value| (*value).to_string()).collect(),
                before: before.iter().map(|value| (*value).to_string()).collect(),
                ..PluginInfo::simple(id)
            }),
            ..RecordingPlugin::default()
        })
    }

    fn run_all(registry: &PluginRegistry, ctx: &PluginCtx) {
        let mut query = SearchQuery::default();
        assert!(registry.run_pre_search(&mut query, ctx));

        let mut result = Result_::Main(MainResult::default());
        registry.run_on_result(&mut result, &query, ctx);

        let mut container = SimpleResultContainer::default();
        registry.run_post_search(&mut container, &query, ctx);
    }

    #[test]
    fn standard_plugin_set_matches_lua_builtins() {
        assert_eq!(STANDARD_PLUGIN_IDS.len(), 10);
        assert!(STANDARD_PLUGIN_IDS.contains(&"calculator"));
        assert!(!STANDARD_PLUGIN_IDS.contains(&"hash_plugin"));
        assert!(STANDARD_PLUGIN_IDS.contains(&"tracker_url_remover"));
        assert!(STANDARD_PLUGIN_IDS.contains(&"tor_check"));
        assert!(STANDARD_PLUGIN_IDS.contains(&"infiniteScroll"));
    }

    #[test]
    fn all_enabled_runs_every_plugin_each_phase() {
        let a = RecordingPlugin::new("a");
        let b = RecordingPlugin::new("b");
        let registry = PluginRegistry::from_plugins([
            a.clone() as Arc<dyn Plugin>,
            b.clone() as Arc<dyn Plugin>,
        ]);

        run_all(&registry, &PluginCtx::all_enabled());

        for p in [&a, &b] {
            assert_eq!(p.pre.load(Ordering::SeqCst), 1);
            assert_eq!(p.on_result.load(Ordering::SeqCst), 1);
            assert_eq!(p.post.load(Ordering::SeqCst), 1);
        }
    }

    #[test]
    fn global_disable_suppresses_all_plugins() {
        let a = RecordingPlugin::new("a");
        let b = RecordingPlugin::new("b");
        let registry = PluginRegistry::from_plugins([
            a.clone() as Arc<dyn Plugin>,
            b.clone() as Arc<dyn Plugin>,
        ]);

        let ctx = PluginCtx::new(PluginGating::globally_disabled());
        run_all(&registry, &ctx);

        for p in [&a, &b] {
            assert_eq!(p.pre.load(Ordering::SeqCst), 0);
            assert_eq!(p.on_result.load(Ordering::SeqCst), 0);
            assert_eq!(p.post.load(Ordering::SeqCst), 0);
        }
        // The enabled-plugins iterator is empty under a global disable.
        assert_eq!(registry.enabled_plugins(None, &ctx).count(), 0);
    }

    #[test]
    fn per_plugin_disable_skips_only_that_plugin() {
        let a = RecordingPlugin::new("a");
        let b = RecordingPlugin::new("b");
        let c = RecordingPlugin::new("c");
        let registry = PluginRegistry::from_plugins([
            a.clone() as Arc<dyn Plugin>,
            b.clone() as Arc<dyn Plugin>,
            c.clone() as Arc<dyn Plugin>,
        ]);

        let ctx = PluginCtx::new(PluginGating::all_enabled().with_plugin("b", false));
        run_all(&registry, &ctx);

        assert_eq!(a.pre.load(Ordering::SeqCst), 1);
        assert_eq!(a.post.load(Ordering::SeqCst), 1);
        assert_eq!(c.pre.load(Ordering::SeqCst), 1);
        assert_eq!(c.post.load(Ordering::SeqCst), 1);

        assert_eq!(b.pre.load(Ordering::SeqCst), 0);
        assert_eq!(b.on_result.load(Ordering::SeqCst), 0);
        assert_eq!(b.post.load(Ordering::SeqCst), 0);

        let enabled: Vec<&str> = registry
            .enabled_plugins(None, &ctx)
            .map(|p| p.id())
            .collect();
        assert_eq!(enabled, vec!["a", "c"]);
    }

    #[test]
    fn default_disabled_plugin_requires_explicit_enable() {
        // Plugins with default_enabled = false only run when prefs enable them.
        let a = RecordingPlugin::with_default_enabled("a", false);
        let b = RecordingPlugin::with_default_enabled("b", false);
        let registry = PluginRegistry::from_plugins([
            a.clone() as Arc<dyn Plugin>,
            b.clone() as Arc<dyn Plugin>,
        ]);

        let ctx = PluginCtx::new(PluginGating::all_enabled().with_plugin("a", true));
        run_all(&registry, &ctx);

        assert_eq!(a.pre.load(Ordering::SeqCst), 1);
        assert_eq!(b.pre.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn sort_plugins_honors_explicit_order_and_dependencies() {
        let mut plugins = vec![
            ordered_plugin("c", 30, &["b"], &[]),
            ordered_plugin("a", 30, &[], &["b"]),
            ordered_plugin("b", 10, &[], &[]),
        ];
        sort_plugins(&mut plugins, &["c".to_string(), "a".to_string()]).expect("sort plugins");
        let ids: Vec<_> = plugins.iter().map(|plugin| plugin.id()).collect();
        assert_eq!(ids, vec!["a", "b", "c"]);
    }

    #[test]
    fn sort_plugins_reports_cycles() {
        let mut plugins = vec![
            ordered_plugin("a", 0, &["b"], &[]),
            ordered_plugin("b", 0, &["a"], &[]),
        ];
        assert!(sort_plugins(&mut plugins, &[]).is_err());
    }

    #[test]
    fn post_search_hook_can_mutate_container() {
        struct DropAll;
        impl Plugin for DropAll {
            fn id(&self) -> &str {
                "drop_all"
            }
            fn on_post_search(&self, c: &mut dyn ResultContainerMut, _ctx: &PluginCtx) {
                c.main_results_mut().clear();
            }
        }

        let registry = PluginRegistry::from_plugins([Arc::new(DropAll) as Arc<dyn Plugin>]);
        let mut container = SimpleResultContainer::new(vec![Result_::Main(MainResult::default())]);
        registry.run_post_search(
            &mut container,
            &SearchQuery::default(),
            &PluginCtx::all_enabled(),
        );
        assert!(container.results.is_empty());
    }
}
