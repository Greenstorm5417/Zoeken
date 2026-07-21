//! Weather engine backed by wttr.in.
//!
//! Only fires when the query starts with a weather keyword
//! (`weather berlin`, `forecast tokyo`); other queries produce no request.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Answer, Result_};

/// Engine name / identifier.
pub const NAME: &str = "weather";

const BASE_URL: &str = "https://wttr.in";

const KEYWORDS: &[&str] = &["weather", "forecast", "wetter"];

/// The wttr.in weather engine.
#[derive(Debug, Clone)]
pub struct Weather {
    meta: EngineMeta,
}

impl Weather {
    pub fn new() -> Self {
        Weather {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string(), "weather".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "wttr".to_string(),
                about: About {
                    website: Some("https://wttr.in/".to_string()),
                    wikidata_id: None,
                    official_api_documentation: Some(
                        "https://github.com/chubin/wttr.in".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
        }
    }
}

impl Default for Weather {
    fn default() -> Self {
        Self::new()
    }
}

/// The location part of a weather query, or `None` when this is not one.
fn weather_location(query: &str) -> Option<String> {
    let mut tokens = query.split_whitespace();
    let first = tokens.next()?.to_ascii_lowercase();
    // Both orders: "weather berlin" and "berlin weather".
    if KEYWORDS.contains(&first.as_str()) {
        let rest = tokens.collect::<Vec<_>>().join(" ");
        return (!rest.is_empty()).then_some(rest);
    }
    let words: Vec<&str> = query.split_whitespace().collect();
    if let Some((last, head)) = words.split_last()
        && KEYWORDS.contains(&last.to_ascii_lowercase().as_str())
        && !head.is_empty()
    {
        return Some(head.join(" "));
    }
    None
}

impl Engine for Weather {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        // Non-weather queries: leave `url` unset so the executor skips us.
        let Some(location) = weather_location(&q.query) else {
            return;
        };
        if q.pageno > 1 {
            return;
        }
        p.method = HttpMethod::Get;
        let encoded: String = url::form_urlencoded::byte_serialize(location.as_bytes()).collect();
        p.url = Some(format!("{BASE_URL}/{encoded}?format=j1"));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid wttr.in JSON: {e}")))?;

        let current = value
            .get("current_condition")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .ok_or_else(|| EngineError::Parse("missing current_condition".to_string()))?;

        let str_of = |obj: &serde_json::Value, key: &str| -> String {
            obj.get(key)
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string()
        };
        let desc = current
            .get("weatherDesc")
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .map(|v| str_of(v, "value"))
            .unwrap_or_default();

        let area = value
            .get("nearest_area")
            .and_then(|a| a.as_array())
            .and_then(|a| a.first());
        let place = area
            .map(|a| {
                let name = a
                    .get("areaName")
                    .and_then(|n| n.as_array())
                    .and_then(|n| n.first())
                    .map(|v| str_of(v, "value"))
                    .unwrap_or_default();
                let country = a
                    .get("country")
                    .and_then(|c| c.as_array())
                    .and_then(|c| c.first())
                    .map(|v| str_of(v, "value"))
                    .unwrap_or_default();
                match (name.is_empty(), country.is_empty()) {
                    (false, false) => format!("{name}, {country}"),
                    (false, true) => name,
                    _ => country,
                }
            })
            .unwrap_or_default();

        let temp_c = str_of(current, "temp_C");
        let temp_f = str_of(current, "temp_F");
        let feels_c = str_of(current, "FeelsLikeC");
        let wind = str_of(current, "windspeedKmph");
        let wind_dir = str_of(current, "winddir16Point");
        let humidity = str_of(current, "humidity");

        if temp_c.is_empty() && desc.is_empty() {
            return Err(EngineError::Parse("empty weather payload".to_string()));
        }

        let mut parts = Vec::new();
        if !desc.is_empty() {
            parts.push(desc.clone());
        }
        parts.push(format!("{temp_c}°C ({temp_f}°F)"));
        if !feels_c.is_empty() && feels_c != temp_c {
            parts.push(format!("feels like {feels_c}°C"));
        }
        if !wind.is_empty() {
            let dir = if wind_dir.is_empty() {
                String::new()
            } else {
                format!(" {wind_dir}")
            };
            parts.push(format!("wind {wind} km/h{dir}"));
        }
        if !humidity.is_empty() {
            parts.push(format!("humidity {humidity}%"));
        }

        let prefix = if place.is_empty() {
            "Weather".to_string()
        } else {
            format!("Weather in {place}")
        };
        res.add(Result_::Answer(Answer {
            answer: format!("{prefix}: {}", parts.join(", ")),
            url: resp
                .url
                .split_once("?")
                .map(|(base, _)| base.to_string())
                .or_else(|| Some(resp.url.clone())),
            engine: NAME.to_string(),
            ..Answer::default()
        }));

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(q: &str) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        }
    }

    const WTTR_JSON: &str = r#"{
      "current_condition": [{
        "temp_C": "18", "temp_F": "64", "FeelsLikeC": "17",
        "windspeedKmph": "12", "winddir16Point": "SW", "humidity": "67",
        "weatherDesc": [{"value": "Partly cloudy"}]
      }],
      "nearest_area": [{
        "areaName": [{"value": "Berlin"}],
        "country": [{"value": "Germany"}]
      }]
    }"#;

    #[test]
    fn detects_weather_queries_in_both_orders() {
        assert_eq!(
            weather_location("weather berlin"),
            Some("berlin".to_string())
        );
        assert_eq!(
            weather_location("forecast new york"),
            Some("new york".to_string())
        );
        assert_eq!(
            weather_location("new york weather"),
            Some("new york".to_string())
        );
        assert_eq!(weather_location("weather"), None, "no location given");
        assert_eq!(weather_location("rust programming"), None);
        assert_eq!(weather_location(""), None);
    }

    #[test]
    fn non_weather_query_builds_no_request() {
        let engine = Weather::new();
        let q = query("rust programming");
        let mut p = RequestParams::default();
        engine.request(&q, &mut p);
        assert!(p.url.is_none());
    }

    #[test]
    fn weather_query_builds_wttr_url() {
        let engine = Weather::new();
        let q = query("weather new york");
        let mut p = RequestParams::default();
        engine.request(&q, &mut p);
        assert_eq!(p.url.as_deref(), Some("https://wttr.in/new+york?format=j1"));
    }

    #[test]
    fn parses_current_conditions_into_an_answer() {
        let engine = Weather::new();
        let resp = EngineResponse {
            status: 200,
            url: "https://wttr.in/berlin?format=j1".to_string(),
            body: WTTR_JSON.as_bytes().to_vec(),
            ..EngineResponse::default()
        };
        let results = engine.response(&resp).unwrap();
        assert_eq!(results.answers.len(), 1, "answer routed to answers channel");
        let answer = &results.answers[0];
        assert_eq!(
            answer.answer,
            "Weather in Berlin, Germany: Partly cloudy, 18°C (64°F), feels like 17°C, \
             wind 12 km/h SW, humidity 67%"
        );
        assert_eq!(answer.url.as_deref(), Some("https://wttr.in/berlin"));
        assert_eq!(answer.engine, NAME);
    }

    #[test]
    fn malformed_payload_is_a_parse_error() {
        let engine = Weather::new();
        let resp = EngineResponse {
            status: 200,
            url: "https://wttr.in/x?format=j1".to_string(),
            body: b"not json".to_vec(),
            ..EngineResponse::default()
        };
        assert!(engine.response(&resp).is_err());
    }
}
