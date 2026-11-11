use std::collections::{HashMap, HashSet};

use psl::Psl;
use zoeken_engine_core::{EngineResults, ErrorCategory};
use zoeken_query::{Intent, interpret};
use zoeken_results::{
    Answer, Correction, Infobox, InfoboxAttribute, InteractiveAnswer, Result_, Suggestion,
};

use crate::execution::{EngineRunStatus, ExecutionReport, UnresponsiveReason};
use crate::metrics::{EngineMetricsRecorder, EngineOutcome, EngineSample};

#[derive(Debug, Clone, Default)]
pub struct EngineWeights {
    weights: HashMap<String, f64>,
}

impl EngineWeights {
    pub fn new(weights: impl IntoIterator<Item = (String, f64)>) -> Self {
        EngineWeights {
            weights: weights.into_iter().collect(),
        }
    }

    pub fn weight_of(&self, engine: &str) -> f64 {
        self.weights.get(engine).copied().unwrap_or(1.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum UnresponsiveCause {
    /// Typed category plus the original message (message is diagnostic only;
    /// user-facing labels come from the category, not string matching).
    Error {
        category: ErrorCategory,
        message: String,
    },
    Timeout,
    DeadlineExceeded,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnresponsiveEngine {
    pub engine: String,
    pub cause: UnresponsiveCause,
}

/// Whether another results page is known to exist.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Continuation {
    /// A paging engine returned hits, but it did not say a further page exists.
    #[default]
    Unknown,
    /// An engine explicitly reported another page. Reserved; most providers never do.
    More,
    /// No selected engine can return a later page, or this page was empty and complete.
    Exhausted,
}

impl Continuation {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Continuation::Unknown => "unknown",
            Continuation::More => "more",
            Continuation::Exhausted => "exhausted",
        }
    }
}

/// How the search ended, without weakening the user's filters to get here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SearchOutcome {
    #[default]
    Ok,
    NoMatches,
    NoEngines,
    Partial,
    Failed,
    RateLimited,
}

impl SearchOutcome {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            SearchOutcome::Ok => "ok",
            SearchOutcome::NoMatches => "no_matches",
            SearchOutcome::NoEngines => "no_engines",
            SearchOutcome::Partial => "partial",
            SearchOutcome::Failed => "failed",
            SearchOutcome::RateLimited => "rate_limited",
        }
    }
}

/// Per-engine timing captured while aggregating. Durations, not query text.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EngineTiming {
    pub engine: String,
    pub elapsed_ms: u64,
    pub status: String,
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ResultContainer {
    pub results: Vec<Result_>,
    pub answers: Vec<Answer>,
    pub suggestions: Vec<Suggestion>,
    pub corrections: Vec<Correction>,
    pub infoboxes: Vec<Infobox>,
    pub unresponsive_engines: Vec<UnresponsiveEngine>,
    pub engine_data: HashMap<String, String>,
    pub number_of_results: usize,
    pub continuation: Continuation,
    pub outcome: SearchOutcome,
    pub selected_engines: usize,
    pub failed_engines: usize,
    pub rate_limited_engines: usize,
    pub operator_notes: Vec<String>,
    pub elapsed_ms: u64,
    pub timings: Vec<EngineTiming>,
}

impl From<EngineResults> for ResultContainer {
    /// Lift a single-engine bag into an aggregate container (no run metadata).
    fn from(results: EngineResults) -> Self {
        Self {
            results: results.results,
            answers: results.answers,
            suggestions: results.suggestions,
            corrections: results.corrections,
            infoboxes: results.infoboxes,
            unresponsive_engines: Vec::new(),
            engine_data: results.engine_data,
            number_of_results: 0,
            ..Self::default()
        }
    }
}

struct Merged {
    result: Result_,
    engines: Vec<String>,
}

/// Merge engine outcomes and score each URL as `sum(weight / position)`.
pub fn aggregate(report: ExecutionReport, weights: &EngineWeights) -> ResultContainer {
    aggregate_query(report, weights, "")
}

/// Same as [`aggregate`], with a query so matching titles and hosts rank higher.
pub fn aggregate_query(
    report: ExecutionReport,
    weights: &EngineWeights,
    query: &str,
) -> ResultContainer {
    let mut builder = ContainerBuilder::default();
    let metrics = EngineMetricsRecorder::new();

    for outcome in report.outcomes {
        builder.observe(&outcome);
        let sample_outcome = match &outcome.status {
            EngineRunStatus::Completed(results) => EngineOutcome::Completed {
                results: results.results.len(),
            },
            EngineRunStatus::Failed(error) => EngineOutcome::Failed { error },
            EngineRunStatus::Unresponsive(reason) => {
                EngineOutcome::Unresponsive { reason: *reason }
            }
        };
        metrics.record_engine(EngineSample {
            engine: &outcome.engine,
            duration: outcome.duration,
            http_duration: outcome.http_duration,
            outcome: sample_outcome,
        });

        match outcome.status {
            EngineRunStatus::Completed(results) => {
                builder.ingest(&outcome.engine, results, weights);
            }
            EngineRunStatus::Failed(error) => {
                builder.add_unresponsive(
                    outcome.engine,
                    UnresponsiveCause::Error {
                        category: ErrorCategory::from(&error),
                        message: error.to_string(),
                    },
                );
            }
            EngineRunStatus::Unresponsive(reason) => {
                let cause = match reason {
                    UnresponsiveReason::EngineTimeout => UnresponsiveCause::Timeout,
                    UnresponsiveReason::GlobalDeadline => UnresponsiveCause::DeadlineExceeded,
                };
                builder.add_unresponsive(outcome.engine, cause);
            }
        }
    }

    builder.finish(weights, query)
}

#[derive(Default)]
struct ContainerBuilder {
    merged: Vec<Merged>,
    by_key: HashMap<String, usize>,
    answers: Vec<Answer>,
    seen_answers: HashSet<String>,
    /// Case-insensitive key → (display suggestion, engine-weight sum, hit count).
    suggestion_rank: HashMap<String, (Suggestion, f64, usize)>,
    corrections: Vec<Correction>,
    seen_corrections: HashSet<String>,
    infoboxes: Vec<Infobox>,
    seen_infoboxes: HashSet<String>,
    unresponsive_engines: Vec<UnresponsiveEngine>,
    engine_data: HashMap<String, String>,
    selected: usize,
    failed: usize,
    rate_limited: usize,
    timings: Vec<EngineTiming>,
}

impl ContainerBuilder {
    fn observe(&mut self, outcome: &crate::execution::EngineRunOutcome) {
        self.selected += 1;
        let status = match &outcome.status {
            crate::execution::EngineRunStatus::Completed(_) => "completed",
            crate::execution::EngineRunStatus::Failed(error) => {
                self.failed += 1;
                if ErrorCategory::from(error) == ErrorCategory::RateLimited {
                    self.rate_limited += 1;
                    "rate_limited"
                } else {
                    "failed"
                }
            }
            crate::execution::EngineRunStatus::Unresponsive(
                crate::execution::UnresponsiveReason::EngineTimeout,
            ) => "timeout",
            crate::execution::EngineRunStatus::Unresponsive(
                crate::execution::UnresponsiveReason::GlobalDeadline,
            ) => "deadline",
        };
        self.timings.push(EngineTiming {
            engine: outcome.engine.clone(),
            elapsed_ms: u64::try_from(outcome.duration.as_millis()).unwrap_or(u64::MAX),
            status: status.to_string(),
        });
    }

    fn ingest(&mut self, engine: &str, results: EngineResults, weights: &EngineWeights) {
        let EngineResults {
            results,
            answers,
            suggestions,
            corrections,
            infoboxes,
            engine_data,
        } = results;
        self.engine_data.extend(engine_data);

        for (idx, mut result) in results.into_iter().enumerate() {
            let position = idx + 1;
            ensure_engine(&mut result, engine);
            self.merge_main(engine, result, position);
        }

        for mut answer in answers {
            if answer.engine.is_empty() {
                answer.engine = engine.to_string();
            }
            if self.seen_answers.insert(answer.answer.clone()) {
                self.answers.push(answer);
            }
        }
        let engine_weight = weights.weight_of(engine);
        for mut suggestion in suggestions {
            if suggestion.engine.is_empty() {
                suggestion.engine = engine.to_string();
            }
            let key = suggestion.suggestion.trim().to_ascii_lowercase();
            if key.is_empty() {
                continue;
            }
            self.suggestion_rank
                .entry(key)
                .and_modify(|(_, weight_sum, count)| {
                    *weight_sum += engine_weight;
                    *count += 1;
                })
                .or_insert((suggestion, engine_weight, 1));
        }
        for mut correction in corrections {
            if correction.engine.is_empty() {
                correction.engine = engine.to_string();
            }
            if self.seen_corrections.insert(correction.correction.clone()) {
                self.corrections.push(correction);
            }
        }
        for mut infobox in infoboxes {
            if infobox.engine.is_empty() {
                infobox.engine = engine.to_string();
            }
            let key = format!(
                "{}|{}",
                infobox.infobox,
                infobox.id.as_deref().unwrap_or("")
            );
            if self.seen_infoboxes.insert(key) {
                self.infoboxes.push(infobox);
            }
        }
    }

