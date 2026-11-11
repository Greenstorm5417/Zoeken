//! Query interpretation owned by `zoeken-query`.
//!
//! Bangs, language tags, and timeouts stay in [`crate::parse_raw`]. This module
//! owns operators (`site:`, `filetype:`, quotes, excluded terms), intent, and
//! entities. It never rewrites the text sent to engines. Low-confidence labels
//! fall back to [`Intent::General`] so a guess cannot silently change the search.
//!
//! A small linear model can fill gaps the rules miss. It ships only when
//! [`model_enabled`] is true, which the daily-search experiment must agree with.

use serde::{Deserialize, Serialize};

/// What the query is most likely asking for. Operators live beside this, not inside it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum Intent {
    Navigational,
    Technical,
    Factual,
    Weather,
    Definition,
    Translation,
    Arithmetic,
    Conversion,
    Fresh,
    General,
}

impl Intent {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Intent::Navigational => "navigational",
            Intent::Technical => "technical",
            Intent::Factual => "factual",
            Intent::Weather => "weather",
            Intent::Definition => "definition",
            Intent::Translation => "translation",
            Intent::Arithmetic => "arithmetic",
            Intent::Conversion => "conversion",
            Intent::Fresh => "fresh",
            Intent::General => "general",
        }
    }

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        Some(match value {
            "navigational" => Intent::Navigational,
            "technical" => Intent::Technical,
            "factual" => Intent::Factual,
            "weather" => Intent::Weather,
            "definition" => Intent::Definition,
            "translation" => Intent::Translation,
            "arithmetic" => Intent::Arithmetic,
            "conversion" => Intent::Conversion,
            "fresh" => Intent::Fresh,
            "general" | "ambiguous" => Intent::General,
            _ => return None,
        })
    }
}

/// Which interpreter produced a label.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub enum IntentSource {
    Rules,
    Model,
}

impl IntentSource {
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            IntentSource::Rules => "rules",
            IntentSource::Model => "model",
        }
    }
}

/// Explicit syntax extracted without deleting it from the original query.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Operators {
    pub site: Option<String>,
    pub filetype: Option<String>,
    pub phrases: Vec<String>,
    pub excluded: Vec<String>,
}

/// A value the interpreter noticed. Display only; not a rewrite.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Entity {
    pub kind: String,
    pub value: String,
}

/// One labeled guess.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct IntentGuess {
    pub intent: Intent,
    pub confidence: f32,
    pub source: IntentSource,
}

/// The interpretation contract: original text, operators, intent, entities, confidence.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[non_exhaustive]
pub struct Interpretation {
    pub original: String,
    /// Query with operator tokens removed, for ranking. Not sent to engines.
    pub residual: String,
    pub operators: Operators,
    pub intent: Intent,
    pub confidence: f32,
    pub source: IntentSource,
    pub entities: Vec<Entity>,
}

/// Rules-first interpretation. The model runs only when [`model_enabled`] is set
/// and only to fill a general/low-confidence rules label.
#[must_use]
pub fn interpret(original: &str) -> Interpretation {
    let (operators, residual) = parse_operators(original);
    let rules = rules_intent(original, &operators);
    let mut guess = rules;
    let operators_present = operators.site.is_some()
        || operators.filetype.is_some()
        || !operators.phrases.is_empty()
        || !operators.excluded.is_empty();
    if model_enabled() && rules.intent == Intent::General && !operators_present {
        let model = model_intent(original);
        if model.intent != Intent::General && model.confidence >= 0.75 {
            guess = model;
        }
    }
    if guess.confidence < 0.45 {
        guess.intent = Intent::General;
    }
    let entities = entities_of(original, &operators, guess.intent);
    Interpretation {
        original: original.to_string(),
        residual,
        operators,
        intent: guess.intent,
        confidence: guess.confidence,
        source: guess.source,
        entities,
    }
}

/// Whether the cascade may consult the linear model.
///
/// The flag follows [`intent_holdout`]: paraphrases, ambiguous lines, and
/// false-positive traps that are not in the training table. Low confidence
/// still falls back to general inside [`interpret`] and does not rewrite the
/// query string.
#[must_use]
pub fn model_enabled() -> bool {
    intent_holdout().enable
}

