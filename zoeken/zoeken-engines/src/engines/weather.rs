//! Weather engine backed by wttr.in.
//!
//! Fires on keyword-first/last (`weather berlin` / `berlin weather`) and
//! natural-language forms (`what is the weather in miami`).

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{Answer, InteractiveAnswer, Result_};

/// Engine name / identifier.
pub const NAME: &str = "weather";

const BASE_URL: &str = "https://wttr.in";

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

fn weather_location(query: &str) -> Option<String> {
    zoeken_query::weather_request_location(query)
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
        let labeled = |obj: &serde_json::Value, key: &str| -> String {
            obj.get(key)
                .and_then(|value| value.as_array())
                .and_then(|items| items.first())
                .map(|value| str_of(value, "value"))
                .unwrap_or_default()
        };
        let place = area
            .map(|a| {
                let name = labeled(a, "areaName");
                let region = labeled(a, "region");
                let country = labeled(a, "country");
                let mut parts = Vec::new();
                if !name.is_empty() {
                    parts.push(name);
                }
                if !region.is_empty() && parts.last().is_none_or(|part| part != &region) {
                    parts.push(region);
                }
                if !country.is_empty() && parts.last().is_none_or(|part| part != &country) {
                    parts.push(country);
                }
                parts.join(", ")
            })
            .unwrap_or_default();
        let observation_time = str_of(current, "localObsDateTime");

        let temp_c = str_of(current, "temp_C");
        let temp_f = str_of(current, "temp_F");
        let feels_c = str_of(current, "FeelsLikeC");
        let wind = str_of(current, "windspeedKmph");
        let wind_dir = str_of(current, "winddir16Point");
        let humidity = str_of(current, "humidity");

        let forecast = value
            .get("weather")
            .and_then(|days| days.as_array())
            .and_then(|days| days.get(1));
        let forecast_date = forecast.map(|day| str_of(day, "date")).unwrap_or_default();
        let forecast_temp_c = forecast
            .map(|day| str_of(day, "avgtempC"))
            .unwrap_or_default();
        let forecast_temp_f = forecast
            .map(|day| str_of(day, "avgtempF"))
            .unwrap_or_default();
        let forecast_summary = forecast
            .and_then(|day| day.get("hourly"))
            .and_then(|hours| hours.as_array())
            .and_then(|hours| hours.get(4).or_else(|| hours.first()))
            .and_then(|hour| hour.get("weatherDesc"))
            .and_then(|d| d.as_array())
            .and_then(|a| a.first())
            .map(|v| str_of(v, "value"))
            .unwrap_or_default();

        if temp_c.is_empty() && desc.is_empty() && forecast_temp_c.is_empty() {
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
        let mut answer_text = format!("{prefix}: current {}", parts.join(", "));
        if !observation_time.is_empty() {
            answer_text.push_str(&format!(". Observed {observation_time}"));
        }
        if forecast_temp_c.is_empty() && forecast_summary.is_empty() {
            answer_text.push_str(". Tomorrow’s forecast is not in this response");
        } else {
            let when = if forecast_date.is_empty() {
                String::new()
            } else {
                format!(" {forecast_date}")
            };
            let summary = if forecast_summary.is_empty() {
                String::new()
            } else {
                format!("{forecast_summary}, ")
            };
            answer_text.push_str(&format!(
                ". Forecast{when}: {summary}{forecast_temp_c}°C ({forecast_temp_f}°F)"
            ));
        }
        res.add(Result_::Answer(Answer {
            answer: answer_text,
            url: resp
                .url
                .split_once("?")
                .map(|(base, _)| base.to_string())
                .or_else(|| Some(resp.url.clone())),
            engine: NAME.to_string(),
            interactive: Some(InteractiveAnswer::Weather {
                place: place.clone(),
                description: desc,
                temp_c,
                temp_f,
                feels_c,
                wind_kmph: wind,
                wind_dir,
                humidity,
                observation_time,
                forecast_date,
                forecast_summary,
                forecast_temp_c,
                forecast_temp_f,
            }),
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
        assert_eq!(
            weather_location("weather tomorrow in Paris, Texas"),
            Some("Paris, Texas".to_string())
        );
        assert_eq!(weather_location("weather tomorrow"), None);
        assert_eq!(weather_location("weather"), None, "no location given");
        assert_eq!(weather_location("rust programming"), None);
        assert_eq!(weather_location(""), None);
        assert_eq!(
            weather_location("raining in seattle tonight"),
            Some("seattle".to_string())
        );
        assert_eq!(weather_location("umbrella in the logo"), None);
    }

    #[test]
    fn detects_natural_language_weather_queries() {
        assert_eq!(
            weather_location("what is the weather in miami"),
            Some("miami".to_string())
        );
        assert_eq!(
            weather_location("whats the weather in miami"),
            Some("miami".to_string())
        );
        assert_eq!(
            weather_location("what is the weather in jax fl"),
            Some("jax fl".to_string())
        );
        assert_eq!(
            weather_location("weather in miami"),
            Some("miami".to_string())
        );
        assert_eq!(
            weather_location("forecast for tokyo"),
            Some("tokyo".to_string())
        );
        assert_eq!(
            weather_location("what's the weather at paris?"),
            Some("paris".to_string())
        );
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
            "Weather in Berlin, Germany: current Partly cloudy, 18°C (64°F), feels like 17°C, \
             wind 12 km/h SW, humidity 67%. Tomorrow’s forecast is not in this response"
        );
        assert_eq!(answer.url.as_deref(), Some("https://wttr.in/berlin"));
        assert_eq!(answer.engine, NAME);
        match &answer.interactive {
            Some(InteractiveAnswer::Weather {
                place,
                description,
                temp_c,
                temp_f,
                feels_c,
                wind_kmph,
                wind_dir,
                humidity,
                forecast_date,
                forecast_temp_c,
                ..
            }) => {
                assert!(forecast_date.is_empty());
                assert!(forecast_temp_c.is_empty());
                assert_eq!(place, "Berlin, Germany");
                assert_eq!(description, "Partly cloudy");
                assert_eq!(temp_c, "18");
                assert_eq!(temp_f, "64");
                assert_eq!(feels_c, "17");
                assert_eq!(wind_kmph, "12");
                assert_eq!(wind_dir, "SW");
                assert_eq!(humidity, "67");
            }
            other => panic!("expected Weather interactive, got {other:?}"),
        }
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

    #[test]
    fn region_and_observation_time_come_from_the_provider() {
        let engine = Weather::new();
        let body = r#"{
          "current_condition": [{
            "temp_C": "20", "temp_F": "68", "FeelsLikeC": "20",
            "windspeedKmph": "5", "winddir16Point": "N", "humidity": "40",
            "localObsDateTime": "2026-11-11 02:00 PM",
            "weatherDesc": [{"value": "Sunny"}]
          }],
          "nearest_area": [{
            "areaName": [{"value": "Paris"}],
            "region": [{"value": "Texas"}],
            "country": [{"value": "United States of America"}]
          }],
          "weather": [
            {"date": "2026-11-11", "avgtempC": "20", "avgtempF": "68", "hourly": [{"weatherDesc": [{"value": "Sunny"}]}]},
            {"date": "2026-11-12", "avgtempC": "22", "avgtempF": "72", "hourly": [{"weatherDesc": [{"value": "Clear"}]}]}
          ]
        }"#;
        let resp = EngineResponse {
            status: 200,
            url: "https://wttr.in/Paris%2C%20Texas?format=j1".to_string(),
            body: body.as_bytes().to_vec(),
            ..EngineResponse::default()
        };
        let results = engine.response(&resp).unwrap();
        let answer = &results.answers[0];
        assert!(
            answer
                .answer
                .contains("Paris, Texas, United States of America")
        );
        assert!(answer.answer.contains("Observed 2026-11-11 02:00 PM"));
        assert!(answer.answer.contains("Forecast 2026-11-12"));
        assert!(!answer.answer.contains("Forecast tomorrow"));
        match &answer.interactive {
            Some(InteractiveAnswer::Weather {
                place,
                observation_time,
                forecast_date,
                ..
            }) => {
                assert_eq!(place, "Paris, Texas, United States of America");
                assert_eq!(observation_time, "2026-11-11 02:00 PM");
                assert_eq!(forecast_date, "2026-11-12");
            }
            other => panic!("expected weather, got {other:?}"),
        }
    }

    #[test]
    fn fixture_weather_labels_follow_real_requests() {
        let raw = include_str!("../../../zoeken-search/tests/fixtures/daily_queries.json");
        let rows: Vec<serde_json::Value> = serde_json::from_str(raw).unwrap();
        let engine = Weather::new();
        let mut tp = 0usize;
        let mut fp = 0usize;
        let mut fn_ = 0usize;
        for row in &rows {
            let q = row["q"].as_str().unwrap();
            let wants = row["positive"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str() == Some("weather"));
            let rejects = row["negative"]
                .as_array()
                .unwrap()
                .iter()
                .any(|item| item.as_str() == Some("weather"));
            let mut params = RequestParams::default();
            engine.request(&query(q), &mut params);
            let requested = params
                .url
                .as_deref()
                .is_some_and(|url| url.starts_with("https://wttr.in/"));
            if rejects {
                assert!(!requested, "{q} requested weather");
            }
            match (requested, wants) {
                (true, true) => tp += 1,
                (true, false) => fp += 1,
                (false, true) => fn_ += 1,
                (false, false) => {}
            }
        }
        let precision = if tp + fp == 0 {
            1.0
        } else {
            tp as f64 / (tp + fp) as f64
        };
        let recall = if tp + fn_ == 0 {
            1.0
        } else {
            tp as f64 / (tp + fn_) as f64
        };
        eprintln!(
            "weather_dispatch tp={tp} fp={fp} fn={fn_} precision={precision:.3} recall={recall:.3}"
        );
        assert_eq!(fp, 0, "a non-weather fixture row requested wttr.in");
        assert!(precision >= 0.9, "weather dispatch precision {precision}");
        assert!(recall >= 0.9, "weather dispatch recall {recall}");
    }
}