    fn merge_main(&mut self, engine: &str, result: Result_, position: usize) {
        match merge_key(&result) {
            Some(key) => match self.by_key.get(&key).copied() {
                Some(existing) => {
                    let merged = &mut self.merged[existing];
                    maybe_upgrade_content(&mut merged.result, &result);
                    record_engine_position(merged, engine, position);
                }
                None => {
                    self.by_key.insert(key, self.merged.len());
                    self.push_new(engine, result, position);
                }
            },
            None => self.push_new(engine, result, position),
        }
    }

    fn push_new(&mut self, engine: &str, mut result: Result_, position: usize) {
        set_positions(&mut result, vec![position]);
        let engines = vec![engine.to_string()];
        set_engines(&mut result, &engines);
        self.merged.push(Merged { result, engines });
    }

    fn add_unresponsive(&mut self, engine: String, cause: UnresponsiveCause) {
        self.unresponsive_engines
            .push(UnresponsiveEngine { engine, cause });
    }

    fn finish(self, weights: &EngineWeights, query: &str) -> ResultContainer {
        let mut results: Vec<Result_> = self
            .merged
            .into_iter()
            .map(|merged| {
                let score = score_of(&merged, weights, query);
                let mut result = merged.result;
                set_score(&mut result, score);
                result
            })
            .collect();

        sort_results(&mut results);
        let engine_names: Vec<String> = self
            .timings
            .iter()
            .map(|timing| timing.engine.clone())
            .collect();
        let (results, operator_notes) = apply_query_policy(results, query, &engine_names);

        // Frequency × engine-weight sum; case-insensitive dedupe already done in ingest.
        let mut ranked: Vec<(Suggestion, f64, usize)> =
            self.suggestion_rank.into_values().collect();
        ranked.sort_by(|(a, aw, ac), (b, bw, bc)| {
            let score_a = aw * (*ac as f64);
            let score_b = bw * (*bc as f64);
            score_b
                .partial_cmp(&score_a)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    a.suggestion
                        .to_ascii_lowercase()
                        .cmp(&b.suggestion.to_ascii_lowercase())
                })
        });
        let suggestions: Vec<Suggestion> = ranked.into_iter().map(|(s, _, _)| s).collect();

        let mut answers = self.answers;
        let mut infoboxes = self.infoboxes;
        coalesce_knowledge_panels(&mut answers, &mut infoboxes);

        let number_of_results = results.len();
        let problems = self.unresponsive_engines.len();
        let outcome = if self.selected == 0 {
            SearchOutcome::NoEngines
        } else if results.is_empty()
            && problems == self.selected
            && self.rate_limited > 0
            && self.rate_limited * 2 >= self.selected
        {
            SearchOutcome::RateLimited
        } else if results.is_empty() && problems == self.selected {
            SearchOutcome::Failed
        } else if results.is_empty() {
            SearchOutcome::NoMatches
        } else if problems > 0 {
            SearchOutcome::Partial
        } else {
            SearchOutcome::Ok
        };
        ResultContainer {
            results,
            answers,
            suggestions,
            corrections: self.corrections,
            infoboxes,
            unresponsive_engines: self.unresponsive_engines,
            engine_data: self.engine_data,
            number_of_results,
            continuation: Continuation::Unknown,
            outcome,
            selected_engines: self.selected,
            failed_engines: self.failed,
            rate_limited_engines: self.rate_limited,
            operator_notes,
            elapsed_ms: 0,
            timings: self.timings,
        }
    }
}

/// One contribution per engine: `weight / position`.
/// ponytail: title/host overlap, not BM25. Revisit if SEO junk still wins live.
fn score_of(merged: &Merged, weights: &EngineWeights, query: &str) -> f64 {
    let positions = positions_of(&merged.result);
    if positions.is_empty() {
        return 0.0;
    }
    let priority = priority_of(&merged.result);
    if priority == "low" {
        return 0.0;
    }
    let mut rank_score = 0.0;
    for (i, &position) in positions.iter().enumerate() {
        let engine = merged.engines.get(i).map(String::as_str).unwrap_or("");
        let weight = weights.weight_of(engine);
        if priority == "high" {
            rank_score += weight;
        } else {
            rank_score += weight / (position.max(1) as f64);
        }
    }
    rank_score
        * relevance(
            query,
            title_of(&merged.result),
            content_of(&merged.result),
            url_of(&merged.result),
        )
}

fn record_engine_position(merged: &mut Merged, engine: &str, position: usize) {
    if let Some(i) = merged.engines.iter().position(|e| e == engine) {
        keep_best_position(&mut merged.result, i, position);
        return;
    }
    merged.engines.push(engine.to_string());
    push_position(&mut merged.result, position);
    set_engines(&mut merged.result, &merged.engines);
}

const QUERY_STOPWORDS: &[&str] = &[
    "a", "an", "the", "of", "and", "or", "in", "on", "to", "for", "is", "at", "by", "from",
];

const SKIP_HOST_LABELS: &[&str] = &[
    "www", "com", "org", "net", "io", "edu", "gov", "co", "uk", "html", "https", "http",
];

fn contains_term(haystack: &str, term: &str) -> bool {
    crate::text::tokenize(haystack)
        .iter()
        .any(|word| word == term)
}

fn host_boost(url: &str, terms: &[String]) -> f64 {
    let Ok(parsed) = url::Url::parse(url) else {
        return 0.0;
    };
    let Some(host) = parsed.host_str() else {
        return 0.0;
    };
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host);
    let hit = host.split(['.', '-']).any(|label| {
        label.len() >= 3
            && !SKIP_HOST_LABELS.contains(&label)
            && terms.iter().any(|term| term == label)
    });
    if hit { 0.75 } else { 0.0 }
}

/// Title, snippet, and hostname overlap in `[1, ~2.4]`. Public for the fusion experiment.
#[must_use]
pub fn relevance_factor(query: &str, title: &str, content: &str, url: &str) -> f64 {
    let q = query.trim();
    if q.is_empty() {
        return 1.0;
    }
    let mut terms: Vec<String> = crate::text::tokenize(q)
        .into_iter()
        .filter(|term| !QUERY_STOPWORDS.contains(&term.as_str()))
        .filter(|term| term.chars().count() >= 2 || term.contains('+') || term.contains('#'))
        .collect();
    if terms.is_empty() && q.chars().count() == 1 && q.chars().all(|c| c.is_alphabetic()) {
        terms.push(q.to_lowercase());
    }
    let n = terms.len() as f64;
    if n == 0.0 {
        return 1.0;
    }
    let title_hits = terms.iter().filter(|t| contains_term(title, t)).count();
    let content_hits = terms.iter().filter(|t| contains_term(content, t)).count();
    let phrases = crate::text::quoted_phrases(q);
    let phrase_hits = phrases
        .iter()
        .filter(|phrase| {
            let needle = phrase.to_lowercase();
            title.to_lowercase().contains(&needle) || content.to_lowercase().contains(&needle)
        })
        .count();
    let phrase = if phrases.is_empty() {
        0.0
    } else {
        0.35 * (phrase_hits as f64 / phrases.len() as f64)
    };
    1.0 + 0.5 * (title_hits as f64 / n)
        + 0.15 * (content_hits as f64 / n)
        + host_boost(url, &terms)
        + phrase
}

fn relevance(query: &str, title: &str, content: &str, url: &str) -> f64 {
    relevance_factor(query, title, content, url)
}

/// Score ↓, then engine consensus ↓, best position ↑, URL ↑.
/// Equal scores are common when redirect wrappers block merge; without
/// tie-breakers, ingest order (engine finish order) decides the top hit.
pub fn sort_results(results: &mut [Result_]) {
    results.sort_by(|a, b| {
        score_of_field(b)
            .partial_cmp(&score_of_field(a))
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| engine_count(b).cmp(&engine_count(a)))
            .then_with(|| min_position(a).cmp(&min_position(b)))
            .then_with(|| url_of(a).cmp(url_of(b)))
    });
}

fn min_position(result: &Result_) -> usize {
    positions_of(result)
        .iter()
        .copied()
        .min()
        .unwrap_or(usize::MAX)
}

fn engine_count(result: &Result_) -> usize {
    match result {
        Result_::Main(r) if !r.engines.is_empty() => r.engines.len(),
        Result_::Main(r) => usize::from(!r.engine.is_empty()),
        Result_::Image(r) => usize::from(!r.engine.is_empty()),
        Result_::Paper(r) => usize::from(!r.engine.is_empty()),
        Result_::Code(r) => usize::from(!r.engine.is_empty()),
        Result_::File(r) => usize::from(!r.engine.is_empty()),
        Result_::KeyValue(r) => usize::from(!r.engine.is_empty()),
        _ => 0,
    }
}

