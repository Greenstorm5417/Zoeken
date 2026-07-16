//! Mastodon search engine.
//!
//! Supports account and hashtag instances through the Mastodon v2 search API.

use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::encode_query;

const BASE_URL: &str = "https://mastodon.social";

const PAGE_SIZE: u32 = 40;

/// The Mastodon search type driving an instance.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MastodonType {
    Accounts,
    Hashtags,
}

impl MastodonType {
    fn as_str(&self) -> &'static str {
        match self {
            MastodonType::Accounts => "accounts",
            MastodonType::Hashtags => "hashtags",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Mastodon {
    meta: EngineMeta,
    mastodon_type: MastodonType,
    base_url: String,
}

impl Mastodon {
    pub fn new(name: &str, shortcut: &str, mastodon_type: MastodonType) -> Self {
        Mastodon {
            meta: EngineMeta {
                name: name.to_string(),
                engine_type: Processor::Online,
                categories: vec!["social media".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: shortcut.to_string(),
                about: About {
                    website: Some("https://joinmastodon.org/".to_string()),
                    wikidata_id: Some("Q27986619".to_string()),
                    official_api_documentation: Some(
                        "https://docs.joinmastodon.org/api/".to_string(),
                    ),
                    use_official_api: true,
                    require_api_key: false,
                    results: "JSON".to_string(),
                },
            },
            mastodon_type,
            base_url: BASE_URL.to_string(),
        }
    }

    pub fn accounts() -> Self {
        Self::new("mastodon users", "mau", MastodonType::Accounts)
    }

    pub fn hashtags() -> Self {
        Self::new("mastodon hashtags", "mah", MastodonType::Hashtags)
    }
}

impl Default for Mastodon {
    fn default() -> Self {
        Self::accounts()
    }
}

/// Sum a numeric field across a tag history array.
fn sum_history(history: &serde_json::Value, field: &str) -> i64 {
    history
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .map(|entry| match entry.get(field) {
                    Some(serde_json::Value::String(s)) => s.parse::<i64>().unwrap_or(0),
                    Some(serde_json::Value::Number(n)) => n.as_i64().unwrap_or(0),
                    _ => 0,
                })
                .sum()
        })
        .unwrap_or(0)
}

impl Engine for Mastodon {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;
        let args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            ("resolve", "false".to_string()),
            ("type", self.mastodon_type.as_str().to_string()),
            ("limit", PAGE_SIZE.to_string()),
        ];
        p.url = Some(format!(
            "{}/api/v2/search?{}",
            self.base_url,
            encode_query(&args)
        ));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();

        let value: serde_json::Value = serde_json::from_slice(&resp.body)
            .map_err(|e| EngineError::Parse(format!("invalid Mastodon JSON: {e}")))?;

        let items = value
            .get(self.mastodon_type.as_str())
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        for item in &items {
            match self.mastodon_type {
                MastodonType::Accounts => {
                    let uri = item.get("uri").and_then(|u| u.as_str()).unwrap_or("");
                    let username = item.get("username").and_then(|u| u.as_str()).unwrap_or("");
                    let followers = item
                        .get("followers_count")
                        .map(|f| match f {
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::String(s) => s.clone(),
                            _ => String::new(),
                        })
                        .unwrap_or_default();
                    let note = item
                        .get("note")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    res.add(Result_::Main(MainResult {
                        url: uri.to_string(),
                        normalized_url: uri.to_string(),
                        title: format!("{username} ({followers} followers)"),
                        content: note,
                        engine: self.meta.name.clone(),
                        ..MainResult::default()
                    }));
                }
                MastodonType::Hashtags => {
                    let url = item.get("url").and_then(|u| u.as_str()).unwrap_or("");
                    let name = item
                        .get("name")
                        .and_then(|n| n.as_str())
                        .unwrap_or("")
                        .to_string();
                    let history = item.get("history").cloned().unwrap_or_default();
                    let uses = sum_history(&history, "uses");
                    let users = sum_history(&history, "accounts");
                    res.add(Result_::Main(MainResult {
                        url: url.to_string(),
                        normalized_url: url.to_string(),
                        title: name,
                        content: format!(
                            "Hashtag has been used {uses} times by {users} different users"
                        ),
                        engine: self.meta.name.clone(),
                        ..MainResult::default()
                    }));
                }
            }
        }

