use proptest::prelude::*;
use zoeken_results::*;

fn tok() -> impl Strategy<Value = String> {
    "[a-zA-Z0-9]{1,10}".prop_map(|s| s)
}

fn incomplete_main() -> impl Strategy<Value = Result_> {
    (tok(), tok()).prop_map(|(title, content)| {
        Result_::Main(MainResult {
            url: String::new(),
            title,
            content,
            ..Default::default()
        })
    })
}

fn incomplete_answer() -> impl Strategy<Value = Result_> {
    tok().prop_map(|_engine| {
        Result_::Answer(Answer {
            answer: String::new(),
            ..Default::default()
        })
    })
}

fn incomplete_image() -> impl Strategy<Value = Result_> {
    (tok(), tok(), tok(), tok(), 0usize..4).prop_map(|(url, img, thumb, res, which)| {
        let mut r = Image {
            url,
            img_src: img,
            thumbnail_src: thumb,
            resolution: res,
            ..Default::default()
        };
        match which {
            0 => r.url = String::new(),
            1 => r.img_src = String::new(),
            2 => r.thumbnail_src = String::new(),
            _ => r.resolution = String::new(),
        }
        Result_::Image(r)
    })
}

fn incomplete_paper() -> impl Strategy<Value = Result_> {
    (tok(), tok(), tok(), tok(), 0usize..5).prop_map(|(url, doi, journal, author, which)| {
        let mut r = Paper {
            url,
            authors: vec![author],
            doi,
            journal,
            published_date: Some("2020-01-01".to_string()),
            ..Default::default()
        };
        match which {
            0 => r.url = String::new(),
            1 => r.authors = Vec::new(),
            2 => r.doi = String::new(),
            3 => r.journal = String::new(),
            _ => r.published_date = None,
        }
        Result_::Paper(r)
    })
}

fn incomplete_code() -> impl Strategy<Value = Result_> {
    (tok(), tok(), tok(), 0usize..3).prop_map(|(url, repo, codeline, which)| {
        let mut r = Code {
            url,
            repository: Some(repo),
            codelines: vec![(1usize, codeline)],
            ..Default::default()
        };
        match which {
            0 => r.url = String::new(),
            1 => r.repository = None,
            _ => r.codelines = Vec::new(),
        }
        Result_::Code(r)
    })
}

fn incomplete_file() -> impl Strategy<Value = Result_> {
    (tok(), tok(), 0usize..2).prop_map(|(url, filename, which)| {
        let mut r = FileResult {
            url,
            filename,
            ..Default::default()
        };
        if which == 0 {
            r.url = String::new();
        } else {
            r.filename = String::new();
        }
        Result_::File(r)
    })
}

fn incomplete_keyvalue() -> impl Strategy<Value = Result_> {
    Just(Result_::KeyValue(KeyValue {
        kvmap: Vec::new(),
        ..Default::default()
    }))
}

fn incomplete_suggestion() -> impl Strategy<Value = Result_> {
    Just(Result_::Suggestion(Suggestion {
        suggestion: String::new(),
        ..Default::default()
    }))
}

fn incomplete_correction() -> impl Strategy<Value = Result_> {
    Just(Result_::Correction(Correction {
        correction: String::new(),
        ..Default::default()
    }))
}

fn incomplete_infobox() -> impl Strategy<Value = Result_> {
    Just(Result_::Infobox(Infobox {
        infobox: String::new(),
        ..Default::default()
    }))
}

fn arb_incomplete_result() -> impl Strategy<Value = Result_> {
    prop_oneof![
        incomplete_main(),
        incomplete_answer(),
        incomplete_image(),
        incomplete_paper(),
        incomplete_code(),
        incomplete_file(),
        incomplete_keyvalue(),
        incomplete_suggestion(),
        incomplete_correction(),
        incomplete_infobox(),
    ]
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn incomplete_results_are_rejected(result in arb_incomplete_result()) {
        let outcome = validate(&result);
        prop_assert!(
            outcome.is_err(),
            "expected an incomplete result to be rejected, but it was accepted: {result:?}"
        );
        match outcome {
            Err(ResultError::MissingField { .. }) => {}
            other => prop_assert!(
                false,
                "expected a MissingField validation error, got {other:?} for {result:?}"
            ),
        }
    }
}
