//! Additional SearXNG autocomplete backends (JSON / light XML).

use std::sync::Arc;

use rand::Rng;
use zoeken_network::{DEFAULT_NETWORK, NetworkManager, NetworkRequest};

use crate::{
    AutocompleteBackend, BackendError, SuggestFuture, encode_query, parse_opensearch_suggestions,
};

async fn fetch_json(
    network: &NetworkManager,
    network_name: &str,
    req: NetworkRequest,
) -> Result<serde_json::Value, BackendError> {
    let resp = network
        .request(network_name, req)
        .await
        .map_err(|e| BackendError::Request(e.to_string()))?;
    let text = resp
        .text()
        .await
        .map_err(|e| BackendError::Response(e.to_string()))?;
    serde_json::from_str(&text).map_err(|e| BackendError::Response(e.to_string()))
}

async fn fetch_text(
    network: &NetworkManager,
    network_name: &str,
    req: NetworkRequest,
) -> Result<String, BackendError> {
    let resp = network
        .request(network_name, req)
        .await
        .map_err(|e| BackendError::Request(e.to_string()))?;
    resp.text()
        .await
        .map_err(|e| BackendError::Response(e.to_string()))
}

macro_rules! simple_backend {
    ($name:ident, $str_name:literal, $build:expr, $parse:expr) => {
        pub struct $name {
            network: Arc<NetworkManager>,
            network_name: String,
        }

        impl $name {
            #[must_use]
            pub fn new(network: Arc<NetworkManager>) -> Self {
                Self {
                    network,
                    network_name: DEFAULT_NETWORK.to_string(),
                }
            }

            fn build_url(query: &str, locale: &str) -> String {
                let _ = locale;
                ($build)(query, locale)
            }
        }

        impl AutocompleteBackend for $name {
            fn name(&self) -> &str {
                $str_name
            }

            fn suggest<'a>(&'a self, query: &'a str, locale: &'a str) -> SuggestFuture<'a> {
                Box::pin(async move {
                    let url = Self::build_url(query, locale);
                    let value =
                        fetch_json(&self.network, &self.network_name, NetworkRequest::get(url))
                            .await?;
                    Ok(($parse)(&value))
                })
            }
        }
    };
}

/// Parse Baidu `sugrec` payload: `g[].q`.
#[must_use]
pub fn parse_baidu_suggestions(value: &serde_json::Value) -> Vec<String> {
    value
        .get("g")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("q").and_then(serde_json::Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

simple_backend!(
    BaiduBackend,
    "baidu",
    |query: &str, _locale: &str| {
        format!(
            "https://www.baidu.com/sugrec?ie=utf-8&json=1&prod=pc&wd={}",
            encode_query(query)
        )
    },
    parse_baidu_suggestions
);

/// Parse Bing AS Suggestions: `s[].q` with PUA highlight chars stripped.
#[must_use]
pub fn parse_bing_suggestions(value: &serde_json::Value) -> Vec<String> {
    value
        .get("s")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("q").and_then(serde_json::Value::as_str))
                .map(|s| s.replace(['\u{e000}', '\u{e001}'], ""))
                .collect()
        })
        .unwrap_or_default()
}

pub struct BingBackend {
    network: Arc<NetworkManager>,
    network_name: String,
}

impl BingBackend {
    #[must_use]
    pub fn new(network: Arc<NetworkManager>) -> Self {
        Self {
            network,
            network_name: DEFAULT_NETWORK.to_string(),
        }
    }

    fn build_url(query: &str) -> String {
        let cvid: String = (0..32)
            .map(|_| {
                const CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789";
                let i = rand::rng().random_range(0..CHARS.len());
                CHARS[i] as char
            })
            .collect();
        format!(
            "https://www.bing.com/AS/Suggestions?qry={}&csr=1&cvid={}",
            encode_query(query),
            cvid
        )
    }
}

impl AutocompleteBackend for BingBackend {
    fn name(&self) -> &str {
        "bing"
    }

    fn suggest<'a>(&'a self, query: &'a str, _locale: &'a str) -> SuggestFuture<'a> {
        Box::pin(async move {
            let url = Self::build_url(query);
            let value =
                fetch_json(&self.network, &self.network_name, NetworkRequest::get(url)).await?;
            Ok(parse_bing_suggestions(&value))
        })
    }
}

/// Parse DBpedia KeywordSearch XML for `Label` text nodes.
#[must_use]
pub fn parse_dbpedia_labels(xml: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut rest = xml;
    while let Some(start) = rest.find("<Label>") {
        let after = &rest[start + "<Label>".len()..];
        let Some(end) = after.find("</Label>") else {
            break;
        };
        let label = after[..end].trim();
        if !label.is_empty() {
            out.push(label.to_string());
        }
        rest = &after[end + "</Label>".len()..];
    }
    out
}

