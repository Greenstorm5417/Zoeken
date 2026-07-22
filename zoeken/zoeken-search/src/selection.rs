//! Engine selection: compute which engines to query.

use std::collections::HashSet;
use std::sync::Arc;
use std::time::Duration;

use zoeken_engine_core::{Engine, EngineError, SuspendConfig};
use zoeken_query::SearchQuery;

/// Check if an engine is enabled in user preferences.
pub trait EnginePreferences {
    fn is_engine_enabled(&self, engine: &str) -> bool;
}

#[derive(Debug, Clone, Copy, Default)]
pub struct AllEnginesEnabled;

impl EnginePreferences for AllEnginesEnabled {
    fn is_engine_enabled(&self, _engine: &str) -> bool {
        true
    }
}

#[derive(Debug, Clone, Default)]
pub struct EnabledEngineSet {
    pub enabled: HashSet<String>,
}

impl EnabledEngineSet {
    pub fn new(names: impl IntoIterator<Item = impl Into<String>>) -> Self {
        EnabledEngineSet {
            enabled: names.into_iter().map(Into::into).collect(),
        }
    }
}

impl EnginePreferences for EnabledEngineSet {
    fn is_engine_enabled(&self, engine: &str) -> bool {
        self.enabled.contains(engine)
    }
}

#[derive(Clone)]
pub struct RegisteredEngine {
    pub engine: Arc<dyn Engine>,
    pub disabled: bool,
    pub tokens: Vec<String>,
    pub timeout: Option<Duration>,
    pub weight: Option<f64>,
    pub categories: Option<Vec<String>>,
    pub shortcuts: Vec<String>,
}

impl RegisteredEngine {
    pub fn new(engine: Arc<dyn Engine>) -> Self {
        RegisteredEngine {
            engine,
            disabled: false,
            tokens: Vec::new(),
            timeout: None,
            weight: None,
            categories: None,
            shortcuts: Vec::new(),
        }
    }

    pub fn disabled(mut self) -> Self {
        self.disabled = true;
        self
    }

    pub fn with_tokens(mut self, tokens: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.tokens = tokens.into_iter().map(Into::into).collect();
        self
    }

    pub fn with_timeout(mut self, timeout: Duration) -> Self {
        self.timeout = Some(timeout);
        self
    }

    pub fn with_weight(mut self, weight: f64) -> Self {
        self.weight = Some(weight);
        self
    }

    pub fn with_categories(
        mut self,
        categories: impl IntoIterator<Item = impl Into<String>>,
    ) -> Self {
        self.categories = Some(categories.into_iter().map(Into::into).collect());
        self
    }

    pub fn with_shortcut(mut self, shortcut: impl Into<String>) -> Self {
        let shortcut = shortcut.into();
        if !shortcut.is_empty() {
            self.shortcuts.push(shortcut);
        }
        self
    }

    pub fn name(&self) -> &str {
        &self.engine.metadata().name
    }
}

#[derive(Clone)]
pub struct SelectedEngine {
    pub name: String,
    pub engine: Arc<dyn Engine>,
    pub timeout: Option<Duration>,
}

#[derive(Clone, Default)]
pub struct EngineRegistry {
    engines: Vec<RegisteredEngine>,
}

impl EngineRegistry {
    pub fn new() -> Self {
        EngineRegistry {
            engines: Vec::new(),
        }
    }

    pub fn from_engines(engines: impl IntoIterator<Item = RegisteredEngine>) -> Self {
        EngineRegistry {
            engines: engines.into_iter().collect(),
        }
    }

    pub fn register(&mut self, engine: RegisteredEngine) {
        self.engines.push(engine);
    }

    pub fn engines(&self) -> &[RegisteredEngine] {
        &self.engines
    }

    pub fn get_mut(&mut self, name: &str) -> Option<&mut RegisteredEngine> {
        self.engines.iter_mut().find(|e| e.name() == name)
    }

    pub fn select<P: EnginePreferences + ?Sized>(
        &self,
        query: &SearchQuery,
        prefs: &P,
        available_tokens: &HashSet<String>,
    ) -> Vec<SelectedEngine> {
        self.engines
            .iter()
            .filter(|re| self.is_eligible(re, query, prefs, available_tokens))
            .map(|re| SelectedEngine {
                name: re.name().to_string(),
                engine: re.engine.clone(),
                timeout: re.timeout,
            })
            .collect()
    }

