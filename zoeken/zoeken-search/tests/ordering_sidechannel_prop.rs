//! Property-based test for aggregation ordering and side-channel separation.

use std::collections::HashSet;
use std::time::Duration;

use zoeken_engine_core::EngineResults;
use zoeken_results::{
    Answer, Code, Correction, FileResult, Image, Infobox, KeyValue, MainResult, Paper, Result_,
    Suggestion,
};
use zoeken_search::execution::{EngineRunOutcome, EngineRunStatus, ExecutionReport};
use zoeken_search::metrics::NoopRecorder;
use zoeken_search::{EngineWeights, aggregate};

use proptest::collection::{hash_set, vec};
use proptest::option;
use proptest::prelude::*;

fn score_of(result: &Result_) -> f64 {
    match result {
        Result_::Main(r) => r.score,
        Result_::Image(r) => r.score,
        Result_::Paper(r) => r.score,
        Result_::Code(r) => r.score,
        Result_::File(r) => r.score,
        Result_::KeyValue(r) => r.score,
        other => panic!("side-channel variant leaked into main results: {other:?}"),
    }
}

fn is_main_area(result: &Result_) -> bool {
    matches!(
        result,
        Result_::Main(_)
            | Result_::Image(_)
            | Result_::Paper(_)
            | Result_::Code(_)
            | Result_::File(_)
            | Result_::KeyValue(_)
    )
}

fn make_main(host: &str, variant: u8) -> Result_ {
    let url = format!("https://{host}.test/");
    match variant % 6 {
        0 => Result_::Main(MainResult {
            url: url.clone(),
            normalized_url: url,
            ..MainResult::default()
        }),
        1 => Result_::Image(Image {
            url: url.clone(),
            normalized_url: url,
            ..Image::default()
        }),
        2 => Result_::Paper(Paper {
            url: url.clone(),
            normalized_url: url,
            ..Paper::default()
        }),
        3 => Result_::Code(Code {
            url: url.clone(),
            normalized_url: url,
            ..Code::default()
        }),
        4 => Result_::File(FileResult {
            url: url.clone(),
            normalized_url: url,
            ..FileResult::default()
        }),
        _ => Result_::KeyValue(KeyValue {
            url: url.clone(),
            normalized_url: url,
            ..KeyValue::default()
        }),
    }
}

fn label() -> impl Strategy<Value = String> {
    "[a-z]{1,5}"
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 100, ..ProptestConfig::default() })]

    #[test]
    fn ordering_and_side_channel_separation(
        num_engines in 1usize..=4,
        main_specs in vec((label(), any::<u8>()), 0..12),
        answers in hash_set(label(), 0..6),
        suggestions in hash_set(label(), 0..6),
        corrections in hash_set(label(), 0..6),
        infoboxes in hash_set((label(), option::of(label())), 0..6),
    ) {
        let mut per_engine: Vec<EngineResults> =
            (0..num_engines).map(|_| EngineResults::new()).collect();

        for (i, (host, variant)) in main_specs.iter().enumerate() {
            per_engine[i % num_engines].add(make_main(host, *variant));
        }
        for (i, answer) in answers.iter().enumerate() {
            per_engine[i % num_engines].add(Result_::Answer(Answer {
                answer: answer.clone(),
                ..Answer::default()
            }));
        }
        for (i, suggestion) in suggestions.iter().enumerate() {
            per_engine[i % num_engines].add(Result_::Suggestion(Suggestion {
                suggestion: suggestion.clone(),
                ..Suggestion::default()
            }));
        }
        for (i, correction) in corrections.iter().enumerate() {
            per_engine[i % num_engines].add(Result_::Correction(Correction {
                correction: correction.clone(),
                ..Correction::default()
            }));
        }
        for (i, (title, id)) in infoboxes.iter().enumerate() {
            per_engine[i % num_engines].add(Result_::Infobox(Infobox {
                infobox: title.clone(),
                id: id.clone(),
                ..Infobox::default()
            }));
        }

        let outcomes = per_engine
            .into_iter()
            .enumerate()
            .map(|(i, results)| EngineRunOutcome {
                engine: format!("engine{i}"),
                status: EngineRunStatus::Completed(results),
                duration: Duration::from_millis(1),
                http_duration: None,
            })
            .collect();
        let report = ExecutionReport { outcomes };

        let container = aggregate(report, &EngineWeights::default(), &NoopRecorder);

        let scores: Vec<f64> = container.results.iter().map(score_of).collect();
        prop_assert!(
            scores.windows(2).all(|w| w[0] >= w[1]),
            "main results not ordered by non-increasing score: {scores:?}"
        );

        for result in &container.results {
            prop_assert!(
                is_main_area(result),
                "side-channel variant found in main results: {result:?}"
            );
        }

        let got_answers: HashSet<String> =
            container.answers.iter().map(|a| a.answer.clone()).collect();
        prop_assert_eq!(&got_answers, &answers);

        let got_suggestions: HashSet<String> = container
            .suggestions
            .iter()
            .map(|s| s.suggestion.clone())
            .collect();
        prop_assert_eq!(&got_suggestions, &suggestions);

        let got_corrections: HashSet<String> = container
            .corrections
            .iter()
            .map(|c| c.correction.clone())
            .collect();
        prop_assert_eq!(&got_corrections, &corrections);

        let got_infoboxes: HashSet<(String, Option<String>)> = container
            .infoboxes
            .iter()
            .map(|i| (i.infobox.clone(), i.id.clone()))
            .collect();
        prop_assert_eq!(&got_infoboxes, &infoboxes);

        prop_assert_eq!(container.number_of_results, container.results.len());
    }
}