pub struct DbpediaBackend {
    network: Arc<NetworkManager>,
    network_name: String,
}

impl DbpediaBackend {
    #[must_use]
    pub fn new(network: Arc<NetworkManager>) -> Self {
        Self {
            network,
            network_name: DEFAULT_NETWORK.to_string(),
        }
    }

    fn build_url(query: &str) -> String {
        format!(
            "https://lookup.dbpedia.org/api/search.asmx/KeywordSearch?QueryString={}",
            encode_query(query)
        )
    }
}

impl AutocompleteBackend for DbpediaBackend {
    fn name(&self) -> &str {
        "dbpedia"
    }

    fn suggest<'a>(&'a self, query: &'a str, _locale: &'a str) -> SuggestFuture<'a> {
        Box::pin(async move {
            let url = Self::build_url(query);
            let text =
                fetch_text(&self.network, &self.network_name, NetworkRequest::get(url)).await?;
            Ok(parse_dbpedia_labels(&text))
        })
    }
}

/// Mwmbl complete API; drop `go:` / `search:` direct-url rows.
#[must_use]
pub fn parse_mwmbl_suggestions(value: &serde_json::Value) -> Vec<String> {
    parse_opensearch_suggestions(value)
        .into_iter()
        .filter(|s| !s.starts_with("go: ") && !s.starts_with("search: "))
        .collect()
}

simple_backend!(
    MwmblBackend,
    "mwmbl",
    |query: &str, _locale: &str| {
        format!(
            "https://api.mwmbl.org/search/complete?q={}",
            encode_query(query)
        )
    },
    parse_mwmbl_suggestions
);

/// Naver: `items[0][][0]`.
#[must_use]
pub fn parse_naver_suggestions(value: &serde_json::Value) -> Vec<String> {
    value
        .get("items")
        .and_then(serde_json::Value::as_array)
        .and_then(|items| items.first())
        .and_then(serde_json::Value::as_array)
        .map(|rows| {
            rows.iter()
                .filter_map(|row| {
                    row.as_array()
                        .and_then(|r| r.first())
                        .and_then(serde_json::Value::as_str)
                        .map(str::to_string)
                })
                .collect()
        })
        .unwrap_or_default()
}

simple_backend!(
    NaverBackend,
    "naver",
    |query: &str, _locale: &str| {
        format!(
            "https://ac.search.naver.com/nx/ac?q={}&r_format=json&st=0",
            encode_query(query)
        )
    },
    parse_naver_suggestions
);

simple_backend!(
    PrivacywallBackend,
    "privacywall",
    |query: &str, locale: &str| {
        let mut url = format!(
            "https://www.privacywall.org/search/secure/suggestions.php?q={}",
            encode_query(query)
        );
        if let Some(cc) = locale.split('-').nth(1).filter(|s| !s.is_empty()) {
            url.push_str("&cc=");
            url.push_str(&encode_query(cc));
        }
        url
    },
    parse_opensearch_suggestions
);

/// 360 Search: `result[].word`.
#[must_use]
pub fn parse_360search_suggestions(value: &serde_json::Value) -> Vec<String> {
    value
        .get("result")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("word").and_then(serde_json::Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

simple_backend!(
    Search360Backend,
    "360search",
    |query: &str, _locale: &str| {
        format!(
            "https://sug.so.360.cn/suggest?format=json&word={}",
            encode_query(query)
        )
    },
    parse_360search_suggestions
);

/// Quark: `r[].w`.
#[must_use]
pub fn parse_quark_suggestions(value: &serde_json::Value) -> Vec<String> {
    value
        .get("r")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("w").and_then(serde_json::Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

simple_backend!(
    QuarkBackend,
    "quark",
    |query: &str, _locale: &str| { format!("https://sugs.m.sm.cn/web?q={}", encode_query(query)) },
    parse_quark_suggestions
);

/// Qwant v3 suggest: `data.items[].value` when `status == success`.
#[must_use]
pub fn parse_qwant_suggestions(value: &serde_json::Value) -> Vec<String> {
    if value.get("status").and_then(serde_json::Value::as_str) != Some("success") {
        return Vec::new();
    }
    value
        .pointer("/data/items")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.get("value").and_then(serde_json::Value::as_str))
                .map(str::to_string)
                .collect()
        })
        .unwrap_or_default()
}

fn qwant_locale(locale: &str) -> String {
    let trimmed = locale.trim();
    if trimmed.is_empty() {
        return "en_US".to_string();
    }
    trimmed.replace('-', "_")
}

simple_backend!(
    QwantBackend,
    "qwant",
    |query: &str, locale: &str| {
        format!(
            "https://api.qwant.com/v3/suggest?q={}&locale={}&version=2",
            encode_query(query),
            encode_query(&qwant_locale(locale))
        )
    },
    parse_qwant_suggestions
);

/// Seznam json-2: join `text[].text` for `ItemType.TEXT` rows.
#[must_use]
pub fn parse_seznam_suggestions(value: &serde_json::Value) -> Vec<String> {
    value
        .get("result")
        .and_then(serde_json::Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter(|item| {
                    item.get("itemType").and_then(serde_json::Value::as_str)
                        == Some("ItemType.TEXT")
                })
                .filter_map(|item| {
                    let parts = item.get("text")?.as_array()?;
                    let joined: String = parts
                        .iter()
                        .filter_map(|p| p.get("text").and_then(serde_json::Value::as_str))
                        .collect();
                    if joined.is_empty() {
                        None
                    } else {
                        Some(joined)
                    }
                })
                .collect()
        })
        .unwrap_or_default()
}

