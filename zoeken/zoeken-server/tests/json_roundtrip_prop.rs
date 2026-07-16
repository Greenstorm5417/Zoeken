use proptest::prelude::*;
use zoeken_results::{
    Answer, Code, Correction, FileResult, Image, Infobox, InfoboxAttribute, InfoboxImage,
    InfoboxUrl, KeyValue, MainResult, Paper, Result_, Suggestion, Template,
};
use zoeken_search::{ResultContainer, UnresponsiveCause, UnresponsiveEngine};
use zoeken_server::serialize::{JsonResponse, format_json};

fn score() -> impl Strategy<Value = f64> {
    -1.0e12f64..1.0e12f64
}

fn positions() -> impl Strategy<Value = Vec<usize>> {
    prop::collection::vec(0usize..1000, 0..5)
}

fn engines() -> impl Strategy<Value = Vec<String>> {
    prop::collection::vec(".*", 0..4)
}

fn opt_text() -> impl Strategy<Value = Option<String>> {
    prop_oneof![Just(None), ".*".prop_map(Some)]
}

fn template() -> impl Strategy<Value = Template> {
    prop_oneof![
        Just(Template::Default),
        Just(Template::Answer),
        Just(Template::Images),
        Just(Template::Videos),
        Just(Template::Paper),
        Just(Template::Code),
        Just(Template::File),
        Just(Template::KeyValue),
        Just(Template::Infobox),
        Just(Template::Suggestion),
        Just(Template::Correction),
    ]
}

fn main_result() -> impl Strategy<Value = MainResult> {
    (
        ".*",
        ".*",
        ".*",
        ".*",
        ".*",
        engines(),
        score(),
        positions(),
        template(),
        ".*",
        ".*",
    )
        .prop_map(
            |(
                url,
                normalized_url,
                title,
                content,
                engine,
                engines,
                score,
                positions,
                template,
                thumbnail,
                iframe_src,
            )| {
                MainResult {
                    url,
                    normalized_url,
                    title,
                    content,
                    engine,
                    engines,
                    score,
                    positions,
                    priority: String::new(),
                    template,
                    thumbnail,
                    iframe_src,
                }
            },
        )
}

fn image() -> impl Strategy<Value = Image> {
    (
        ".*",
        ".*",
        ".*",
        ".*",
        ".*",
        ".*",
        ".*",
        ".*",
        score(),
        positions(),
    )
        .prop_map(
            |(
                url,
                normalized_url,
                title,
                content,
                engine,
                img_src,
                thumbnail_src,
                resolution,
                score,
                positions,
            )| Image {
                url,
                normalized_url,
                title,
                content,
                engine,
                img_src,
                thumbnail_src,
                resolution,
                score,
                positions,
                ..Image::default()
            },
        )
}

fn paper() -> impl Strategy<Value = Paper> {
    (
        (".*", ".*", ".*", ".*", ".*"),
        (
            prop::collection::vec(".*", 0..3),
            ".*",
            ".*",
            opt_text(),
            score(),
            positions(),
        ),
    )
        .prop_map(
            |(
                (url, normalized_url, title, content, engine),
                (authors, doi, journal, published_date, score, positions),
            )| Paper {
                url,
                normalized_url,
                title,
                content,
                engine,
                authors,
                doi,
                journal,
                published_date,
                score,
                positions,
                ..Paper::default()
            },
        )
}

fn codelines() -> impl Strategy<Value = Vec<(usize, String)>> {
    prop::collection::vec((0usize..1000, ".*".prop_map(|s| s)), 0..4)
}

fn code() -> impl Strategy<Value = Code> {
    (
        ".*",
        ".*",
        ".*",
        ".*",
        ".*",
        opt_text(),
        codelines(),
        score(),
        positions(),
    )
        .prop_map(
            |(
                url,
                normalized_url,
                title,
                content,
                engine,
                repository,
                codelines,
                score,
                positions,
            )| {
                Code {
                    url,
                    normalized_url,
                    title,
                    content,
                    engine,
                    repository,
                    codelines,
                    score,
                    positions,
                    ..Code::default()
                }
            },
        )
}

fn file_result() -> impl Strategy<Value = FileResult> {
    (".*", ".*", ".*", ".*", ".*", ".*", score(), positions()).prop_map(
        |(url, normalized_url, title, content, engine, filename, score, positions)| FileResult {
            url,
            normalized_url,
            title,
            content,
            engine,
            filename,
            score,
            positions,
            ..FileResult::default()
        },
    )
}

