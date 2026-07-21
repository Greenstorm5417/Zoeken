pub mod aggregation;
pub mod execution;
pub mod metrics;
pub mod selection;

use std::collections::HashMap;
use std::collections::HashSet;
use std::time::{Duration, Instant};

use zoeken_answerers::AnswererRegistry;
use zoeken_engine_core::{EngineMeta, SearchQueryView};
use zoeken_plugins::{PluginCtx, PluginRegistry};
use zoeken_query::SearchQuery;

pub use aggregation::{
    EngineWeights, ResultContainer, UnresponsiveCause, UnresponsiveEngine, aggregate,
};
pub use execution::{
    EngineExecResult, EngineExecutor, EngineFuture, EngineRunOutcome, EngineRunStatus,
    ExecutionReport, UnresponsiveReason, run_engines,
};
pub use metrics::{EngineOutcome, EngineSample, MetricsRecorder, NoopRecorder};
pub use selection::{
    AllEnginesEnabled, EnabledEngineSet, EnginePreferences, EngineRegistry, RegisteredEngine,
    SelectedEngine, SuspensionPolicy,
};

#[derive(Debug, Clone, Copy)]
pub struct SearchConfig {
    pub default_engine_timeout: Duration,
    pub max_request_timeout: Duration,
    pub suspension: SuspensionPolicy,
}

impl Default for SearchConfig {
    fn default() -> Self {
        SearchConfig {
            default_engine_timeout: Duration::from_secs(3),
            max_request_timeout: Duration::from_secs(3),
            suspension: SuspensionPolicy::default(),
        }
    }
}

#[derive(Clone)]
pub struct Search {
    registry: EngineRegistry,
    executor: std::sync::Arc<dyn EngineExecutor>,
    config: SearchConfig,
    answerers: AnswererRegistry,
    plugins: PluginRegistry,
}

impl Search {
    pub fn new(
        registry: EngineRegistry,
        executor: std::sync::Arc<dyn EngineExecutor>,
        config: SearchConfig,
    ) -> Self {
        Search {
            registry,
            executor,
            config,
            answerers: AnswererRegistry::new(),
            plugins: PluginRegistry::new(),
        }
    }

    pub fn with_answerers(mut self, answerers: AnswererRegistry) -> Self {
        self.answerers = answerers;
        self
    }

    pub fn answerers(&self) -> &AnswererRegistry {
        &self.answerers
    }

    pub fn with_plugins(mut self, plugins: PluginRegistry) -> Self {
        self.plugins = plugins;
        self
    }

    pub fn plugins(&self) -> &PluginRegistry {
        &self.plugins
    }

    pub fn registry(&self) -> &EngineRegistry {
        &self.registry
    }

    pub fn registry_mut(&mut self) -> &mut EngineRegistry {
        &mut self.registry
    }

    pub async fn run_engines<P: EnginePreferences + ?Sized>(
        &self,
        query: &SearchQuery,
        prefs: &P,
        available_tokens: &HashSet<String>,
    ) -> ExecutionReport {
        let now = Instant::now();
        let selected = self.registry.select(query, prefs, available_tokens, now);

        let view = search_query_view(query);
        let deadline = now + self.request_timeout(query);

        let report = run_engines(
            self.executor.clone(),
            selected,
            view,
            self.config.default_engine_timeout,
            deadline,
        )
        .await;
        self.registry
            .record_outcomes(&report, &self.config.suspension, Instant::now());
        report
    }

    pub async fn run<P: EnginePreferences + ?Sized>(
        &self,
        query: &SearchQuery,
        prefs: &P,
        available_tokens: &HashSet<String>,
        recorder: &dyn MetricsRecorder,
    ) -> ResultContainer {
        self.run_with_plugin_ctx(
            query,
            prefs,
            available_tokens,
            recorder,
            &PluginCtx::all_enabled(),
        )
        .await
    }

