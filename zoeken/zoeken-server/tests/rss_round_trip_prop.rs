use proptest::prelude::*;
use zoeken_results::{
    Answer, Code, Correction, FileResult, Image, Infobox, KeyValue, MainResult, Paper, Result_,
    Suggestion,
};
use zoeken_search::ResultContainer;
use zoeken_server::serialize::{RssFeed, format_rss};

#[derive(Debug, Clone, PartialEq, Eq)]
struct ExpectedItem {
    title: String,
    link: String,
    description: String,
    author: String,
}

fn expected_items(container: &ResultContainer) -> Vec<ExpectedItem> {
    container
        .results
        .iter()
        .filter_map(|result| match result {
            Result_::Main(main) => Some(ExpectedItem {
                title: main.title.clone(),
                link: main.url.clone(),
                description: main.content.clone(),
                author: main.engine.clone(),
            }),
            _ => None,
        })
        .collect()
}

fn text_fragment() -> impl Strategy<Value = String> {
    prop_oneof![
        "[a-zA-Z0-9][a-zA-Z0-9 ]{0,6}[a-zA-Z0-9]",
        "[a-zA-Z0-9]",
        Just("<".to_string()),
        Just(">".to_string()),
        Just("&".to_string()),
        Just("\"".to_string()),
        Just("'".to_string()),
        Just("<tag attr=\"v\">".to_string()),
        Just("a & b < c > d".to_string()),
        Just("é".to_string()),
        Just("naïve café".to_string()),
        Just("日本語".to_string()),
        Just("Ω≈ç√".to_string()),
        Just("🦀🔎".to_string()),
    ]
}

fn text() -> impl Strategy<Value = String> {
    prop::collection::vec(text_fragment(), 0..4).prop_map(|parts| parts.concat())
}

fn main_result() -> impl Strategy<Value = MainResult> {
    (text(), text(), text(), text()).prop_map(|(title, url, content, engine)| MainResult {
        title,
        url,
        content,
        engine,
        ..MainResult::default()
    })
}

fn non_main_result() -> impl Strategy<Value = Result_> {
    prop_oneof![
        text().prop_map(|url| Result_::Image(Image {
            url,
            ..Image::default()
        })),
        text().prop_map(|url| Result_::Paper(Paper {
            url,
            ..Paper::default()
        })),
        text().prop_map(|url| Result_::Code(Code {
            url,
            ..Code::default()
        })),
        text().prop_map(|url| Result_::File(FileResult {
            url,
            ..FileResult::default()
        })),
        text().prop_map(|title| Result_::KeyValue(KeyValue {
            title,
            ..KeyValue::default()
        })),
        text().prop_map(|answer| Result_::Answer(Answer {
            answer,
            ..Answer::default()
        })),
        text().prop_map(|suggestion| Result_::Suggestion(Suggestion {
            suggestion,
            ..Suggestion::default()
        })),
        text().prop_map(|correction| Result_::Correction(Correction {
            correction,
            ..Correction::default()
        })),
        text().prop_map(|infobox| Result_::Infobox(Infobox {
            infobox,
            ..Infobox::default()
        })),
    ]
}

fn any_result() -> impl Strategy<Value = Result_> {
    prop_oneof![
        3 => main_result().prop_map(Result_::Main),
        2 => non_main_result(),
    ]
}

fn result_container() -> impl Strategy<Value = ResultContainer> {
    (
        prop::collection::vec(any_result(), 0..6),
        prop::collection::vec(
            text().prop_map(|answer| Answer {
                answer,
                ..Answer::default()
            }),
            0..3,
        ),
        prop::collection::vec(
            text().prop_map(|suggestion| Suggestion {
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
    #![proptest_config(ProptestConfig { cases: 256, ..ProptestConfig::default() })]

    #[test]
    fn rss_round_trip(container in result_container()) {
        let body = format_rss(&container);

        let feed: RssFeed = quick_xml::de::from_str(&body)
            .expect("format_rss output must parse back as an RssFeed");

        prop_assert_eq!(&feed.version, "2.0");

        let expected = expected_items(&container);
        prop_assert_eq!(feed.channel.items.len(), expected.len());
        for (item, want) in feed.channel.items.iter().zip(expected.iter()) {
            prop_assert_eq!(&item.title, &want.title);
            prop_assert_eq!(&item.link, &want.link);
            prop_assert_eq!(&item.description, &want.description);
            prop_assert_eq!(&item.author, &want.author);
        }
    }
}
