//! Currency conversion engine backed by the ECB daily reference rates.
//!
//! Only fires on conversion-shaped queries (`100 usd to eur`, `$5 in gbp`).
//! The ECB feed is a single EUR-based XML document; the requested
//! amount/from/to ride along as marker query parameters (ignored by the ECB
//! server) so the response step can recover them from the request URL.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Answer, InteractiveAnswer, Result_};

/// Engine name / identifier.
pub const NAME: &str = "currency";

const ECB_URL: &str = "https://www.ecb.europa.eu/stats/eurofxref/eurofxref-daily.xml";

/// Currencies in the ECB daily feed (EUR is the base and always available).
const ECB_CURRENCIES: &[&str] = &[
    "USD", "JPY", "BGN", "CZK", "DKK", "GBP", "HUF", "PLN", "RON", "SEK", "CHF", "ISK", "NOK",
    "TRY", "AUD", "BRL", "CAD", "CNY", "HKD", "IDR", "ILS", "INR", "KRW", "MXN", "MYR", "NZD",
    "PHP", "SGD", "THB", "ZAR",
];

/// `(alias, ISO code)` — names and symbols people actually type.
const ALIASES: &[(&str, &str)] = &[
    ("$", "USD"),
    ("usd", "USD"),
    ("dollar", "USD"),
    ("dollars", "USD"),
    ("€", "EUR"),
    ("eur", "EUR"),
    ("euro", "EUR"),
    ("euros", "EUR"),
    ("£", "GBP"),
    ("gbp", "GBP"),
    ("pound", "GBP"),
    ("pounds", "GBP"),
    ("¥", "JPY"),
    ("jpy", "JPY"),
    ("yen", "JPY"),
    ("chf", "CHF"),
    ("franc", "CHF"),
    ("francs", "CHF"),
    ("cad", "CAD"),
    ("aud", "AUD"),
    ("nzd", "NZD"),
    ("cny", "CNY"),
    ("rmb", "CNY"),
    ("yuan", "CNY"),
    ("inr", "INR"),
    ("rupee", "INR"),
    ("rupees", "INR"),
    ("krw", "KRW"),
    ("won", "KRW"),
    ("sek", "SEK"),
    ("nok", "NOK"),
    ("dkk", "DKK"),
    ("pln", "PLN"),
    ("zloty", "PLN"),
    ("czk", "CZK"),
    ("koruna", "CZK"),
    ("huf", "HUF"),
    ("forint", "HUF"),
    ("ron", "RON"),
    ("leu", "RON"),
    ("bgn", "BGN"),
    ("lev", "BGN"),
    ("try", "TRY"),
    ("lira", "TRY"),
    ("ils", "ILS"),
    ("shekel", "ILS"),
    ("shekels", "ILS"),
    ("mxn", "MXN"),
    ("peso", "MXN"),
    ("pesos", "MXN"),
    ("brl", "BRL"),
    ("real", "BRL"),
    ("reais", "BRL"),
    ("zar", "ZAR"),
    ("rand", "ZAR"),
    ("sgd", "SGD"),
    ("hkd", "HKD"),
    ("thb", "THB"),
    ("baht", "THB"),
    ("php", "PHP"),
    ("myr", "MYR"),
    ("ringgit", "MYR"),
    ("idr", "IDR"),
    ("rupiah", "IDR"),
    ("isk", "ISK"),
];

fn currency_code(raw: &str) -> Option<&'static str> {
    let needle = raw.trim().to_ascii_lowercase();
    ALIASES
        .iter()
        .find(|(alias, _)| *alias == needle)
        .map(|(_, code)| *code)
        .filter(|code| *code == "EUR" || ECB_CURRENCIES.contains(code))
}

/// Parse `100 usd to eur` / `$5 in gbp` / `usd to eur` (amount defaults to 1).
pub fn parse_currency_query(query: &str) -> Option<(f64, &'static str, &'static str)> {
    let lower = query.trim().to_ascii_lowercase();
    let tokens: Vec<&str> = lower.split_whitespace().collect();
    let sep = tokens
        .iter()
        .rposition(|t| matches!(*t, "to" | "in" | "as" | "->" | "="))?;
    if sep + 1 != tokens.len() - 1 {
        return None;
    }
    let to = currency_code(tokens[sep + 1])?;

    let (amount, from) = match &tokens[..sep] {
        [amount, from] => (parse_amount(amount)?, currency_code(from)?),
        [joined] => split_amount_currency(joined)?,
        _ => return None,
    };
    if from == to {
        return None;
    }
    Some((amount, from, to))
}

fn parse_amount(raw: &str) -> Option<f64> {
    raw.replace(',', "")
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v > 0.0)
}