    pub async fn run_with_plugin_ctx<P: EnginePreferences + ?Sized>(
        &self,
        query: &SearchQuery,
        prefs: &P,
        available_tokens: &HashSet<String>,
        recorder: &dyn MetricsRecorder,
        plugin_ctx: &PluginCtx,
    ) -> ResultContainer {
        let mut query = query.clone();
        let proceed = self.run_pre_search_plugins(&mut query, plugin_ctx);

        let mut container = if proceed {
            let now = Instant::now();
            let report = self.run_engines(&query, prefs, available_tokens).await;
            let weights = self.engine_weights();
            let mut container = aggregate(report, &weights, recorder);
            for (engine, reason) in
                self.registry
                    .suspended_for_query(&query, prefs, available_tokens, now)
            {
                if container
                    .unresponsive_engines
                    .iter()
                    .any(|entry| entry.engine == engine)
                {
                    continue;
                }
                container.unresponsive_engines.push(UnresponsiveEngine {
                    engine,
                    cause: UnresponsiveCause::Error(reason),
                });
            }
            container
        } else {
            ResultContainer::default()
        };

        let answers = self.answerers.ask(&query);
        container.answers.extend(answers);

        self.run_plugins(&query, &mut container, plugin_ctx);

        container
    }

    fn run_pre_search_plugins(&self, query: &mut SearchQuery, ctx: &PluginCtx) -> bool {
        if self.plugins.is_empty() {
            return true;
        }
        self.plugins.run_pre_search(query, ctx)
    }

    fn run_plugins(&self, query: &SearchQuery, container: &mut ResultContainer, ctx: &PluginCtx) {
        if self.plugins.is_empty() {
            return;
        }

        let plugin_answers = self.plugins.run_pre_search_answers(query, ctx);

        container
            .results
            .retain_mut(|result| self.plugins.run_on_result(result, query, ctx));

        self.plugins.run_on_results(container, query, ctx);
        self.plugins.run_post_search(container, query, ctx);

        container.answers.extend(plugin_answers);
        container.number_of_results = container.results.len();
    }

    pub fn engine_weights(&self) -> EngineWeights {
        let weights: HashMap<String, f64> = self
            .registry
            .engines()
            .iter()
            .map(|re| {
                let meta = re.engine.metadata();
                let weight = re.weight.unwrap_or(meta.weight as f64);
                (meta.name.clone(), weight)
            })
            .collect();
        EngineWeights::new(weights)
    }

    fn request_timeout(&self, query: &SearchQuery) -> Duration {
        match query.timeout {
            Some(timeout) => timeout.min(self.config.max_request_timeout),
            None => self.config.max_request_timeout,
        }
    }
}

pub fn search_query_view(query: &SearchQuery) -> SearchQueryView {
    SearchQueryView {
        query: query.query.clone(),
        pageno: query.pageno,
        safesearch: map_safesearch(query.safesearch),
        time_range: query.time_range.map(map_time_range),
        locale: query.locale.as_str().to_string(),
        categories: query.categories.clone(),
        engines: query.engines.clone(),
        engine_data: query.engine_data.clone(),
    }
}

pub fn engine_query_view(view: &SearchQueryView, meta: &EngineMeta) -> SearchQueryView {
    let mut tailored = view.clone();
    if !meta.safesearch {
        tailored.safesearch = zoeken_engine_core::SafeSearch::Off;
    }
    if !meta.time_range_support {
        tailored.time_range = None;
    }
    tailored
}

fn map_safesearch(level: zoeken_query::SafeSearch) -> zoeken_engine_core::SafeSearch {
    match level {
        zoeken_query::SafeSearch::Off => zoeken_engine_core::SafeSearch::Off,
        zoeken_query::SafeSearch::Moderate => zoeken_engine_core::SafeSearch::Moderate,
        zoeken_query::SafeSearch::Strict => zoeken_engine_core::SafeSearch::Strict,
    }
}

