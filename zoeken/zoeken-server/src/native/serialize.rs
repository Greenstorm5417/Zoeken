//! Map `ResultContainer` → `NativeSearchResponse` with image/favicon proxy rewriting.

use zoeken_results::{
    Answer, Code, Correction, FileResult, Image, Infobox, InteractiveAnswer, KeyValue, MainResult,
    Paper, Result_, Suggestion,
};
use zoeken_search::{ResultContainer, UnresponsiveCause};

use super::schema::{
    NATIVE_SCHEMA_VERSION, NativeAnswer, NativeCorrection, NativeInfobox, NativeInfoboxAttribute,
    NativeInfoboxImage, NativeInfoboxUrl, NativeInteractiveAnswer, NativeResult,
    NativeSearchResponse, NativeSuggestion, NativeUnresponsiveEngine,
};
use crate::serialize::{ProxySettings, signed_proxy_url};

#[derive(Debug, Clone, Copy, Default)]
pub struct NativeMapContext<'a> {
    /// Search-tab category (e.g. `shopping`) applied to `kind=main` results.
    pub category: &'a str,
}

impl NativeSearchResponse {
    pub fn from_container(
        query: &str,
        container: &ResultContainer,
        proxies: ProxySettings<'_>,
        ctx: NativeMapContext<'_>,
    ) -> Self {
        let mut response = Self {
            schema_version: NATIVE_SCHEMA_VERSION,
            query: query.to_string(),
            number_of_results: container.number_of_results as u64,
            results: container
                .results
                .iter()
                .filter_map(|result| map_result(result, ctx))
                .collect(),
            answers: container.answers.iter().map(map_answer).collect(),
            corrections: container.corrections.iter().map(map_correction).collect(),
            suggestions: container.suggestions.iter().map(map_suggestion).collect(),
            infoboxes: container.infoboxes.iter().map(map_infobox).collect(),
            unresponsive_engines: container
                .unresponsive_engines
                .iter()
                .map(|engine| NativeUnresponsiveEngine {
                    engine: engine.engine.clone(),
                    cause: translated_cause(&engine.cause).to_string(),
                })
                .collect(),
            engine_data: container.engine_data.clone(),
        };
        apply_proxies(&mut response, proxies);
        response
    }
}

fn map_result(result: &Result_, ctx: NativeMapContext<'_>) -> Option<NativeResult> {
    match result {
        Result_::Main(r) => Some(map_main(r, ctx.category)),
        Result_::Image(r) => Some(map_image(r)),
        Result_::Paper(r) => Some(map_paper(r)),
        Result_::Code(r) => Some(map_code(r)),
        Result_::File(r) => Some(map_file(r)),
        Result_::KeyValue(r) => Some(map_key_value(r)),
        // Answers / suggestions / corrections / infoboxes live in dedicated arrays.
        Result_::Answer(_)
        | Result_::Suggestion(_)
        | Result_::Correction(_)
        | Result_::Infobox(_) => None,
    }
}

fn map_main(result: &MainResult, category: &str) -> NativeResult {
    NativeResult::Main {
        url: result.url.clone(),
        title: result.title.clone(),
        content: result.content.clone(),
        engine: result.engine.clone(),
        engines: engines_list(&result.engine, &result.engines),
        category: category.to_string(),
        score: result.score,
        positions: positions_u32(&result.positions),
        priority: result.priority.clone(),
        thumbnail: result.thumbnail.clone(),
        iframe_src: result.iframe_src.clone(),
        favicon: String::new(),
        pretty_url: pretty_url(&result.url),
        published_date: result.published_date.clone(),
        length: result.length.clone(),
        author: result.author.clone(),
    }
}

fn map_image(result: &Image) -> NativeResult {
    NativeResult::Image {
        url: result.url.clone(),
        title: result.title.clone(),
        content: result.content.clone(),
        engine: result.engine.clone(),
        engines: engines_list(&result.engine, &[]),
        score: result.score,
        positions: positions_u32(&result.positions),
        priority: result.priority.clone(),
        img_src: result.img_src.clone(),
        thumbnail_src: result.thumbnail_src.clone(),
        resolution: result.resolution.clone(),
        img_format: result.img_format.clone(),
        source: result.source.clone(),
        filesize: result.filesize.clone(),
    }
}

