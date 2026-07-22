use std::collections::HashMap;

use zoeken_engine_core::{EngineResults, ErrorCategory};
use zoeken_results::{Answer, Correction, Infobox, Result_, Suggestion};

use crate::execution::{EngineRunStatus, ExecutionReport, UnresponsiveReason};
use crate::metrics::{EngineOutcome, EngineSample, MetricsRecorder};

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
}

struct Merged {
    result: Result_,
    engines: Vec<String>,
}

pub fn aggregate(
    report: ExecutionReport,
    weights: &EngineWeights,
    recorder: &dyn MetricsRecorder,
) -> ResultContainer {
    let mut builder = ContainerBuilder::default();

    for outcome in report.outcomes {
        let sample_outcome = match &outcome.status {
            EngineRunStatus::Completed(results) => EngineOutcome::Completed {
                results: results.results.len(),
            },
            EngineRunStatus::Failed(error) => EngineOutcome::Failed { error },
            EngineRunStatus::Unresponsive(reason) => {
                EngineOutcome::Unresponsive { reason: *reason }
            }
        };
        recorder.record_engine(EngineSample {
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

    builder.finish(weights)
}

#[derive(Default)]
struct ContainerBuilder {
    merged: Vec<Merged>,
    by_key: HashMap<String, usize>,
    answers: Vec<Answer>,
    seen_answers: std::collections::HashSet<String>,
    /// Case-insensitive key → (display suggestion, engine-weight sum, hit count).
    suggestion_rank: HashMap<String, (Suggestion, f64, usize)>,
    corrections: Vec<Correction>,
    seen_corrections: std::collections::HashSet<String>,
    infoboxes: Vec<Infobox>,
    seen_infoboxes: std::collections::HashSet<String>,
    unresponsive_engines: Vec<UnresponsiveEngine>,
    engine_data: HashMap<String, String>,
}

impl ContainerBuilder {
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
                    push_position(&mut merged.result, position);
                    if !merged.engines.iter().any(|e| e == engine) {
                        merged.engines.push(engine.to_string());
                    }
                    set_engines(&mut merged.result, &merged.engines);
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

    fn finish(self, weights: &EngineWeights) -> ResultContainer {
        let mut results: Vec<Result_> = self
            .merged
            .into_iter()
            .map(|merged| {
                let score = score_of(
                    positions_of(&merged.result),
                    &merged.engines,
                    priority_of(&merged.result),
                    weights,
                );
                let mut result = merged.result;
                set_score(&mut result, score);
                result
            })
            .collect();

        results.sort_by(|a, b| {
            score_of_field(b)
                .partial_cmp(&score_of_field(a))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

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

        let number_of_results = results.len();
        ResultContainer {
            results,
            answers: self.answers,
            suggestions,
            corrections: self.corrections,
            infoboxes: self.infoboxes,
            unresponsive_engines: self.unresponsive_engines,
            engine_data: self.engine_data,
            number_of_results,
        }
    }
}

fn score_of(
    positions: &[usize],
    engines: &[String],
    priority: &str,
    weights: &EngineWeights,
) -> f64 {
    if positions.is_empty() {
        return 0.0;
    }
    let mut weight = 1.0;
    for engine in engines {
        weight *= weights.weight_of(engine);
    }
    weight *= positions.len() as f64;

    let mut score = 0.0;
    for &position in positions {
        if priority == "low" {
            continue;
        }
        if priority == "high" {
            score += weight;
            continue;
        }
        let position = position.max(1);
        score += weight / position as f64;
    }
    score
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
    let Ok(mut url) = url::Url::parse(raw) else {
        return raw.trim_end_matches('/').to_ascii_lowercase();
    };

    // Normalize common URL variants so equivalent pages merge together.
    url.set_fragment(None);

    if matches!(url.scheme(), "http" | "https") {
        let _ = url.set_port(None);
        // Engines disagree on scheme for the same page; treat http == https.
        let _ = url.set_scheme("https");
        strip_tracking_params(&mut url);
    }

    if let Some(host) = url.host_str() {
        let host = host.to_ascii_lowercase();
        let host = host.strip_prefix("www.").unwrap_or(&host).to_string();
        let _ = url.set_host(Some(&host));
    }

    let path = url.path().trim_end_matches('/').to_string();
    let canonical_path = if path.is_empty() || is_locale_root_path(&path) {
        "/".to_string()
    } else {
        path
    };
    url.set_path(&canonical_path);

    url.to_string()
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
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !key.starts_with("utm_") && !TRACKING_PARAMS.contains(&key.as_ref()))
        .map(|(k, v)| (k.into_owned(), v.into_owned()))
        .collect();
    if kept.is_empty() {
        url.set_query(None);
    } else {
        let query = url::form_urlencoded::Serializer::new(String::new())
            .extend_pairs(kept)
            .finish();
        url.set_query(Some(&query));
    }
}

fn is_locale_root_path(path: &str) -> bool {
    let segment = path.strip_prefix('/').unwrap_or(path);
    let mut parts = segment.split('-');
    let Some(language) = parts.next() else {
        return false;
    };
    let region = parts.next();
    parts.next().is_none()
        && language.len() == 2
        && language.chars().all(|c| c.is_ascii_lowercase())
        && region.is_none_or(|r| r.len() == 2 && r.chars().all(|c| c.is_ascii_uppercase()))
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

const EMPTY_POSITIONS: &[usize] = &[];

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::time::Duration;

    use zoeken_engine_core::{EngineError, EngineResults};
    use zoeken_results::{Answer, MainResult, Result_, Suggestion};

    use crate::execution::{EngineRunOutcome, EngineRunStatus, ExecutionReport};
    use crate::metrics::NoopRecorder;

    #[derive(Default)]
    struct CountingRecorder {
        count: AtomicUsize,
    }

    impl MetricsRecorder for CountingRecorder {
        fn record_engine(&self, _sample: EngineSample<'_>) {
            self.count.fetch_add(1, Ordering::SeqCst);
        }
    }

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
            &NoopRecorder,
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
            &NoopRecorder,
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
            &NoopRecorder,
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
            &NoopRecorder,
        );

        assert_eq!(container.results.len(), 1);
        let result = as_main(&container.results[0]);
        assert_eq!(
            result.engines,
            vec!["alpha".to_string(), "beta".to_string()]
        );
        assert_eq!(result.positions, vec![1, 1]);
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
            &NoopRecorder,
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
            &NoopRecorder,
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
            &NoopRecorder,
        );

        let mut c = EngineResults::new();
        c.add(main_result("https://x.test/", "x"));
        let single = aggregate(
            report(vec![completed("a", c)]),
            &weights(&[("a", 1.0)]),
            &NoopRecorder,
        );

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
        let container = aggregate(
            report(vec![completed("a", a)]),
            &weights(&[("a", 2.0)]),
            &NoopRecorder,
        );
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
            &NoopRecorder,
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

        let container = aggregate(
            report(vec![completed("a", a)]),
            &weights(&[("a", 1.0)]),
            &NoopRecorder,
        );

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
            &NoopRecorder,
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
            &NoopRecorder,
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

        let container = aggregate(report(outcomes), &EngineWeights::default(), &NoopRecorder);

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
    fn records_one_metric_sample_per_engine_outcome() {
        let mut a = EngineResults::new();
        a.add(main_result("https://x.test/", "x"));
        let recorder = CountingRecorder::default();

        let outcomes = vec![
            completed("a", a),
            EngineRunOutcome {
                engine: "b".to_string(),
                status: EngineRunStatus::Failed(EngineError::Timeout),
                duration: Duration::from_millis(1),
                http_duration: None,
            },
            EngineRunOutcome {
                engine: "c".to_string(),
                status: EngineRunStatus::Unresponsive(UnresponsiveReason::EngineTimeout),
                duration: Duration::from_secs(1),
                http_duration: None,
            },
        ];

        let _ = aggregate(report(outcomes), &weights(&[("a", 1.0)]), &recorder);
        assert_eq!(recorder.count.load(Ordering::SeqCst), 3);
    }

    #[test]
    fn unknown_engine_weight_defaults_to_one() {
        let mut a = EngineResults::new();
        a.add(main_result("https://x.test/", "x"));
        let container = aggregate(
            report(vec![completed("a", a)]),
            &EngineWeights::default(),
            &NoopRecorder,
        );
        assert_eq!(as_main(&container.results[0]).score, 1.0);
    }
}
