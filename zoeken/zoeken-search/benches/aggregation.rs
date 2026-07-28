use std::hint::black_box;
use std::time::Duration;

use criterion::{BenchmarkId, Criterion, Throughput, criterion_group, criterion_main};
use zoeken_engine_core::{EngineError, EngineResults};
use zoeken_results::{MainResult, Result_};
use zoeken_search::{
    EngineRunOutcome, EngineRunStatus, EngineWeights, ExecutionReport, NoopRecorder,
    UnresponsiveReason, aggregate, sort_results,
};

fn main_result(url: &str, title: &str) -> Result_ {
    Result_::Main(MainResult {
        url: url.to_string(),
        normalized_url: url.to_string(),
        title: title.to_string(),
        ..MainResult::default()
    })
}

fn scored_result(index: usize, score: f64, engines: usize) -> Result_ {
    Result_::Main(MainResult {
        url: format!("https://example.test/{index}"),
        normalized_url: format!("https://example.test/{index}"),
        engine: "alpha".to_string(),
        engines: (0..engines).map(|i| format!("eng{i}")).collect(),
        score,
        positions: vec![index % 10 + 1],
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

fn report_for(engine_count: usize, urls_per_engine: usize, overlap: bool) -> ExecutionReport {
    let outcomes = (0..engine_count)
        .map(|ei| {
            let name = format!("eng{ei}");
            let mut bag = EngineResults::new();
            for ui in 0..urls_per_engine {
                let url_id = if overlap {
                    (ei * 2 + ui) % urls_per_engine.max(1)
                } else {
                    ei * urls_per_engine + ui
                };
                bag.add(main_result(
                    &format!("https://www.example.test/page/{url_id}?utm_source={name}"),
                    &format!("title {url_id}"),
                ));
            }
            completed(&name, bag)
        })
        .collect();
    ExecutionReport { outcomes }
}

fn bench_sort_results(c: &mut Criterion) {
    let mut group = c.benchmark_group("sort_results");
    for size in [100usize, 1_000, 5_000] {
        group.throughput(Throughput::Elements(size as u64));
        group.bench_with_input(BenchmarkId::from_parameter(size), &size, |b, &size| {
            let mut results: Vec<Result_> = (0..size)
                .map(|i| scored_result(i, (i % 20) as f64, 4))
                .collect();
            b.iter(|| {
                results.reverse();
                sort_results(black_box(&mut results));
                black_box(results.len())
            });
        });
    }
    group.finish();
}

fn bench_aggregate(c: &mut Criterion) {
    let mut group = c.benchmark_group("aggregate");
    let cases = [
        ("8x20_overlap", 8usize, 20usize, true),
        ("20x40_overlap", 20, 40, true),
        ("20x40_unique", 20, 40, false),
    ];
    for (label, engines, urls, overlap) in cases {
        let weights = EngineWeights::new((0..engines).map(|i| (format!("eng{i}"), 1.0)));
        group.bench_function(label, |b| {
            b.iter(|| {
                let report = report_for(engines, urls, overlap);
                let container = aggregate(black_box(report), black_box(&weights), &NoopRecorder);
                black_box(container.number_of_results)
            });
        });
    }
    group.finish();
}

fn bench_aggregate_failures(c: &mut Criterion) {
    c.bench_function("aggregate_all_failed", |b| {
        let weights = EngineWeights::new((0..16).map(|i| (format!("eng{i}"), 1.0)));
        b.iter(|| {
            let outcomes = (0..16)
                .map(|i| EngineRunOutcome {
                    engine: format!("eng{i}"),
                    status: if i % 2 == 0 {
                        EngineRunStatus::Failed(EngineError::Unexpected("boom".into()))
                    } else {
                        EngineRunStatus::Unresponsive(UnresponsiveReason::EngineTimeout)
                    },
                    duration: Duration::from_millis(1),
                    http_duration: None,
                })
                .collect();
            let container = aggregate(
                black_box(ExecutionReport { outcomes }),
                black_box(&weights),
                &NoopRecorder,
            );
            black_box(container.unresponsive_engines.len())
        });
    });
}

criterion_group!(
    benches,
    bench_sort_results,
    bench_aggregate,
    bench_aggregate_failures
);
criterion_main!(benches);