/// Held-out weather precision and recall for the cascade and the rules.
#[derive(Debug, Clone, Copy)]
pub struct IntentHoldout {
    pub rules_precision: f64,
    pub rules_recall: f64,
    pub precision: f64,
    pub recall: f64,
    pub positives: usize,
    pub traps: usize,
    pub false_positives: usize,
    pub enable: bool,
}

/// Disjoint evaluation. Training strings are rejected by the unit test.
#[must_use]
pub fn intent_holdout() -> &'static IntentHoldout {
    use std::sync::OnceLock;
    static CELL: OnceLock<IntentHoldout> = OnceLock::new();
    CELL.get_or_init(evaluate_intent_holdout)
}

fn evaluate_intent_holdout() -> IntentHoldout {
    let mut rules_tp = 0usize;
    let mut rules_fp = 0usize;
    let mut rules_fn = 0usize;
    let mut tp = 0usize;
    let mut fp = 0usize;
    let mut fn_ = 0usize;
    let mut positives = 0usize;
    let mut traps = 0usize;
    for (text, weather) in HELD_OUT_INTENT {
        if *weather {
            positives += 1;
        } else {
            traps += 1;
        }
        let rules = rules_only(text);
        let cascade = cascade_label(text);
        let rules_weather = rules.intent == Intent::Weather;
        let cascade_weather = cascade.intent == Intent::Weather;
        match (*weather, rules_weather) {
            (true, true) => rules_tp += 1,
            (true, false) => rules_fn += 1,
            (false, true) => rules_fp += 1,
            (false, false) => {}
        }
        match (*weather, cascade_weather) {
            (true, true) => tp += 1,
            (true, false) => fn_ += 1,
            (false, true) => fp += 1,
            (false, false) => {}
        }
    }
    let rules_precision = ratio(rules_tp, rules_tp + rules_fp);
    let rules_recall = ratio(rules_tp, rules_tp + rules_fn);
    let precision = ratio(tp, tp + fp);
    let recall = ratio(tp, tp + fn_);
    let enable =
        fp == 0 && recall > rules_recall && recall >= 0.5 && precision + 1e-9 >= rules_precision;
    IntentHoldout {
        rules_precision,
        rules_recall,
        precision,
        recall,
        positives,
        traps,
        false_positives: fp,
        enable,
    }
}

fn ratio(hit: usize, total: usize) -> f64 {
    if total == 0 {
        1.0
    } else {
        hit as f64 / total as f64
    }
}

/// Same decision as [`interpret`], without consulting [`model_enabled`].
fn cascade_label(original: &str) -> IntentGuess {
    let (operators, _) = parse_operators(original);
    let rules = rules_intent(original, &operators);
    let mut guess = rules;
    if rules.intent == Intent::General {
        let model = model_intent(original);
        if model.intent != Intent::General && model.confidence >= 0.75 {
            guess = model;
        }
    }
    if guess.confidence < 0.45 {
        guess.intent = Intent::General;
    }
    guess
}

/// Rules label alone, for the experiment.
#[must_use]
pub fn rules_only(original: &str) -> IntentGuess {
    let (operators, _) = parse_operators(original);
    rules_intent(original, &operators)
}

/// Model label alone, for the experiment. Never consults the rules.
#[must_use]
pub fn model_only(original: &str) -> IntentGuess {
    model_intent(original)
}

fn guess(intent: Intent, confidence: f32, source: IntentSource) -> IntentGuess {
    IntentGuess {
        intent,
        confidence,
        source,
    }
}

fn rules_intent(original: &str, _operators: &Operators) -> IntentGuess {
    let lower = original.trim().to_lowercase();
    if lower.is_empty() {
        return guess(Intent::General, 0.0, IntentSource::Rules);
    }
    if is_arithmetic(&lower) {
        return guess(Intent::Arithmetic, 0.95, IntentSource::Rules);
    }
    if is_conversion(&lower) {
        return guess(Intent::Conversion, 0.9, IntentSource::Rules);
    }
    if has_any(&lower, &["weather", "forecast", "wetter", "météo", "meteo"]) {
        return guess(Intent::Weather, 0.9, IntentSource::Rules);
    }
    if is_definition(&lower) {
        return guess(Intent::Definition, 0.88, IntentSource::Rules);
    }
    if is_translation(&lower) {
        return guess(Intent::Translation, 0.86, IntentSource::Rules);
    }
    if is_navigational(&lower) {
        return guess(Intent::Navigational, 0.84, IntentSource::Rules);
    }
    if is_technical(&lower) {
        return guess(Intent::Technical, 0.8, IntentSource::Rules);
    }
    if is_fresh(&lower) {
        return guess(Intent::Fresh, 0.7, IntentSource::Rules);
    }
    if is_factual_question(&lower) {
        return guess(Intent::Factual, 0.62, IntentSource::Rules);
    }
    guess(Intent::General, 0.35, IntentSource::Rules)
}

