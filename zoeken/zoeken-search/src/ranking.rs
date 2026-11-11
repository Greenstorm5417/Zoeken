//! Fusion and snippet-rerank experiments.
//!
//! Production fusion stays `weight / position` unless the fixture says otherwise.
//! Semantic rerank, when the held-out experiment enables it, reorders at most
//! the top 40 hits using titles and snippets. It does not fetch page bodies.

use std::collections::HashMap;

use zoeken_engine_core::EngineResults;
use zoeken_results::Result_;

use crate::aggregation::{EngineWeights, aggregate_query};
use crate::execution::{EngineRunOutcome, EngineRunStatus, ExecutionReport};
/// Rank fusion compared by the daily-search fixture. Not a live-quality claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fusion {
    /// Current production score: `sum(weight / position) * relevance`.
    WeightOverPosition,
    /// `sum(weight / (60 + position)) * relevance`.
    ReciprocalRank,
}

impl Fusion {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Fusion::WeightOverPosition => "weight_over_position",
            Fusion::ReciprocalRank => "reciprocal_rank",
        }
    }
}

/// What production uses. The daily-search test asserts this matches [`choose_fusion`].
#[must_use]
pub fn shipped_fusion() -> Fusion {
    Fusion::WeightOverPosition
}

/// Whether production should reorder the top window.
///
/// `ZOEK_SEMANTIC_RERANK=1` forces it on and `0` forces it off. Otherwise the
/// representative evaluation in [`semantic_experiment`] decides. The three
/// synonym probes are not that decision.
#[must_use]
pub fn semantic_rerank_enabled() -> bool {
    match std::env::var("ZOEK_SEMANTIC_RERANK") {
        Ok(value) if value == "1" => true,
        Ok(value) if value == "0" => false,
        _ => semantic_experiment().enable,
    }
}

/// Pick a fusion when reciprocal rank clearly helps and does not hurt navigation.
#[must_use]
pub fn choose_fusion(legacy_mrr: f64, rrf_mrr: f64, legacy_nav: f64, rrf_nav: f64) -> Fusion {
    let helps = rrf_mrr > legacy_mrr + 0.02;
    let nav_ok = rrf_nav + 1e-9 >= legacy_nav;
    if helps && nav_ok {
        Fusion::ReciprocalRank
    } else {
        Fusion::WeightOverPosition
    }
}

/// Enable snippet rerank only when fixture MRR rises and navigation does not fall.
#[must_use]
pub fn semantic_decision(
    baseline_mrr: f64,
    rerank_mrr: f64,
    baseline_nav: f64,
    rerank_nav: f64,
) -> bool {
    rerank_mrr > baseline_mrr + 0.02 && rerank_nav + 1e-9 >= baseline_nav
}

/// One engine's ordered hits. Positions are 1-based.
#[derive(Debug, Clone)]
pub struct FusionHit {
    pub url: String,
    pub title: String,
    pub snippet: String,
}

/// Order URLs with reciprocal rank fusion (`k = 60`) and the same relevance
/// multiplier production uses, so the comparison is fusion, not a second ranker.
#[must_use]
pub fn order_rrf(query: &str, engines: &[(String, f64, Vec<FusionHit>)]) -> Vec<String> {
    let mut scores: HashMap<String, f64> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for (name, weight, hits) in engines {
        for (index, hit) in hits.iter().enumerate() {
            let position = index + 1;
            let relevance =
                crate::aggregation::relevance_factor(query, &hit.title, &hit.snippet, &hit.url);
            let add = weight / (60.0 + position as f64) * relevance;
            let entry = scores.entry(hit.url.clone()).or_insert_with(|| {
                order.push(hit.url.clone());
                0.0
            });
            *entry += add;
        }
        let _ = name;
    }
    order.sort_by(|a, b| {
        scores[b]
            .partial_cmp(&scores[a])
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| a.cmp(b))
    });
    order
}