fn ensure_engine(result: &mut Result_, engine: &str) {
    macro_rules! fill {
        ($r:expr) => {
            if $r.engine.is_empty() {
                $r.engine = engine.to_string();
            }
        };
    }
    match result {
        Result_::Main(r) => fill!(r),
        Result_::Image(r) => fill!(r),
        Result_::Paper(r) => fill!(r),
        Result_::Code(r) => fill!(r),
        Result_::File(r) => fill!(r),
        Result_::KeyValue(r) => fill!(r),
        _ => {}
    }
}

fn merge_key(result: &Result_) -> Option<String> {
    let normalized = normalized_url_of(result);
    if !normalized.is_empty() {
        // Prefer the normalized URL when it is present so aliases collapse.
        return Some(canonical_merge_key(normalized));
    }
    let url = url_of(result);
    if !url.is_empty() {
        return Some(canonical_merge_key(url));
    }
    None
}

fn canonical_merge_key(raw: &str) -> String {
    let unwrapped = unwrap_engine_redirect(raw);
    let Ok(mut url) = url::Url::parse(&unwrapped) else {
        return unwrapped.trim_end_matches('/').to_ascii_lowercase();
    };

    // Normalize common URL variants so equivalent pages merge together.
    url.set_fragment(None);

    if matches!(url.scheme(), "http" | "https") {
        // Drop only default ports. :8080 and :8443 are different services.
        if matches!(url.port(), Some(80 | 443) | None) {
            let _ = url.set_port(None);
        }
        // Engines disagree on scheme for the same page; treat http == https.
        let _ = url.set_scheme("https");
        strip_tracking_params(&mut url);
    }

    if let Some(host) = url.host_str() {
        let host = canonical_host(host);
        let _ = url.set_host(Some(&host));
    }

    let path = strip_index_file(url.path().trim_end_matches('/'));
    // `/en` and `/fr` are different pages. Only an empty path is the site root.
    let canonical_path = if path.is_empty() {
        "/".to_string()
    } else {
        path.to_string()
    };
    url.set_path(&canonical_path);

    url.to_string()
}

fn canonical_host(host: &str) -> String {
    let host = host.to_ascii_lowercase();
    let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
    // Wikipedia's mobile host is the same article. Other `m.` hosts are not.
    if let Some(rest) = host.strip_prefix("m.")
        && (rest == "wikipedia.org" || rest.ends_with(".wikipedia.org"))
    {
        return rest.to_string();
    }
    if let Some((lang, rest)) = host.split_once(".m.")
        && (rest == "wikipedia.org" || rest.ends_with(".wikipedia.org"))
        && !lang.is_empty()
    {
        return format!("{lang}.{rest}");
    }
    host
}

fn strip_index_file(path: &str) -> &str {
    path.strip_suffix("/index.html")
        .or_else(|| path.strip_suffix("/index.htm"))
        .unwrap_or(path)
}

/// Peel common engine click-wrappers so the same page merges across engines.
fn unwrap_engine_redirect(raw: &str) -> String {
    let candidate = if raw.starts_with("//") {
        format!("https:{raw}")
    } else {
        raw.to_string()
    };
    let Ok(url) = url::Url::parse(&candidate) else {
        return raw.to_string();
    };
    let host = url.host_str().unwrap_or("").to_ascii_lowercase();

    if (host == "duckduckgo.com" || host.ends_with(".duckduckgo.com"))
        && url.path().starts_with("/l")
    {
        if let Some((_, value)) = url.query_pairs().find(|(key, _)| key == "uddg") {
            let dest = value.into_owned();
            if dest.starts_with("http://") || dest.starts_with("https://") {
                return dest;
            }
        }
    }

    if host.ends_with("google.com") && url.path() == "/url" {
        for (key, value) in url.query_pairs() {
            if key != "q" && key != "url" {
                continue;
            }
            let dest = value.into_owned();
            if dest.starts_with("http://") || dest.starts_with("https://") {
                return dest;
            }
        }
    }

    raw.to_string()
}

/// Query parameters that vary per engine/click without changing the page.
const TRACKING_PARAMS: &[&str] = &[
    "gclid", "fbclid", "msclkid", "yclid", "wbraid", "gbraid", "mc_eid", "igshid", "ref_src",
    "ref_url", "spm", "sk", "ved", "ei",
];

fn strip_tracking_params(url: &mut url::Url) {
    if url.query().is_none() {
        return;
    }
    let mut kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !key.starts_with("utm_") && !TRACKING_PARAMS.contains(&key.as_ref()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        url.set_query(None);
        return;
    }
    kept.sort_by(|a, b| a.0.cmp(&b.0).then_with(|| a.1.cmp(&b.1)));
    let query = url::form_urlencoded::Serializer::new(String::new())
        .extend_pairs(kept)
        .finish();
    url.set_query(Some(&query));
}

macro_rules! main_get {
    ($result:expr, $field:ident, $default:expr) => {
        match $result {
            Result_::Main(r) => &r.$field,
            Result_::Image(r) => &r.$field,
            Result_::Paper(r) => &r.$field,
            Result_::Code(r) => &r.$field,
            Result_::File(r) => &r.$field,
            Result_::KeyValue(r) => &r.$field,
            _ => $default,
        }
    };
}

fn normalized_url_of(result: &Result_) -> &str {
    main_get!(result, normalized_url, "")
}

fn url_of(result: &Result_) -> &str {
    main_get!(result, url, "")
}

fn positions_of(result: &Result_) -> &[usize] {
    main_get!(result, positions, EMPTY_POSITIONS)
}

fn priority_of(result: &Result_) -> &str {
    main_get!(result, priority, "")
}

fn content_of(result: &Result_) -> &str {
    main_get!(result, content, "")
}

fn title_of(result: &Result_) -> &str {
    main_get!(result, title, "")
}

/// Heuristic for stub/junk descriptions (stale redirect snapshots, empty
/// snippets) that shouldn't win the canonical description when a duplicate
/// result from another engine has real content.
fn is_low_quality_content(content: &str) -> bool {
    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed.chars().count() < 12 {
        return true;
    }
    let lower = trimmed.to_ascii_lowercase();
    lower.starts_with("redirecting to") || lower.starts_with("redirect to")
}

/// Swap in `incoming`'s title/content as canonical when the merged result's
/// current description is low quality and the incoming one is better.
fn maybe_upgrade_content(existing: &mut Result_, incoming: &Result_) {
    if !is_low_quality_content(content_of(existing)) {
        return;
    }
    let incoming_content = content_of(incoming);
    if is_low_quality_content(incoming_content) {
        return;
    }
    let new_content = incoming_content.to_string();
    let new_title = title_of(incoming).to_string();
    macro_rules! set {
        ($r:expr) => {{
            $r.content = new_content;
            if !new_title.is_empty() {
                $r.title = new_title;
            }
        }};
    }
    match existing {
        Result_::Main(r) => set!(r),
        Result_::Image(r) => set!(r),
        Result_::Paper(r) => set!(r),
        Result_::Code(r) => set!(r),
        Result_::File(r) => set!(r),
        Result_::KeyValue(r) => set!(r),
        _ => {}
    }
}

fn score_of_field(result: &Result_) -> f64 {
    *main_get!(result, score, &0.0)
}

fn set_positions(result: &mut Result_, positions: Vec<usize>) {
    macro_rules! set {
        ($r:expr) => {
            $r.positions = positions
        };
    }
    match result {
        Result_::Main(r) => set!(r),
        Result_::Image(r) => set!(r),
        Result_::Paper(r) => set!(r),
        Result_::Code(r) => set!(r),
        Result_::File(r) => set!(r),
        Result_::KeyValue(r) => set!(r),
        _ => {}
    }
}

fn push_position(result: &mut Result_, position: usize) {
    macro_rules! push {
        ($r:expr) => {
            $r.positions.push(position)
        };
    }
    match result {
        Result_::Main(r) => push!(r),
        Result_::Image(r) => push!(r),
        Result_::Paper(r) => push!(r),
        Result_::Code(r) => push!(r),
        Result_::File(r) => push!(r),
        Result_::KeyValue(r) => push!(r),
        _ => {}
    }
}

fn keep_best_position(result: &mut Result_, index: usize, position: usize) {
    macro_rules! keep {
        ($r:expr) => {
            if let Some(existing) = $r.positions.get_mut(index) {
                *existing = (*existing).min(position);
            }
        };
    }
    match result {
        Result_::Main(r) => keep!(r),
        Result_::Image(r) => keep!(r),
        Result_::Paper(r) => keep!(r),
        Result_::Code(r) => keep!(r),
        Result_::File(r) => keep!(r),
        Result_::KeyValue(r) => keep!(r),
        _ => {}
    }
}

fn set_score(result: &mut Result_, score: f64) {
    macro_rules! set {
        ($r:expr) => {
            $r.score = score
        };
    }
    match result {
        Result_::Main(r) => set!(r),
        Result_::Image(r) => set!(r),
        Result_::Paper(r) => set!(r),
        Result_::Code(r) => set!(r),
        Result_::File(r) => set!(r),
        Result_::KeyValue(r) => set!(r),
        _ => {}
    }
}

