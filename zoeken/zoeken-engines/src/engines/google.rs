//! Google WEB engine: queries HTML endpoint and parses results with bot-protection detection.
//!
//! Builds locale parameters (hl, lr, cr), pagination, time range, and safe-search filters.

use scraper::{ElementRef, Html, Selector};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod,
    LocaleTranslate, Processor, RequestParams, SafeSearch, SearchQueryView, TimeRange,
};
use zoeken_results::{MainResult, Result_, Suggestion};

use super::util::{encode_query, looks_like_bot_wall, percent_decode, text_content_skipping};

/// Engine name / identifier.
pub const NAME: &str = "google";

const BASE_URL: &str = "https://www.google.com";
#[derive(Debug, Clone)]
pub struct Google {
    meta: EngineMeta,
}

impl Google {
    /// Create the engine with its reference metadata.
    pub fn new() -> Self {
        Google {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["general".to_string(), "web".to_string()],
                paging: true,
                max_page: 50,
                time_range_support: true,
                safesearch: true,
                language_support: true,
                weight: 1,
                shortcut: "go".to_string(),
                about: About {
                    website: Some("https://www.google.com".to_string()),
                    wikidata_id: Some("Q9366".to_string()),
                    official_api_documentation: Some(
                        "https://developers.google.com/custom-search/".to_string(),
                    ),
                    use_official_api: false,
                    require_api_key: false,
                    results: "HTML".to_string(),
                },
            },
        }
    }
}

impl Default for Google {
    fn default() -> Self {
        Self::new()
    }
}

fn time_range_qdr(time_range: TimeRange) -> &'static str {
    match time_range {
        TimeRange::Day => "d",
        TimeRange::Week => "w",
        TimeRange::Month => "m",
        TimeRange::Year => "y",
    }
}

fn safe_filter(safesearch: SafeSearch) -> &'static str {
    match safesearch {
        SafeSearch::Off => "off",
        SafeSearch::Moderate => "medium",
        SafeSearch::Strict => "high",
    }
}

/// Compose Google's `hl`/`lr`/`cr` locale parameters, mirroring upstream's
/// `get_google_info`: `hl`/`lr` come from the engine's bundled `language`
/// trait (`lang_xx`), and `cr` (country restriction) is only set when the
/// resolved SearXNG locale itself carries a region.
fn google_locale_params(
    traits: Option<&zoeken_data::EngineTraits>,
    locale: &str,
) -> (String, String, String) {
    let is_all = locale.is_empty() || locale == "all";
    let has_region = locale.split('-').nth(1).is_some_and(|r| !r.is_empty());

    if let Some(traits) = traits {
        let eng_lang = traits
            .get_language(locale, Some("lang_en"))
            .unwrap_or_else(|| "lang_en".to_string());
        let lang_code = eng_lang.rsplit('_').next().unwrap_or(&eng_lang).to_string();

        let hl = lang_code;
        let lr = if is_all { String::new() } else { eng_lang };
        let cr = if has_region {
            traits
                .get_region(locale, traits.all_locale.as_deref())
                .map(|country| format!("country{country}"))
                .unwrap_or_default()
        } else {
            String::new()
        };
        return (hl, lr, cr);
    }

    // Fallback when bundled traits are unavailable: approximate the mapping
    // directly from the locale tag.
    let lang = locale.split(['-', '_']).next().unwrap_or("");
    let lang_code = if lang.is_empty() || is_all {
        "en".to_string()
    } else {
        lang.to_lowercase()
    };

    let hl = lang_code.clone();
    let lr = if is_all {
        String::new()
    } else {
        format!("lang_{lang_code}")
    };

    let cr = if has_region {
        locale
            .split('-')
            .nth(1)
            .map(|region| format!("country{}", region.to_uppercase()))
            .unwrap_or_default()
    } else {
        String::new()
    };

    (hl, lr, cr)
}

fn is_sorry(resp: &EngineResponse) -> bool {
    if resp.url.contains("sorry.google.com") || resp.url.contains("/sorry") {
        return true;
    }
    if resp.status == 302 {
        return true;
    }
    let body = resp.text();
    (body.len() < 2000 && body.contains("/sorry/"))
        || (body.contains("emsg=SG_REL") && body.contains("trouble accessing Google Search"))
}

impl Engine for Google {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;