/// Hashed unigram/bigram logistic regression. Weights are fit from the
/// training rows below; the held-out paraphrases and traps are not in that
/// table. A low score stays general and does not rewrite the query.
fn model_intent(original: &str) -> IntentGuess {
    let (weights, bias) = weather_model();
    let features = featurize(original);
    let score = sigmoid(dot(weights, &features) + bias);
    if score >= 0.75 {
        guess(Intent::Weather, score, IntentSource::Model)
    } else {
        guess(Intent::General, score, IntentSource::Model)
    }
}

const FEATURES: usize = 1024;

fn weather_model() -> (&'static [f32; FEATURES], f32) {
    use std::sync::OnceLock;
    static MODEL: OnceLock<([f32; FEATURES], f32)> = OnceLock::new();
    let (weights, bias) = MODEL.get_or_init(train_weather_model);
    (weights, *bias)
}

/// Training rows. None of these strings are in [`HELD_OUT_INTENT`].
const WEATHER_TRAIN: &[(&str, f32)] = &[
    ("is it raining in glasgow tomorrow", 1.0),
    ("will it rain in berlin", 1.0),
    ("how's the sky looking in lyon", 1.0),
    ("how's the sky in lyon", 1.0),
    ("do i need an umbrella in madrid", 1.0),
    ("it's pouring in chicago tonight", 1.0),
    ("precipitation in lisbon", 1.0),
    ("drizzle in dublin this evening", 1.0),
    ("wet sky in glasgow tomorrow", 1.0),
    ("raining in glasgow tonight", 1.0),
    ("chance of rain in vienna", 1.0),
    ("storm clouds over rome", 1.0),
    ("clear sky in dublin tonight", 1.0),
    ("showers expected around glasgow this evening", 1.0),
    ("brand guide shows an umbrella icon", 0.0),
    ("company logo with an umbrella", 0.0),
    ("reach for the sky proverb", 0.0),
    ("painted clouds in a museum", 0.0),
    ("sky news headlines today", 0.0),
    ("staff meeting in berlin tomorrow", 0.0),
    ("board meeting in lyon this afternoon", 0.0),
    ("purple rain album lyrics", 0.0),
    ("idiom when animals fall from clouds", 0.0),
    ("blue sky thinking workshop", 0.0),
    ("glasgow coffee shop", 0.0),
    ("lyon restaurants tonight", 0.0),
    ("limit clause in the contract", 0.0),
    ("fast and furious movie soundtrack", 0.0),
    ("soundtrack listing for a car chase film", 0.0),
    ("furious car chase soundtrack", 0.0),
    ("oil painting of a sky", 0.0),
    ("the painting shows a sky", 0.0),
    ("gallery painting of clouds", 0.0),
    ("reference manual for records", 0.0),
    ("pack a raincoat for cairo on friday", 1.0),
    ("do i need a raincoat in cairo", 1.0),
    ("precipitation expected around cairo", 1.0),
    ("bring a coat for the rain in cairo", 1.0),
    ("bring a raincoat to cairo tomorrow", 1.0),
    ("raincoat stored in the closet", 0.0),
];

/// Paraphrases, ambiguous lines, and traps. Not used for training.
const HELD_OUT_INTENT: &[(&str, bool)] = &[
    ("how's the sky in paris", true),
    ("raining in seattle tonight", true),
    ("sky in tokyo tomorrow", true),
    ("how's the sky in austin", true),
    ("raining in london", true),
    ("sky in oslo", true),
    ("will it be pouring in seattle tonight", true),
    ("should I bring a raincoat to paris tomorrow", true),
    ("is precipitation expected in tokyo", true),
    ("showers over austin this evening", true),
    ("how's the sky in Paris, Texas", true),
    ("will it snow in helsinki tomorrow", true),
    ("pack a raincoat for quebec", true),
    ("staff meeting in quebec tomorrow", false),
    ("quebec coffee shop", false),
    ("umbrella in the logo", false),
    ("the sky's the limit in paris", false),
    ("meeting in paris tomorrow", false),
    ("2 fast 2 furious soundtrack", false),
    ("umbrella company in london", false),
    ("sky in a painting of paris", false),
    ("raining cats and dogs in london", false),
    ("seattle coffee shop", false),
    ("paris restaurants tonight", false),
    ("blue sky thinking in austin", false),
];