    fn is_eligible<P: EnginePreferences + ?Sized>(
        &self,
        re: &RegisteredEngine,
        query: &SearchQuery,
        prefs: &P,
        available_tokens: &HashSet<String>,
    ) -> bool {
        let meta = re.engine.metadata();
        // A disabled engine can still be summoned explicitly (`!bang` or an
        // `engines=` selection), matching SearXNG semantics.
        let explicitly_requested = !query.engines.is_empty()
            && bang_match(&meta.name, &meta.shortcut, &re.shortcuts, &query.engines);
        if re.disabled && !explicitly_requested {
            return false;
        }
        if !has_required_tokens(&re.tokens, available_tokens) {
            return false;
        }
        if query.pageno > 1 && (!meta.paging || (meta.max_page > 0 && query.pageno > meta.max_page))
        {
            return false;
        }
        if query.time_range.is_some() && !meta.time_range_support {
            return false;
        }
        if query.safesearch != zoeken_query::SafeSearch::Off && !meta.safesearch {
            return false;
        }
        if !meta.language_support && !query.locale.is_all() && !query.locale.is_auto() {
            return false;
        }
        let categories = re
            .categories
            .as_deref()
            .unwrap_or(meta.categories.as_slice());
        category_match(categories, &query.categories)
            && bang_match(&meta.name, &meta.shortcut, &re.shortcuts, &query.engines)
            && prefs.is_engine_enabled(&meta.name)
    }
}

/// Cooldown durations from `search.suspended_times` / ban settings.
/// Shared by the storage circuit (`zoeken-server` `cooldown_for`).
#[derive(Debug, Clone, Copy)]
pub struct SuspensionPolicy {
    pub config: SuspendConfig,
    pub access_denied: Duration,
    pub captcha: Duration,
    pub too_many_requests: Duration,
    pub cf_captcha: Duration,
    pub cf_access_denied: Duration,
    pub recaptcha_captcha: Duration,
}

impl Default for SuspensionPolicy {
    fn default() -> Self {
        Self {
            config: SuspendConfig::default(),
            access_denied: Duration::from_secs(86_400),
            captcha: Duration::from_secs(86_400),
            too_many_requests: Duration::from_secs(3_600),
            cf_captcha: Duration::from_secs(1_296_000),
            cf_access_denied: Duration::from_secs(86_400),
            recaptcha_captcha: Duration::from_secs(604_800),
        }
    }
}

impl SuspensionPolicy {
    /// Explicit cooldown for known error kinds. Keyed by **engine name** so
    /// network-mapped captchas (payload ≠ engine id) still hit the DDG rule.
    /// `Duration::ZERO` means do not open a circuit (DDG captcha: no IP ban).
    pub fn explicit_duration(&self, engine: &str, error: &EngineError) -> Option<Duration> {
        match error {
            EngineError::AccessDenied(_) => Some(self.access_denied),
            EngineError::CloudflareAccessDenied(_) => Some(self.cf_access_denied),
            EngineError::Captcha(_) if engine == "duckduckgo" => Some(Duration::ZERO),
            EngineError::Captcha(_) => Some(self.captcha),
            EngineError::CloudflareCaptcha(_) => Some(self.cf_captcha),
            EngineError::RecaptchaCaptcha(_) => Some(self.recaptcha_captcha),
            EngineError::TooManyRequests(_) => Some(self.too_many_requests),
            _ => None,
        }
    }

    /// Build circuit cooldown policy from resolved health durations.
    #[must_use]
    #[allow(clippy::too_many_arguments)]
    pub fn from_durations(
        ban_time_on_fail: Duration,
        max_ban_time_on_fail: Duration,
        access_denied: Duration,
        captcha: Duration,
        too_many_requests: Duration,
        cf_captcha: Duration,
        cf_access_denied: Duration,
        recaptcha_captcha: Duration,
    ) -> Self {
        Self {
            config: SuspendConfig::new(1, ban_time_on_fail, max_ban_time_on_fail),
            access_denied,
            captcha,
            too_many_requests,
            cf_captcha,
            cf_access_denied,
            recaptcha_captcha,
        }
    }
}

fn has_required_tokens(required: &[String], available: &HashSet<String>) -> bool {
    required.iter().all(|token| available.contains(token))
}

fn category_match(engine_categories: &[String], query_categories: &[String]) -> bool {
    if query_categories.is_empty() {
        return true;
    }
    engine_categories
        .iter()
        .any(|c| query_categories.iter().any(|q| q == c))
}