simple_backend!(
    SeznamBackend,
    "seznam",
    |query: &str, _locale: &str| {
        format!(
            "https://suggest.seznam.cz/fulltext/cs?phrase={}&cursorPosition={}&format=json-2&highlight=1&count=6",
            encode_query(query),
            query.len()
        )
    },
    parse_seznam_suggestions
);

/// Sogou: JSON array embedded in response text; take second element.
#[must_use]
pub fn parse_sogou_suggestions_text(text: &str) -> Vec<String> {
    let Some(start) = text.find('[') else {
        return Vec::new();
    };
    let Some(end) = text.rfind(']') else {
        return Vec::new();
    };
    if end <= start {
        return Vec::new();
    }
    let Ok(value) = serde_json::from_str::<serde_json::Value>(&text[start..=end]) else {
        return Vec::new();
    };
    parse_opensearch_suggestions(&value)
}

pub struct SogouBackend {
    network: Arc<NetworkManager>,
    network_name: String,
}

impl SogouBackend {
    #[must_use]
    pub fn new(network: Arc<NetworkManager>) -> Self {
        Self {
            network,
            network_name: DEFAULT_NETWORK.to_string(),
        }
    }

    fn build_url(query: &str) -> String {
        format!(
            "https://sor.html5.qq.com/api/getsug?m=searxng&key={}",
            encode_query(query)
        )
    }
}

impl AutocompleteBackend for SogouBackend {
    fn name(&self) -> &str {
        "sogou"
    }

    fn suggest<'a>(&'a self, query: &'a str, _locale: &'a str) -> SuggestFuture<'a> {
        Box::pin(async move {
            let url = Self::build_url(query);
            let text =
                fetch_text(&self.network, &self.network_name, NetworkRequest::get(url)).await?;
            Ok(parse_sogou_suggestions_text(&text))
        })
    }
}