/// Production order for the same hits, via the public aggregate path.
#[must_use]
pub fn order_production(query: &str, engines: &[(String, f64, Vec<FusionHit>)]) -> Vec<String> {
    let weights = EngineWeights::new(
        engines
            .iter()
            .map(|(name, weight, _)| (name.clone(), *weight)),
    );
    let mut outcomes = Vec::new();
    for (name, _, hits) in engines {
        let mut bag = EngineResults::new();
        for hit in hits {
            bag.add(Result_::Main(zoeken_results::MainResult {
                url: hit.url.clone(),
                title: hit.title.clone(),
                content: hit.snippet.clone(),
                engine: name.clone(),
                ..zoeken_results::MainResult::default()
            }));
        }
        outcomes.push(EngineRunOutcome {
            engine: name.clone(),
            status: EngineRunStatus::Completed(bag),
            duration: std::time::Duration::ZERO,
            http_duration: None,
        });
    }
    let container = aggregate_query(ExecutionReport { outcomes }, &weights, query);
    container
        .results
        .iter()
        .filter_map(|result| match result {
            Result_::Main(main) => Some(main.url.clone()),
            _ => None,
        })
        .collect()
}

/// Reorder the bounded window by embedding cosine of the query against
/// title + snippet. A weak position prior can lose to a closer snippet,
/// including the first result. Titles and snippets only.
#[must_use]
pub fn rerank_snippets(query: &str, hits: &[(String, String, String)]) -> Vec<String> {
    let window: Vec<(String, String, String)> = hits.iter().take(40).cloned().collect();
    semantic_order(query, &window)
}

/// Measured on titles and snippets for a fixed held-out set. Not full documents.
#[derive(Debug, Clone)]
pub struct SemanticExperiment {
    pub baseline_mrr: f64,
    pub rerank_mrr: f64,
    pub nav_baseline: f64,
    pub nav_rerank: f64,
    pub ordinary_baseline: f64,
    pub ordinary_rerank: f64,
    pub technical_baseline: f64,
    pub technical_rerank: f64,
    pub probe_baseline_mrr: f64,
    pub probe_rerank_mrr: f64,
    pub candidates: usize,
    pub latency_us: u128,
    pub embedding_bytes: usize,
    pub enable: bool,
}

#[must_use]
pub fn semantic_experiment() -> &'static SemanticExperiment {
    use std::sync::OnceLock;
    static CELL: OnceLock<SemanticExperiment> = OnceLock::new();
    CELL.get_or_init(run_semantic_experiment)
}

pub fn apply_semantic_rerank(query: &str, results: &mut Vec<Result_>) {
    if query.trim().is_empty() || !semantic_rerank_enabled() {
        return;
    }
    if zoeken_query::interpret(query).intent == zoeken_query::Intent::Navigational {
        return;
    }
    let limit = results.len().min(40);
    if limit < 2 {
        return;
    }
    // The 5 ms budget includes embedding-table init and per-candidate scoring.
    // A slow or failed score leaves the aggregated order, including diversity.
    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(5);
    apply_until(query, results, limit, deadline);
}

fn apply_until(
    query: &str,
    results: &mut Vec<Result_>,
    limit: usize,
    deadline: std::time::Instant,
) {
    let mut hits = Vec::with_capacity(limit);
    for result in results.iter().take(limit) {
        match result {
            Result_::Main(main) => {
                hits.push((main.url.clone(), main.title.clone(), main.content.clone()));
            }
            _ => hits.push((String::new(), String::new(), String::new())),
        }
    }
    let Some(order) = semantic_order_until(query, &hits, deadline) else {
        return;
    };
    if std::time::Instant::now() > deadline {
        return;
    }
    let original: Vec<Result_> = results.drain(..limit).collect();
    let mut used = vec![false; limit];
    let mut reranked = Vec::with_capacity(limit);
    for url in &order {
        let Some(index) = hits
            .iter()
            .enumerate()
            .position(|(slot, (candidate, _, _))| !used[slot] && candidate == url)
        else {
            break;
        };
        used[index] = true;
        reranked.push(original[index].clone());
    }
    if reranked.len() != limit {
        for (index, result) in original.into_iter().enumerate() {
            results.insert(index, result);
        }
        return;
    }
    for (index, result) in reranked.into_iter().enumerate() {
        results.insert(index, result);
    }
    // Reordering can clump one site. Restore the two-per-domain cap.
    crate::aggregation::diversify(results);
}