/// Split `100usd` / `$100` / `usd` into amount + currency.
fn split_amount_currency(raw: &str) -> Option<(f64, &'static str)> {
    // Symbol prefix: `$100`, `€5.50`.
    for (alias, _) in ALIASES {
        if !alias.chars().next().is_some_and(|c| c.is_alphanumeric())
            && let Some(rest) = raw.strip_prefix(alias)
        {
            let amount = if rest.is_empty() {
                1.0
            } else {
                parse_amount(rest)?
            };
            return Some((amount, currency_code(alias)?));
        }
    }
    // Bare code: `usd` (amount 1) or trailing code: `100usd`.
    if let Some(code) = currency_code(raw) {
        return Some((1.0, code));
    }
    let split = raw
        .find(|c: char| c.is_ascii_alphabetic())
        .filter(|&i| i > 0)?;
    let amount = parse_amount(&raw[..split])?;
    Some((amount, currency_code(&raw[split..])?))
}

/// EUR-per-unit rate for `code` from the ECB XML body (EUR itself is 1).
/// The ECB feed quotes attributes with single quotes, but tests/other feeds
/// may use double quotes, so both are accepted.
fn rate_of(xml: &str, code: &str) -> Option<f64> {
    if code == "EUR" {
        return Some(1.0);
    }
    let at = xml
        .find(&format!("currency='{code}'"))
        .or_else(|| xml.find(&format!("currency=\"{code}\"")))?;
    let tail = &xml[at..];
    let rate_at = tail.find("rate=")? + "rate=".len();
    let after = &tail[rate_at..];
    // Skip the opening quote (either kind) and read until the closing one.
    let quote = after.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let body = &after[1..];
    let end = body.find(quote)?;
    body[..end].parse::<f64>().ok().filter(|r| *r > 0.0)
}

fn marker_param(url: &str, key: &str) -> Option<String> {
    let query = url.split_once('?')?.1;
    url::form_urlencoded::parse(query.as_bytes())
        .find(|(k, _)| k == key)
        .map(|(_, v)| v.into_owned())
}

fn format_amount(value: f64) -> String {
    if value >= 100.0 {
        format!("{value:.2}")
    } else {
        format!("{value:.4}")
    }
    .trim_end_matches('0')
    .trim_end_matches('.')
    .to_string()
}

/// The ECB currency conversion engine.
#[derive(Debug, Clone)]
pub struct Currency {
    meta: EngineMeta,
}

impl Currency {
    pub fn new() -> Self {
        Currency {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "cc".to_string(),
                about: About {
                    website: Some("https://www.ecb.europa.eu/".to_string()),
                    wikidata_id: None,
                    official_api_documentation: Some(
                        "https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html"
                            .to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "XML".to_string(),
                },
            },
        }
    }
}

impl Default for Currency {
    fn default() -> Self {
        Self::new()
    }
}

impl Engine for Currency {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        let Some((amount, from, to)) = parse_currency_query(&q.query) else {
            return;
        };
        if q.pageno > 1 {
            return;
        }
        p.method = HttpMethod::Get;
        // The markers are ignored by the ECB's static file server but let the
        // response step recover the request without shared state.
        let markers = url::form_urlencoded::Serializer::new(String::new())
            .append_pair("zk_a", &amount.to_string())
            .append_pair("zk_f", from)
            .append_pair("zk_t", to)
            .finish();
        p.url = Some(format!("{ECB_URL}?{markers}"));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let amount = marker_param(&resp.url, "zk_a")
            .and_then(|v| v.parse::<f64>().ok())
            .ok_or_else(|| EngineError::Parse("missing amount marker".to_string()))?;
        let from = marker_param(&resp.url, "zk_f")
            .ok_or_else(|| EngineError::Parse("missing from marker".to_string()))?;
        let to = marker_param(&resp.url, "zk_t")
            .ok_or_else(|| EngineError::Parse("missing to marker".to_string()))?;

        let xml = String::from_utf8_lossy(&resp.body);
        let from_rate = rate_of(&xml, &from)
            .ok_or_else(|| EngineError::Parse(format!("no ECB rate for {from}")))?;
        let to_rate = rate_of(&xml, &to)
            .ok_or_else(|| EngineError::Parse(format!("no ECB rate for {to}")))?;

        // Rates are units-per-EUR; cross rate via EUR.
        let converted = amount / from_rate * to_rate;
        let rate = converted / amount;