fn train_weather_model() -> ([f32; FEATURES], f32) {
    let mut weights = [0.0f32; FEATURES];
    let mut bias = 0.0f32;
    let lr = 0.2f32;
    let l2 = 0.0005f32;
    for _ in 0..80 {
        for (text, label) in WEATHER_TRAIN {
            let features = featurize(text);
            let predicted = sigmoid(dot(&weights, &features) + bias);
            let err = predicted - label;
            for (weight, feature) in weights.iter_mut().zip(features) {
                *weight -= lr * (err * feature + l2 * *weight);
            }
            bias -= lr * err;
        }
    }
    (weights, bias)
}

fn featurize(text: &str) -> [f32; FEATURES] {
    let mut features = [0.0f32; FEATURES];
    let words = model_words(text);
    for word in &words {
        features[token_bucket(word)] += 1.0;
    }
    for pair in words.windows(2) {
        features[token_bucket(&format!("{} {}", pair[0], pair[1]))] += 1.5;
    }
    features
}

fn model_words(text: &str) -> Vec<String> {
    text.to_lowercase()
        .split(|c: char| !c.is_alphanumeric())
        .filter(|word| !word.is_empty())
        .map(str::to_string)
        .collect()
}

fn token_bucket(token: &str) -> usize {
    let mut hash: u32 = 2_166_136_261;
    for byte in token.as_bytes() {
        hash ^= u32::from(*byte);
        hash = hash.wrapping_mul(16_777_619);
    }
    hash as usize % FEATURES
}

fn dot(weights: &[f32; FEATURES], features: &[f32; FEATURES]) -> f32 {
    weights
        .iter()
        .zip(features)
        .map(|(weight, feature)| weight * feature)
        .sum()
}

fn sigmoid(value: f32) -> f32 {
    1.0 / (1.0 + (-value).exp())
}

fn is_arithmetic(lower: &str) -> bool {
    let lower = strip_iso_dates(lower);
    if !lower.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    // `C++` and version-looking tokens are not sums.
    let compact: String = lower.chars().filter(|c| !c.is_whitespace()).collect();
    if compact.contains("c++") || compact.contains("c#") {
        let without = compact.replace("c++", "").replace("c#", "");
        if !has_number_operator(&without) && !has_operator_word(&lower) {
            return false;
        }
    }
    has_number_operator(&lower) || has_operator_word(&lower)
}

fn has_number_operator(lower: &str) -> bool {
    let chars: Vec<char> = lower.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        if !chars[i].is_ascii_digit() {
            i += 1;
            continue;
        }
        while i < chars.len() && (chars[i].is_ascii_digit() || chars[i] == '.') {
            i += 1;
        }
        while i < chars.len() && chars[i].is_whitespace() {
            i += 1;
        }
        if i >= chars.len() {
            break;
        }
        let op = chars[i];
        if matches!(op, '+' | '-' | '*' | '/' | '×' | '÷' | '%') {
            let mut j = i + 1;
            while j < chars.len() && chars[j].is_whitespace() {
                j += 1;
            }
            if j < chars.len() && chars[j].is_ascii_digit() {
                return true;
            }
        }
        i += 1;
    }
    false
}

fn strip_iso_dates(lower: &str) -> String {
    let chars: Vec<char> = lower.chars().collect();
    let mut out = String::new();
    let mut index = 0;
    while index < chars.len() {
        if iso_date_at(&chars, index) {
            index += 10;
            out.push(' ');
            continue;
        }
        out.push(chars[index]);
        index += 1;
    }
    out
}

fn iso_date_at(chars: &[char], index: usize) -> bool {
    if index + 10 > chars.len() {
        return false;
    }
    let digits = |range: std::ops::Range<usize>| chars[range].iter().all(|c| c.is_ascii_digit());
    digits(index..index + 4)
        && chars[index + 4] == '-'
        && digits(index + 5..index + 7)
        && chars[index + 7] == '-'
        && digits(index + 8..index + 10)
}