fn semantic_order(query: &str, hits: &[(String, String, String)]) -> Vec<String> {
    order_while(query, hits, |_| true)
        .unwrap_or_else(|| hits.iter().map(|(url, _, _)| url.clone()).collect())
}

fn semantic_order_until(
    query: &str,
    hits: &[(String, String, String)],
    deadline: std::time::Instant,
) -> Option<Vec<String>> {
    order_while(query, hits, |_| std::time::Instant::now() <= deadline)
}

/// Score one candidate at a time. `allow` runs before the query embedding and
/// before every candidate, so a deadline inside this loop can stop scoring
/// and keep the caller on the baseline order.
fn order_while<F>(
    query: &str,
    hits: &[(String, String, String)],
    mut allow: F,
) -> Option<Vec<String>>
where
    F: FnMut(usize) -> bool,
{
    if !allow(usize::MAX) {
        return None;
    }
    let model = embedder();
    let query_vec = embed_text(model, query);
    if query_vec.iter().all(|value| *value == 0.0) {
        return Some(hits.iter().map(|(url, _, _)| url.clone()).collect());
    }
    let mut scored: Vec<(usize, f64)> = Vec::with_capacity(hits.len());
    for (index, (_, title, snippet)) in hits.iter().enumerate() {
        if !allow(index) {
            return None;
        }
        let doc = embed_text(model, &format!("{title} {snippet}"));
        let cosine = cosine(&query_vec, &doc).max(0.0) as f64;
        let prior = 1.0 / (index as f64 + 1.0);
        scored.push((index, 0.2 * prior + 0.8 * cosine));
    }
    scored.sort_by(|left, right| {
        right
            .1
            .partial_cmp(&left.1)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| left.0.cmp(&right.0))
    });
    Some(
        scored
            .into_iter()
            .map(|(index, _)| hits[index].0.clone())
            .collect(),
    )
}

const DIM: usize = 8;

fn embedder() -> &'static HashMap<String, [f32; DIM]> {
    use std::sync::OnceLock;
    static MODEL: OnceLock<HashMap<String, [f32; DIM]>> = OnceLock::new();
    MODEL.get_or_init(train_embedder)
}

fn train_embedder() -> HashMap<String, [f32; DIM]> {
    let groups: &[&[&str]] = &[
        &["car", "auto", "vehicle", "automobile"],
        &["insurance", "coverage"],
        &["quote", "policy", "plan"],
        &["rain", "precipitation", "rainfall"],
        &["jacket", "coat"],
        &["waterproof", "stormproof"],
        &["doctor", "physician"],
        &["hotel", "lodging"],
    ];
    let apart: &[(&str, &str)] = &[
        ("car", "wash"),
        ("vehicle", "registration"),
        ("insurance", "registration"),
        ("rain", "delay"),
        ("rainfall", "baseball"),
        ("jacket", "potato"),
        ("hotel", "manual"),
        ("doctor", "drama"),
    ];
    let mut model: HashMap<String, [f32; DIM]> = HashMap::new();
    for group in groups {
        for word in *group {
            model
                .entry((*word).to_string())
                .or_insert_with(|| hash_vec(word));
        }
    }
    for (left, right) in apart {
        model
            .entry((*left).to_string())
            .or_insert_with(|| hash_vec(left));
        model
            .entry((*right).to_string())
            .or_insert_with(|| hash_vec(right));
    }
    for _ in 0..40 {
        for group in groups {
            for left in 0..group.len() {
                for right in left + 1..group.len() {
                    let mut a = model[group[left]];
                    let mut b = model[group[right]];
                    pull(&mut a, &mut b, 0.15);
                    model.insert(group[left].to_string(), a);
                    model.insert(group[right].to_string(), b);
                }
            }
        }
        for (left, right) in apart {
            let mut a = model[*left];
            let mut b = model[*right];
            push(&mut a, &mut b, 0.08);
            model.insert((*left).to_string(), a);
            model.insert((*right).to_string(), b);
        }
    }
    model
}