fn map_paper(result: &Paper) -> NativeResult {
    NativeResult::Paper {
        url: result.url.clone(),
        title: result.title.clone(),
        content: result.content.clone(),
        engine: result.engine.clone(),
        engines: engines_list(&result.engine, &[]),
        score: result.score,
        positions: positions_u32(&result.positions),
        priority: result.priority.clone(),
        authors: result.authors.clone(),
        doi: result.doi.clone(),
        journal: result.journal.clone(),
        published_date: result.published_date.clone(),
        publisher: result.publisher.clone(),
        editor: result.editor.clone(),
        volume: result.volume.clone(),
        pages: result.pages.clone(),
        number: result.number.clone(),
        type_: result.type_.clone(),
        tags: result.tags.clone(),
        issn: result.issn.clone(),
        isbn: result.isbn.clone(),
        pdf_url: result.pdf_url.clone(),
        html_url: result.html_url.clone(),
        comments: result.comments.clone(),
    }
}

fn map_code(result: &Code) -> NativeResult {
    NativeResult::Code {
        url: result.url.clone(),
        title: result.title.clone(),
        content: result.content.clone(),
        engine: result.engine.clone(),
        engines: engines_list(&result.engine, &[]),
        score: result.score,
        positions: positions_u32(&result.positions),
        priority: result.priority.clone(),
        repository: result.repository.clone().unwrap_or_default(),
        filename: result.filename.clone().unwrap_or_default(),
        code_language: result.code_language.clone(),
        codelines: result
            .codelines
            .iter()
            .map(|(n, line)| (*n as u32, line.clone()))
            .collect(),
        hl_lines: result.hl_lines.iter().map(|n| *n as u32).collect(),
    }
}

fn map_file(result: &FileResult) -> NativeResult {
    NativeResult::File {
        url: result.url.clone(),
        title: result.title.clone(),
        content: result.content.clone(),
        engine: result.engine.clone(),
        engines: engines_list(&result.engine, &[]),
        score: result.score,
        positions: positions_u32(&result.positions),
        priority: result.priority.clone(),
        filename: result.filename.clone(),
        size: result.size.clone(),
        time: result.time.clone(),
        mimetype: result.mimetype.clone(),
        abstract_: result.abstract_.clone(),
        author: result.author.clone(),
        embedded: result.embedded.clone(),
        mtype: result.mtype.clone(),
        subtype: result.subtype.clone(),
        filesize: result
            .filesize
            .clone()
            .unwrap_or_else(|| result.size.clone()),
        seed: result.seed,
        leech: result.leech,
        magnetlink: result.magnetlink.clone().unwrap_or_default(),
    }
}

fn map_key_value(result: &KeyValue) -> NativeResult {
    NativeResult::KeyValue {
        url: result.url.clone(),
        title: result.title.clone(),
        content: result.content.clone(),
        engine: result.engine.clone(),
        engines: engines_list(&result.engine, &[]),
        score: result.score,
        positions: positions_u32(&result.positions),
        priority: result.priority.clone(),
        caption: result.caption.clone(),
        key_title: result.key_title.clone(),
        value_title: result.value_title.clone(),
        kvmap: result.kvmap.clone(),
    }
}

fn map_answer(answer: &Answer) -> NativeAnswer {
    NativeAnswer {
        answer: answer.answer.clone(),
        url: answer.url.clone(),
        engine: answer.engine.clone(),
        interactive: answer.interactive.as_ref().map(map_interactive),
    }
}