fn set_engines(result: &mut Result_, engines: &[String]) {
    if let Result_::Main(r) = result {
        r.engines = engines.to_vec();
    }
}

/// Collapse Wikipedia Answer + Wikidata Infobox (and duplicate Infobox pairs)
/// into one coherent knowledge panel so the SPA does not show the same abstract
/// twice (top Answer + bottom Infobox / Q-id panel).
fn coalesce_knowledge_panels(answers: &mut [Answer], infoboxes: &mut Vec<Infobox>) {
    let mut keep_infoboxes: Vec<Infobox> = Vec::new();

    for box_ in infoboxes.drain(..) {
        let mut absorbed = false;
        for answer in answers.iter_mut() {
            if absorb_infobox_into_wikipedia_answer(answer, &box_) {
                absorbed = true;
                break;
            }
        }
        if !absorbed {
            keep_infoboxes.push(box_);
        }
    }

    *infoboxes = merge_related_infoboxes(keep_infoboxes);
}

fn absorb_infobox_into_wikipedia_answer(answer: &mut Answer, box_: &Infobox) -> bool {
    let Some(InteractiveAnswer::Wikipedia {
        title,
        extract,
        description,
        img_src,
        url,
        wikidata_id,
        attributes,
        related_topics,
    }) = answer.interactive.as_mut()
    else {
        return false;
    };

    let answer_qid = normalize_qid(wikidata_id);
    let box_qid = box_.id.as_deref().and_then(qid_from_url);
    let same_entity = match (&answer_qid, &box_qid) {
        (Some(a), Some(b)) => a == b,
        _ => false,
    };
    let same_url = answer
        .url
        .as_deref()
        .zip(box_.id.as_deref())
        .is_some_and(|(a, b)| urls_loosely_equal(a, b))
        || (!url.is_empty() && box_.urls.iter().any(|u| urls_loosely_equal(url, &u.url)));
    let same_topic = topics_match(title, &box_.infobox)
        || (!description.is_empty() && topics_match(description, &box_.content));

    if !(same_entity || same_url || (same_topic && is_knowledge_engine(&box_.engine))) {
        return false;
    }

    if wikidata_id.is_empty()
        && let Some(qid) = box_qid
    {
        *wikidata_id = qid;
    }
    if img_src.is_empty()
        && let Some(src) = box_.img_src.as_deref().filter(|s| !s.is_empty())
    {
        *img_src = src.to_string();
    }
    if extract.is_empty() && !box_.content.is_empty() && box_.engine != "wikidata" {
        *extract = box_.content.clone();
    }
    if description.is_empty() && box_.engine == "wikidata" && !box_.content.is_empty() {
        *description = box_.content.clone();
    }

    merge_attributes(attributes, &box_.attributes);
    if !description.is_empty() {
        attributes.retain(|a| !a.label.eq_ignore_ascii_case("description"));
    }
    if box_.engine == "wikidata"
        && !box_.content.is_empty()
        && description.is_empty()
        && !attributes
            .iter()
            .any(|a| a.label.eq_ignore_ascii_case("description"))
    {
        attributes.push(InfoboxAttribute {
            label: "Description".to_string(),
            value: box_.content.clone(),
            image: None,
        });
    }
    merge_topics(related_topics, &box_.related_topics);
    true
}

fn merge_related_infoboxes(boxes: Vec<Infobox>) -> Vec<Infobox> {
    let mut merged: Vec<Infobox> = Vec::new();
    for box_ in boxes {
        if let Some(existing) = merged
            .iter_mut()
            .find(|other| infoboxes_related(other, &box_))
        {
            merge_infobox(existing, box_);
        } else {
            merged.push(box_);
        }
    }
    merged
}

fn infoboxes_related(a: &Infobox, b: &Infobox) -> bool {
    let aq = a.id.as_deref().and_then(qid_from_url);
    let bq = b.id.as_deref().and_then(qid_from_url);
    if let (Some(a), Some(b)) = (aq, bq) {
        return a == b;
    }
    // Fuzzy title merge is only for Wikipedia/Wikidata knowledge panels —
    // arbitrary engines must not collapse just because titles look similar.
    if is_knowledge_engine(&a.engine) && is_knowledge_engine(&b.engine) {
        return topics_match(&a.infobox, &b.infobox);
    }
    false
}

fn is_knowledge_engine(engine: &str) -> bool {
    let e = engine.to_ascii_lowercase();
    e == "wikipedia" || e == "wikidata" || e.contains("wikipedia") || e.contains("wikidata")
}

fn merge_infobox(into: &mut Infobox, from: Infobox) {
    if into.content.len() < from.content.len() {
        into.content = from.content;
    }
    if into.img_src.as_ref().is_none_or(|s| s.is_empty()) {
        into.img_src = from.img_src;
    }
    if into.id.as_ref().and_then(|id| qid_from_url(id)).is_none()
        && from.id.as_ref().and_then(|id| qid_from_url(id)).is_some()
    {
        into.id = from.id;
    }
    if into.infobox.len() < from.infobox.len() {
        into.infobox = from.infobox;
    }
    for url in from.urls {
        if !into
            .urls
            .iter()
            .any(|u| urls_loosely_equal(&u.url, &url.url))
        {
            into.urls.push(url);
        }
    }
    merge_attributes(&mut into.attributes, &from.attributes);
    merge_topics(&mut into.related_topics, &from.related_topics);
    if into.engine.is_empty() {
        into.engine = from.engine;
    } else if into.engine != from.engine && !into.engine.contains(&from.engine) {
        into.engine = format!("{}, {}", into.engine, from.engine);
    }
}

fn merge_attributes(into: &mut Vec<InfoboxAttribute>, from: &[InfoboxAttribute]) {
    for attr in from {
        if attr.label.is_empty() {
            continue;
        }
        if into
            .iter()
            .any(|a| a.label.eq_ignore_ascii_case(&attr.label) && a.value == attr.value)
        {
            continue;
        }
        if into
            .iter()
            .any(|a| a.label.eq_ignore_ascii_case(&attr.label) && !a.value.is_empty())
        {
            continue;
        }
        into.push(attr.clone());
    }
}

fn merge_topics(into: &mut Vec<String>, from: &[String]) {
    for topic in from {
        let trimmed = topic.trim();
        if trimmed.is_empty() {
            continue;
        }
        if into.iter().any(|t| t.eq_ignore_ascii_case(trimmed)) {
            continue;
        }
        into.push(trimmed.to_string());
    }
}

fn topics_match(a: &str, b: &str) -> bool {
    let a = normalize_topic(a);
    let b = normalize_topic(b);
    !a.is_empty() && !b.is_empty() && (a == b || a.contains(&b) || b.contains(&a))
}

fn normalize_topic(s: &str) -> String {
    let lower = s.trim().to_ascii_lowercase();
    match lower.find('(') {
        Some(i) => lower[..i].trim().to_string(),
        None => lower,
    }
}

fn qid_from_url(raw: &str) -> Option<String> {
    let trimmed = raw.trim().trim_end_matches('/');
    let marker = "/entity/";
    let idx = trimmed.rfind(marker)?;
    let qid = &trimmed[idx + marker.len()..];
    normalize_qid(qid)
}

fn normalize_qid(raw: &str) -> Option<String> {
    let q = raw.trim();
    if q.len() >= 2
        && q.as_bytes()[0].eq_ignore_ascii_case(&b'Q')
        && q[1..].chars().all(|c| c.is_ascii_digit())
    {
        Some(format!("Q{}", &q[1..]))
    } else {
        None
    }
}

fn urls_loosely_equal(a: &str, b: &str) -> bool {
    canonical_merge_key(a) == canonical_merge_key(b)
}

const EMPTY_POSITIONS: &[usize] = &[];

/// Check retrieved rows against operators, then soften domain runs.
/// Engines that understand web operators already received the original
/// syntax. The notes name the engines that did not.
fn apply_query_policy(
    mut results: Vec<Result_>,
    query: &str,
    engines: &[String],
) -> (Vec<Result_>, Vec<String>) {
    if query.trim().is_empty() {
        return (results, Vec::new());
    }
    let parsed = interpret(query);
    let mut notes = operator_dispatch_notes(&parsed.operators, engines);
    if let Some(site) = &parsed.operators.site {
        let before = results.len();
        results.retain(|result| host_matches_site(url_of(result), site));
        if results.len() != before {
            notes.push(format!(
                "Zoeken removed retrieved results that are not on {site}."
            ));
        }
        if results.is_empty() {
            notes.push(format!("No retrieved result is on {site}."));
        }
    }
    if !parsed.operators.excluded.is_empty() {
        let excluded = parsed.operators.excluded.clone();
        let before = results.len();
        results.retain(|result| !contains_excluded(result, &excluded));
        if results.len() != before {
            notes.push(
                "Zoeken dropped retrieved titles, snippets, and URLs that contain an excluded term."
                    .to_string(),
            );
        }
    }
    if let Some(filetype) = &parsed.operators.filetype {
        let kind = filetype.clone();
        let before = results.len();
        results.retain(|result| filetype_keeps(url_of(result), &kind));
        if results.len() != before {
            notes.push(format!(
                "Zoeken dropped retrieved URLs whose extension is not {kind}."
            ));
        }
    }
    if parsed.operators.site.is_none() && parsed.intent != Intent::Navigational {
        diversify(&mut results);
    }
    (results, notes)
}