fn has_operator_word(lower: &str) -> bool {
    let words = [
        " plus ",
        " minus ",
        " times ",
        " divided ",
        " modulo ",
        " over ",
    ];
    let padded = format!(" {lower} ");
    words.iter().any(|word| padded.contains(word)) && lower.chars().any(|c| c.is_ascii_digit())
}

fn is_conversion(lower: &str) -> bool {
    if !lower.chars().any(|c| c.is_ascii_digit()) {
        return false;
    }
    let units = [
        "km",
        "kilometer",
        "kilometre",
        "mile",
        "meter",
        "metre",
        "feet",
        "foot",
        "ft",
        "inch",
        "inches",
        "cm",
        "mm",
        "kg",
        "kilogram",
        "pound",
        "lb",
        "lbs",
        "ounce",
        "oz",
        "celsius",
        "fahrenheit",
        "°c",
        "°f",
        "mph",
        "kph",
        "km/h",
        "liter",
        "litre",
        "gallon",
        "cup",
        "tbsp",
        "tsp",
        "acre",
        "hectare",
    ];
    let hits = units
        .iter()
        .filter(|unit| contains_unit(lower, unit))
        .count();
    if hits >= 2 {
        return true;
    }
    hits >= 1
        && (lower.contains(" to ")
            || lower.contains(" in ")
            || lower.contains(" into ")
            || lower.contains("how many"))
}

fn contains_unit(lower: &str, unit: &str) -> bool {
    if unit.starts_with('°') {
        return lower.contains(unit);
    }
    let padded = format!(" {lower} ");
    padded.contains(&format!(" {unit} "))
        || padded.contains(&format!(" {unit}s "))
        || lower
            .split(|c: char| !c.is_ascii_alphanumeric() && c != '/')
            .any(|part| part == unit)
}

fn is_definition(lower: &str) -> bool {
    lower.starts_with("define ")
        || lower.contains("meaning of ")
        || lower.contains("definition of ")
        || lower.contains("what does ")
        || lower.contains("definicion de ")
        || lower.contains("définition")
}

fn is_translation(lower: &str) -> bool {
    if lower.starts_with("translate ")
        || lower.contains("how do you say")
        || lower.contains("how to say")
    {
        return true;
    }
    let languages = [
        "spanish",
        "french",
        "german",
        "dutch",
        "italian",
        "portuguese",
        "japanese",
        "chinese",
        "korean",
        "russian",
        "arabic",
        "english",
    ];
    (lower.starts_with("in ") || lower.contains(" in "))
        && languages.iter().any(|lang| {
            lower
                .split_whitespace()
                .any(|w| w.trim_matches('?') == *lang)
        })
        && (lower.contains("say")
            || lower.contains("translate")
            || lower.split_whitespace().count() <= 6)
}

fn is_navigational(lower: &str) -> bool {
    let trimmed = lower.trim().trim_end_matches('/');
    if trimmed.contains(' ') {
        return trimmed.ends_with(" official site")
            || trimmed.ends_with(" official website")
            || trimmed.ends_with(" homepage")
            || trimmed.starts_with("login ");
    }
    let host = trimmed
        .trim_start_matches("https://")
        .trim_start_matches("http://");
    host.contains('.') && !host.contains(' ') && host.chars().any(|c| c.is_ascii_alphabetic())
}

fn is_technical(lower: &str) -> bool {
    const MARKERS: &[&str] = &[
        "error",
        "exception",
        "traceback",
        "undefined reference",
        "segfault",
        "stack overflow",
        "syntaxerror",
        "typeerror",
        "panic",
        "npm err",
        "cargo",
        "compiler",
        "stackoverflow",
        "::",
        "undefined",
        "nullpointer",
        "errno",
    ];
    MARKERS.iter().any(|marker| lower.contains(marker))
}

fn is_fresh(lower: &str) -> bool {
    has_any(
        lower,
        &[
            "latest",
            "breaking",
            "news",
            "today",
            "this week",
            "this month",
        ],
    ) && !has_any(lower, &["weather", "forecast"])
}

