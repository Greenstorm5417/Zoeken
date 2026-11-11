//! Fixture regression for the daily-search set.
//!
//! Numbers here are judgments against canned titles and snippets. They are not
//! live provider quality and they are not a claim about pages we did not fetch.

use std::collections::HashSet;
use std::fs;
use std::time::Instant;

use serde::Deserialize;
use zoeken_query::{Intent, intent_holdout, interpret, model_enabled, model_only, rules_only};
use zoeken_search::ranking::FusionHit;
use zoeken_search::{
    Fusion, choose_fusion, order_production, order_rrf, reciprocal_rank, rerank_snippets,
    semantic_decision, semantic_experiment, semantic_rerank_enabled, shipped_fusion,
};

#[derive(Debug, Deserialize)]
struct DailyQuery {
    id: String,
    q: String,
    bucket: String,
    intent: String,
    role: String,
    positive: Vec<String>,
    negative: Vec<String>,
    relevant: Vec<String>,
    hits: Vec<Hit>,
    place: Option<String>,
    site: Option<String>,
    filetype: Option<String>,
}

#[derive(Debug, Deserialize)]
struct Hit {
    engine: String,
    weight: f64,
    url: String,
    title: String,
    snippet: String,
}

#[test]
fn daily_search_fixture_drives_shipping_decisions() {
    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/fixtures/daily_queries.json"
    );
    let queries: Vec<DailyQuery> =
        serde_json::from_str(&fs::read_to_string(path).expect("fixture")).expect("json");
    assert!(
        (100..=200).contains(&queries.len()),
        "expected 100–200 queries, got {}",
        queries.len()
    );

    let started = Instant::now();
    let mut intent_hits = 0usize;
    let mut mismatches = Vec::new();
    let mut paraphrase_rules = 0usize;
    let mut paraphrase_cascade = 0usize;
    let mut paraphrase_n = 0usize;
    let mut trap_overrides = 0usize;
    let mut tp = 0usize;
    let mut fp = 0usize;
    let mut fn_ = 0usize;
    let mut negative_hits = 0usize;

    let mut legacy_mrr = Vec::new();
    let mut rrf_mrr = Vec::new();
    let mut rerank_mrr = Vec::new();
    let mut legacy_nav = Vec::new();
    let mut rrf_nav = Vec::new();
    let mut rerank_nav = Vec::new();
    let mut empty_results = 0usize;
    let mut result_queries = 0usize;
    let mut duplicate_outputs = 0usize;
    let mut ranked_outputs = 0usize;

    for query in &queries {
        let rules = rules_only(&query.q);
        let parsed = interpret(&query.q);
        assert_eq!(parsed.original, query.q, "{}", query.id);
        if parsed.intent.as_str() == query.intent {
            intent_hits += 1;
        } else {
            mismatches.push(format!(
                "{} {:?} rules={} cascade={} expected={}",
                query.id,
                query.q,
                rules.intent.as_str(),
                parsed.intent.as_str(),
                query.intent
            ));
        }
        if let Some(place) = &query.place {
            let found = parsed
                .entities
                .iter()
                .any(|entity| entity.kind == "place" && entity.value == *place);
            assert!(found, "{} missing place {place}", query.id);
        }
        if let Some(site) = &query.site {
            assert_eq!(
                parsed.operators.site.as_deref(),
                Some(site.as_str()),
                "{}",
                query.id
            );
        }
        if let Some(filetype) = &query.filetype {
            assert_eq!(
                parsed.operators.filetype.as_deref(),
                Some(filetype.as_str()),
                "{}",
                query.id
            );
        }

        if query.role == "paraphrase" {
            paraphrase_n += 1;
            if rules.intent.as_str() == query.intent {
                paraphrase_rules += 1;
            }
            if parsed.intent.as_str() == query.intent {
                paraphrase_cascade += 1;
            }
        }
        if query.role == "trap" && rules.intent == Intent::General {
            let model = model_only(&query.q);
            let harmful = matches!(
                model.intent,
                Intent::Weather | Intent::Arithmetic | Intent::Conversion
            ) && model.confidence >= 0.75;
            if harmful {
                trap_overrides += 1;
            }
        }

        let predicted = answer_kind(parsed.intent.as_str());
        let mut produced: Vec<&str> = Vec::new();
        if zoeken_query::weather_request_location(&query.q).is_some() {
            produced.push("weather");
        }
        if let Some(kind) = predicted.filter(|kind| *kind != "weather") {
            produced.push(kind);
        }
        for kind in &produced {
            if query.positive.iter().any(|item| item == *kind) {
                tp += 1;
            } else {
                fp += 1;
            }
            if query.negative.iter().any(|item| item == *kind) {
                negative_hits += 1;
            }
        }
        if !query.positive.is_empty()
            && !query
                .positive
                .iter()
                .any(|kind| produced.iter().any(|item| item == kind))
        {
            fn_ += 1;
        }

        let engines = group_hits(&query.hits);
        let production = order_production(&query.q, &engines);
        result_queries += 1;
        if production.is_empty() {
            empty_results += 1;
        }
        if query.hits.is_empty() || query.relevant.is_empty() {
            continue;
        }
        let rrf = order_rrf(&query.q, &engines);
        let lookup: Vec<(String, String, String)> = production
            .iter()
            .map(|url| {
                let hit = query
                    .hits
                    .iter()
                    .find(|hit| &hit.url == url)
                    .expect("url from hits");
                (url.clone(), hit.title.clone(), hit.snippet.clone())
            })
            .collect();
        let reranked = rerank_snippets(&query.q, &lookup);
        let legacy = reciprocal_rank(&production, &query.relevant);
        let fused = reciprocal_rank(&rrf, &query.relevant);
        let semantic = reciprocal_rank(&reranked, &query.relevant);
        legacy_mrr.push(legacy);
        rrf_mrr.push(fused);
        rerank_mrr.push(semantic);
        if query.bucket == "navigation" {
            legacy_nav.push(legacy);
            rrf_nav.push(fused);
            rerank_nav.push(semantic);
        }
        ranked_outputs += 1;
        let unique = production.iter().collect::<HashSet<_>>().len();
        if unique != production.len() {
            duplicate_outputs += 1;
        }
    }
    let elapsed = started.elapsed();

    let n = queries.len() as f64;
    let intent_acc = intent_hits as f64 / n;
    let precision = if tp + fp == 0 {
        1.0
    } else {
        tp as f64 / (tp + fp) as f64
    };
    let recall = if tp + fn_ == 0 {
        1.0
    } else {
        tp as f64 / (tp + fn_) as f64
    };
    let legacy_mean = mean(&legacy_mrr);
    let rrf_mean = mean(&rrf_mrr);
    let rerank_mean = mean(&rerank_mrr);
    let legacy_nav_mean = mean(&legacy_nav);
    let rrf_nav_mean = mean(&rrf_nav);
    let rerank_nav_mean = mean(&rerank_nav);
    let empty_rate = if result_queries == 0 {
        0.0
    } else {
        empty_results as f64 / result_queries as f64
    };
    let duplicate_rate = if ranked_outputs == 0 {
        0.0
    } else {
        duplicate_outputs as f64 / ranked_outputs as f64
    };
    let fusion = choose_fusion(legacy_mean, rrf_mean, legacy_nav_mean, rrf_nav_mean);
    let rerank_on = semantic_decision(legacy_mean, rerank_mean, legacy_nav_mean, rerank_nav_mean);
    let model_on = paraphrase_cascade > paraphrase_rules && trap_overrides == 0;

    eprintln!(
        "daily-search fixture n={n:.0} intent_acc={intent_acc:.3} dispatch_p={precision:.3} dispatch_r={recall:.3} empty_from_results={empty_rate:.3} dup={duplicate_rate:.3} fixture_mrr_legacy={legacy_mean:.3} fixture_mrr_rrf={rrf_mean:.3} fixture_mrr_rerank={rerank_mean:.3} nav_legacy={legacy_nav_mean:.3} nav_rrf={rrf_nav_mean:.3} nav_rerank={rerank_nav_mean:.3} paraphrase {paraphrase_cascade}/{paraphrase_n} vs rules {paraphrase_rules} trap_overrides={trap_overrides} local_ms={} fusion={} rerank={rerank_on} model={model_on}",
        elapsed.as_secs_f64() * 1000.0,
        fusion.as_str(),
    );

    assert!(
        mismatches.is_empty(),
        "intent mismatches:\n{}",
        mismatches.join("\n")
    );
    assert_eq!(
        negative_hits, 0,
        "an answer trigger fired on a negative example"
    );
    assert!(precision >= 0.9, "trigger precision {precision}");
    assert!(recall >= 0.9, "trigger recall {recall}");
    assert_eq!(
        duplicate_rate, 0.0,
        "production ranking emitted duplicate urls"
    );
    assert_eq!(
        shipped_fusion(),
        fusion,
        "shipped fusion must match the fixture decision"
    );
    assert!(
        !rerank_on,
        "relevant-vs-unrelated rows cannot decide a semantic reranker ({legacy_mean} -> {rerank_mean})"
    );
    let experiment = semantic_experiment();
    eprintln!(
        "semantic representative baseline_mrr={:.3} rerank_mrr={:.3} ordinary={:.3}->{:.3} technical={:.3}->{:.3} nav={:.3}->{:.3} probe={:.3}->{:.3} candidates={} latency_us={} bytes={} enable={}",
        experiment.baseline_mrr,
        experiment.rerank_mrr,
        experiment.ordinary_baseline,
        experiment.ordinary_rerank,
        experiment.technical_baseline,
        experiment.technical_rerank,
        experiment.nav_baseline,
        experiment.nav_rerank,
        experiment.probe_baseline_mrr,
        experiment.probe_rerank_mrr,
        experiment.candidates,
        experiment.latency_us,
        experiment.embedding_bytes,
        experiment.enable
    );
    assert_eq!(semantic_rerank_enabled(), experiment.enable);
    let holdout = intent_holdout();
    eprintln!(
        "intent holdout rules_p={:.3} rules_r={:.3} p={:.3} r={:.3} fp={} positives={} traps={} enable={}",
        holdout.rules_precision,
        holdout.rules_recall,
        holdout.precision,
        holdout.recall,
        holdout.false_positives,
        holdout.positives,
        holdout.traps,
        holdout.enable
    );
    assert_eq!(model_enabled(), holdout.enable);
    assert_eq!(shipped_fusion(), Fusion::WeightOverPosition);
    let _ = empty_rate;
}
fn answer_kind(intent: &str) -> Option<&'static str> {
    match intent {
        "arithmetic" => Some("calculator"),
        "conversion" => Some("unit"),
        "weather" => Some("weather"),
        "definition" => Some("dictionary"),
        "translation" => Some("translate"),
        _ => None,
    }
}

fn group_hits(hits: &[Hit]) -> Vec<(String, f64, Vec<FusionHit>)> {
    let mut engines: Vec<(String, f64, Vec<FusionHit>)> = Vec::new();
    for hit in hits {
        if let Some(engine) = engines.iter_mut().find(|(name, _, _)| name == &hit.engine) {
            engine.2.push(FusionHit {
                url: hit.url.clone(),
                title: hit.title.clone(),
                snippet: hit.snippet.clone(),
            });
        } else {
            engines.push((
                hit.engine.clone(),
                hit.weight,
                vec![FusionHit {
                    url: hit.url.clone(),
                    title: hit.title.clone(),
                    snippet: hit.snippet.clone(),
                }],
            ));
        }
    }
    engines
}

fn mean(values: &[f64]) -> f64 {
    if values.is_empty() {
        0.0
    } else {
        values.iter().sum::<f64>() / values.len() as f64
    }
}
