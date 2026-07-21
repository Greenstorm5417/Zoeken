//! Local date/time math answerer: `days until 2026-12-25`, `days until
//! christmas`, `3pm est in cet`.

use chrono::{Datelike, NaiveDate, Utc};
use zoeken_query::SearchQuery;
use zoeken_results::Answer;

use crate::Answerer;

/// `(name, month, day)` — recurring dates people ask about.
const NAMED_DAYS: &[(&str, u32, u32)] = &[
    ("christmas", 12, 25),
    ("christmas eve", 12, 24),
    ("new year", 1, 1),
    ("new years", 1, 1),
    ("new year's", 1, 1),
    ("halloween", 10, 31),
    ("valentine's day", 2, 14),
    ("valentines day", 2, 14),
    ("valentines", 2, 14),
];

/// `(abbreviation, UTC offset in minutes)` — fixed-offset zone table.
const ZONES: &[(&str, i32)] = &[
    ("utc", 0),
    ("gmt", 0),
    ("est", -5 * 60),
    ("edt", -4 * 60),
    ("cst", -6 * 60),
    ("cdt", -5 * 60),
    ("mst", -7 * 60),
    ("mdt", -6 * 60),
    ("pst", -8 * 60),
    ("pdt", -7 * 60),
    ("cet", 60),
    ("cest", 2 * 60),
    ("eet", 2 * 60),
    ("eest", 3 * 60),
    ("bst", 60),
    ("ist", 5 * 60 + 30),
    ("jst", 9 * 60),
    ("kst", 9 * 60),
    ("hkt", 8 * 60),
    ("sgt", 8 * 60),
    ("aest", 10 * 60),
    ("aedt", 11 * 60),
    ("nzst", 12 * 60),
    ("nzdt", 13 * 60),
];

fn zone_offset(raw: &str) -> Option<i32> {
    let needle = raw.trim().to_ascii_lowercase();
    ZONES
        .iter()
        .find(|(name, _)| *name == needle)
        .map(|(_, offset)| *offset)
}

/// The date/time math answerer.
#[derive(Debug, Default)]
pub struct DateTimeAnswerer;

impl DateTimeAnswerer {
    pub fn new() -> Self {
        DateTimeAnswerer
    }
}

impl Answerer for DateTimeAnswerer {
    fn keywords(&self) -> &[&str] {
        &["days", "time"]
    }

    fn unconditional(&self) -> bool {
        true
    }

    fn answer(&self, query: &SearchQuery) -> Vec<Answer> {
        let today = Utc::now().date_naive();
        let text = query.query.trim();
        if let Some(answer) = days_until(text, today) {
            return vec![Answer {
                answer,
                engine: "date math".to_string(),
                ..Answer::default()
            }];
        }
        if let Some(answer) = zone_convert(text) {
            return vec![Answer {
                answer,
                engine: "time zones".to_string(),
                ..Answer::default()
            }];
        }
        Vec::new()
    }
}

/// `days until <date-or-named-day>` relative to `today`.
fn days_until(query: &str, today: NaiveDate) -> Option<String> {
    let lower = query.to_ascii_lowercase();
    let target_text = lower
        .strip_prefix("days until ")
        .or_else(|| lower.strip_prefix("days till "))
        .or_else(|| lower.strip_prefix("how many days until "))
        .or_else(|| lower.strip_prefix("how many days till "))?
        .trim()
        .trim_end_matches('?')
        .trim();

    let target = parse_target_date(target_text, today)?;
    let days = (target - today).num_days();
    Some(match days {
        0 => format!("{target_text} is today ({target})"),
        1 => format!("1 day until {target_text} ({target})"),
        n => format!("{n} days until {target_text} ({target})"),
    })
}

/// An ISO date or a named recurring day (next occurrence on/after `today`).
fn parse_target_date(text: &str, today: NaiveDate) -> Option<NaiveDate> {
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return (date >= today).then_some(date);
    }
    let (_, month, day) = NAMED_DAYS.iter().find(|(name, _, _)| *name == text)?;
    let this_year = NaiveDate::from_ymd_opt(today.year(), *month, *day)?;
    if this_year >= today {
        Some(this_year)
    } else {
        NaiveDate::from_ymd_opt(today.year() + 1, *month, *day)
    }
}

/// `3pm est in cet` / `15:30 utc to pst`.
fn zone_convert(query: &str) -> Option<String> {
    let lower = query.to_ascii_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let [time_raw, from_raw, sep, to_raw] = tokens.as_slice() else {
        return None;
    };
    if !matches!(*sep, "in" | "to") {
        return None;
    }
    let from = zone_offset(from_raw)?;
    let to = zone_offset(to_raw)?;
    let minutes = parse_clock(time_raw)?;

    let converted = (minutes - from + to).rem_euclid(24 * 60);
    let day_shift = (minutes - from + to).div_euclid(24 * 60);
    let shift_note = match day_shift {
        1 => " (next day)",
        -1 => " (previous day)",
        _ => "",
    };
    Some(format!(
        "{} {} = {} {}{}",
        format_clock(minutes),
        from_raw.to_ascii_uppercase(),
        format_clock(converted),
        to_raw.to_ascii_uppercase(),
        shift_note
    ))
}

