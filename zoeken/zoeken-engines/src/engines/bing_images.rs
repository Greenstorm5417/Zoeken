//! Bing Images engine.
//!
//! Parses the `/images/async` HTML response: each result's `a.iusc` carries a
//! JSON `m` attribute with `purl` / `murl` / `turl`.

use scraper::{Html, Selector};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, HttpMethod,
    LocaleTranslate, Processor, RequestParams, SearchQueryView, TimeRange, normalize_whitespace,
};
use zoeken_results::{Image, Result_};

use super::util::encode_query;

fn el_text(el: scraper::ElementRef<'_>) -> String {
    normalize_whitespace(&el.text().collect::<String>())
}

/// Engine name / identifier.
pub const NAME: &str = "bing_images";

const BASE_URL: &str = "https://www.bing.com";
const PAGE_SIZE: u32 = 35;

#[derive(Debug, Clone)]
pub struct BingImages {
    meta: EngineMeta,
}

impl BingImages {
    pub fn new() -> Self {
        BingImages {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Online,
                categories: vec!["images".to_string(), "web".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: true,
                safesearch: true,
                language_support: true,
                weight: 1,
                shortcut: "bii".to_string(),
                about: About {
                    website: Some("https://www.bing.com/images".to_string()),
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

impl Default for BingImages {
    fn default() -> Self {
        Self::new()
    }
}

fn resolve_bing_market(traits: Option<&zoeken_data::EngineTraits>, locale: &str) -> Option<String> {
    // Traits live under the web `bing` key (shared market tables).
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

fn time_range_minutes(range: TimeRange) -> u32 {
    match range {
        TimeRange::Day => 60 * 24,
        TimeRange::Week => 60 * 24 * 7,
        TimeRange::Month => 60 * 24 * 31,
        TimeRange::Year => 60 * 24 * 365,
    }
}

impl Engine for BingImages {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, p: &mut RequestParams) {
        p.method = HttpMethod::Get;

        let first = (p.pageno.saturating_sub(1)) * PAGE_SIZE + 1;
        let mut args: Vec<(&str, String)> = vec![
            ("q", q.query.clone()),
            ("async", "1".to_string()),
            ("first", first.to_string()),
            ("count", PAGE_SIZE.to_string()),
        ];

        if let Some(mkt) = resolve_bing_market(zoeken_engine_core::engine_traits("bing"), &q.locale)
        {
            let lang = mkt.split('-').next().unwrap_or(&mkt).to_string();
            p.headers
                .insert("Accept-Language".to_string(), format!("{mkt},{lang};q=0.9"));
            args.push(("mkt", mkt));
        }

        if let Some(range) = p.time_range {
            args.push((
                "qft",
                format!("filterui:age-lt{}", time_range_minutes(range)),
            ));
        }

        p.url = Some(format!("{BASE_URL}/images/async?{}", encode_query(&args)));
    }

    fn response(&self, resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let mut res = EngineResults::new();
        let html = resp.text();
        let doc = Html::parse_document(&html);

        let li_sel = Selector::parse("ul.dgControl_list > li, ul[class*='dgControl_list'] > li")
            .map_err(|e| EngineError::Parse(format!("bing_images selector: {e}")))?;
        let iusc_sel = Selector::parse("a.iusc")
            .map_err(|e| EngineError::Parse(format!("bing_images iusc: {e}")))?;
        let title_sel = Selector::parse("div.infnmpt a")
            .map_err(|e| EngineError::Parse(format!("bing_images title: {e}")))?;
        let format_sel = Selector::parse("div.imgpt > div > span")
            .map_err(|e| EngineError::Parse(format!("bing_images format: {e}")))?;
        let source_sel = Selector::parse("div.imgpt div.lnkw a")
            .map_err(|e| EngineError::Parse(format!("bing_images source: {e}")))?;

        for li in doc.select(&li_sel) {
            let Some(iusc) = li.select(&iusc_sel).next() else {
                continue;
            };
            let Some(raw_m) = iusc.value().attr("m") else {
                continue;
            };
            let meta: serde_json::Value = match serde_json::from_str(raw_m) {
                Ok(v) => v,
                Err(_) => continue,
            };

            let img_src = meta
                .get("murl")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let url = meta
                .get("purl")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            if img_src.is_empty() || url.is_empty() {
                continue;
            }

            let thumbnail_src = meta
                .get("turl")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            // ponytail: fall back to full image when Bing omits turl
            let thumbnail_src = if thumbnail_src.is_empty() {
                img_src.clone()
            } else {
                thumbnail_src
            };

            let mut title = li
                .select(&title_sel)
                .next()
                .map(el_text)
                .unwrap_or_default();
            if title.is_empty() {
                title = meta
                    .get("t")
                    .and_then(|v| v.as_str())
                    .unwrap_or("Image")
                    .to_string();
            }

            let content = meta
                .get("desc")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();

            let format_text = li
                .select(&format_sel)
                .next()
                .map(el_text)
                .unwrap_or_default();
            let mut parts = format_text
                .split(" · ")
                .map(str::trim)
                .filter(|s| !s.is_empty());
            let resolution = parts.next().unwrap_or("").to_string();
            let img_format = parts.next().unwrap_or("").to_string();
            // ponytail: synthetic resolution keeps incomplete results displayable
            let resolution = if resolution.is_empty() {
                "unknown".to_string()
            } else {
                resolution
            };

            let source = li
                .select(&source_sel)
                .next()
                .map(el_text)
                .unwrap_or_default();

            res.add(Result_::Image(Image {
                url: url.clone(),
                normalized_url: url,
                title,
                content,
                engine: NAME.to_string(),
                img_src,
                thumbnail_src,
                resolution,
                img_format,
                source,
                ..Image::default()
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

    fn query(q: &str, pageno: u32) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno,
            ..SearchQueryView::default()
        }
    }

    fn response(status: u16, body: &str) -> EngineResponse {
        EngineResponse {
            status,
            url: format!("{BASE_URL}/images/async"),
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

    const BASIC_HTML: &str = r#"
<ul class="dgControl_list">
  <li>
    <a class="iusc" m='{"purl":"https://example.com/photos/1","murl":"https://cdn.example.com/1.jpg","turl":"https://th.bing.com/1.jpg","desc":"A blue cat","t":"Blue Cat"}'></a>
    <div class="infnmpt"><a>Blue Cat</a></div>
    <div class="imgpt">
      <div><span>1920×1080 · jpeg</span></div>
      <div class="lnkw"><a>example.com</a></div>
    </div>
  </li>
  <li>
    <a class="iusc" m='{"purl":"https://example.com/photos/2","murl":"https://cdn.example.com/2.jpg","turl":"","t":"Green Cat"}'></a>
    <div class="infnmpt"><a></a></div>
    <div class="imgpt"><div><span></span></div></div>
  </li>
  <li>
    <a class="iusc" m='{"purl":"","murl":""}'></a>
  </li>
</ul>
"#;

    #[test]
    #[ignore = "regenerates the on-disk conformance fixtures"]
    fn generate_fixtures() {
        let dir = fixtures_root().join(NAME);

        let mut basic = EngineResults::new();
        basic.add(Result_::Image(Image {
            url: "https://example.com/photos/1".to_string(),
            normalized_url: "https://example.com/photos/1".to_string(),
            title: "Blue Cat".to_string(),
            content: "A blue cat".to_string(),
            engine: NAME.to_string(),
            img_src: "https://cdn.example.com/1.jpg".to_string(),
            thumbnail_src: "https://th.bing.com/1.jpg".to_string(),
            resolution: "1920×1080".to_string(),
            img_format: "jpeg".to_string(),
            source: "example.com".to_string(),
            ..Image::default()
        }));
        basic.add(Result_::Image(Image {
            url: "https://example.com/photos/2".to_string(),
            normalized_url: "https://example.com/photos/2".to_string(),
            title: "Green Cat".to_string(),
            content: String::new(),
            engine: NAME.to_string(),
            img_src: "https://cdn.example.com/2.jpg".to_string(),
            thumbnail_src: "https://cdn.example.com/2.jpg".to_string(),
            resolution: "unknown".to_string(),
            ..Image::default()
        }));
        Fixture::capture(NAME, query("cat", 1), response(200, BASIC_HTML), basic)
            .with_case("basic")
            .save(dir.join("basic.json"))
            .unwrap();

        let q = query("blue cat", 2);
        let mut golden = prepopulated(&q);
        golden.method = HttpMethod::Get;
        golden.url = Some(format!(
            "{BASE_URL}/images/async?q=blue+cat&async=1&first=36&count=35"
        ));
        Fixture::capture(
            NAME,
            q.clone(),
            response(200, r#"<ul class="dgControl_list"></ul>"#),
            EngineResults::new(),
        )
        .with_case("request-page2")
        .with_golden_request(golden)
        .save(dir.join("request-page2.json"))
        .unwrap();
    }

    #[test]
    fn bing_images_conformance() {
        let fixtures = load_fixtures_for(fixtures_root(), NAME).expect("load fixtures");
        assert!(
            !fixtures.is_empty(),
            "no fixtures found under fixtures/{NAME}"
        );
        let engine = BingImages::new();
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
    fn skips_entries_without_urls() {
        let engine = BingImages::new();
        let res = engine
            .response(&response(
                200,
                r#"<ul class="dgControl_list"><li><a class="iusc" m='{"purl":"","murl":""}'></a></li></ul>"#,
            ))
            .unwrap();
        assert!(res.is_empty());
    }

    #[test]
    fn builds_paged_request() {
        let engine = BingImages::new();
        let q = query("cats", 2);
        let mut p = prepopulated(&q);
        engine.request(&q, &mut p);
        assert_eq!(
            p.url.as_deref(),
            Some("https://www.bing.com/images/async?q=cats&async=1&first=36&count=35")
        );
    }
}