        let start = (p.pageno.saturating_sub(1)) * 10;
        let (hl, lr, cr) = google_locale_params(zoeken_engine_core::engine_traits(NAME), &q.locale);
        let mut args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            ("hl", hl),
            ("lr", lr),
            ("cr", cr),
            ("ie", "utf8".to_string()),
            ("oe", "utf8".to_string()),
            ("filter", "0".to_string()),
            ("start", start.to_string()),
        ];
        if let Some(time_range) = p.time_range {
            args.push(("tbs", format!("qdr:{}", time_range_qdr(time_range))));
        }
        if q.safesearch != SafeSearch::Off {
            args.push(("safe", safe_filter(q.safesearch).to_string()));
        }

        p.url = Some(format!("{BASE_URL}/search?{}", encode_query(&args)));
        p.headers.insert("Accept".to_string(), "*/*".to_string());
        p.cookies.insert("CONSENT".to_string(), "YES+".to_string());
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        if is_sorry(resp) {
            return Err(EngineError::Captcha(NAME.to_string()));
        }

        let mut res = EngineResults::new();
        let html = resp.text();
        if looks_like_bot_wall(resp.status, &html) {
            return Err(EngineError::Captcha(NAME.to_string()));
        }
        let doc = Html::parse_document(&html);

        let result_sel = Selector::parse("a[data-ved]:not([class])").unwrap();
        let title_sel = Selector::parse("div[style]").unwrap();
        let content_sel = Selector::parse(r#"div[class*="ilUpNd H66NU aSRlid"]"#).unwrap();
        let suggestion_sel = Selector::parse("div.gGQDvd.iIWm4b a").unwrap();

        for anchor in doc.select(&result_sel) {
            let Some(title_tag) = anchor.select(&title_sel).next() else {
                continue;
            };
            let title =
                zoeken_engine_core::normalize_whitespace(&title_tag.text().collect::<String>());

            let Some(raw_url) = anchor.value().attr("href") else {
                continue;
            };
            let url = if let Some(rest) = raw_url.strip_prefix("/url?q=") {
                let target = rest.split("&sa=U").next().unwrap_or(rest);
                percent_decode(target)
            } else {
                raw_url.to_string()
            };

            let grandparent = anchor
                .parent()
                .and_then(|p| p.parent())
                .and_then(ElementRef::wrap);
            let Some(grandparent) = grandparent else {
                continue;
            };
            let Some(content_el) = grandparent.select(&content_sel).next() else {
                continue;
            };
            let content = text_content_skipping(content_el, &[]);

            res.add(Result_::Main(MainResult {
                url: url.clone(),
                normalized_url: url,
                title,
                content,
                engine: NAME.to_string(),
                ..MainResult::default()
            }));
        }

        for sug in doc.select(&suggestion_sel) {
            let suggestion =
                zoeken_engine_core::normalize_whitespace(&sug.text().collect::<String>());
            if !suggestion.is_empty() {
                res.add(Result_::Suggestion(Suggestion {
                    suggestion,
                    engine: NAME.to_string(),
                }));
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

    fn response(status: u16, url: &str, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: url.to_string(),
            body: body.as_bytes().to_vec(),
            ..EngineResponse::default()
        }
    }

    const BASIC_HTML: &str = r#"<!DOCTYPE html>
<html><body>
<div id="search">
  <div class="g">
    <div class="tf">
      <a data-ved="0ah1" href="/url?q=https://www.rust-lang.org/&amp;sa=U&amp;ved=xyz">
        <div style="color:#1a0dab">Rust Programming Language</div>
      </a>
    </div>
    <div class="vt">
      <div class="ilUpNd H66NU aSRlid">A language empowering everyone to build reliable software.<script>var a=1;</script></div>
    </div>
  </div>
  <div class="g">
    <div class="tf">
      <a data-ved="0ah2" href="https://doc.rust-lang.org/book/">
        <div style="color:#1a0dab">The Rust Programming Language - Book</div>
      </a>
    </div>
    <div class="vt">
      <div class="ilUpNd H66NU aSRlid">This book teaches the concepts of Rust.</div>
    </div>
  </div>
</div>
<div class="gGQDvd iIWm4b"><a href="/search?q=rustup">rustup install</a></div>
</body></html>"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(main_result(
            "https://www.rust-lang.org/",
            "Rust Programming Language",
            "A language empowering everyone to build reliable software.",
        ));
        basic.add(main_result(
            "https://doc.rust-lang.org/book/",
            "The Rust Programming Language - Book",
            "This book teaches the concepts of Rust.",
        ));
        basic.add(Result_::Suggestion(Suggestion {
            suggestion: "rustup install".to_string(),
            engine: NAME.to_string(),
        }));
        Fixture::capture(
            NAME,
            query("rust programming", "all", 1),
            response(200, "https://www.google.com/search", BASIC_HTML),
            basic,
        )
        .with_case("basic")
        .save(dir.join("basic.json"))
        .unwrap();

        // sorry page -> captcha (no golden results; response returns an error).
        Fixture::capture(
            NAME,
            query("rust", "all", 1),
            response(302, "https://www.google.com/sorry/index", "<html></html>"),
            EngineResults::new(),
        )
        .with_case("sorry-captcha")
        .save(dir.join("sorry-captcha.json"))
        .unwrap();
    }

    #[test]
    fn google_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = Google::new();
        // The sorry-captcha fixture is expected to error; verify it separately
        // and run response conformance only for the non-error cases.
        for fixture in &fixtures {
            if fixture.case.as_deref() == Some("sorry-captcha") {
                let resp = &fixture.response;
                assert!(matches!(
                    engine.response(resp),
                    Err(EngineError::Captcha(_))
                ));
            }
        }
        let ok_fixtures: Vec<_> = fixtures
            .iter()
            .filter(|f| f.case.as_deref() != Some("sorry-captcha"))
            .cloned()
            .collect();
        if let Err(mismatches) = run_all(&engine, &ok_fixtures) {
            let report = mismatches
                .iter()
                .map(|m| m.to_string())
                .collect::<Vec<_>>()
                .join("\n");
            panic!("conformance failures:\n{report}");
        }
    }

    #[test]
    fn request_builds_search_url_with_start() {
        let engine = Google::new();
        let q = query("rust", "all", 2);
        let mut p = RequestParams {
            query: q.query.clone(),
            pageno: q.pageno,
            ..RequestParams::default()
        };
        engine.request(&q, &mut p);
        let url = p.url.unwrap();
        assert!(url.contains("start=10"));
        assert!(url.contains("filter=0"));
        // For the `all` locale: `get_language`/`get_region` both special-case
        // "all" to the engine's `all_locale` trait (`"ZZ"` for google), so
        // hl="ZZ", with lr/cr left empty (mirrors get_google_info).
        assert!(url.contains("hl=ZZ"));
        assert!(url.contains("lr=&"));
        assert!(url.contains("cr=&"));
        assert_eq!(p.cookies.get("CONSENT").map(String::as_str), Some("YES+"));
    }

    /// The locale params match the shape of the reference `get_google_info`,
    /// cross-checked against the bundled `engine_traits.json` (identical to
    /// upstream's fetched traits).
    #[test]
    fn locale_params_mirror_reference() {
        let traits = zoeken_engine_core::engine_traits(NAME);
        assert!(traits.is_some(), "google traits should be bundled");

        // `all` -> both `get_language`/`get_region` special-case "all" to the
        // engine's `all_locale` trait ("ZZ" for google), so hl="ZZ" with
        // empty lr/cr.
        assert_eq!(
            google_locale_params(traits, "all"),
            ("ZZ".to_string(), String::new(), String::new())
        );
        // Language only -> hl/lr set, no country restriction.
        assert_eq!(
            google_locale_params(traits, "de"),
            ("de".to_string(), "lang_de".to_string(), String::new())
        );
        // Language + region -> hl/lr set and a country restriction.
        assert_eq!(
            google_locale_params(traits, "en-US"),
            (
                "en".to_string(),
                "lang_en".to_string(),
                "countryUS".to_string()
            )
        );
        // Empty locale falls back to English with no restrictions.
        assert_eq!(
            google_locale_params(traits, ""),
            ("en".to_string(), String::new(), String::new())
        );
    }

    #[test]
    fn response_maps_search_gate_to_captcha() {
        let engine = Google::new();
        let body = r#"<html><body><div id="yvlrue" style="display:none">If you're having trouble accessing Google Search, please&nbsp;<a href="/search?q=rust&amp;emsg=SG_REL">click here</a>.</div></body></html>"#;
        assert!(matches!(
            engine.response(&response(200, "https://www.google.com/search", body)),
            Err(EngineError::Captcha(_))
        ));
    }

    /// Without bundled traits (e.g. an unrecognized engine name), the mapping
    /// falls back to the ad hoc locale-tag heuristic.
    #[test]
    fn locale_params_fallback_without_traits() {
        assert_eq!(
            google_locale_params(None, "en-US"),
            (
                "en".to_string(),
                "lang_en".to_string(),
                "countryUS".to_string()
            )
        );
    }
}