fn map_interactive(interactive: &InteractiveAnswer) -> NativeInteractiveAnswer {
    match interactive {
        InteractiveAnswer::Unit {
            amount,
            from,
            to,
            result,
            dimension,
        } => NativeInteractiveAnswer::Unit {
            amount: *amount,
            from: from.clone(),
            to: to.clone(),
            result: *result,
            dimension: dimension.clone(),
        },
        InteractiveAnswer::Currency {
            amount,
            from,
            to,
            result,
            rate,
        } => NativeInteractiveAnswer::Currency {
            amount: *amount,
            from: from.clone(),
            to: to.clone(),
            result: *result,
            rate: *rate,
        },
        InteractiveAnswer::Calculator { expression, result } => {
            NativeInteractiveAnswer::Calculator {
                expression: expression.clone(),
                result: *result,
            }
        }
        InteractiveAnswer::Weather {
            place,
            description,
            temp_c,
            temp_f,
            feels_c,
            wind_kmph,
            wind_dir,
            humidity,
        } => NativeInteractiveAnswer::Weather {
            place: place.clone(),
            description: description.clone(),
            temp_c: temp_c.clone(),
            temp_f: temp_f.clone(),
            feels_c: feels_c.clone(),
            wind_kmph: wind_kmph.clone(),
            wind_dir: wind_dir.clone(),
            humidity: humidity.clone(),
        },
        InteractiveAnswer::SelfInfo { kind, value } => NativeInteractiveAnswer::SelfInfo {
            kind: kind.clone(),
            value: value.clone(),
        },
        InteractiveAnswer::Crypto {
            mode,
            algorithm,
            input,
        } => NativeInteractiveAnswer::Crypto {
            mode: mode.clone(),
            algorithm: algorithm.clone(),
            input: input.clone(),
        },
        InteractiveAnswer::Translate {
            source,
            target_lang,
            translated,
        } => NativeInteractiveAnswer::Translate {
            source: source.clone(),
            target_lang: target_lang.clone(),
            translated: translated.clone(),
        },
        InteractiveAnswer::Dictionary { term, definitions } => {
            NativeInteractiveAnswer::Dictionary {
                term: term.clone(),
                definitions: definitions.clone(),
            }
        }
        InteractiveAnswer::Wikipedia {
            title,
            extract,
            description,
            img_src,
            url,
            wikidata_id,
            attributes,
            related_topics,
        } => NativeInteractiveAnswer::Wikipedia {
            title: title.clone(),
            extract: extract.clone(),
            description: description.clone(),
            img_src: img_src.clone(),
            url: url.clone(),
            wikidata_id: wikidata_id.clone(),
            attributes: attributes
                .iter()
                .map(|attr| NativeInfoboxAttribute {
                    label: attr.label.clone(),
                    value: attr.value.clone(),
                    image: attr.image.as_ref().map(|img| NativeInfoboxImage {
                        src: img.src.clone(),
                        alt: img.alt.clone(),
                    }),
                })
                .collect(),
            related_topics: related_topics.clone(),
        },
    }
}

fn map_correction(correction: &Correction) -> NativeCorrection {
    NativeCorrection {
        correction: correction.correction.clone(),
        url: correction.url.clone(),
        engine: correction.engine.clone(),
    }
}

fn map_suggestion(suggestion: &Suggestion) -> NativeSuggestion {
    NativeSuggestion {
        suggestion: suggestion.suggestion.clone(),
        engine: suggestion.engine.clone(),
    }
}

fn map_infobox(infobox: &Infobox) -> NativeInfobox {
    NativeInfobox {
        infobox: infobox.infobox.clone(),
        id: infobox.id.clone(),
        content: infobox.content.clone(),
        img_src: infobox.img_src.clone(),
        urls: infobox
            .urls
            .iter()
            .map(|url| NativeInfoboxUrl {
                title: url.title.clone(),
                url: url.url.clone(),
            })
            .collect(),
        attributes: infobox
            .attributes
            .iter()
            .map(|attr| NativeInfoboxAttribute {
                label: attr.label.clone(),
                value: attr.value.clone(),
                image: attr.image.as_ref().map(|image| NativeInfoboxImage {
                    src: image.src.clone(),
                    alt: image.alt.clone(),
                }),
            })
            .collect(),
        related_topics: infobox.related_topics.clone(),
        engine: infobox.engine.clone(),
    }
}

