//! The statistics answerer: min/max/avg/sum/prod/range/median over parsed numbers.

use zoeken_query::SearchQuery;
use zoeken_results::Answer;

use crate::Answerer;

/// A statistical operation the [`StatisticsAnswerer`] can compute.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StatisticsOp {
    /// Minimum of the arguments (`min`).
    Min,
    /// Maximum of the arguments (`max`).
    Max,
    /// Arithmetic mean of the arguments (`avg`).
    Avg,
    /// Sum of the arguments (`sum`).
    Sum,
    /// Product of the arguments (`prod`).
    Prod,
    /// Range `max - min` of the arguments (`range`).
    Range,
    /// Median of the arguments (`median`).
    Median,
}

const OPS: [StatisticsOp; 7] = [
    StatisticsOp::Min,
    StatisticsOp::Max,
    StatisticsOp::Avg,
    StatisticsOp::Sum,
    StatisticsOp::Prod,
    StatisticsOp::Range,
    StatisticsOp::Median,
];

impl StatisticsOp {
    pub fn keyword(self) -> &'static str {
        match self {
            StatisticsOp::Min => "min",
            StatisticsOp::Max => "max",
            StatisticsOp::Avg => "avg",
            StatisticsOp::Sum => "sum",
            StatisticsOp::Prod => "prod",
            StatisticsOp::Range => "range",
            StatisticsOp::Median => "median",
        }
    }

    pub fn from_keyword(token: &str) -> Option<Self> {
        OPS.into_iter().find(|op| op.keyword() == token)
    }

    /// Compute this statistic over `nums`. Returns `None` for empty list.
    pub fn compute(self, nums: &[f64]) -> Option<f64> {
        if nums.is_empty() {
            return None;
        }
        let value = match self {
            StatisticsOp::Min => nums.iter().copied().fold(f64::INFINITY, f64::min),
            StatisticsOp::Max => nums.iter().copied().fold(f64::NEG_INFINITY, f64::max),
            StatisticsOp::Avg => nums.iter().sum::<f64>() / nums.len() as f64,
            StatisticsOp::Sum => nums.iter().sum::<f64>(),
            StatisticsOp::Prod => nums.iter().product::<f64>(),
            StatisticsOp::Range => {
                let min = nums.iter().copied().fold(f64::INFINITY, f64::min);
                let max = nums.iter().copied().fold(f64::NEG_INFINITY, f64::max);
                max - min
            }
            StatisticsOp::Median => median(nums),
        };
        Some(value)
    }
}

fn median(nums: &[f64]) -> f64 {
    let mut sorted = nums.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let n = sorted.len();
    if n % 2 == 1 {
        sorted[n / 2]
    } else {
        (sorted[n / 2 - 1] + sorted[n / 2]) / 2.0
    }
}

#[derive(Debug, Clone)]
pub struct StatisticsAnswerer {
    keywords: Vec<&'static str>,
}

impl StatisticsAnswerer {
    pub fn new() -> Self {
        StatisticsAnswerer {
            keywords: OPS.iter().map(|op| op.keyword()).collect(),
        }
    }
}

impl Default for StatisticsAnswerer {
    fn default() -> Self {
        StatisticsAnswerer::new()
    }
}

fn parse_args(query: &str) -> Option<Vec<f64>> {
    let mut parts = query.split_whitespace();
    parts.next()?; // skip the operation keyword
    let mut nums = Vec::new();
    for part in parts {
        let value: f64 = part.parse().ok()?;
        if !value.is_finite() {
            return None;
        }
        nums.push(value);
    }
    if nums.is_empty() { None } else { Some(nums) }
}

fn format_number(value: f64) -> String {
    if value.fract() == 0.0 && value.abs() < 1e15 {
        format!("{}", value as i64)
    } else {
        // Trim to a compact representation without trailing zeros noise.
        let s = format!("{value}");
        s
    }
}

impl Answerer for StatisticsAnswerer {
    fn keywords(&self) -> &[&str] {
        &self.keywords
    }

    fn answer(&self, query: &SearchQuery) -> Vec<Answer> {
        let text = &query.query;
        let Some(token) = text.split_whitespace().next() else {
            return Vec::new();
        };
        let Some(op) = StatisticsOp::from_keyword(token) else {
            return Vec::new();
        };
        let Some(nums) = parse_args(text) else {
            return Vec::new();
        };
        let Some(result) = op.compute(&nums) else {
            return Vec::new();
        };

        let args = nums
            .iter()
            .map(|n| format_number(*n))
            .collect::<Vec<_>>()
            .join(", ");
        vec![Answer {
            answer: format!("{}({}) = {}", op.keyword(), args, format_number(result)),
            ..Answer::default()
        }]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(text: &str) -> SearchQuery {
        SearchQuery {
            query: text.to_string(),
            ..SearchQuery::default()
        }
    }

    #[test]
    fn from_keyword_resolves_all_ops() {
        for op in OPS {
            assert_eq!(StatisticsOp::from_keyword(op.keyword()), Some(op));
        }
        assert_eq!(StatisticsOp::from_keyword("nope"), None);
    }

    #[test]
    fn compute_matches_expected_values() {
        let nums = [1.0, 2.0, 3.0, 4.0];
        assert_eq!(StatisticsOp::Min.compute(&nums), Some(1.0));
        assert_eq!(StatisticsOp::Max.compute(&nums), Some(4.0));
        assert_eq!(StatisticsOp::Sum.compute(&nums), Some(10.0));
        assert_eq!(StatisticsOp::Avg.compute(&nums), Some(2.5));
        assert_eq!(StatisticsOp::Prod.compute(&nums), Some(24.0));
        assert_eq!(StatisticsOp::Range.compute(&nums), Some(3.0));
        // Even count median: mean of 2 and 3.
        assert_eq!(StatisticsOp::Median.compute(&nums), Some(2.5));
    }

    #[test]
    fn median_odd_count_is_middle_element() {
        assert_eq!(StatisticsOp::Median.compute(&[3.0, 1.0, 2.0]), Some(2.0));
    }

    #[test]
    fn compute_over_empty_is_none() {
        assert_eq!(StatisticsOp::Sum.compute(&[]), None);
    }

    #[test]
    fn answer_computes_sum() {
        let answers = StatisticsAnswerer::new().answer(&query("sum 1 2 3"));
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].answer, "sum(1, 2, 3) = 6");
    }

    #[test]
    fn answer_computes_avg_with_decimals() {
        let answers = StatisticsAnswerer::new().answer(&query("avg 1 2"));
        assert_eq!(answers[0].answer, "avg(1, 2) = 1.5");
    }

    #[test]
    fn non_numeric_argument_yields_no_answer() {
        assert!(
            StatisticsAnswerer::new()
                .answer(&query("sum 1 two 3"))
                .is_empty()
        );
    }

    #[test]
    fn keyword_without_numbers_yields_no_answer() {
        assert!(StatisticsAnswerer::new().answer(&query("sum")).is_empty());
    }

    #[test]
    fn unrelated_keyword_yields_no_answer() {
        assert!(
            StatisticsAnswerer::new()
                .answer(&query("hello 1 2 3"))
                .is_empty()
        );
    }
}