/// Swisscows: top-level JSON string array.
#[must_use]
pub fn parse_swisscows_suggestions(value: &serde_json::Value) -> Vec<String> {
    value
        .as_array()
        .map(|items| {
            items
                .iter()
                .filter_map(|item| item.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

simple_backend!(
    SwisscowsBackend,
    "swisscows",
    |query: &str, _locale: &str| {
        format!(
            "https://swisscows.ch/api/suggest?query={}&itemsCount=5",
            encode_query(query)
        )
    },
    parse_swisscows_suggestions
);

simple_backend!(
    YandexBackend,
    "yandex",
    |query: &str, _locale: &str| {
        format!(
            "https://suggest.yandex.com/suggest-ff.cgi?part={}",
            encode_query(query)
        )
    },
    parse_opensearch_suggestions
);

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baidu_url_and_parse() {
        let url = BaiduBackend::build_url("rust", "");
        assert!(url.contains("baidu.com/sugrec"));
        assert!(url.contains("wd=rust"));
        let value = serde_json::json!({"g": [{"q": "rust lang"}, {"q": "rustc"}]});
        assert_eq!(
            parse_baidu_suggestions(&value),
            vec!["rust lang".to_string(), "rustc".to_string()]
        );
    }

    #[test]
    fn bing_parse_strips_pua() {
        let url = BingBackend::build_url("rust");
        assert!(url.contains("bing.com/AS/Suggestions"));
        assert!(url.contains("qry=rust"));
        assert!(url.contains("cvid="));
        let value = serde_json::json!({"s": [{"q": "\u{e000}rust\u{e001} lang"}]});
        assert_eq!(
            parse_bing_suggestions(&value),
            vec!["rust lang".to_string()]
        );
    }

    #[test]
    fn dbpedia_extracts_labels() {
        let url = DbpediaBackend::build_url("rust");
        assert!(url.contains("lookup.dbpedia.org"));
        assert!(url.contains("QueryString=rust"));
        let xml = r#"<Results><Result><Label>Rust</Label></Result><Result><Label>Rust (lang)</Label></Result></Results>"#;
        assert_eq!(
            parse_dbpedia_labels(xml),
            vec!["Rust".to_string(), "Rust (lang)".to_string()]
        );
    }

    #[test]
    fn mwmbl_filters_direct_urls() {
        let url = MwmblBackend::build_url("rust", "");
        assert!(url.contains("api.mwmbl.org/search/complete"));
        assert!(url.contains("q=rust"));
        let value = serde_json::json!(["q", ["rust", "go: example", "search: x", "rustc"]]);
        assert_eq!(
            parse_mwmbl_suggestions(&value),
            vec!["rust".to_string(), "rustc".to_string()]
        );
    }

    #[test]
    fn naver_parse() {
        let url = NaverBackend::build_url("rust", "");
        assert!(url.contains("ac.search.naver.com"));
        assert!(url.contains("q=rust"));
        let value = serde_json::json!({"items": [[["rust"], ["rust lang"]]]});
        assert_eq!(
            parse_naver_suggestions(&value),
            vec!["rust".to_string(), "rust lang".to_string()]
        );
    }

    #[test]
    fn search360_and_quark_parse() {
        let url360 = Search360Backend::build_url("rust", "");
        assert!(url360.contains("sug.so.360.cn/suggest"));
        assert!(url360.contains("word=rust"));
        let url_quark = QuarkBackend::build_url("rust", "");
        assert!(url_quark.contains("sugs.m.sm.cn/web"));
        assert!(url_quark.contains("q=rust"));
        assert_eq!(
            parse_360search_suggestions(&serde_json::json!({"result": [{"word": "a"}]})),
            vec!["a".to_string()]
        );
        assert_eq!(
            parse_quark_suggestions(&serde_json::json!({"r": [{"w": "b"}]})),
            vec!["b".to_string()]
        );
    }

    #[test]
    fn qwant_parse_and_locale() {
        assert_eq!(qwant_locale("fr-FR"), "fr_FR");
        let url = QwantBackend::build_url("rust", "fr-FR");
        assert!(url.contains("api.qwant.com/v3/suggest"));
        assert!(url.contains("q=rust"));
        assert!(url.contains("locale=fr_FR"));
        let value = serde_json::json!({
            "status": "success",
            "data": {"items": [{"value": "rust"}, {"value": "rustc"}]}
        });
        assert_eq!(
            parse_qwant_suggestions(&value),
            vec!["rust".to_string(), "rustc".to_string()]
        );
    }

    #[test]
    fn seznam_joins_text_parts() {
        let url = SeznamBackend::build_url("rust", "");
        assert!(url.contains("suggest.seznam.cz"));
        assert!(url.contains("phrase=rust"));
        let value = serde_json::json!({
            "result": [{
                "itemType": "ItemType.TEXT",
                "text": [{"text": "rust"}, {"text": " lang"}]
            }]
        });
        assert_eq!(
            parse_seznam_suggestions(&value),
            vec!["rust lang".to_string()]
        );
    }

    #[test]
    fn sogou_and_swisscows_and_yandex() {
        let sogou = SogouBackend::build_url("rust");
        assert!(sogou.contains("sor.html5.qq.com/api/getsug"));
        assert!(sogou.contains("key=rust"));
        let swiss = SwisscowsBackend::build_url("rust", "");
        assert!(swiss.contains("swisscows.ch/api/suggest"));
        assert!(swiss.contains("query=rust"));
        let yandex = YandexBackend::build_url("rust", "");
        assert!(yandex.contains("suggest.yandex.com"));
        assert!(yandex.contains("part=rust"));
        assert_eq!(
            parse_sogou_suggestions_text(r#"cb(["q", ["a", "b"]])"#),
            vec!["a".to_string(), "b".to_string()]
        );
        assert_eq!(
            parse_swisscows_suggestions(&serde_json::json!(["x", "y"])),
            vec!["x".to_string(), "y".to_string()]
        );
        assert_eq!(
            parse_opensearch_suggestions(&serde_json::json!(["q", ["z"]])),
            vec!["z".to_string()]
        );
    }

    #[test]
    fn privacywall_url_includes_country() {
        let url = PrivacywallBackend::build_url("rust", "en-US");
        assert!(url.contains("privacywall.org"));
        assert!(url.contains("q=rust"));
        assert!(url.contains("cc=US"));
    }
}