fn operator_dispatch_notes(operators: &zoeken_query::Operators, engines: &[String]) -> Vec<String> {
    let supported: Vec<&str> = engines
        .iter()
        .filter(|engine| crate::supports_web_operators(engine))
        .map(String::as_str)
        .collect();
    let unsupported: Vec<&str> = engines
        .iter()
        .filter(|engine| !crate::supports_web_operators(engine))
        .map(String::as_str)
        .collect();
    let mut notes = Vec::new();
    if let Some(site) = &operators.site {
        push_operator_note(
            &mut notes,
            &format!("site:{site}"),
            &supported,
            &unsupported,
        );
    }
    if let Some(filetype) = &operators.filetype {
        push_operator_note(
            &mut notes,
            &format!("filetype:{filetype}"),
            &supported,
            &unsupported,
        );
    }
    if !operators.phrases.is_empty() {
        push_operator_note(&mut notes, "quoted phrases", &supported, &unsupported);
        notes.push("Zoeken does not check quoted phrases against the retrieved pages.".to_string());
    }
    if !operators.excluded.is_empty() {
        push_operator_note(&mut notes, "excluded terms", &supported, &unsupported);
    }
    notes
}

fn push_operator_note(
    notes: &mut Vec<String>,
    label: &str,
    supported: &[&str],
    unsupported: &[&str],
) {
    if !supported.is_empty() {
        notes.push(format!("{label} was sent to {}.", supported.join(", ")));
    }
    if !unsupported.is_empty() {
        let verb = if unsupported.len() == 1 { "does" } else { "do" };
        notes.push(format!(
            "{} {verb} not support {label}. The constraint was removed from their query.",
            unsupported.join(", ")
        ));
    }
    if supported.is_empty() && unsupported.is_empty() {
        notes.push(format!(
            "{label} was not sent to an engine. Zoeken applied it to retrieved results."
        ));
    }
}

fn host_matches_site(url: &str, site: &str) -> bool {
    let Ok(parsed) = url::Url::parse(url) else {
        return false;
    };
    let Some(host) = parsed.host_str() else {
        return false;
    };
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    let site = site
        .trim()
        .trim_start_matches("www.")
        .trim_start_matches("*.")
        .trim_end_matches('/')
        .to_ascii_lowercase();
    host == site || host.ends_with(&format!(".{site}"))
}

fn contains_excluded(result: &Result_, excluded: &[String]) -> bool {
    let blob = format!(
        "{} {} {}",
        title_of(result),
        content_of(result),
        url_of(result)
    );
    excluded.iter().any(|term| {
        crate::text::tokenize(&blob)
            .iter()
            .any(|token| token == &term.to_lowercase())
    })
}

fn filetype_keeps(url: &str, filetype: &str) -> bool {
    match url_extension(url) {
        Some(ext) => ext.eq_ignore_ascii_case(filetype),
        None => true,
    }
}

fn url_extension(url: &str) -> Option<String> {
    let parsed = url::Url::parse(url).ok()?;
    let segment = parsed.path().rsplit('/').next().unwrap_or("");
    let (_, ext) = segment.rsplit_once('.')?;
    if ext.is_empty() || ext.len() > 8 || !ext.chars().all(|c| c.is_ascii_alphanumeric()) {
        None
    } else {
        Some(ext.to_string())
    }
}

/// Keep the first two hits per registrable domain, then the rest. Nothing is deleted.
pub(crate) fn diversify(results: &mut Vec<Result_>) {
    let mut counts: HashMap<String, usize> = HashMap::new();
    let mut kept = Vec::with_capacity(results.len());
    let mut deferred = Vec::new();
    for result in results.drain(..) {
        let domain = registrable_domain(url_of(&result));
        if domain.is_empty() {
            kept.push(result);
            continue;
        }
        let count = counts.entry(domain).or_insert(0);
        if *count < 2 {
            *count += 1;
            kept.push(result);
        } else {
            deferred.push(result);
        }
    }
    kept.extend(deferred);
    *results = kept;
}

