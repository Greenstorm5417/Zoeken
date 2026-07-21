//! Bing WEB engine.
//!
//! Parses Bing search results and resolves `ck/a` redirect URLs.

use scraper::{Html, Selector};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod,
    LocaleTranslate, Processor, RequestParams, SafeSearch, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};

use super::util::{encode_query, text_content_skipping};

pub const NAME: &str = "bing";

const BASE_URL: &str = "https://www.bing.com";

const CK_PREFIX: &str = "https://www.bing.com/ck/a?";

#[derive(Debug, Clone)]
pub struct Bing {
    meta: EngineMeta,
}

impl Bing {
    pub fn new() -> Self {
        Bing {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string(), "web".to_string()],
                paging: false,
                max_page: 0,
                time_range_support: false,
                safesearch: true,
                language_support: true,
                weight: 1,
                shortcut: "bi".to_string(),
                about: About {
                    website: Some("https://www.bing.com".to_string()),
                    wikidata_id: Some("Q182496".to_string()),
                    official_api_documentation: Some(
                        "https://github.com/MicrosoftDocs/bing-docs".to_string(),
                    ),
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for Bing {
    fn default() -> Self {
        Self::new()
    }
}

/// Resolve Bing's `mkt` market code for `locale`, mirroring upstream's
/// `request()`: `traits.get_region(locale, traits.all_locale)`, treating the
/// sentinel value `"clear"` as "no market restriction". Falls back to an
/// ad hoc `<lang>-<TERRITORY>` guess when bundled traits are unavailable.
fn resolve_bing_market(traits: Option<&zoeken_data::EngineTraits>, locale: &str) -> Option<String> {
    if let Some(traits) = traits {
        let region = traits.get_region(locale, traits.all_locale.as_deref())?;
        return (region != "clear").then_some(region);
    }

    let (lang, territory) = locale.split_once('-')?;
    if lang.is_empty() || territory.is_empty() {
        return None;
    }
    Some(format!(
        "{}-{}",
        lang.to_lowercase(),
        territory.to_uppercase()
    ))
}

fn safesearch_adlt(safesearch: SafeSearch) -> &'static str {
    match safesearch {
        SafeSearch::Off => "off",
        SafeSearch::Moderate => "moderate",
        SafeSearch::Strict => "strict",
    }
}

/// Decode a base64url string without padding into bytes, mirroring Python's
/// `base64.urlsafe_b64decode` after the reference re-pads the value. Returns
/// `None` on any invalid input.
fn base64url_decode(input: &str) -> Option<Vec<u8>> {
    fn val(c: u8) -> Option<u8> {
        match c {
            b'A'..=b'Z' => Some(c - b'A'),
            b'a'..=b'z' => Some(c - b'a' + 26),
            b'0'..=b'9' => Some(c - b'0' + 52),
            b'-' => Some(62),
            b'_' => Some(63),
            _ => None,
        }
    }
    let bytes: Vec<u8> = input.bytes().filter(|&b| b != b'=').collect();
    let mut out = Vec::with_capacity(bytes.len() * 3 / 4);
    for chunk in bytes.chunks(4) {
        let mut buf = [0u8; 4];
        let mut n = 0;
        for (i, &b) in chunk.iter().enumerate() {
            buf[i] = val(b)?;
            n += 1;
        }
        if n >= 2 {
            out.push((buf[0] << 2) | (buf[1] >> 4));
        }
        if n >= 3 {
            out.push((buf[1] << 4) | (buf[2] >> 2));
        }
        if n == 4 {
            out.push((buf[2] << 6) | buf[3]);
        }
    }
    Some(out)
}

/// Recover the real destination URL from a Bing `ck/a?...` redirect, or return
/// the href unchanged when it is not such a redirect (mirrors the reference).
fn resolve_ck_redirect(href: &str) -> String {
    if !href.starts_with(CK_PREFIX) {
        return href.to_string();
    }
    // Parse the query string and find the first `u` value.
    let query = href.split_once('?').map(|x| x.1).unwrap_or("");
    for pair in query.split('&') {
        let mut it = pair.splitn(2, '=');
        if it.next() == Some("u") {
            let u_val = it.next().unwrap_or("");
            // Reference: only `a1`-prefixed values carry a base64url payload.
            if let Some(encoded) = u_val.strip_prefix("a1")
                && let Some(bytes) = base64url_decode(encoded)
            {
                return String::from_utf8_lossy(&bytes).into_owned();
            }
            break;
        }
    }
    href.to_string()
}

impl Engine for Bing {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;

        let mut args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            ("adlt", safesearch_adlt(q.safesearch).to_string()),
        ];

        if let Some(mkt) = resolve_bing_market(zoeken_engine_core::engine_traits(NAME), &q.locale) {
            let lang = mkt.split('-').next().unwrap_or(&mkt).to_string();
            p.headers
                .insert("Accept-Language".to_string(), format!("{mkt},{lang};q=0.9"));
            args.push(("mkt", mkt));
        }

        p.url = Some(format!("{BASE_URL}/search?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let html = resp.text();
        let doc = Html::parse_document(&html);

        let challenge_sel = Selector::parse("div.captcha").unwrap();
        if doc.select(&challenge_sel).next().is_some() {
            return Err(EngineError::Captcha(NAME.to_string()));
        }

        let item_sel = Selector::parse("ol#b_results > li.b_algo").unwrap();
        let link_sel = Selector::parse("h2 a").unwrap();
        let p_sel = Selector::parse("p").unwrap();

        for item in doc.select(&item_sel) {
            let Some(link) = item.select(&link_sel).next() else {
                continue;
            };
            let href = link.value().attr("href").unwrap_or("");
            let title = zoeken_engine_core::normalize_whitespace(&link.text().collect::<String>());
            if href.is_empty() || title.is_empty() {
                continue;
            }
            let url = resolve_ck_redirect(href);

            let content = item
                .select(&p_sel)
                .map(|p| text_content_skipping(p, &["algoSlug_icon"]))
                .collect::<Vec<_>>()
                .join(" ");
            let content = zoeken_engine_core::normalize_whitespace(&content);

            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title,
                content,
                engine: NAME.to_string(),
                ..MainResult::default()
            }));
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

    fn query(q: &str, locale: &str, pageno: u32) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno,
            locale: locale.to_string(),
            ..SearchQueryView::default()
        }
    }

    fn main_result(url: &str, title: &str, content: &str) -> Result_ {
        Result_::Main(MainResult {
            url: url.to_string(),
            normalized_url: url.to_string(),
            title: title.to_string(),
            content: content.to_string(),
            engine: NAME.to_string(),
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

    #[test]
    fn response_maps_captcha_challenge_page_to_captcha_error() {
        let engine = Bing::new();
        let resp = response(
            200,
            r#"<html><body><div class="captcha"><div class="captcha_header">One last step</div></div></body></html>"#,
        );
        assert!(matches!(
            engine.response(&resp),
            Err(EngineError::Captcha(_))
        ));
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

    const BASIC_HTML: &str = r#"<!DOCTYPE html>
<html><body>
<ol id="b_results">
  <li class="b_algo">
    <h2><a href="https://www.rust-lang.org/">Rust Programming Language</a></h2>
    <div class="b_caption"><p><span class="algoSlug_icon"></span>A language empowering everyone to build reliable and efficient software.</p></div>
  </li>
  <li class="b_algo">
    <h2><a href="https://www.bing.com/ck/a?!&&p=abc&u=a1aHR0cHM6Ly9kb2MucnVzdC1sYW5nLm9yZy9ib29rLw&ntb=1">The Rust Book</a></h2>
    <div class="b_caption"><p>This book teaches you the concepts of the Rust programming language.</p></div>
  </li>
  <li class="b_ad"><h2><a href="https://ad.example.com/">Ad</a></h2></li>
</ol>
</body></html>"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://www.rust-lang.org/",
            "Rust Programming Language",
            "A language empowering everyone to build reliable and efficient software.",
        ));
        basic.add(main_result(
            "https://doc.rust-lang.org/book/",
            "The Rust Book",
            "This book teaches you the concepts of the Rust programming language.",
        ));
        Fixture::capture(
            NAME,
            query("rust", "all", 1),
            response(200, BASIC_HTML),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        let q = query("rust programming", "en-US", 1);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{BASE_URL}/search?q=rust+programming&adlt=off&mkt=en-us"
        ));
        golden
            .headers
            .insert("Accept-Language".to_string(), "en-us,en;q=0.9".to_string());
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, "<html><body></body></html>"),
            EngineResults::new(),
        )
        .with_case("request-basic")
        .with_golden_request(golden)
        .save(dir.join("request-basic.json"))
        .unwrap();
    }

    #[test]
    fn bing_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Bing::new();
        if let Err(mismatches) = run_all(&engine, &fixtures) {
            let report = mismatches
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            panic!("conformance failures:\n{report}");
        }
    }

    #[test]
    fn decodes_ck_redirect() {
        let href = "https://www.bing.com/ck/a?!&&u=a1aHR0cHM6Ly9leGFtcGxlLmNvbS94&ntb=1";
        assert_eq!(resolve_ck_redirect(href), "https://example.com/x");
    }

    #[test]
    fn plain_href_is_unchanged() {
        assert_eq!(
            resolve_ck_redirect("https://www.rust-lang.org/"),
            "https://www.rust-lang.org/"
        );
    }

    /// Cross-checks `resolve_bing_market` against bundled `engine_traits.json`
    /// (identical to upstream's fetched traits): the `regions` map is
    /// lowercased (e.g. `en-US` -> `en-us`), and the `"clear"` sentinel means
    /// no market restriction.
    #[test]
    fn resolve_bing_market_uses_bundled_traits() {
        let traits = zoeken_engine_core::engine_traits(NAME);
        assert!(traits.is_some(), "bing traits should be bundled");

        assert_eq!(
            resolve_bing_market(traits, "en-US").as_deref(),
            Some("en-us")
        );
        assert_eq!(
            resolve_bing_market(traits, "am-ET").as_deref(),
            Some("am-et")
        );
        // `all_locale` is `"clear"` for bing: unmapped/`"all"` locales carry
        // no market restriction.
        assert_eq!(resolve_bing_market(traits, "all"), None);
    }

    #[test]
    fn resolve_bing_market_fallback_without_traits() {
        assert_eq!(resolve_bing_market(None, "en-US").as_deref(), Some("en-US"));
        assert_eq!(resolve_bing_market(None, "all"), None);
    }
}