fn bang_match(
    name: &str,
    meta_shortcut: &str,
    shortcuts: &[String],
    bang_engines: &[String],
) -> bool {
    bang_engines.is_empty()
        || bang_engines.iter().any(|e| {
            e == name || e == meta_shortcut || shortcuts.iter().any(|shortcut| shortcut == e)
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use zoeken_engine_core::{
        EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams, SearchQueryView,
    };

    struct StubEngine {
        meta: EngineMeta,
    }

    impl StubEngine {
        fn arc(name: &str, categories: &[&str]) -> Arc<dyn Engine> {
            Arc::new(StubEngine {
                meta: EngineMeta {
                    name: name.to_string(),
                    categories: categories.iter().map(|c| c.to_string()).collect(),
                    ..EngineMeta::default()
                },
            })
        }
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

    fn registry() -> EngineRegistry {
        EngineRegistry::from_engines([
            RegisteredEngine::new(StubEngine::arc("alpha", &["general", "web"])),
            RegisteredEngine::new(StubEngine::arc("beta", &["images"])),
            RegisteredEngine::new(StubEngine::arc("gamma", &["general"])),
        ])
    }

    fn query_with(categories: &[&str], engines: &[&str]) -> SearchQuery {
        SearchQuery {
            categories: categories.iter().map(|c| c.to_string()).collect(),
            engines: engines.iter().map(|e| e.to_string()).collect(),
            ..SearchQuery::default()
        }
    }

    fn names(selected: &[SelectedEngine]) -> Vec<String> {
        let mut n: Vec<String> = selected.iter().map(|s| s.name.clone()).collect();
        n.sort();
        n
    }

    #[test]
    fn selects_by_category() {
        let reg = registry();
        let q = query_with(&["general"], &[]);
        let selected = reg.select(&q, &AllEnginesEnabled, &HashSet::new());
        assert_eq!(names(&selected), vec!["alpha", "gamma"]);
    }

    #[test]
    fn bang_intersects_with_category() {
        let reg = registry();
        let q = query_with(&["general"], &["alpha", "beta"]);
        let selected = reg.select(&q, &AllEnginesEnabled, &HashSet::new());
        assert_eq!(names(&selected), vec!["alpha"]);
    }

    #[test]
    fn empty_categories_impose_no_restriction() {
        let reg = registry();
        let q = query_with(&[], &["beta"]);
        let selected = reg.select(&q, &AllEnginesEnabled, &HashSet::new());
        assert_eq!(names(&selected), vec!["beta"]);
    }

    #[test]
    fn preferences_filter_out_disabled_engines() {
        let reg = registry();
        let q = query_with(&["general"], &[]);
        let prefs = EnabledEngineSet::new(["alpha"]);
        let selected = reg.select(&q, &prefs, &HashSet::new());
        assert_eq!(names(&selected), vec!["alpha"]);
    }

    #[test]
    fn excludes_disabled_engine() {
        let reg = EngineRegistry::from_engines([
            RegisteredEngine::new(StubEngine::arc("alpha", &["general"])).disabled(),
            RegisteredEngine::new(StubEngine::arc("gamma", &["general"])),
        ]);
        let q = query_with(&["general"], &[]);
        let selected = reg.select(&q, &AllEnginesEnabled, &HashSet::new());
        assert_eq!(names(&selected), vec!["gamma"]);
    }

    #[test]
    fn excludes_tokenless_engine() {
        let reg = EngineRegistry::from_engines([
            RegisteredEngine::new(StubEngine::arc("alpha", &["general"])).with_tokens(["secret"]),
            RegisteredEngine::new(StubEngine::arc("gamma", &["general"])),
        ]);
        let q = query_with(&["general"], &[]);

        let selected = reg.select(&q, &AllEnginesEnabled, &HashSet::new());
        assert_eq!(names(&selected), vec!["gamma"]);

        let tokens = HashSet::from(["secret".to_string()]);
        let selected = reg.select(&q, &AllEnginesEnabled, &tokens);
        assert_eq!(names(&selected), vec!["alpha", "gamma"]);
    }

    #[test]
    fn duckduckgo_captcha_does_not_open_circuit_but_other_engines_do() {
        let policy = SuspensionPolicy::default();
        assert_eq!(
            policy.explicit_duration(
                "duckduckgo",
                &EngineError::Captcha("duckduckgo".to_string())
            ),
            Some(Duration::ZERO)
        );
        // Network-mapped captchas carry a message, not the engine id — still DDG.
        assert_eq!(
            policy.explicit_duration(
                "duckduckgo",
                &EngineError::Captcha("captcha: challenge page".to_string())
            ),
            Some(Duration::ZERO)
        );
        assert_eq!(
            policy.explicit_duration("bing", &EngineError::Captcha("bing".to_string())),
            Some(policy.captcha)
        );
    }
}