fn apply_proxies(response: &mut NativeSearchResponse, proxies: ProxySettings<'_>) {
    for result in &mut response.results {
        match result {
            NativeResult::Main {
                url,
                thumbnail,
                favicon,
                ..
            } => {
                if proxies.favicon_proxy {
                    *favicon = favicon_proxy_url(url, proxies.secret_key).unwrap_or_default();
                }
                if proxies.image_proxy {
                    *thumbnail = rewrite_image_url(thumbnail, proxies.secret_key);
                }
            }
            NativeResult::Image {
                img_src,
                thumbnail_src,
                ..
            } if proxies.image_proxy => {
                *img_src = rewrite_image_url(img_src, proxies.secret_key);
                *thumbnail_src = rewrite_image_url(thumbnail_src, proxies.secret_key);
            }
            _ => {}
        }
    }
    if proxies.image_proxy {
        for infobox in &mut response.infoboxes {
            if let Some(original) = infobox.img_src.as_deref() {
                let rewritten = rewrite_image_url(original, proxies.secret_key);
                infobox.img_src = if rewritten.is_empty() {
                    None
                } else {
                    Some(rewritten)
                };
            }
        }
    }
}

fn rewrite_image_url(original: &str, secret_key: &str) -> String {
    if original.is_empty() {
        return String::new();
    }
    if zoeken_favicons::validate_proxy_url(original).is_ok() {
        signed_proxy_url("/image_proxy", "url", original, secret_key)
    } else {
        String::new()
    }
}

fn favicon_proxy_url(page_url: &str, secret_key: &str) -> Option<String> {
    let authority = url::Url::parse(page_url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))?;
    if zoeken_favicons::validate_proxy_authority(&authority).is_err() {
        return None;
    }
    Some(signed_proxy_url(
        "/favicon_proxy",
        "authority",
        &authority,
        secret_key,
    ))
}

fn engines_list(primary: &str, engines: &[String]) -> Vec<String> {
    let mut names: Vec<String> = if engines.is_empty() {
        if primary.is_empty() {
            Vec::new()
        } else {
            vec![primary.to_string()]
        }
    } else {
        engines.to_vec()
    };
    names.sort();
    names.dedup();
    names
}

fn positions_u32(positions: &[usize]) -> Vec<u32> {
    positions.iter().map(|n| *n as u32).collect()
}

fn pretty_url(raw: &str) -> String {
    url::Url::parse(raw)
        .ok()
        .and_then(|url| url.host_str().map(str::to_string))
        .unwrap_or_default()
}

