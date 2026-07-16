use proptest::prelude::*;
use zoeken_results::{Answer, MainResult, Result_, Suggestion};
use zoeken_search::ResultContainer;
use zoeken_server::serialize::format_csv;

const EXPECTED_HEADER: [&str; 7] = ["title", "url", "content", "host", "engine", "score", "type"];

fn arb_text() -> impl Strategy<Value = String> {
    prop_oneof![
        Just(String::new()),
        "[a-zA-Z0-9 _.:/@-]{0,24}",
        ".{0,24}",
        Just(",".to_string()),
        Just("comma, then more".to_string()),
        Just("has \"double\" quotes".to_string()),
        Just("embedded\nnewline".to_string()),
        Just("carriage\r\nreturn".to_string()),
        Just("trailing space ".to_string()),
        Just(" leading space".to_string()),
        Just("\"quoted, comma\nand newline\"".to_string()),
        Just("café ☕ 日本語 — Ω".to_string()),
    ]
}

fn arb_score() -> impl Strategy<Value = f64> {
    proptest::num::f64::ANY.prop_filter("finite scores only", |x| x.is_finite())
}

fn arb_main_result() -> impl Strategy<Value = MainResult> {
    (arb_text(), arb_text(), arb_text(), arb_text(), arb_score()).prop_map(
        |(title, url, content, engine, score)| MainResult {
            url,
            title,
            content,
            engine,
            score,
            ..MainResult::default()
        },
    )
}

fn arb_container() -> impl Strategy<Value = ResultContainer> {
    (
        prop::collection::vec(arb_main_result().prop_map(Result_::Main), 0..10),
        prop::collection::vec(
            arb_text().prop_map(|answer| Answer {
                answer,
                ..Answer::default()
            }),
            0..3,
        ),
        prop::collection::vec(
            arb_text().prop_map(|suggestion| Suggestion {
                suggestion,
                ..Suggestion::default()
            }),
            0..3,
        ),
    )
        .prop_map(|(results, answers, suggestions)| ResultContainer {
            results,
            answers,
            suggestions,
            ..ResultContainer::default()
        })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    #[test]
    fn csv_round_trips_main_results(container in arb_container()) {
        let body = format_csv(&container);

        let mains: Vec<&MainResult> = container
            .results
            .iter()
            .filter_map(|result| match result {
                Result_::Main(main) => Some(main),
                _ => None,
            })
            .collect();

        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(body.as_bytes());

        let header = reader.headers().expect("readable header").clone();
        let header_fields: Vec<&str> = header.iter().collect();
        prop_assert_eq!(header_fields.as_slice(), EXPECTED_HEADER.as_slice());

        let records: Vec<csv::StringRecord> = reader
            .records()
            .map(|record| record.expect("well-formed csv record"))
            .collect();

        prop_assert_eq!(records.len(), mains.len() + container.answers.len() + container.suggestions.len());

        for (record, main) in records.iter().zip(mains.iter()) {
            prop_assert_eq!(record.len(), 7);

            prop_assert_eq!(&record[0], main.title.as_str());
            prop_assert_eq!(&record[1], main.url.as_str());
            prop_assert_eq!(&record[2], main.content.as_str());
            prop_assert_eq!(&record[4], main.engine.as_str());
            let score: f64 = record[5].parse().expect("score field parses as f64");
            prop_assert_eq!(score, main.score);
            prop_assert_eq!(&record[6], "result");
        }

        for (record, answer) in records.iter().skip(mains.len()).zip(container.answers.iter()) {
            prop_assert_eq!(&record[0], answer.answer.as_str());
            prop_assert_eq!(&record[6], "answer");
        }

        for (record, suggestion) in records
            .iter()
            .skip(mains.len() + container.answers.len())
            .zip(container.suggestions.iter())
        {
            prop_assert_eq!(&record[0], suggestion.suggestion.as_str());
            prop_assert_eq!(&record[6], "suggestion");
        }
    }

    #[test]
    fn csv_is_header_only_without_main_results(
        _unit in Just(()),
    ) {
        let container = ResultContainer {
            ..ResultContainer::default()
        };

        let body = format_csv(&container);
        let mut reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .from_reader(body.as_bytes());

        let header = reader.headers().expect("readable header").clone();
        let header_fields: Vec<&str> = header.iter().collect();
        prop_assert_eq!(header_fields.as_slice(), EXPECTED_HEADER.as_slice());

        let record_count = reader.records().count();
        prop_assert_eq!(record_count, 0);
    }
}