fn map_time_range(range: zoeken_query::TimeRange) -> zoeken_engine_core::TimeRange {
    match range {
        zoeken_query::TimeRange::Day => zoeken_engine_core::TimeRange::Day,
        zoeken_query::TimeRange::Week => zoeken_engine_core::TimeRange::Week,
        zoeken_query::TimeRange::Month => zoeken_engine_core::TimeRange::Month,
        zoeken_query::TimeRange::Year => zoeken_engine_core::TimeRange::Year,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    use zoeken_engine_core::{
        Engine, EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams,
    };
    use zoeken_query::{Locale, SafeSearch, TimeRange};
    use zoeken_results::{MainResult, Result_};

    struct StubEngine {
        meta: EngineMeta,
    }

    fn stub(name: &str, categories: &[&str]) -> Arc<dyn Engine> {
        Arc::new(StubEngine {
            meta: EngineMeta {
                name: name.to_string(),
                categories: categories.iter().map(|c| c.to_string()).collect(),
                ..EngineMeta::default()
            },
        })
    }

    impl Engine for StubEngine {
        fn metadata(&self) -> &EngineMeta {
            &self.meta
        }
        fn request(&self, _q: &SearchQueryView, _p: &mut RequestParams) {}
        fn response(&self, _resp: &EngineResponse) -> Result<EngineResults, EngineError> {
            Ok(EngineResults::new())
        }
    }

    struct ImmediateExecutor;

    impl EngineExecutor for ImmediateExecutor {
        fn execute(&self, engine: Arc<dyn Engine>, _query: SearchQueryView) -> EngineFuture {
            let name = engine.metadata().name.clone();
            Box::pin(async move {
                let mut results = EngineResults::new();
                results.add(Result_::Main(MainResult {
                    url: format!("https://{name}.test/"),
                    normalized_url: format!("https://{name}.test/"),
                    title: name.clone(),
                    engine: name,
                    ..MainResult::default()
                }));
                EngineExecResult::from_result(Ok(results))
            })
        }
    }

    fn search_query() -> SearchQuery {
        SearchQuery {
            categories: vec!["general".to_string()],
            ..SearchQuery::default()
        }
    }

    #[test]
    fn search_query_view_maps_fields() {
        let mut q = search_query();
        q.query = "rust".to_string();
        q.pageno = 3;
        q.safesearch = SafeSearch::Strict;
        q.time_range = Some(TimeRange::Week);
        q.locale = Locale::new("en-US");
        q.engines = vec!["alpha".to_string()];

        let view = search_query_view(&q);
        assert_eq!(view.query, "rust");
        assert_eq!(view.pageno, 3);
        assert_eq!(view.safesearch, zoeken_engine_core::SafeSearch::Strict);
        assert_eq!(view.time_range, Some(zoeken_engine_core::TimeRange::Week));
        assert_eq!(view.locale, "en-US");
        assert_eq!(view.categories, vec!["general".to_string()]);
        assert_eq!(view.engines, vec!["alpha".to_string()]);
    }

    fn meta_with_support(safesearch: bool, time_range: bool) -> EngineMeta {
        EngineMeta {
            name: "eng".to_string(),
            safesearch,
            time_range_support: time_range,
            ..EngineMeta::default()
        }
    }

    fn filtered_view() -> SearchQueryView {
        SearchQueryView {
            safesearch: zoeken_engine_core::SafeSearch::Strict,
            time_range: Some(zoeken_engine_core::TimeRange::Week),
            ..SearchQueryView::default()
        }
    }

    #[test]
    fn supporting_engine_keeps_both_filters() {
        let view = engine_query_view(&filtered_view(), &meta_with_support(true, true));
        assert_eq!(view.safesearch, zoeken_engine_core::SafeSearch::Strict);
        assert_eq!(view.time_range, Some(zoeken_engine_core::TimeRange::Week));
    }

    #[test]
    fn supporting_engine_receives_default_when_unspecified() {
        let base = SearchQueryView {
            safesearch: zoeken_engine_core::SafeSearch::Moderate,
            time_range: None,
            ..SearchQueryView::default()
        };
        let view = engine_query_view(&base, &meta_with_support(true, true));
        assert_eq!(view.safesearch, zoeken_engine_core::SafeSearch::Moderate);
        assert_eq!(view.time_range, None);
    }

    #[test]
    fn non_supporting_engine_omits_unsupported_filters() {
        let view = engine_query_view(&filtered_view(), &meta_with_support(false, false));
        assert_eq!(view.safesearch, zoeken_engine_core::SafeSearch::Off);
        assert_eq!(view.time_range, None);
    }

    #[test]
    fn each_filter_is_propagated_independently() {
        // An engine that supports only safesearch keeps it but loses the time
        // range; the reverse holds for a time-range-only engine.
        let safe_only = engine_query_view(&filtered_view(), &meta_with_support(true, false));
        assert_eq!(safe_only.safesearch, zoeken_engine_core::SafeSearch::Strict);
        assert_eq!(safe_only.time_range, None);

        let time_only = engine_query_view(&filtered_view(), &meta_with_support(false, true));
        assert_eq!(time_only.safesearch, zoeken_engine_core::SafeSearch::Off);
        assert_eq!(
            time_only.time_range,
            Some(zoeken_engine_core::TimeRange::Week)
        );
    }

    #[tokio::test]
    async fn run_engines_selects_and_executes() {
        let registry = EngineRegistry::from_engines([
            RegisteredEngine::new(stub("alpha", &["general"])),
            RegisteredEngine::new(stub("beta", &["images"])),
        ]);
        let search = Search::new(
            registry,
            Arc::new(ImmediateExecutor),
            SearchConfig::default(),
        );

        let report = search
            .run_engines(&search_query(), &AllEnginesEnabled, &HashSet::new())
            .await;

        assert_eq!(report.outcomes.len(), 1);
        let responders = report.responders();
        assert_eq!(responders.len(), 1);
        assert_eq!(responders[0].0, "alpha");
        assert!(report.unresponsive_engines().is_empty());
    }

    #[tokio::test]
    async fn run_folds_answerer_output_into_container() {
        let registry =
            EngineRegistry::from_engines([RegisteredEngine::new(stub("alpha", &["general"]))]);
        let search = Search::new(
            registry,
            Arc::new(ImmediateExecutor),
            SearchConfig::default(),
        )
        .with_answerers(zoeken_answerers::AnswererRegistry::with_builtins());

        let mut query = search_query();
        query.query = "sum 1 2 3".to_string();

        let container = search
            .run(&query, &AllEnginesEnabled, &HashSet::new(), &NoopRecorder)
            .await;

        assert_eq!(container.answers.len(), 1);
        assert!(container.answers[0].answer.contains('6'));
        assert_eq!(container.results.len(), 1);
    }

    #[tokio::test]
    async fn run_folds_plugin_answers_and_runs_post_search() {
        let registry =
            EngineRegistry::from_engines([RegisteredEngine::new(stub("alpha", &["general"]))]);
        let plugin_dir = workspace_root()
            .join("zoeken")
            .join("zoeken-plugins")
            .join("plugins");
        let plugins = zoeken_plugins::PluginRegistry::from_plugins(
            zoeken_plugins::lua::load_plugins_from_dir(
                &plugin_dir,
                Arc::new(zoeken_data::DataBundle::default()),
                zoeken_plugins::lua::LuaRuntimeConfig::default(),
            ),
        );
        let search = Search::new(
            registry,
            Arc::new(ImmediateExecutor),
            SearchConfig::default(),
        )
        .with_plugins(plugins);

        let mut query = search_query();
        query.query = "2 + 2".to_string();

        let ctx = zoeken_plugins::PluginCtx::new(zoeken_plugins::PluginGating {
            globally_enabled: true,
            per_plugin: [("calculator".to_string(), true)].into_iter().collect(),
            default_enabled: true,
        });
        let container = search
            .run_with_plugin_ctx(
                &query,
                &AllEnginesEnabled,
                &HashSet::new(),
                &NoopRecorder,
                &ctx,
            )
            .await;

        assert!(container.answers.iter().any(|a| a.answer == "4"));
        assert_eq!(container.results.len(), 1);
        assert_eq!(container.number_of_results, 1);
    }

    #[tokio::test]
    async fn run_plugins_can_remove_results_via_post_search() {
        struct OnionExecutor;
        impl EngineExecutor for OnionExecutor {
            fn execute(&self, engine: Arc<dyn Engine>, _q: SearchQueryView) -> EngineFuture {
                let name = engine.metadata().name.clone();
                Box::pin(async move {
                    let mut results = EngineResults::new();
                    results.add(Result_::Main(MainResult {
                        url: "http://blockedonionxxxx.onion/".to_string(),
                        normalized_url: "http://blockedonionxxxx.onion/".to_string(),
                        title: "blocked".to_string(),
                        engine: name,
                        ..MainResult::default()
                    }));
                    EngineExecResult::from_result(Ok(results))
                })
            }
        }

        let registry =
            EngineRegistry::from_engines([RegisteredEngine::new(stub("alpha", &["general"]))]);
        let mut data = zoeken_data::DataBundle::default();
        data.plugin_data.using_tor_proxy = true;
        data.ahmia_blacklist
            .insert("e1cce76deeda180a4f77b14fde215aa1".to_string());
        let plugin_dir = workspace_root()
            .join("zoeken")
            .join("zoeken-plugins")
            .join("plugins");
        let plugins = zoeken_plugins::PluginRegistry::from_plugins(
            zoeken_plugins::lua::load_plugins_from_dir(
                &plugin_dir,
                Arc::new(data),
                zoeken_plugins::lua::LuaRuntimeConfig::default(),
            ),
        );
        let search = Search::new(registry, Arc::new(OnionExecutor), SearchConfig::default())
            .with_plugins(plugins);

        let container = search
            .run(
                &search_query(),
                &AllEnginesEnabled,
                &HashSet::new(),
                &NoopRecorder,
            )
            .await;

        assert!(container.results.is_empty());
        assert_eq!(container.number_of_results, 0);
    }

    fn workspace_root() -> std::path::PathBuf {
        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .parent()
            .and_then(|p| p.parent())
            .expect("workspace root")
            .to_path_buf()
    }

    #[tokio::test]
    async fn run_without_answerers_produces_no_local_answers() {
        let registry =
            EngineRegistry::from_engines([RegisteredEngine::new(stub("alpha", &["general"]))]);
        let search = Search::new(
            registry,
            Arc::new(ImmediateExecutor),
            SearchConfig::default(),
        );

        let mut query = search_query();
        query.query = "sum 1 2 3".to_string();

        let container = search
            .run(&query, &AllEnginesEnabled, &HashSet::new(), &NoopRecorder)
            .await;

        assert!(container.answers.is_empty());
    }

    #[test]
    fn request_timeout_clamps_to_ceiling() {
        let search = Search::new(
            EngineRegistry::new(),
            Arc::new(ImmediateExecutor),
            SearchConfig {
                default_engine_timeout: Duration::from_secs(3),
                max_request_timeout: Duration::from_secs(10),
                suspension: SuspensionPolicy::default(),
            },
        );

        let mut q = search_query();
        assert_eq!(search.request_timeout(&q), Duration::from_secs(10));

        q.timeout = Some(Duration::from_secs(4));
        assert_eq!(search.request_timeout(&q), Duration::from_secs(4));

        q.timeout = Some(Duration::from_secs(30));
        assert_eq!(search.request_timeout(&q), Duration::from_secs(10));
    }
}
