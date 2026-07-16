//! Property tests for the statistics answerer.

use proptest::prelude::*;
use zoeken_answerers::{Answerer, AnswererRegistry, StatisticsAnswerer, StatisticsOp};
use zoeken_query::SearchQuery;

/// Tolerance for approximate float comparisons that accumulate rounding
/// (`sum`, `avg`, `median`).
const EPS: f64 = 1e-6;

/// Assert two floats are equal within an absolute-or-relative tolerance.
fn approx_eq(a: f64, b: f64) -> bool {
    let diff = (a - b).abs();
    diff <= EPS || diff <= EPS * a.abs().max(b.abs())
}

fn ref_min(nums: &[f64]) -> f64 {
    let mut m = nums[0];
    for &x in &nums[1..] {
        if x < m {
            m = x;
        }
    }
    m
}

fn ref_max(nums: &[f64]) -> f64 {
    let mut m = nums[0];
    for &x in &nums[1..] {
        if x > m {
            m = x;
        }
    }
    m
}

fn ref_sum(nums: &[f64]) -> f64 {
    let mut acc = 0.0;
    for &x in nums {
        acc += x;
    }
    acc
}

fn ref_avg(nums: &[f64]) -> f64 {
    ref_sum(nums) / nums.len() as f64
}

fn ref_median(nums: &[f64]) -> f64 {
    let mut sorted = nums.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

fn numbers() -> impl Strategy<Value = Vec<f64>> {
    prop::collection::vec(-1_000_000.0f64..1_000_000.0, 1..30)
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(200))]

    #[test]
    fn statistics_answerer_matches_reference(nums in numbers()) {
        let min = StatisticsOp::Min.compute(&nums).unwrap();
        let max = StatisticsOp::Max.compute(&nums).unwrap();
        let sum = StatisticsOp::Sum.compute(&nums).unwrap();
        let avg = StatisticsOp::Avg.compute(&nums).unwrap();
        let median = StatisticsOp::Median.compute(&nums).unwrap();

        prop_assert_eq!(min, ref_min(&nums));
        prop_assert_eq!(max, ref_max(&nums));
        prop_assert!(
            approx_eq(sum, ref_sum(&nums)),
            "sum {} vs reference {}", sum, ref_sum(&nums)
        );
        prop_assert!(
            approx_eq(avg, ref_avg(&nums)),
            "avg {} vs reference {}", avg, ref_avg(&nums)
        );
        prop_assert!(
            approx_eq(median, ref_median(&nums)),
            "median {} vs reference {}", median, ref_median(&nums)
        );
    }

    #[test]
    fn answerer_end_to_end_sum_matches_reference(nums in numbers()) {
        let ints: Vec<i64> = nums.iter().map(|n| *n as i64).collect();
        let query_text = format!(
            "sum {}",
            ints.iter().map(|n| n.to_string()).collect::<Vec<_>>().join(" ")
        );
        let query = SearchQuery {
            query: query_text,
            ..SearchQuery::default()
        };

        let answers = StatisticsAnswerer::new().answer(&query);
        prop_assert_eq!(answers.len(), 1);

        let floats: Vec<f64> = ints.iter().map(|n| *n as f64).collect();
        let expected = ref_sum(&floats);
        // The answer text ends with `= <result>`; compare the parsed result.
        let text = &answers[0].answer;
        let reported: f64 = text
            .rsplit('=')
            .next()
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        prop_assert!(
            approx_eq(reported, expected),
            "reported {} vs reference {} (query answer: {})", reported, expected, text
        );
        let registry = AnswererRegistry::with_builtins();
        let reg_answers = registry.ask(&query);
        prop_assert_eq!(reg_answers.len(), 1);
        prop_assert_eq!(&reg_answers[0].answer, text);
    }
}