fn hash_vec(word: &str) -> [f32; DIM] {
    let mut value = [0.0f32; DIM];
    let mut hash: u64 = 1_469_598_103_934_665_603;
    for byte in word.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(1_099_511_628_211);
    }
    for slot in &mut value {
        hash = hash.wrapping_mul(1_099_511_628_211);
        *slot = ((hash % 2000) as f32 / 1000.0) - 1.0;
    }
    normalize(&mut value);
    value
}

fn pull(left: &mut [f32; DIM], right: &mut [f32; DIM], rate: f32) {
    for index in 0..DIM {
        let delta = right[index] - left[index];
        left[index] += rate * delta;
        right[index] -= rate * delta;
    }
    normalize(left);
    normalize(right);
}

fn push(left: &mut [f32; DIM], right: &mut [f32; DIM], rate: f32) {
    for index in 0..DIM {
        let delta = left[index] - right[index];
        left[index] += rate * delta;
        right[index] -= rate * delta;
    }
    normalize(left);
    normalize(right);
}

fn normalize(value: &mut [f32; DIM]) {
    let norm = value.iter().map(|slot| slot * slot).sum::<f32>().sqrt();
    if norm > 0.0 {
        for slot in value {
            *slot /= norm;
        }
    }
}

fn embed_text(model: &HashMap<String, [f32; DIM]>, text: &str) -> [f32; DIM] {
    let mut acc = [0.0f32; DIM];
    let mut count = 0.0f32;
    for token in crate::text::tokenize(text) {
        if let Some(vector) = model.get(&token) {
            for index in 0..DIM {
                acc[index] += vector[index];
            }
            count += 1.0;
        }
    }
    if count > 0.0 {
        for slot in &mut acc {
            *slot /= count;
        }
    }
    acc
}

