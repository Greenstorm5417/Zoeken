//! Property tests for the random answerer.

use proptest::prelude::*;
use rand::SeedableRng;
use rand::rngs::StdRng;
use zoeken_answerers::{Answerer, RandomAnswerer, RandomKind};
use zoeken_query::SearchQuery;

/// All kinds the random answerer supports, paired with their query token.
const KINDS: [(RandomKind, &str); 6] = [
    (RandomKind::String, "string"),
    (RandomKind::Int, "int"),
    (RandomKind::Float, "float"),
    (RandomKind::Sha256, "sha256"),
    (RandomKind::Uuid, "uuid"),
    (RandomKind::Color, "color"),
];

fn query(text: &str) -> SearchQuery {
    SearchQuery {
        query: text.to_string(),
        ..SearchQuery::default()
    }
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(100))]

    // Feature: Property 28: random answerer conforms to requested kind
    // Validates: Requirements 9.3
    //
    // For any requested random kind, the produced value conforms to the format
    // of that kind. Generation is driven by a proptest-seeded RNG so the check
    // covers many distinct random outputs per kind.
    #[test]
    fn generated_value_conforms_to_requested_kind(seed in any::<u64>()) {
        let mut rng = StdRng::seed_from_u64(seed);
        for (kind, _token) in KINDS {
            let value = kind.generate(&mut rng);
            prop_assert!(
                kind.matches_format(&value),
                "{kind:?} produced non-conforming value {value:?}"
            );
        }
    }

    // Feature: Property 28: random answerer conforms to requested kind
    // Validates: Requirements 9.3
    //
    // End-to-end: for every kind, the RandomAnswerer given `random <kind>`
    // produces exactly one answer whose value conforms to that kind's format.
    #[test]
    fn answerer_produces_conforming_value(_seed in any::<u64>()) {
        let answerer = RandomAnswerer::new();
        for (kind, token) in KINDS {
            let answers = answerer.answer(&query(&format!("random {token}")));
            prop_assert_eq!(answers.len(), 1, "expected one answer for kind {:?}", kind);
            prop_assert!(
                kind.matches_format(&answers[0].answer),
                "{kind:?} answerer produced non-conforming value {:?}",
                answers[0].answer
            );
        }
    }
}