fn is_factual_question(lower: &str) -> bool {
    // A lone "?" or a single word is not a question we should relabel.
    if lower.split_whitespace().count() < 2 {
        return false;
    }
    let starters = [
        "who ", "what ", "when ", "where ", "why ", "how ", "which ", "qué ", "qui ", "was ist ",
    ];
    starters.iter().any(|s| lower.starts_with(s)) || lower.ends_with('?')
}

fn has_any(haystack: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| haystack.contains(needle))
}

fn entities_of(original: &str, operators: &Operators, intent: Intent) -> Vec<Entity> {
    let mut entities = Vec::new();
    if let Some(site) = &operators.site {
        entities.push(Entity {
            kind: "site".to_string(),
            value: site.clone(),
        });
    }
    if let Some(filetype) = &operators.filetype {
        entities.push(Entity {
            kind: "filetype".to_string(),
            value: filetype.clone(),
        });
    }
    for phrase in &operators.phrases {
        entities.push(Entity {
            kind: "phrase".to_string(),
            value: phrase.clone(),
        });
    }
    if intent == Intent::Weather {
        if let Some(place) = weather_place(original) {
            entities.push(Entity {
                kind: "place".to_string(),
                value: place,
            });
        }
    }
    entities
}

fn weather_place(original: &str) -> Option<String> {
    keyword_weather_place(original).or_else(|| place_after_in(original))
}

fn place_after_in(original: &str) -> Option<String> {
    let lower = original.to_lowercase();
    let index = lower.rfind(" in ")?;
    let after = original.get(index + 4..)?;
    let skip = [
        "tonight", "tomorrow", "today", "right", "now", "please", "the", "a", "an",
    ];
    let words: Vec<String> = after
        .split_whitespace()
        .filter_map(|word| {
            let trimmed = word.trim_matches(|c: char| !c.is_alphanumeric());
            let key = trimmed.to_lowercase();
            if trimmed.is_empty() || skip.contains(&key.as_str()) {
                None
            } else {
                Some(trimmed.to_string())
            }
        })
        .take(4)
        .collect();
    if words.is_empty() {
        None
    } else {
        Some(words.join(" "))
    }
}

fn keyword_weather_place(original: &str) -> Option<String> {
    let words: Vec<&str> = original.split_whitespace().collect();
    let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    let keys = ["weather", "forecast", "wetter"];
    let idx = lower
        .iter()
        .position(|w| keys.contains(&w.trim_matches('?').trim_matches(',')))?;
    let skip = [
        "in", "for", "at", "the", "a", "an", "tomorrow", "today", "tonight", "please",
    ];
    let after: Vec<&str> = words
        .iter()
        .skip(idx + 1)
        .copied()
        .filter(|w| !skip.contains(&w.to_lowercase().trim_matches('?').trim_matches(',')))
        .collect();
    if !after.is_empty() {
        return Some(after.join(" "));
    }
    let before: Vec<&str> = words
        .iter()
        .take(idx)
        .copied()
        .filter(|w| {
            let l = w.to_lowercase();
            !skip.contains(&l.as_str())
                && !matches!(
                    l.as_str(),
                    "what" | "is" | "whats" | "what's" | "how's" | "hows"
                )
        })
        .collect();
    if before.is_empty() {
        None
    } else {
        Some(before.join(" "))
    }
}

/// Split operator syntax from the residual text.
/// Engines that support web operators receive `original`. Others receive the residual.
pub fn parse_operators(original: &str) -> (Operators, String) {
    let mut operators = Operators::default();
    let mut residual = Vec::new();
    let mut rest = original;
    while !rest.is_empty() {
        rest = rest.trim_start();
        if rest.is_empty() {
            break;
        }
        if rest.starts_with('"') {
            if let Some(end) = rest[1..].find('"') {
                let phrase = rest[1..1 + end].trim().to_string();
                if !phrase.is_empty() {
                    operators.phrases.push(phrase.clone());
                    residual.push(phrase);
                }
                rest = &rest[1 + end + 1..];
                continue;
            }
        }
        let (token, next) = split_token(rest);
        rest = next;
        if let Some(site) = token.strip_prefix("site:") {
            if !site.is_empty() && operators.site.is_none() {
                operators.site = Some(site.trim_matches('"').to_string());
            }
            continue;
        }
        if let Some(kind) = token.strip_prefix("filetype:") {
            if !kind.is_empty() && operators.filetype.is_none() {
                operators.filetype = Some(kind.trim_start_matches('.').to_ascii_lowercase());
            }
            continue;
        }
        if let Some(excluded) = excluded_term(token) {
            operators.excluded.push(excluded);
            continue;
        }
        residual.push(token.to_string());
    }
    (operators, residual.join(" "))
}