        res.add(Result_::Answer(Answer {
            answer: format!(
                "{} {from} = {} {to}",
                format_amount(amount),
                format_amount(converted)
            ),
            url: Some(
                "https://www.ecb.europa.eu/stats/policy_and_exchange_rates/euro_reference_exchange_rates/html/index.en.html"
                    .to_string(),
            ),
            engine: NAME.to_string(),
            interactive: Some(InteractiveAnswer::Currency {
                amount,
                from: from.clone(),
                to: to.clone(),
                result: converted,
                rate,
            }),
            ..Answer::default()
        }));

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const ECB_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<gesmes:Envelope>
  <Cube><Cube time="2026-07-17">
    <Cube currency="USD" rate="1.0850"/>
    <Cube currency="GBP" rate="0.8420"/>
    <Cube currency="JPY" rate="171.50"/>
  </Cube></Cube>
</gesmes:Envelope>"#;

    #[test]
    fn parses_conversion_queries() {
        assert_eq!(
            parse_currency_query("100 usd to eur"),
            Some((100.0, "USD", "EUR"))
        );
        assert_eq!(parse_currency_query("$5 in gbp"), Some((5.0, "USD", "GBP")));
        assert_eq!(
            parse_currency_query("usd to eur"),
            Some((1.0, "USD", "EUR"))
        );
        assert_eq!(
            parse_currency_query("100usd to eur"),
            Some((100.0, "USD", "EUR"))
        );
        assert_eq!(
            parse_currency_query("2,500 euros in dollars"),
            Some((2500.0, "EUR", "USD"))
        );
    }

    #[test]
    fn rejects_non_currency_queries() {
        assert_eq!(parse_currency_query("rust to go migration"), None);
        assert_eq!(parse_currency_query("10 km to miles"), None);
        assert_eq!(parse_currency_query("usd to usd"), None, "same currency");
        assert_eq!(parse_currency_query(""), None);
        assert_eq!(parse_currency_query("-5 usd to eur"), None);
    }

    #[test]
    fn non_currency_query_builds_no_request() {
        let engine = Currency::new();
        let q = SearchQueryView {
            query: "weather berlin".to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        };
        let mut p = RequestParams::default();
        engine.request(&q, &mut p);
        assert!(p.url.is_none());
    }

    #[test]
    fn request_carries_markers() {
        let engine = Currency::new();
        let q = SearchQueryView {
            query: "100 usd to eur".to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        };
        let mut p = RequestParams::default();
        engine.request(&q, &mut p);
        let url = p.url.expect("url set");
        assert!(url.starts_with(ECB_URL));
        assert!(url.contains("zk_a=100"));
        assert!(url.contains("zk_f=USD"));
        assert!(url.contains("zk_t=EUR"));
    }

    #[test]
    fn converts_via_eur_cross_rate() {
        let engine = Currency::new();
        let resp = EngineResponse {
            status: 200,
            url: format!("{ECB_URL}?zk_a=100&zk_f=USD&zk_t=GBP"),
            body: ECB_XML.as_bytes().to_vec(),
            ..EngineResponse::default()
        };
        let results = engine.response(&resp).unwrap();
        assert_eq!(results.answers.len(), 1);
        // 100 USD -> EUR (100/1.085) -> GBP (*0.842) = 77.60…
        assert_eq!(results.answers[0].answer, "100 USD = 77.6037 GBP");
        assert_eq!(results.answers[0].engine, NAME);
        assert_eq!(
            results.answers[0].interactive,
            Some(InteractiveAnswer::Currency {
                amount: 100.0,
                from: "USD".into(),
                to: "GBP".into(),
                result: 100.0 / 1.085 * 0.842,
                rate: (100.0 / 1.085 * 0.842) / 100.0,
            })
        );
    }

    #[test]
    fn parses_single_quoted_ecb_attributes() {
        // The real ECB feed quotes attributes with single quotes.
        let xml = "<Cube currency='USD' rate='1.1426'/><Cube currency='GBP' rate='0.84888'/>";
        assert_eq!(rate_of(xml, "USD"), Some(1.1426));
        assert_eq!(rate_of(xml, "GBP"), Some(0.84888));
        assert_eq!(rate_of(xml, "EUR"), Some(1.0));
    }

    #[test]
    fn eur_base_conversions_work_both_ways() {
        let engine = Currency::new();
        let resp = EngineResponse {
            status: 200,
            url: format!("{ECB_URL}?zk_a=1&zk_f=EUR&zk_t=USD"),
            body: ECB_XML.as_bytes().to_vec(),
            ..EngineResponse::default()
        };
        let results = engine.response(&resp).unwrap();
        assert_eq!(results.answers[0].answer, "1 EUR = 1.085 USD");
    }

    #[test]
    fn unknown_currency_in_feed_is_a_parse_error() {
        let engine = Currency::new();
        let resp = EngineResponse {
            status: 200,
            url: format!("{ECB_URL}?zk_a=1&zk_f=USD&zk_t=KRW"),
            body: ECB_XML.as_bytes().to_vec(),
            ..EngineResponse::default()
        };
        assert!(engine.response(&resp).is_err());
    }
}