fn key_value() -> impl Strategy<Value = KeyValue> {
    (
        ".*",
        ".*",
        ".*",
        ".*",
        ".*",
        prop::collection::vec((".*", ".*"), 0..4),
        score(),
        positions(),
    )
        .prop_map(
            |(url, normalized_url, title, content, engine, kvmap, score, positions)| KeyValue {
                url,
                normalized_url,
                title,
                content,
                engine,
                kvmap,
                score,
                positions,
                ..KeyValue::default()
            },
        )
}

/// A result displayed in the main results area, across its typed variants.
fn main_area_result() -> impl Strategy<Value = Result_> {
    prop_oneof![
        main_result().prop_map(Result_::Main),
        image().prop_map(Result_::Image),
        paper().prop_map(Result_::Paper),
        code().prop_map(Result_::Code),
        file_result().prop_map(Result_::File),
        key_value().prop_map(Result_::KeyValue),
    ]
}

fn answer() -> impl Strategy<Value = Answer> {
    (".*", opt_text(), ".*", template()).prop_map(|(answer, url, engine, template)| Answer {
        answer,
        url,
        engine,
        template,
    })
}

fn suggestion() -> impl Strategy<Value = Suggestion> {
    (".*", ".*").prop_map(|(suggestion, engine)| Suggestion { suggestion, engine })
}

fn correction() -> impl Strategy<Value = Correction> {
    (".*", opt_text(), ".*").prop_map(|(correction, url, engine)| Correction {
        correction,
        url,
        engine,
    })
}

fn infobox_url() -> impl Strategy<Value = InfoboxUrl> {
    (".*", ".*").prop_map(|(title, url)| InfoboxUrl { title, url })
}

fn infobox_image() -> impl Strategy<Value = InfoboxImage> {
    (".*", ".*").prop_map(|(src, alt)| InfoboxImage { src, alt })
}

fn infobox_attribute() -> impl Strategy<Value = InfoboxAttribute> {
    (
        ".*",
        ".*",
        prop_oneof![Just(None), infobox_image().prop_map(Some)],
    )
        .prop_map(|(label, value, image)| InfoboxAttribute {
            label,
            value,
            image,
        })
}

fn infobox() -> impl Strategy<Value = Infobox> {
    (
        ".*",
        opt_text(),
        ".*",
        opt_text(),
        prop::collection::vec(infobox_url(), 0..3),
        prop::collection::vec(infobox_attribute(), 0..3),
        prop::collection::vec(".*", 0..3),
        ".*",
    )
        .prop_map(
            |(infobox, id, content, img_src, urls, attributes, related_topics, engine)| Infobox {
                infobox,
                id,
                content,
                img_src,
                urls,
                attributes,
                related_topics,
                engine,
            },
        )
}

fn unresponsive_cause() -> impl Strategy<Value = UnresponsiveCause> {
    prop_oneof![
        ".*".prop_map(UnresponsiveCause::Error),
        Just(UnresponsiveCause::Timeout),
        Just(UnresponsiveCause::DeadlineExceeded),
    ]
}

fn unresponsive_engine() -> impl Strategy<Value = UnresponsiveEngine> {
    (".*", unresponsive_cause()).prop_map(|(engine, cause)| UnresponsiveEngine { engine, cause })
}

fn result_container() -> impl Strategy<Value = ResultContainer> {
    (
        prop::collection::vec(main_area_result(), 0..5),
        prop::collection::vec(answer(), 0..3),
        prop::collection::vec(suggestion(), 0..3),
        prop::collection::vec(correction(), 0..3),
        prop::collection::vec(infobox(), 0..3),
        prop::collection::vec(unresponsive_engine(), 0..4),
        0usize..100_000,
    )
        .prop_map(
            |(
                results,
                answers,
                suggestions,
                corrections,
                infoboxes,
                unresponsive_engines,
                number_of_results,
            )| ResultContainer {
                results,
                answers,
                suggestions,
                corrections,
                infoboxes,
                unresponsive_engines,
                number_of_results,
                engine_data: std::collections::HashMap::new(),
            },
        )
}

proptest! {
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn json_serialization_round_trip(container in result_container()) {
        let json = format_json(&container);
        let parsed: JsonResponse = serde_json::from_str(&json)
            .expect("format_json output must parse back as a JsonResponse");
        prop_assert_eq!(parsed.results.len(), container.results.len());
        prop_assert_eq!(parsed.answers.len(), container.answers.len());
        prop_assert_eq!(parsed.suggestions.len(), container.suggestions.len());
        prop_assert_eq!(parsed.corrections.len(), container.corrections.len());
        prop_assert_eq!(parsed.infoboxes.len(), container.infoboxes.len());
        prop_assert_eq!(parsed.unresponsive_engines.len(), container.unresponsive_engines.len());
    }
}