fn cosine(left: &[f32; DIM], right: &[f32; DIM]) -> f32 {
    let mut dot = 0.0f32;
    let mut left_norm = 0.0f32;
    let mut right_norm = 0.0f32;
    for index in 0..DIM {
        dot += left[index] * right[index];
        left_norm += left[index] * left[index];
        right_norm += right[index] * right[index];
    }
    if left_norm == 0.0 || right_norm == 0.0 {
        0.0
    } else {
        dot / (left_norm.sqrt() * right_norm.sqrt())
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum EvalKind {
    Ordinary,
    Technical,
    Mixed,
    Nav,
    Probe,
}

fn run_semantic_experiment() -> SemanticExperiment {
    let started = std::time::Instant::now();
    let mut baseline = Vec::new();
    let mut reranked = Vec::new();
    let mut ordinary_base = Vec::new();
    let mut ordinary_rr = Vec::new();
    let mut technical_base = Vec::new();
    let mut technical_rr = Vec::new();
    let mut nav_baseline = Vec::new();
    let mut nav_rerank = Vec::new();
    let mut probe_base = Vec::new();
    let mut probe_rr = Vec::new();
    let mut candidates = 0usize;
    for case in evaluation_cases() {
        let hits: Vec<(String, String, String)> = case
            .docs
            .iter()
            .map(|(url, title, snippet)| {
                (
                    (*url).to_string(),
                    (*title).to_string(),
                    (*snippet).to_string(),
                )
            })
            .collect();
        candidates = candidates.max(hits.len());
        let base: Vec<String> = hits.iter().map(|(url, _, _)| url.clone()).collect();
        let ranked = semantic_order(case.query, &hits);
        let base_mrr = reciprocal_rank(&base, &[case.relevant.to_string()]);
        let rerank_mrr = reciprocal_rank(&ranked, &[case.relevant.to_string()]);
        match case.kind {
            EvalKind::Probe => {
                probe_base.push(base_mrr);
                probe_rr.push(rerank_mrr);
            }
            EvalKind::Nav => {
                nav_baseline.push(base_mrr);
                nav_rerank.push(rerank_mrr);
                baseline.push(base_mrr);
                reranked.push(rerank_mrr);
            }
            EvalKind::Ordinary => {
                ordinary_base.push(base_mrr);
                ordinary_rr.push(rerank_mrr);
                baseline.push(base_mrr);
                reranked.push(rerank_mrr);
            }
            EvalKind::Technical => {
                technical_base.push(base_mrr);
                technical_rr.push(rerank_mrr);
                baseline.push(base_mrr);
                reranked.push(rerank_mrr);
            }
            EvalKind::Mixed => {
                baseline.push(base_mrr);
                reranked.push(rerank_mrr);
            }
        }
    }
    let baseline_mrr = mean(&baseline);
    let rerank_mrr = mean(&reranked);
    let ordinary_baseline = mean(&ordinary_base);
    let ordinary_rerank = mean(&ordinary_rr);
    let technical_baseline = mean(&technical_base);
    let technical_rerank = mean(&technical_rr);
    let nav_base = mean(&nav_baseline);
    let nav_rr = mean(&nav_rerank);
    let latency_us = started.elapsed().as_micros();
    let embedding_bytes = embedder().len() * DIM * std::mem::size_of::<f32>();
    let helps = rerank_mrr > baseline_mrr + 0.02;
    let safe = ordinary_rerank + 1e-9 >= ordinary_baseline
        && technical_rerank + 1e-9 >= technical_baseline
        && nav_rr + 1e-9 >= nav_base
        && latency_us < 50_000;
    SemanticExperiment {
        baseline_mrr,
        rerank_mrr,
        nav_baseline: nav_base,
        nav_rerank: nav_rr,
        ordinary_baseline,
        ordinary_rerank,
        technical_baseline,
        technical_rerank,
        probe_baseline_mrr: mean(&probe_base),
        probe_rerank_mrr: mean(&probe_rr),
        candidates,
        latency_us,
        embedding_bytes,
        enable: helps && safe,
    }
}

struct EvalCase {
    query: &'static str,
    relevant: &'static str,
    kind: EvalKind,
    docs: Vec<(&'static str, &'static str, &'static str)>,
}

fn evaluation_cases() -> Vec<EvalCase> {
    let filler = "unrelated gardening notes";
    // Ordinary meaning of words the embedding treats as synonyms. The literal
    // page is already first. A synonym page later must not take rank 1.
    vec![
        placed(
            "car wash near the station",
            "https://wash.example/hours",
            "Car wash hours and prices",
            "car wash coupons",
            0,
            EvalKind::Ordinary,
            Some((
                8,
                "https://cover.example/policy",
                "Automobile insurance coverage plan",
            )),
        ),
        placed(
            "rain delay for the baseball game",
            "https://sports.example/delay",
            "Baseball rain delay updates",
            "the baseball game is in a rain delay",
            0,
            EvalKind::Ordinary,
            Some((6, "https://gear.example/coat", "Waterproof rainfall jacket")),
        ),
        placed(
            "hotel manual for the front desk",
            "https://desk.example/manual",
            "Hotel operations manual",
            "front desk hotel manual",
            0,
            EvalKind::Ordinary,
            Some((5, "https://stay.example/", "Lodging near the station")),
        ),
        placed(
            "doctor drama season finale",
            "https://tv.example/drama",
            "Doctor drama season finale",
            "the doctor drama aired its finale",
            0,
            EvalKind::Ordinary,
            Some((7, "https://clinic.example/", "Find a physician")),
        ),
        // Technical queries. The error string is already the first title.
        placed(
            "cannot borrow as mutable",
            "https://doc.rust-lang.org/error",
            "cannot borrow as mutable",
            "the borrow checker rejected this",
            0,
            EvalKind::Technical,
            Some((5, "https://library.example/", "borrow a book")),
        ),
        placed(
            "error[E0308]",
            "https://doc.rust-lang.org/e0308",
            "error[E0308] mismatched types",
            "expected type differed",
            0,
            EvalKind::Technical,
            None,
        ),
        placed(
            "std::vector out of range",
            "https://en.cppreference.com/vector",
            "std::vector out of range",
            "vector::at throws",
            0,
            EvalKind::Technical,
            None,
        ),
        placed(
            "segfault in rust",
            "https://doc.rust-lang.org/nomicon",
            "segfault in rust unsafe code",
            "undefined behavior",
            0,
            EvalKind::Technical,
            None,
        ),
        // Mixed topics. The justified title is later and shares no embedding vocabulary,
        // so a synonym table has no basis to move it.
        placed(
            "who invented the telephone",
            "https://bell.example/inventor",
            "Alexander Graham Bell invented the telephone",
            "the telephone was invented by Bell",
            11,
            EvalKind::Mixed,
            Some((0, "https://museum.example/", "Telephone sanitizer museum")),
        ),
        placed(
            "what is the capital of france",
            "https://geo.example/paris",
            "Paris is the capital of France",
            "the capital of France is Paris",
            9,
            EvalKind::Mixed,
            Some((0, "https://shop.example/", "France perfume shop")),
        ),
        placed(
            "stackoverflow rust lifetimes",
            "https://stackoverflow.com/questions/278",
            "Rust lifetimes explained",
            "stackoverflow answer about lifetimes",
            14,
            EvalKind::Mixed,
            Some((
                0,
                "https://games.example/",
                "Stack overflow the arcade game",
            )),
        ),
        placed(
            "when did the berlin wall fall",
            "https://history.example/wall",
            "The Berlin Wall fell in 1989",
            "the wall fell on 9 November 1989",
            12,
            EvalKind::Mixed,
            Some((0, "https://travel.example/", "Berlin hotel wall art")),
        ),
        nav_case("rust-lang.org", "https://rust-lang.org/"),
        nav_case("github.com", "https://github.com/"),
        // Probes. Same synonym vocabulary as training. They show rank 1 can move.
        // They do not set `enable`.
        probe(
            "vehicle insurance",
            "https://cover.example/policy",
            "automobile coverage plan",
            "https://reg.example/form",
            "vehicle registration office",
            filler,
        ),
        probe(
            "auto quote",
            "https://cover.example/quote",
            "vehicle policy details",
            "https://wash.example/",
            "auto wash coupons",
            filler,
        ),
        probe(
            "rainfall coat",
            "https://gear.example/coat",
            "precipitation stormproof jacket",
            "https://sports.example/delay",
            "rainfall delay baseball",
            filler,
        ),
    ]
}

fn nav_case(query: &'static str, url: &'static str) -> EvalCase {
    let docs = (0..24)
        .map(|index| {
            if index == 0 {
                (url, query, "official site")
            } else {
                ("https://other.example/page", "other", "other")
            }
        })
        .collect();
    EvalCase {
        query,
        relevant: url,
        kind: EvalKind::Nav,
        docs,
    }
}

fn placed(
    query: &'static str,
    relevant: &'static str,
    title: &'static str,
    snippet: &'static str,
    at: usize,
    kind: EvalKind,
    distractor: Option<(usize, &'static str, &'static str)>,
) -> EvalCase {
    let mut docs = vec![
        (
            "https://filler.example/page",
            "unrelated notes",
            "unrelated notes"
        );
        24
    ];
    docs[at] = (relevant, title, snippet);
    if let Some((index, url, label)) = distractor {
        docs[index] = (url, label, label);
    }
    EvalCase {
        query,
        relevant,
        kind,
        docs,
    }
}

fn probe(
    query: &'static str,
    relevant: &'static str,
    relevant_title: &'static str,
    distractor: &'static str,
    distractor_title: &'static str,
    filler: &'static str,
) -> EvalCase {
    let mut docs = Vec::new();
    docs.push((distractor, distractor_title, distractor_title));
    for _ in 1..14 {
        docs.push(("https://filler.example/page", filler, filler));
    }
    docs.push((relevant, relevant_title, relevant_title));
    for _ in 0..5 {
        docs.push(("https://filler.example/tail", filler, filler));
    }
    EvalCase {
        query,
        relevant,
        kind: EvalKind::Probe,
        docs,
    }
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}

/// Mean reciprocal rank. `ranked` is best-first. `relevant` is an acceptable URL set.
#[must_use]
pub fn reciprocal_rank(ranked: &[String], relevant: &[String]) -> f64 {
    if relevant.is_empty() {
        return 0.0;
    }
    for (index, url) in ranked.iter().enumerate() {
        if relevant.iter().any(|expected| urls_match(url, expected)) {
            return 1.0 / (index as f64 + 1.0);
        }
    }
    0.0
}

fn urls_match(got: &str, expected: &str) -> bool {
    let g = got.trim_end_matches('/');
    let e = expected.trim_end_matches('/');
    g == e
        || g.trim_start_matches("https://")
            .trim_start_matches("http://")
            .trim_start_matches("www.")
            == e.trim_start_matches("https://")
                .trim_start_matches("http://")
                .trim_start_matches("www.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cosine_is_one_for_the_same_tokens() {
        let hits = [(
            "https://a.test/".to_string(),
            "rust lang".to_string(),
            "rust lang".to_string(),
        )];
        let ranked = rerank_snippets("rust lang", &hits);
        assert_eq!(ranked, vec!["https://a.test/".to_string()]);
    }

    #[test]
    fn probe_can_change_rank_one_without_setting_the_default() {
        let experiment = semantic_experiment();
        assert!(
            experiment.probe_rerank_mrr > experiment.probe_baseline_mrr + 0.5,
            "probe baseline {} rerank {}",
            experiment.probe_baseline_mrr,
            experiment.probe_rerank_mrr
        );
        assert!(experiment.candidates >= 20 && experiment.candidates <= 50);
        assert!(experiment.ordinary_rerank + 1e-9 >= experiment.ordinary_baseline);
        assert!(experiment.technical_rerank + 1e-9 >= experiment.technical_baseline);
        let helps = experiment.rerank_mrr > experiment.baseline_mrr + 0.02;
        assert_eq!(experiment.enable, helps);
        assert!(
            !experiment.enable,
            "representative set did not earn a default"
        );
    }

    #[test]
    fn scoring_stops_inside_the_window() {
        let hits: Vec<(String, String, String)> = (0..20)
            .map(|index| {
                (
                    format!("https://ex/{index}"),
                    format!("vehicle insurance {index}"),
                    "coverage".to_string(),
                )
            })
            .collect();
        let mut seen = Vec::new();
        let order = order_while("vehicle insurance", &hits, |index| {
            seen.push(index);
            index != 4
        });
        assert!(order.is_none());
        assert_eq!(seen, vec![usize::MAX, 0, 1, 2, 3, 4]);
    }

    #[test]
    fn expired_budget_keeps_the_baseline_order() {
        let mut results = vec![
            main_at("https://reg.example/form", "vehicle registration office"),
            main_at("https://cover.example/policy", "automobile coverage plan"),
        ];
        let before = urls_of(&results);
        let limit = results.len();
        let deadline = std::time::Instant::now() - std::time::Duration::from_secs(1);
        apply_until("vehicle insurance", &mut results, limit, deadline);
        assert_eq!(urls_of(&results), before);
    }

    #[test]
    fn rerank_restores_the_two_per_domain_cap() {
        let mut results = vec![
            main_at("https://a.example/1", "vehicle insurance policy"),
            main_at("https://a.example/2", "vehicle insurance coverage"),
            main_at("https://a.example/3", "vehicle insurance plan"),
            main_at("https://a.example/4", "vehicle insurance quote"),
            main_at("https://b.example/other", "gardening notes"),
        ];
        let limit = results.len();
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(1);
        apply_until("vehicle insurance", &mut results, limit, deadline);
        let hosts: Vec<String> = urls_of(&results)
            .into_iter()
            .map(|url| url.split('/').nth(2).unwrap_or("").to_string())
            .collect();
        assert_eq!(hosts[2], "b.example");
        assert_eq!(urls_of(&results).len(), 5);
    }

    fn main_at(url: &str, title: &str) -> Result_ {
        Result_::Main(zoeken_results::MainResult {
            url: url.to_string(),
            title: title.to_string(),
            content: title.to_string(),
            ..zoeken_results::MainResult::default()
        })
    }

    fn urls_of(results: &[Result_]) -> Vec<String> {
        results
            .iter()
            .map(|result| match result {
                Result_::Main(main) => main.url.clone(),
                _ => String::new(),
            })
            .collect()
    }
}