fn translated_cause(cause: &UnresponsiveCause) -> &'static str {
    match cause {
        UnresponsiveCause::Error { category, .. } => category.user_label(),
        UnresponsiveCause::Timeout | UnresponsiveCause::DeadlineExceeded => "timeout",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::serialize::ProxySettings;
    use zoeken_results::{
        Answer, Code, Correction, FileResult, Image, Infobox, InteractiveAnswer, KeyValue,
        MainResult, Paper, Result_, Suggestion,
    };
    use zoeken_search::ResultContainer;

    fn sample_container() -> ResultContainer {
        ResultContainer {
            results: vec![
                Result_::Main(MainResult {
                    url: "https://www.rust-lang.org/".into(),
                    title: "Rust".into(),
                    content: "A language".into(),
                    engine: "duckduckgo".into(),
                    engines: vec!["duckduckgo".into(), "brave".into()],
                    score: 1.2,
                    positions: vec![1, 3],
                    thumbnail: "https://cdn.example/thumb.png".into(),
                    iframe_src: "".into(),
                    ..MainResult::default()
                }),
                Result_::Image(Image {
                    url: "https://example.test/photo".into(),
                    title: "Crab".into(),
                    engine: "bing_images".into(),
                    img_src: "https://cdn.example/full.jpg".into(),
                    thumbnail_src: "https://cdn.example/thumb.jpg".into(),
                    resolution: "1920x1080".into(),
                    img_format: "jpeg".into(),
                    source: "example.test".into(),
                    filesize: "240 KB".into(),
                    score: 0.9,
                    positions: vec![1],
                    ..Image::default()
                }),
                Result_::Paper(Paper {
                    url: "https://arxiv.org/abs/1234.5678".into(),
                    title: "Attention Is All You Need".into(),
                    content: "Abstract…".into(),
                    engine: "arxiv".into(),
                    authors: vec!["Ashish Vaswani".into(), "Noam Shazeer".into()],
                    doi: "10.48550/arXiv.1234.5678".into(),
                    journal: "".into(),
                    published_date: Some("2017-06-12".into()),
                    type_: "preprint".into(),
                    tags: vec!["transformers".into()],
                    pdf_url: "https://arxiv.org/pdf/1234.5678".into(),
                    comments: "15 pages, 5 figures".into(),
                    score: 1.0,
                    positions: vec![1],
                    ..Paper::default()
                }),
                Result_::Code(Code {
                    url: "https://github.com/rust-lang/rust".into(),
                    title: "fn main()".into(),
                    engine: "github_code".into(),
                    repository: Some("rust-lang/rust".into()),
                    filename: Some("main.rs".into()),
                    code_language: "rust".into(),
                    codelines: vec![
                        (1, "fn main() {".into()),
                        (2, "    println!(\"hi\");".into()),
                        (3, "}".into()),
                    ],
                    hl_lines: vec![1],
                    score: 0.8,
                    positions: vec![1],
                    ..Code::default()
                }),
                Result_::File(FileResult {
                    url: "https://thepiratebay.org/torrent/1".into(),
                    title: "Some.Torrent".into(),
                    engine: "piratebay".into(),
                    filename: "Some.Torrent".into(),
                    size: "1.2 GiB".into(),
                    time: "2024-01-02".into(),
                    mimetype: "application/x-bittorrent".into(),
                    author: "uploader".into(),
                    filesize: Some("1.2 GiB".into()),
                    seed: Some(120),
                    leech: Some(4),
                    magnetlink: Some("magnet:?xt=urn:btih:abc".into()),
                    score: 0.7,
                    positions: vec![1],
                    ..FileResult::default()
                }),
                Result_::KeyValue(KeyValue {
                    title: "Package info".into(),
                    engine: "crates".into(),
                    caption: "Metadata".into(),
                    key_title: "Field".into(),
                    value_title: "Value".into(),
                    kvmap: vec![
                        ("license".into(), "MIT OR Apache-2.0".into()),
                        ("downloads".into(), "1_000_000".into()),
                    ],
                    score: 0.5,
                    positions: vec![1],
                    ..KeyValue::default()
                }),
            ],
            answers: vec![Answer {
                answer: "42".into(),
                engine: "calculator".into(),
                interactive: Some(InteractiveAnswer::Calculator {
                    expression: "6*7".into(),
                    result: 42.0,
                }),
                ..Answer::default()
            }],
            corrections: vec![Correction {
                correction: "rust lang".into(),
                engine: "duckduckgo".into(),
                ..Correction::default()
            }],
            suggestions: vec![Suggestion {
                suggestion: "rust programming language".into(),
                engine: "brave".into(),
            }],
            infoboxes: vec![Infobox {
                infobox: "Rust".into(),
                id: Some("Q575650".into()),
                content: "general-purpose programming language".into(),
                img_src: Some("https://cdn.example/rust.png".into()),
                urls: vec![zoeken_results::InfoboxUrl {
                    title: "Wikipedia".into(),
                    url: "https://en.wikipedia.org/wiki/Rust_(programming_language)".into(),
                }],
                attributes: vec![zoeken_results::InfoboxAttribute {
                    label: "Paradigm".into(),
                    value: "multi-paradigm".into(),
                    image: None,
                }],
                related_topics: vec!["Cargo (package manager)".into(), "LLVM".into()],
                engine: "wikidata".into(),
            }],
            unresponsive_engines: vec![zoeken_search::UnresponsiveEngine {
                engine: "google".into(),
                cause: UnresponsiveCause::Timeout,
            }],
            number_of_results: 42,
            ..ResultContainer::default()
        }
    }

    #[test]
    fn maps_each_result_kind_and_preserves_dropped_fields() {
        let response = NativeSearchResponse::from_container(
            "rust lang",
            &sample_container(),
            ProxySettings {
                secret_key: "test-secret",
                image_proxy: false,
                favicon_proxy: false,
            },
            NativeMapContext {
                category: "general",
            },
        );
        assert_eq!(response.schema_version, 2);
        assert_eq!(response.number_of_results, 42);
        assert_eq!(response.results.len(), 6);
        assert!(matches!(response.results[0], NativeResult::Main { .. }));
        assert!(matches!(response.results[1], NativeResult::Image { .. }));
        assert!(matches!(response.results[2], NativeResult::Paper { .. }));
        assert!(matches!(response.results[3], NativeResult::Code { .. }));
        assert!(matches!(response.results[4], NativeResult::File { .. }));
        assert!(matches!(response.results[5], NativeResult::KeyValue { .. }));

        let NativeResult::File {
            time,
            seed,
            leech,
            magnetlink,
            ..
        } = &response.results[4]
        else {
            panic!("expected file");
        };
        assert_eq!(time, "2024-01-02");
        assert_eq!(*seed, Some(120));
        assert_eq!(*leech, Some(4));
        assert!(magnetlink.starts_with("magnet:"));

        let NativeResult::Code { hl_lines, .. } = &response.results[3] else {
            panic!("expected code");
        };
        assert_eq!(hl_lines, &[1]);

        let NativeResult::KeyValue { caption, kvmap, .. } = &response.results[5] else {
            panic!("expected key_value");
        };
        assert_eq!(caption, "Metadata");
        assert_eq!(kvmap.len(), 2);

        assert_eq!(response.corrections[0].correction, "rust lang");
        assert_eq!(response.suggestions[0].engine, "brave");
        assert_eq!(response.unresponsive_engines[0].cause, "timeout");
        assert_eq!(response.answers[0].answer, "42");
    }

    #[test]
    fn proxy_rewrite_signs_image_and_favicon_urls() {
        let response = NativeSearchResponse::from_container(
            "rust",
            &sample_container(),
            ProxySettings {
                secret_key: "test-secret",
                image_proxy: true,
                favicon_proxy: true,
            },
            NativeMapContext {
                category: "general",
            },
        );
        let NativeResult::Main {
            favicon, thumbnail, ..
        } = &response.results[0]
        else {
            panic!("expected main");
        };
        assert!(favicon.starts_with("/favicon_proxy?"));
        assert!(favicon.contains("h="));
        assert!(thumbnail.starts_with("/image_proxy?"));

        let NativeResult::Image {
            img_src,
            thumbnail_src,
            ..
        } = &response.results[1]
        else {
            panic!("expected image");
        };
        assert!(img_src.starts_with("/image_proxy?"));
        assert!(thumbnail_src.starts_with("/image_proxy?"));
        assert!(
            response.infoboxes[0]
                .img_src
                .as_deref()
                .unwrap()
                .starts_with("/image_proxy?")
        );
    }

    #[test]
    fn schema_version_is_current() {
        let response = NativeSearchResponse::from_container(
            "",
            &ResultContainer::default(),
            ProxySettings {
                secret_key: "",
                image_proxy: false,
                favicon_proxy: false,
            },
            NativeMapContext::default(),
        );
        assert_eq!(response.schema_version, NATIVE_SCHEMA_VERSION);
        let json = serde_json::to_value(&response).unwrap();
        assert_eq!(json["schema_version"], 2);
        assert_eq!(json["number_of_results"], 0);
    }
}