fn registrable_domain(url: &str) -> String {
    let Ok(parsed) = url::Url::parse(url) else {
        return String::new();
    };
    let Some(host) = parsed.host_str() else {
        return String::new();
    };
    let host = host.trim_start_matches("www.").to_ascii_lowercase();
    // The public suffix list keeps github.io tenants, co.uk, and
    // s3.amazonaws.com buckets as separate sites. Ports are not part of this key.
    if let Some(domain) = psl::List.domain(host.as_bytes()) {
        return String::from_utf8_lossy(domain.as_bytes()).into_owned();
    }
    host
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    use zoeken_engine_core::{EngineError, EngineResults};
    use zoeken_results::{
        Answer, Infobox, InfoboxAttribute, InfoboxUrl, InteractiveAnswer, MainResult, Result_,
        Suggestion,
    };

    use crate::execution::{EngineRunOutcome, EngineRunStatus, ExecutionReport};

    fn main_result(url: &str, title: &str) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            ..MainResult::default()
        })
    }

    fn main_result_with_content(url: &str, title: &str, content: &str) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            ..MainResult::default()
        })
    }

    fn completed(engine: &str, results: EngineResults) -> EngineRunOutcome {
        EngineRunOutcome {
            engine: engine.to_string(),
            status: EngineRunStatus::Completed(results),
            duration: Duration::from_millis(5),
            http_duration: None,
        }
    }

    fn report(outcomes: Vec<EngineRunOutcome>) -> ExecutionReport {
        ExecutionReport { outcomes }
    }

    fn weights(pairs: &[(&str, f64)]) -> EngineWeights {
        EngineWeights::new(pairs.iter().map(|(n, w)| (n.to_string(), *w)))
    }

    fn as_main(result: &Result_) -> &MainResult {
        match result {
            Result_::Main(m) => m,
            other => panic!("expected MainResult, got {other:?}"),
        }
    }

    #[test]
    fn merge_prefers_real_content_over_redirect_stub() {
        let mut alpha = EngineResults::new();
        alpha.add(main_result_with_content(
            "https://a.test/",
            "Install Rust",
            "Redirecting to /tools/install",
        ));
        let mut beta = EngineResults::new();
        beta.add(main_result_with_content(
            "https://a.test/",
            "Rust Programming Language",
            "A language empowering everyone to build reliable and efficient software.",
        ));

        let container = aggregate(
            report(vec![completed("alpha", alpha), completed("beta", beta)]),
            &weights(&[("alpha", 1.0), ("beta", 1.0)]),
        );

        let merged = container
            .results
            .iter()
            .map(as_main)
            .find(|m| m.normalized_url == "https://a.test/")
            .expect("merged result present");
        assert_eq!(
            merged.content,
            "A language empowering everyone to build reliable and efficient software."
        );
        assert_eq!(merged.title, "Rust Programming Language");
        // Both engines still get credit for the merged result.
        assert_eq!(
            merged.engines,
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn merge_keeps_first_content_when_incoming_is_also_low_quality() {
        let mut alpha = EngineResults::new();
        alpha.add(main_result_with_content(
            "https://a.test/",
            "First",
            "Redirecting to /somewhere",
        ));
        let mut beta = EngineResults::new();
        beta.add(main_result_with_content("https://a.test/", "Second", ""));

        let container = aggregate(
            report(vec![completed("alpha", alpha), completed("beta", beta)]),
            &weights(&[("alpha", 1.0), ("beta", 1.0)]),
        );

        let merged = container
            .results
            .iter()
            .map(as_main)
            .find(|m| m.normalized_url == "https://a.test/")
            .expect("merged result present");
        assert_eq!(merged.content, "Redirecting to /somewhere");
        assert_eq!(merged.title, "First");
    }

    #[test]
    fn dedups_by_normalized_url_and_unions_engines_and_positions() {
        let mut alpha = EngineResults::new();
        alpha.add(main_result("https://a.test/", "shared"));
        let mut beta = EngineResults::new();
        beta.add(main_result("https://b.test/", "other"));
        beta.add(main_result("https://a.test/", "shared"));

        let container = aggregate(
            report(vec![completed("alpha", alpha), completed("beta", beta)]),
            &weights(&[("alpha", 1.0), ("beta", 1.0)]),
        );

        assert_eq!(container.results.len(), 2);
        assert_eq!(container.number_of_results, 2);

        let shared = container
            .results
            .iter()
            .map(as_main)
            .find(|m| m.normalized_url == "https://a.test/")
            .expect("shared result present");
        assert_eq!(
            shared.engines,
            vec!["alpha".to_string(), "beta".to_string()]
        );
        assert_eq!(shared.positions, vec![1, 2]);
    }

    #[test]
    fn dedups_common_url_aliases_for_merge_key() {
        let mut alpha = EngineResults::new();
        alpha.add(main_result(
            "https://rust-lang.org/",
            "Rust Programming Language",
        ));

        let mut beta = EngineResults::new();
        beta.add(main_result(
            "https://www.rust-lang.org/en-US",
            "Rust Programming Language",
        ));

        let container = aggregate(
            report(vec![completed("alpha", alpha), completed("beta", beta)]),
            &weights(&[("alpha", 1.0), ("beta", 1.0)]),
        );

        assert_eq!(
            container.results.len(),
            2,
            "a locale root is a different page"
        );
    }

    #[test]
    fn keeps_distinct_locale_ports_and_tenant_hosts() {
        let mut alpha = EngineResults::new();
        alpha.add(main_result("https://example.com/en", "English"));
        alpha.add(main_result("https://example.com/fr", "French"));
        alpha.add(main_result("https://example.com:8443/", "Other port"));
        alpha.add(main_result("https://docs.github.io/rust", "Docs"));
        alpha.add(main_result("https://rust-lang.github.io/", "Book"));
        let container = aggregate(
            report(vec![completed("alpha", alpha)]),
            &weights(&[("alpha", 1.0)]),
        );
        assert_eq!(container.results.len(), 5);
        let domains: Vec<String> = container
            .results
            .iter()
            .map(|result| registrable_domain(&as_main(result).url))
            .collect();
        assert!(domains.iter().any(|d| d == "docs.github.io"));
        assert!(domains.iter().any(|d| d == "rust-lang.github.io"));
        assert_eq!(registrable_domain("https://news.bbc.co.uk/a"), "bbc.co.uk");
        assert_eq!(registrable_domain("https://sport.bbc.co.uk/b"), "bbc.co.uk");
        assert_eq!(
            registrable_domain("https://bucket-a.s3.amazonaws.com/x"),
            "bucket-a.s3.amazonaws.com"
        );
        assert_ne!(
            registrable_domain("https://bucket-a.s3.amazonaws.com/x"),
            registrable_domain("https://bucket-b.s3.amazonaws.com/y")
        );
        assert_eq!(
            registrable_domain("https://www.example.com/en"),
            registrable_domain("https://example.com/fr")
        );
    }

    #[test]
    fn dedups_across_scheme_and_tracking_params() {
        let mut alpha = EngineResults::new();
        alpha.add(main_result("http://example.com/page", "Page"));
        let mut beta = EngineResults::new();
        beta.add(main_result(
            "https://example.com/page?utm_source=x&utm_medium=y&fbclid=abc",
            "Page",
        ));

        let container = aggregate(
            report(vec![completed("alpha", alpha), completed("beta", beta)]),
            &weights(&[("alpha", 1.0), ("beta", 1.0)]),
        );

        assert_eq!(container.results.len(), 1);
        assert_eq!(
            as_main(&container.results[0]).engines,
            vec!["alpha".to_string(), "beta".to_string()]
        );
    }

    #[test]
    fn meaningful_query_params_still_distinguish_results() {
        let mut alpha = EngineResults::new();
        alpha.add(main_result("https://example.com/watch?v=abc", "A"));
        alpha.add(main_result("https://example.com/watch?v=def", "B"));

        let container = aggregate(
            report(vec![completed("alpha", alpha)]),
            &weights(&[("alpha", 1.0)]),
        );
        assert_eq!(container.results.len(), 2);
    }

    #[test]
    fn multi_engine_result_scores_strictly_higher() {
        let mut a = EngineResults::new();
        a.add(main_result("https://x.test/", "x"));
        let mut b = EngineResults::new();
        b.add(main_result("https://x.test/", "x"));
        let multi = aggregate(
            report(vec![completed("a", a), completed("b", b)]),
            &weights(&[("a", 1.0), ("b", 1.0)]),
        );

        let mut c = EngineResults::new();
        c.add(main_result("https://x.test/", "x"));
        let single = aggregate(report(vec![completed("a", c)]), &weights(&[("a", 1.0)]));

        let multi_score = as_main(&multi.results[0]).score;
        let single_score = as_main(&single.results[0]).score;
        assert!(
            multi_score > single_score,
            "multi-engine score {multi_score} should exceed single-engine score {single_score}"
        );
    }

    #[test]
    fn scoring_matches_reference_formula() {
        let mut a = EngineResults::new();
        a.add(main_result("https://x.test/", "x"));
        let container = aggregate(report(vec![completed("a", a)]), &weights(&[("a", 2.0)]));
        assert_eq!(as_main(&container.results[0]).score, 2.0);
    }

    #[test]
    fn orders_results_by_descending_score() {
        let mut a = EngineResults::new();
        a.add(main_result("https://high.test/", "high"));
        a.add(main_result("https://mid.test/", "mid"));
        a.add(main_result("https://low.test/", "low"));
        let mut b = EngineResults::new();
        b.add(main_result("https://high.test/", "high"));

        let container = aggregate(
            report(vec![completed("a", a), completed("b", b)]),
            &weights(&[("a", 1.0), ("b", 1.0)]),
        );

        let titles: Vec<&str> = container
            .results
            .iter()
            .map(|r| as_main(r).title.as_str())
            .collect();
        assert_eq!(titles, vec!["high", "mid", "low"]);
        let scores: Vec<f64> = container.results.iter().map(|r| as_main(r).score).collect();
        assert!(scores.windows(2).all(|w| w[0] >= w[1]));
    }

    #[test]
    fn merges_duckduckgo_uddg_wrapper_with_bare_url() {
        let mut ddg = EngineResults::new();
        ddg.add(main_result(
            "https://duckduckgo.com/l/?uddg=https%3A%2F%2Frust-lang.org%2F&rut=x",
            "Rust",
        ));
        let mut brave = EngineResults::new();
        brave.add(main_result("https://www.rust-lang.org/", "Rust"));

        let container = aggregate(
            report(vec![
                completed("duckduckgo", ddg),
                completed("brave", brave),
            ]),
            &weights(&[("duckduckgo", 1.0), ("brave", 1.0)]),
        );

        assert_eq!(container.results.len(), 1);
        let merged = as_main(&container.results[0]);
        assert_eq!(
            merged.engines,
            vec!["duckduckgo".to_string(), "brave".to_string()]
        );
        assert!(merged.score > 1.0);
    }

    #[test]
    fn equal_score_ties_break_by_position_then_url_not_ingest_order() {
        // Distinct URLs so nothing merges: both #1 hits score 1.0.
        // Ingest "z" first so a finish-order sort would put it on top.
        let mut first = EngineResults::new();
        first.add(main_result("https://z.test/", "z-first"));
        let mut second = EngineResults::new();
        second.add(main_result("https://a.test/", "a-second"));
        second.add(main_result("https://m.test/", "m-third"));

        let container = aggregate(
            report(vec![completed("first", first), completed("second", second)]),
            &weights(&[("first", 1.0), ("second", 1.0)]),
        );

        let titles: Vec<&str> = container
            .results
            .iter()
            .map(|r| as_main(r).title.as_str())
            .collect();
        assert_eq!(
            titles,
            vec!["a-second", "z-first", "m-third"],
            "equal #1 scores must order by URL, not engine ingest order"
        );
    }

    #[test]
    fn mid_page_multi_engine_hit_does_not_beat_unique_first_result() {
        // Four engines listing the same URL at rank 10 used to score n²/10 = 1.6
        // and bury a unique #1. Linear sum is 0.4.
        let mut top = EngineResults::new();
        top.add(main_result("https://relevant.test/", "Relevant"));

        let junk_url = "https://seo-farm.test/spam";
        let mut farm = Vec::new();
        for name in ["a", "b", "c", "d"] {
            let mut results = EngineResults::new();
            for i in 0..9 {
                results.add(main_result(&format!("https://{name}.test/{i}"), "pad"));
            }
            results.add(main_result(junk_url, "Spam"));
            farm.push(completed(name, results));
        }

        let mut outcomes = vec![completed("top", top)];
        outcomes.extend(farm);
        let container = aggregate(
            report(outcomes),
            &weights(&[("top", 1.0), ("a", 1.0), ("b", 1.0), ("c", 1.0), ("d", 1.0)]),
        );

        let relevant = container
            .results
            .iter()
            .map(as_main)
            .find(|m| m.title == "Relevant")
            .expect("relevant hit present");
        let junk = container
            .results
            .iter()
            .map(as_main)
            .find(|m| m.normalized_url == junk_url)
            .expect("junk still present");
        assert!(
            junk.score < relevant.score,
            "rank-10 consensus {} must stay below unique #1 {}",
            junk.score,
            relevant.score
        );
    }

    #[test]
    fn query_prefers_official_host_over_unrelated_equal_rank_hit() {
        // Typical failure: Wikipedia #1 and the official site #1 both score 1.0,
        // then URL sort puts en.wikipedia.org first.
        let mut wiki = EngineResults::new();
        wiki.add(main_result("https://en.wikipedia.org/wiki/Rust", "Rust"));
        let mut ddg = EngineResults::new();
        ddg.add(main_result(
            "https://www.rust-lang.org/",
            "Rust Programming Language",
        ));

        let container = aggregate_query(
            report(vec![
                completed("wikipedia", wiki),
                completed("duckduckgo", ddg),
            ]),
            &weights(&[("wikipedia", 1.0), ("duckduckgo", 1.0)]),
            "rust",
        );

        assert_eq!(
            as_main(&container.results[0]).normalized_url,
            "https://www.rust-lang.org/",
            "official host matching the query must outrank a same-position Wikipedia hit"
        );
    }

    #[test]
    fn query_does_not_boost_title_or_content_substrings() {
        let mut email = EngineResults::new();
        email.add(main_result_with_content(
            "https://a.test/email",
            "Email",
            "Tickets available now.",
        ));
        let mut docs = EngineResults::new();
        docs.add(main_result("https://b.test/docs", "Docs"));
        let mut ai = EngineResults::new();
        ai.add(main_result("https://c.test/about", "AI"));

        let container = aggregate_query(
            report(vec![
                completed("email", email),
                completed("docs", docs),
                completed("ai", ai),
            ]),
            &weights(&[("email", 1.0), ("docs", 1.0), ("ai", 1.0)]),
            "ai",
        );

        assert_eq!(as_main(&container.results[0]).title, "AI");
        let email_score = container
            .results
            .iter()
            .map(as_main)
            .find(|m| m.title == "Email")
            .expect("email")
            .score;
        let docs_score = container
            .results
            .iter()
            .map(as_main)
            .find(|m| m.title == "Docs")
            .expect("docs")
            .score;
        assert!(
            (email_score - docs_score).abs() < 1e-9,
            "substring 'ai' in Email / crust must not outrank Docs: {email_score} vs {docs_score}"
        );
    }

    #[test]
    fn merges_be_tarask_mobile_wikipedia_with_desktop() {
        let mut a = EngineResults::new();
        a.add(main_result(
            "https://be-tarask.m.wikipedia.org/wiki/Foo",
            "Foo",
        ));
        let mut b = EngineResults::new();
        b.add(main_result(
            "https://be-tarask.wikipedia.org/wiki/Foo",
            "Foo",
        ));

        let container = aggregate(
            report(vec![completed("a", a), completed("b", b)]),
            &weights(&[("a", 1.0), ("b", 1.0)]),
        );
        assert_eq!(container.results.len(), 1);
        assert_eq!(as_main(&container.results[0]).engines.len(), 2);
    }

    #[test]
    fn unwraps_google_url_param_when_q_is_not_http() {
        let mut google = EngineResults::new();
        google.add(main_result(
            "https://www.google.com/url?q=foo&url=https%3A%2F%2Frust-lang.org%2F",
            "Rust",
        ));
        let mut brave = EngineResults::new();
        brave.add(main_result("https://www.rust-lang.org/", "Rust"));

        let container = aggregate(
            report(vec![completed("google", google), completed("brave", brave)]),
            &weights(&[("google", 1.0), ("brave", 1.0)]),
        );
        assert_eq!(container.results.len(), 1);
    }

    #[test]
    fn three_engine_number_one_is_the_top_result() {
        let mut ddg = EngineResults::new();
        ddg.add(main_result(
            "https://www.python.org/",
            "Welcome to Python.org",
        ));
        let mut brave = EngineResults::new();
        brave.add(main_result("https://python.org/", "Python"));
        let mut bing = EngineResults::new();
        bing.add(main_result(
            "https://www.python.org/index.html",
            "Python.org",
        ));
        let mut wiki = EngineResults::new();
        wiki.add(main_result(
            "https://en.wikipedia.org/wiki/Python",
            "Python",
        ));

        let container = aggregate_query(
            report(vec![
                completed("duckduckgo", ddg),
                completed("brave", brave),
                completed("bing", bing),
                completed("wikipedia", wiki),
            ]),
            &weights(&[
                ("duckduckgo", 1.0),
                ("brave", 1.0),
                ("bing", 1.0),
                ("wikipedia", 1.0),
            ]),
            "python",
        );

        let top = as_main(&container.results[0]);
        assert!(
            canonical_merge_key(&top.normalized_url) == canonical_merge_key("https://python.org/"),
            "consensus #1 must surface, got {}",
            top.normalized_url
        );
        assert_eq!(top.engines.len(), 3);
    }

    #[test]
    fn same_engine_duplicate_keeps_best_position_only() {
        let mut a = EngineResults::new();
        a.add(main_result("https://a.test/", "First"));
        a.add(main_result("http://www.a.test/", "Alias"));

        let container = aggregate(report(vec![completed("a", a)]), &weights(&[("a", 1.0)]));
        assert_eq!(container.results.len(), 1);
        let merged = as_main(&container.results[0]);
        assert_eq!(merged.positions, vec![1]);
        assert_eq!(merged.engines, vec!["a".to_string()]);
        assert_eq!(merged.score, 1.0);
    }

    #[test]
    fn merges_mobile_wikipedia_with_desktop() {
        let mut a = EngineResults::new();
        a.add(main_result("https://en.m.wikipedia.org/wiki/Rust", "Rust"));
        let mut b = EngineResults::new();
        b.add(main_result("https://en.wikipedia.org/wiki/Rust", "Rust"));

        let container = aggregate(
            report(vec![completed("a", a), completed("b", b)]),
            &weights(&[("a", 1.0), ("b", 1.0)]),
        );
        assert_eq!(container.results.len(), 1);
        assert_eq!(as_main(&container.results[0]).engines.len(), 2);
    }

    #[test]
    fn merges_query_params_regardless_of_order() {
        let mut a = EngineResults::new();
        a.add(main_result("https://example.com/watch?v=abc&t=1", "A"));
        let mut b = EngineResults::new();
        b.add(main_result("https://example.com/watch?t=1&v=abc", "A"));

        let container = aggregate(
            report(vec![completed("a", a), completed("b", b)]),
            &weights(&[("a", 1.0), ("b", 1.0)]),
        );
        assert_eq!(container.results.len(), 1);
    }

    #[test]
    fn aggregates_side_channels_separately() {
        let mut a = EngineResults::new();
        a.add(main_result("https://x.test/", "x"));
        a.add(Result_::Answer(Answer {
            answer: "42".to_string(),
            ..Answer::default()
        }));
        a.add(Result_::Suggestion(Suggestion {
            suggestion: "rust lang".to_string(),
            ..Suggestion::default()
        }));

        let container = aggregate(report(vec![completed("a", a)]), &weights(&[("a", 1.0)]));

        assert_eq!(container.results.len(), 1);
        assert_eq!(container.answers.len(), 1);
        assert_eq!(container.suggestions.len(), 1);
        assert_eq!(container.answers[0].engine, "a");
        assert_eq!(container.suggestions[0].engine, "a");
    }

    #[test]
    fn dedups_side_channels_across_engines() {
        let mut a = EngineResults::new();
        a.add(Result_::Suggestion(Suggestion {
            suggestion: "same".to_string(),
            ..Suggestion::default()
        }));
        let mut b = EngineResults::new();
        b.add(Result_::Suggestion(Suggestion {
            suggestion: "Same".to_string(),
            ..Suggestion::default()
        }));

        let container = aggregate(
            report(vec![completed("a", a), completed("b", b)]),
            &weights(&[("a", 1.0), ("b", 1.0)]),
        );
        assert_eq!(container.suggestions.len(), 1);
    }

    #[test]
    fn ranks_suggestions_by_frequency_and_weight() {
        let mut a = EngineResults::new();
        a.add(Result_::Suggestion(Suggestion {
            suggestion: "rare".to_string(),
            ..Suggestion::default()
        }));
        a.add(Result_::Suggestion(Suggestion {
            suggestion: "popular".to_string(),
            ..Suggestion::default()
        }));
        let mut b = EngineResults::new();
        b.add(Result_::Suggestion(Suggestion {
            suggestion: "popular".to_string(),
            ..Suggestion::default()
        }));
        let mut c = EngineResults::new();
        c.add(Result_::Suggestion(Suggestion {
            suggestion: "heavy".to_string(),
            ..Suggestion::default()
        }));

        let container = aggregate(
            report(vec![
                completed("a", a),
                completed("b", b),
                completed("c", c),
            ]),
            &weights(&[("a", 1.0), ("b", 1.0), ("c", 5.0)]),
        );

        let texts: Vec<&str> = container
            .suggestions
            .iter()
            .map(|s| s.suggestion.as_str())
            .collect();
        // heavy: 1×5=5, popular: 2×1=2, rare: 1×1=1
        assert_eq!(texts, vec!["heavy", "popular", "rare"]);
    }

    #[test]
    fn full_failure_still_produces_empty_container_and_lists_engines() {
        let outcomes = vec![
            EngineRunOutcome {
                engine: "boom".to_string(),
                status: EngineRunStatus::Failed(EngineError::Unexpected("nope".to_string())),
                duration: Duration::from_millis(2),
                http_duration: None,
            },
            EngineRunOutcome {
                engine: "slow".to_string(),
                status: EngineRunStatus::Unresponsive(UnresponsiveReason::GlobalDeadline),
                duration: Duration::from_secs(1),
                http_duration: None,
            },
            EngineRunOutcome {
                engine: "stuck".to_string(),
                status: EngineRunStatus::Unresponsive(UnresponsiveReason::EngineTimeout),
                duration: Duration::from_secs(1),
                http_duration: None,
            },
        ];

        let container = aggregate(report(outcomes), &EngineWeights::default());

        assert!(container.results.is_empty());
        assert_eq!(container.number_of_results, 0);
        assert_eq!(container.unresponsive_engines.len(), 3);
        assert_eq!(
            container.unresponsive_engines[0],
            UnresponsiveEngine {
                engine: "boom".to_string(),
                cause: UnresponsiveCause::Error {
                    category: ErrorCategory::Unexpected,
                    message: "unexpected engine error: nope".to_string(),
                },
            }
        );
        assert_eq!(
            container.unresponsive_engines[1].cause,
            UnresponsiveCause::DeadlineExceeded
        );
        assert_eq!(
            container.unresponsive_engines[2].cause,
            UnresponsiveCause::Timeout
        );
    }

    #[test]
    fn unknown_engine_weight_defaults_to_one() {
        let mut a = EngineResults::new();
        a.add(main_result("https://x.test/", "x"));
        let container = aggregate(report(vec![completed("a", a)]), &EngineWeights::default());
        assert_eq!(as_main(&container.results[0]).score, 1.0);
    }

    #[test]
    fn absorbs_wikidata_infobox_into_wikipedia_answer() {
        let mut wiki = EngineResults::new();
        wiki.add(Result_::Answer(Answer {
            answer: "Rust: A language.".to_string(),
            url: Some("https://en.wikipedia.org/wiki/Rust_(programming_language)".to_string()),
            engine: "wikipedia".to_string(),
            interactive: Some(InteractiveAnswer::Wikipedia {
                title: "Rust (programming language)".to_string(),
                extract: "Rust is a systems programming language.".to_string(),
                description: "General-purpose programming language".to_string(),
                img_src: "https://upload.wikimedia.org/rust.png".to_string(),
                url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".to_string(),
                wikidata_id: "Q575650".to_string(),
                attributes: Vec::new(),
                related_topics: Vec::new(),
            }),
        }));

        let mut wd = EngineResults::new();
        wd.add(Result_::Infobox(Infobox {
            infobox: "Rust".to_string(),
            id: Some("https://www.wikidata.org/entity/Q575650".to_string()),
            content: "multi-paradigm system programming language".to_string(),
            img_src: None,
            urls: vec![InfoboxUrl {
                title: "Wikidata".to_string(),
                url: "http://www.wikidata.org/entity/Q575650".to_string(),
            }],
            attributes: vec![InfoboxAttribute {
                label: "Instance of".to_string(),
                value: "programming language".to_string(),
                image: None,
            }],
            related_topics: vec!["programming language".to_string()],
            engine: "wikidata".to_string(),
        }));

        let container = aggregate(
            report(vec![
                completed("wikipedia", wiki),
                completed("wikidata", wd),
            ]),
            &weights(&[("wikipedia", 1.0), ("wikidata", 1.0)]),
        );

        assert_eq!(container.answers.len(), 1);
        assert!(
            container.infoboxes.is_empty(),
            "Wikidata panel should merge into the Wikipedia answer"
        );
        let InteractiveAnswer::Wikipedia {
            wikidata_id,
            attributes,
            related_topics,
            extract,
            ..
        } = container.answers[0].interactive.as_ref().unwrap()
        else {
            panic!("expected wikipedia interactive");
        };
        assert_eq!(wikidata_id, "Q575650");
        assert!(extract.contains("systems programming"));
        assert!(
            attributes
                .iter()
                .any(|a| a.label == "Instance of" && a.value == "programming language")
        );
        assert!(related_topics.iter().any(|t| t == "programming language"));
    }

    #[test]
    fn merges_duplicate_infoboxes_by_qid() {
        let mut a = EngineResults::new();
        a.add(Result_::Infobox(Infobox {
            infobox: "Berlin".to_string(),
            id: Some("https://www.wikidata.org/entity/Q64".to_string()),
            content: "capital of Germany".to_string(),
            img_src: None,
            urls: vec![InfoboxUrl {
                title: "Wikidata".to_string(),
                url: "http://www.wikidata.org/entity/Q64".to_string(),
            }],
            attributes: vec![InfoboxAttribute {
                label: "Description".to_string(),
                value: "capital of Germany".to_string(),
                image: None,
            }],
            related_topics: Vec::new(),
            engine: "wikidata".to_string(),
        }));
        let mut b = EngineResults::new();
        b.add(Result_::Infobox(Infobox {
            infobox: "Berlin, Germany".to_string(),
            id: Some("https://www.wikidata.org/entity/Q64".to_string()),
            content: "capital and largest city of Germany".to_string(),
            img_src: Some("https://commons.wikimedia.org/wiki/Special:FilePath/Berlin.jpg".into()),
            urls: vec![InfoboxUrl {
                title: "Wikipedia".to_string(),
                url: "https://en.wikipedia.org/wiki/Berlin".to_string(),
            }],
            attributes: vec![InfoboxAttribute {
                label: "Instance of".to_string(),
                value: "city".to_string(),
                image: None,
            }],
            related_topics: vec!["city".to_string()],
            engine: "wikipedia".to_string(),
        }));

        let container = aggregate(
            report(vec![completed("wikidata", a), completed("wikipedia", b)]),
            &weights(&[("wikidata", 1.0), ("wikipedia", 1.0)]),
        );

        assert_eq!(container.infoboxes.len(), 1);
        let box_ = &container.infoboxes[0];
        assert_eq!(box_.content, "capital and largest city of Germany");
        assert!(box_.img_src.is_some());
        assert_eq!(box_.urls.len(), 2);
        assert!(box_.attributes.iter().any(|a| a.label == "Instance of"));
    }

    #[test]
    fn non_default_ports_and_mobile_hosts_stay_distinct() {
        let mut a = EngineResults::new();
        a.add(main_result("https://example.com/", "Root"));
        a.add(main_result("https://example.com:8443/", "Port"));
        a.add(main_result("https://m.example.com/", "Mobile"));
        a.add(main_result("https://example.com/fr/docs", "French"));
        a.add(main_result("https://example.com/docs", "Docs"));

        let container = aggregate(report(vec![completed("a", a)]), &weights(&[("a", 1.0)]));
        assert_eq!(container.results.len(), 5);
    }

    #[test]
    fn default_https_port_still_merges() {
        let mut a = EngineResults::new();
        a.add(main_result("https://example.com:443/page", "A"));
        let mut b = EngineResults::new();
        b.add(main_result("http://example.com/page", "B"));
        let container = aggregate(
            report(vec![completed("a", a), completed("b", b)]),
            &weights(&[("a", 1.0), ("b", 1.0)]),
        );
        assert_eq!(container.results.len(), 1);
    }

    #[test]
    fn site_operator_drops_other_hosts_and_diversity_keeps_the_rest() {
        let mut a = EngineResults::new();
        a.add(main_result("https://docs.rs/tokio", "Tokio"));
        a.add(main_result("https://docs.rs/tokio/latest", "Latest"));
        a.add(main_result("https://docs.rs/tokio/legacy", "Legacy"));
        a.add(main_result("https://en.wikipedia.org/wiki/Tokio", "Wiki"));
        let kept = aggregate_query(
            report(vec![completed("a", a)]),
            &weights(&[("a", 1.0)]),
            "site:docs.rs tokio -wiki",
        );
        assert!(kept.results.iter().all(|r| url_of(r).contains("docs.rs")));
        assert!(!kept.operator_notes.is_empty());

        let mut crowded = EngineResults::new();
        crowded.add(main_result("https://spam.example/1", "Soup one"));
        crowded.add(main_result("https://spam.example/2", "Soup two"));
        crowded.add(main_result("https://spam.example/3", "Soup three"));
        crowded.add(main_result("https://other.example/recipe", "Other soup"));
        let diverse = aggregate_query(
            report(vec![completed("a", crowded)]),
            &weights(&[("a", 1.0)]),
            "soup recipes",
        );
        let urls: Vec<&str> = diverse.results.iter().map(url_of).collect();
        assert_eq!(urls.len(), 4);
        assert!(urls.iter().position(|u| u.contains("other.example")) < Some(3));
    }

    #[test]
    fn unicode_query_matches_accented_title() {
        let mut a = EngineResults::new();
        a.add(main_result("https://a.test/cafe", "Café guide"));
        a.add(main_result("https://b.test/other", "Unrelated"));
        let container = aggregate_query(
            report(vec![completed("a", a)]),
            &weights(&[("a", 1.0)]),
            "café",
        );
        assert_eq!(as_main(&container.results[0]).title, "Café guide");
    }
}