        Ok(res)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::conformance::{Fixture, load_fixtures_for, run_all};
    use std::path::PathBuf;

    fn fixtures_root() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
    }

    fn query(q: &str) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno: 1,
            ..SearchQueryView::default()
        }
    }

    fn main_result(url: &str, title: &str, content: &str, engine: &str) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            engine: engine.to_string(),
            ..MainResult::default()
        })
    }

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: BASE_URL.to_string(),
            body: body.as_bytes().to_vec(),
            ..EngineResponse::default()
        }
    }

    fn prepopulated(q: &SearchQueryView) -> RequestParams {
        RequestParams {
            query: q.query.clone(),
            pageno: q.pageno,
            safesearch: q.safesearch,
            time_range: q.time_range,
            locale_key: q.locale.clone(),
            ..RequestParams::default()
        }
    }

    const ACCOUNTS_JSON: &str = r#"{
      "accounts": [
        {
          "uri": "https://mastodon.social/users/rustlang",
          "username": "rustlang",
          "followers_count": 12345,
          "note": "The Rust programming language.",
          "avatar": "https://files.mastodon.social/a.png",
          "created_at": "2017-04-05T00:00:00.000Z"
        }
      ],
      "hashtags": [],
      "statuses": []
    }"#;

    const HASHTAGS_JSON: &str = r#"{
      "accounts": [],
      "hashtags": [
        {
          "name": "rust",
          "url": "https://mastodon.social/tags/rust",
          "history": [
            {"day": "1700000000", "uses": "10", "accounts": "5"},
            {"day": "1699913600", "uses": "20", "accounts": "8"}
          ]
        }
      ],
      "statuses": []
    }"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join("mastodon");

        let mut accounts = EngineResults::new();
        accounts.add(main_result(
            "https://mastodon.social/users/rustlang",
            "rustlang (12345 followers)",
            "The Rust programming language.",
            "mastodon users",
        ));
        Fixture::capture(
            "mastodon users",
            query("rust"),
            response(200, ACCOUNTS_JSON),
            accounts,
        )
        .with_case("accounts")
        .save(dir.join("accounts.json"))
        .unwrap();

        let q = query("rust");
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{BASE_URL}/api/v2/search?q=rust&resolve=false&type=accounts&limit=40"
        ));
        Fixture::capture(
            "mastodon users",
            q.clone(),
            response(200, r#"{"accounts":[],"hashtags":[],"statuses":[]}"#),
            EngineResults::new(),
        )
        .with_case("accounts-request")
        .with_golden_request(golden)
        .save(dir.join("accounts-request.json"))
        .unwrap();

        let mut hashtags = EngineResults::new();
        hashtags.add(main_result(
            "https://mastodon.social/tags/rust",
            "rust",
            "Hashtag has been used 30 times by 13 different users",
            "mastodon hashtags",
        ));
        Fixture::capture(
            "mastodon hashtags",
            query("rust"),
            response(200, HASHTAGS_JSON),
            hashtags,
        )
        .with_case("hashtags")
        .save(dir.join("hashtags.json"))
        .unwrap();
    }

    #[test]
    fn mastodon_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), "mastodon").expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/mastodon"
        );
        for fixture in &fixtures {
            let engine = if fixture.engine == "mastodon hashtags" {
                Mastodon::hashtags()
            } else {
                Mastodon::accounts()
            };
            if let Err(mismatches) = run_all(&engine, std::slice::from_ref(fixture)) {
                let report = mismatches
                    .iter()
                    .map(|m| m.to_string())
                    .collect::<Vec<_>>()
                    .join("\n");
                panic!("conformance failures:\n{report}");
            }
        }
    }

    #[test]
    fn sums_hashtag_history() {
        let engine = Mastodon::hashtags();
        let res = engine.response(&response(200, HASHTAGS_JSON)).unwrap();
        if let Result_::Main(r) = &res.results[0] {
            assert_eq!(
                r.content,
                "Hashtag has been used 30 times by 13 different users"
            );
        } else {
            panic!("expected main result");
        }
    }
}