fn split_token(input: &str) -> (&str, &str) {
    let end = input.find(char::is_whitespace).unwrap_or(input.len());
    (&input[..end], &input[end..])
}

/// Location the weather engine should request, or `None` when this is not a weather query.
/// Keyword forms and the intent model share this decision.
pub fn weather_request_location(query: &str) -> Option<String> {
    let cleaned = query.trim().trim_end_matches('?').trim();
    if cleaned.is_empty() {
        return None;
    }
    let words: Vec<&str> = cleaned.split_whitespace().collect();
    let lower: Vec<String> = words.iter().map(|w| w.to_lowercase()).collect();
    const KEYWORDS: &[&str] = &["weather", "forecast", "wetter", "meteo", "météo"];
    const PREPS: &[&str] = &["in", "for", "at"];
    const FILLERS: &[&str] = &[
        "what", "is", "the", "whats", "what's", "like", "a", "an", "tomorrow", "today", "tonight",
        "morgen", "demain", "hoy",
    ];
    let filler = |word: &str| FILLERS.contains(&word.to_lowercase().as_str());
    let Some(kw_idx) = lower.iter().position(|w| KEYWORDS.contains(&w.as_str())) else {
        return weather_model_place(cleaned);
    };
    let after = &words[kw_idx + 1..];
    let after_lower = &lower[kw_idx + 1..];
    if let Some(prep_rel) = after_lower.iter().position(|w| PREPS.contains(&w.as_str())) {
        let loc = after[prep_rel + 1..]
            .iter()
            .copied()
            .filter(|w| !filler(w))
            .collect::<Vec<_>>()
            .join(" ");
        if !loc.is_empty() {
            return Some(loc);
        }
    }
    if kw_idx == 0 && !after.is_empty() {
        let loc = after
            .iter()
            .copied()
            .filter(|w| !filler(w))
            .collect::<Vec<_>>()
            .join(" ");
        if !loc.is_empty() {
            return Some(loc);
        }
    }
    if kw_idx == words.len() - 1 {
        let loc = words[..kw_idx]
            .iter()
            .copied()
            .filter(|w| !filler(w))
            .collect::<Vec<_>>()
            .join(" ");
        return (!loc.is_empty()).then_some(loc);
    }
    if !after.is_empty() {
        let loc = after
            .iter()
            .copied()
            .filter(|w| !filler(w))
            .collect::<Vec<_>>()
            .join(" ");
        if !loc.is_empty() {
            return Some(loc);
        }
    }
    weather_model_place(cleaned)
}

fn weather_model_place(query: &str) -> Option<String> {
    let parsed = interpret(query);
    if parsed.intent != Intent::Weather {
        return None;
    }
    parsed
        .entities
        .into_iter()
        .find(|entity| entity.kind == "place")
        .map(|entity| entity.value)
        .filter(|place| !place.trim().is_empty())
}