/// `3pm`, `3:30pm`, `15:00`, `9am` → minutes since midnight.
fn parse_clock(raw: &str) -> Option<i32> {
    let (body, pm_offset) = if let Some(body) = raw.strip_suffix("pm") {
        (body, Some(12 * 60))
    } else if let Some(body) = raw.strip_suffix("am") {
        (body, Some(0))
    } else {
        (raw, None)
    };
    let (hours, minutes) = match body.split_once(':') {
        Some((h, m)) => (h.parse::<i32>().ok()?, m.parse::<i32>().ok()?),
        None => (body.parse::<i32>().ok()?, 0),
    };
    if !(0..60).contains(&minutes) {
        return None;
    }
    match pm_offset {
        Some(offset) => {
            // 12-hour clock: 12am -> 0h, 12pm -> 12h.
            if !(1..=12).contains(&hours) {
                return None;
            }
            let base = if hours == 12 { 0 } else { hours * 60 };
            Some(base + minutes + offset)
        }
        None => (0..24).contains(&hours).then_some(hours * 60 + minutes),
    }
}

fn format_clock(minutes: i32) -> String {
    let hours = minutes / 60;
    let minutes = minutes % 60;
    let (display, suffix) = match hours {
        0 => (12, "AM"),
        1..=11 => (hours, "AM"),
        12 => (12, "PM"),
        _ => (hours - 12, "PM"),
    };
    format!("{display}:{minutes:02} {suffix}")
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

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).unwrap()
    }

    #[test]
    fn days_until_iso_date() {
        let today = day(2026, 7, 20);
        assert_eq!(
            days_until("days until 2026-12-25", today).unwrap(),
            "158 days until 2026-12-25 (2026-12-25)"
        );
        assert_eq!(
            days_until("days until 2026-07-21", today).unwrap(),
            "1 day until 2026-07-21 (2026-07-21)"
        );
        assert_eq!(
            days_until("days until 2026-07-20", today).unwrap(),
            "2026-07-20 is today (2026-07-20)"
        );
        assert!(days_until("days until 2020-01-01", today).is_none(), "past");
    }

    #[test]
    fn days_until_named_days_roll_to_next_year() {
        let today = day(2026, 12, 26);
        assert_eq!(
            days_until("days until christmas", today).unwrap(),
            "364 days until christmas (2027-12-25)"
        );
        let before = day(2026, 12, 20);
        assert_eq!(
            days_until("how many days until christmas?", before).unwrap(),
            "5 days until christmas (2026-12-25)"
        );
    }

    #[test]
    fn zone_conversion_basic() {
        assert_eq!(
            zone_convert("3pm est in cet").unwrap(),
            "3:00 PM EST = 9:00 PM CET"
        );
        assert_eq!(
            zone_convert("15:30 utc to ist").unwrap(),
            "3:30 PM UTC = 9:00 PM IST"
        );
    }

    #[test]
    fn zone_conversion_crossing_midnight_is_flagged() {
        assert_eq!(
            zone_convert("11pm est in cet").unwrap(),
            "11:00 PM EST = 5:00 AM CET (next day)"
        );
        assert_eq!(
            zone_convert("1am cet in pst").unwrap(),
            "1:00 AM CET = 4:00 PM PST (previous day)"
        );
    }

    #[test]
    fn twelve_hour_edges() {
        assert_eq!(parse_clock("12am"), Some(0));
        assert_eq!(parse_clock("12pm"), Some(12 * 60));
        assert_eq!(parse_clock("12:30am"), Some(30));
        assert_eq!(parse_clock("13pm"), None);
        assert_eq!(parse_clock("25:00"), None);
    }

    #[test]
    fn non_matching_queries_do_not_answer() {
        let answerer = DateTimeAnswerer::new();
        assert!(answerer.answer(&query("rust programming")).is_empty());
        assert!(answerer.answer(&query("days until")).is_empty());
        assert!(answerer.answer(&query("3pm xyz in cet")).is_empty());
        assert!(answerer.answer(&query("")).is_empty());
    }

    #[test]
    fn answerer_tags_engines() {
        let answerer = DateTimeAnswerer::new();
        let answers = answerer.answer(&query("3pm est in cet"));
        assert_eq!(answers.len(), 1);
        assert_eq!(answers[0].engine, "time zones");
    }
}