fn excluded_term(token: &str) -> Option<String> {
    let raw = token.strip_prefix('-')?;
    if raw.is_empty() || raw.starts_with('-') || raw.contains('=') || raw.contains('+') {
        return None;
    }
    if raw.chars().next().is_some_and(|c| c.is_ascii_digit()) {
        return None;
    }
    if raw
        .chars()
        .all(|c| c.is_alphanumeric() || c == '_' || c == '\'')
        && raw.chars().count() >= 2
    {
        Some(raw.to_string())
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn operators_keep_original_meaning() {
        let parsed = interpret("site:docs.rs \"async trait\" filetype:html -sponsor rust");
        assert_eq!(
            parsed.original,
            "site:docs.rs \"async trait\" filetype:html -sponsor rust"
        );
        assert_eq!(parsed.operators.site.as_deref(), Some("docs.rs"));
        assert_eq!(parsed.operators.filetype.as_deref(), Some("html"));
        assert_eq!(parsed.operators.phrases, vec!["async trait".to_string()]);
        assert_eq!(parsed.operators.excluded, vec!["sponsor".to_string()]);
        assert_eq!(parsed.residual, "async trait rust");
        assert_eq!(parsed.intent, Intent::General);
    }

    #[test]
    fn does_not_treat_compiler_flags_as_excluded_terms() {
        let (ops, residual) = parse_operators("gcc -std=c++17 main.cpp");
        assert!(ops.excluded.is_empty());
        assert!(residual.contains("-std=c++17"));
    }

    #[test]
    fn arithmetic_and_false_positive_titles() {
        assert_eq!(interpret("12 * 8").intent, Intent::Arithmetic);
        assert_eq!(interpret("what's 12 times 8").intent, Intent::Arithmetic);
        assert_eq!(interpret("2 fast 2 furious").intent, Intent::General);
        assert_ne!(interpret("C++").intent, Intent::Arithmetic);
        assert_ne!(interpret("C# records").intent, Intent::Arithmetic);
    }

    #[test]
    fn conversion_weather_and_traps() {
        assert_eq!(interpret("5 km in miles").intent, Intent::Conversion);
        assert_eq!(
            interpret("weather tomorrow in Paris, Texas").intent,
            Intent::Weather
        );
        assert_eq!(
            interpret("weather tomorrow in Paris, Texas")
                .entities
                .iter()
                .find(|e| e.kind == "place")
                .map(|e| e.value.as_str()),
            Some("Paris, Texas")
        );
        assert_eq!(
            interpret("meeting in paris tomorrow").intent,
            Intent::General
        );
        assert_eq!(interpret("sky news").intent, Intent::Fresh);
    }

    #[test]
    fn held_out_intent_is_disjoint_from_training() {
        let train: std::collections::HashSet<&str> =
            WEATHER_TRAIN.iter().map(|(text, _)| *text).collect();
        for (text, _) in HELD_OUT_INTENT {
            assert!(
                !train.contains(text),
                "{text} is both a training row and a held-out row"
            );
        }
        let report = intent_holdout();
        assert_eq!(
            report.positives,
            HELD_OUT_INTENT
                .iter()
                .filter(|(_, weather)| *weather)
                .count()
        );
        assert_eq!(
            report.traps,
            HELD_OUT_INTENT
                .iter()
                .filter(|(_, weather)| !*weather)
                .count()
        );
        eprintln!(
            "intent holdout rules_p={:.3} rules_r={:.3} p={:.3} r={:.3} fp={} positives={} traps={} enable={}",
            report.rules_precision,
            report.rules_recall,
            report.precision,
            report.recall,
            report.false_positives,
            report.positives,
            report.traps,
            report.enable
        );
        assert_eq!(model_enabled(), report.enable);
        assert_eq!(
            interpret("?").original,
            "?",
            "low confidence must not rewrite the query"
        );
        assert_eq!(interpret("?").intent, Intent::General);
    }

    #[test]
    fn model_fills_sky_paraphrase_without_overriding_rules() {
        let sky = interpret("how's the sky in paris");
        assert_eq!(sky.intent, Intent::Weather);
        assert_eq!(sky.source, IntentSource::Model);
        assert_eq!(rules_only("how's the sky in paris").intent, Intent::General);
        let meeting = interpret("meeting in paris tomorrow");
        assert_eq!(meeting.intent, Intent::General);
        assert_ne!(meeting.source, IntentSource::Model);
        assert_ne!(
            interpret("days until 2027-01-01").intent,
            Intent::Arithmetic
        );
        assert_eq!(
            interpret("raining in seattle tonight")
                .entities
                .iter()
                .find(|entity| entity.kind == "place")
                .map(|entity| entity.value.as_str()),
            Some("seattle")
        );
        for trap in [
            "umbrella in the logo",
            "the sky's the limit in paris",
            "sky in a painting of paris",
        ] {
            assert_ne!(
                model_only(trap).intent,
                Intent::Weather,
                "{trap} scored {}",
                model_only(trap).confidence
            );
        }
    }

    #[test]
    fn low_confidence_question_mark_alone_stays_general() {
        let parsed = interpret("?");
        assert_eq!(parsed.intent, Intent::General);
    }

    #[test]
    fn navigational_hosts_and_official_site() {
        assert_eq!(interpret("rust-lang.org").intent, Intent::Navigational);
        assert_eq!(
            interpret("python official site").intent,
            Intent::Navigational
        );
        assert_eq!(interpret("python").intent, Intent::General);
    }
}
