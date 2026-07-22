//! Aggregate conformance test for all ported engines.

use std::path::PathBuf;

use zoeken_engine_core::Engine;
use zoeken_engines::{
    AppleAppStore, Arxiv, Bandcamp, Bing, BingImages, Brave, Core, Crates, Crossref, Dailymotion,
    DockerHub, Dogpile, DuckDuckGo, Elasticsearch, ElasticsearchConfig, Fixture,
    GenericEngineConfig, GenericHtmlEngine, GenericJsonEngine, Genius, Github, GithubCode, Gitlab,
    Google, Hackernews, Imdb, Invidious, Lemmy, Marginalia, MarginaliaConfig, Mastodon,
    Meilisearch, MeilisearchConfig, Mojeek, NineGag, Nyaa, Openstreetmap, Openverse, Peertube,
    Photon, Piped, Piratebay, Pypi, Qwant, Reddit, SemanticScholar, SensCritique, SepiaSearch,
    SolidTorrents, Soundcloud, Stackexchange, Startpage, Swisscows, Tootfinder, Unsplash, Vimeo,
    Wikibooks, Wikidata, Wikipedia, Yacy, YacyConfig, builtin_generic_config, load_fixtures_for,
    run_conformance,
};

fn fixtures_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures")
}

struct EngineEntry {
    name: &'static str,
    dir: &'static str,
    engine_for: fn(&Fixture) -> Box<dyn Engine>,
    expect_error: fn(&Fixture) -> bool,
}

fn never_errors(_fixture: &Fixture) -> bool {
    false
}

fn engine_entries() -> Vec<EngineEntry> {
    vec![
        EngineEntry {
            name: "arxiv",
            dir: "arxiv",
            engine_for: |_| Box::new(Arxiv::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "bandcamp",
            dir: "bandcamp",
            engine_for: |_| Box::new(Bandcamp::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "bing",
            dir: "bing",
            engine_for: |_| Box::new(Bing::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "bing_images",
            dir: "bing_images",
            engine_for: |_| Box::new(BingImages::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "brave",
            dir: "brave",
            engine_for: |_| Box::new(Brave::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "core",
            dir: "core",
            engine_for: |_| Box::new(Core::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "crossref",
            dir: "crossref",
            engine_for: |_| Box::new(Crossref::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "crates",
            dir: "crates",
            engine_for: |_| Box::new(Crates::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "dailymotion",
            dir: "dailymotion",
            engine_for: |_| Box::new(Dailymotion::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "docker_hub",
            dir: "docker_hub",
            engine_for: |_| Box::new(DockerHub::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "dogpile",
            dir: "dogpile",
            engine_for: |_| Box::new(Dogpile::default()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "duckduckgo",
            dir: "duckduckgo",
            engine_for: |_| Box::new(DuckDuckGo::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "elasticsearch",
            dir: "elasticsearch",
            engine_for: |_| {
                Box::new(
                    Elasticsearch::new(ElasticsearchConfig {
                        base_url: "http://localhost:9200".to_string(),
                        username: String::new(),
                        password: String::new(),
                        index: "my-index".to_string(),
                        query_type: "match".to_string(),
                        custom_query_json: Default::default(),
                        show_metadata: false,
                        page_size: 10,
                    })
                    .expect("elasticsearch engine"),
                )
            },
            expect_error: never_errors,
        },
        EngineEntry {
            name: "github",
            dir: "github",
            engine_for: |_| Box::new(Github::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "github_code",
            dir: "github_code",
            engine_for: |_| Box::new(GithubCode::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "gitlab",
            dir: "gitlab",
            engine_for: |_| Box::new(Gitlab::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "google",
            dir: "google",
            engine_for: |_| Box::new(Google::new()),
            expect_error: |f| f.case.as_deref() == Some("sorry-captcha"),
        },
        EngineEntry {
            name: "generic",
            dir: "generic",
            engine_for: generic_engine_for,
            expect_error: |f| f.engine == "bitbucket" || generic_empty_expected_error(f),
        },
        EngineEntry {
            name: "genius",
            dir: "genius",
            engine_for: |_| Box::new(Genius::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "hackernews",
            dir: "hackernews",
            engine_for: |_| Box::new(Hackernews::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "imdb",
            dir: "imdb",
            engine_for: |_| Box::new(Imdb::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "invidious",
            dir: "invidious",
            engine_for: |_| Box::new(Invidious::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "lemmy",
            dir: "lemmy",
            engine_for: |_| Box::new(Lemmy::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "mastodon",
            dir: "mastodon",
            engine_for: |f| {
                if f.engine == "mastodon hashtags" {
                    Box::new(Mastodon::hashtags())
                } else {
                    Box::new(Mastodon::accounts())
                }
            },
            expect_error: never_errors,
        },
        EngineEntry {
            name: "marginalia",
            dir: "marginalia",
            engine_for: |_| {
                Box::new(
                    Marginalia::new(MarginaliaConfig {
                        base_url: "https://api2.marginalia-search.com".to_string(),
                        api_key: "test-key".to_string(),
                    })
                    .expect("marginalia engine"),
                )
            },
            expect_error: never_errors,
        },
        EngineEntry {
            name: "meilisearch",
            dir: "meilisearch",
            engine_for: |_| {
                Box::new(
                    Meilisearch::new(MeilisearchConfig {
                        base_url: "http://localhost:7700".to_string(),
                        index: "my-index".to_string(),
                        auth_key: String::new(),
                        facet_filters: Vec::new(),
                    })
                    .expect("meilisearch engine"),
                )
            },
            expect_error: never_errors,
        },
        EngineEntry {
            name: "mojeek",
            dir: "mojeek",
            engine_for: |_| Box::new(Mojeek::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "9gag",
            dir: "9gag",
            engine_for: |_| Box::new(NineGag::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "nyaa",
            dir: "nyaa",
            engine_for: |_| Box::new(Nyaa::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "openstreetmap",
            dir: "openstreetmap",
            engine_for: |_| Box::new(Openstreetmap::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "openverse",
            dir: "openverse",
            engine_for: |_| Box::new(Openverse::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "peertube",
            dir: "peertube",
            engine_for: |_| Box::new(Peertube::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "photon",
            dir: "photon",
            engine_for: |_| Box::new(Photon::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "piratebay",
            dir: "piratebay",
            engine_for: |_| Box::new(Piratebay::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "piped",
            dir: "piped",
            engine_for: |_| Box::new(Piped::videos()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "pypi",
            dir: "pypi",
            engine_for: |_| Box::new(Pypi::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "qwant",
            dir: "qwant",
            engine_for: |_| Box::new(Qwant::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "reddit",
            dir: "reddit",
            engine_for: |_| Box::new(Reddit::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "semantic_scholar",
            dir: "semantic_scholar",
            engine_for: |_| Box::new(SemanticScholar::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "senscritique",
            dir: "senscritique",
            engine_for: |_| Box::new(SensCritique::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "sepiasearch",
            dir: "sepiasearch",
            engine_for: |_| Box::new(SepiaSearch::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "solidtorrents",
            dir: "solidtorrents",
            engine_for: |_| Box::new(SolidTorrents::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "soundcloud",
            dir: "soundcloud",
            engine_for: |_| Box::new(Soundcloud::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "stackexchange",
            dir: "stackexchange",
            engine_for: |f| match f.engine.as_str() {
                "askubuntu" => Box::new(Stackexchange::askubuntu()),
                "superuser" => Box::new(Stackexchange::superuser()),
                _ => Box::new(Stackexchange::stackoverflow()),
            },
            expect_error: never_errors,
        },
        EngineEntry {
            name: "startpage",
            dir: "startpage",
            engine_for: |_| Box::new(Startpage::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "swisscows",
            dir: "swisscows",
            engine_for: |_| Box::new(Swisscows::default()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "swisscows_news",
            dir: "swisscows_news",
            engine_for: |_| {
                Box::new(
                    Swisscows::new(zoeken_engines::SwisscowsConfig {
                        base_url: "https://api.swisscows.com".to_string(),
                        swisscows_category: "news".to_string(),
                        results_per_page: 20,
                    })
                    .expect("swisscows news engine"),
                )
            },
            expect_error: never_errors,
        },
        EngineEntry {
            name: "apple_app_store",
            dir: "apple_app_store",
            engine_for: |_| Box::new(AppleAppStore::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "tootfinder",
            dir: "tootfinder",
            engine_for: |_| Box::new(Tootfinder::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "unsplash",
            dir: "unsplash",
            engine_for: |_| Box::new(Unsplash::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "vimeo",
            dir: "vimeo",
            engine_for: |_| Box::new(Vimeo::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "wikibooks",
            dir: "wikibooks",
            engine_for: |_| Box::new(Wikibooks::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "wikidata",
            dir: "wikidata",
            engine_for: |_| Box::new(Wikidata::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "wikipedia",
            dir: "wikipedia",
            engine_for: |_| Box::new(Wikipedia::new()),
            expect_error: never_errors,
        },
        EngineEntry {
            name: "yacy",
            dir: "yacy",
            engine_for: |_| {
                Box::new(
                    Yacy::new(YacyConfig {
                        base_url: vec!["https://search.example.test".to_string()],
                        search_mode: "global".to_string(),
                        search_type: "text".to_string(),
                        http_digest_auth_user: String::new(),
                        http_digest_auth_pass: String::new(),
                    })
                    .expect("yacy engine"),
                )
            },
            expect_error: never_errors,
        },
    ]
}

fn generic_engine_for(fixture: &Fixture) -> Box<dyn Engine> {
    match builtin_generic_config(&fixture.engine)
        .unwrap_or_else(|| panic!("unknown generic engine `{}`", fixture.engine))
    {
        GenericEngineConfig::Html(config) => {
            Box::new(GenericHtmlEngine::new(config).expect("generic HTML engine"))
        }
        GenericEngineConfig::Json(config) => {
            Box::new(GenericJsonEngine::new(config).expect("generic JSON engine"))
        }
    }
}

fn generic_empty_expected_error(fixture: &Fixture) -> bool {
    if fixture.case.as_deref() != Some("generic-empty") {
        return false;
    }
    match builtin_generic_config(&fixture.engine) {
        Some(GenericEngineConfig::Html(config)) => config.empty_result_error.is_some(),
        Some(GenericEngineConfig::Json(config)) => config.empty_result_error.is_some(),
        None => false,
    }
}

#[test]
fn all_ported_engines_conform_to_golden_output() {
    let root = fixtures_root();
    let entries = engine_entries();

    let mut passed = 0usize;
    let mut total_fixtures = 0usize;
    let mut failures: Vec<String> = Vec::new();

    for entry in &entries {
        let fixtures = load_fixtures_for(&root, entry.dir)
            .unwrap_or_else(|e| panic!("failed to load fixtures for `{}`: {e}", entry.name));

        if fixtures.is_empty() {
            failures.push(format!(
                "{}: no golden fixtures found under fixtures/{}",
                entry.name, entry.dir
            ));
            continue;
        }

        let mut engine_ok = true;
        for fixture in &fixtures {
            total_fixtures += 1;
            let engine = (entry.engine_for)(fixture);

            if (entry.expect_error)(fixture) {
                if engine.response(&fixture.response).is_ok() {
                    engine_ok = false;
                    failures.push(format!(
                        "{}: fixture `{}` was expected to error but parsed successfully",
                        entry.name,
                        fixture.label()
                    ));
                }
                continue;
            }

            if let Err(mismatch) = run_conformance(engine.as_ref(), fixture) {
                engine_ok = false;
                failures.push(format!("{}: {mismatch}", entry.name));
            }
        }

        if engine_ok {
            passed += 1;
        }
    }

    println!(
        "conformance parity: {passed}/{} ported engines pass the harness \
         across {total_fixtures} golden fixtures",
        entries.len()
    );

    assert!(
        failures.is_empty(),
        "conformance failures across ported engines:\n{}",
        failures.join("\n")
    );
    assert_eq!(
        passed,
        entries.len(),
        "every ported engine must pass the conformance harness"
    );
}

/// Catches the case this file exists to prevent: a fixtures/<engine> directory
/// with real conformance fixtures that nothing here loads, so it silently
/// never runs. Directories with no `.json` files (e.g. sqlite's `.sql` seed)
/// aren't conformance fixtures and are skipped.
#[test]
fn every_fixture_directory_with_json_is_registered() {
    let root = fixtures_root();
    let registered: std::collections::HashSet<&str> =
        engine_entries().iter().map(|e| e.dir).collect();

    let mut unregistered = Vec::new();
    for entry in std::fs::read_dir(&root).expect("read fixtures dir") {
        let entry = entry.expect("read fixtures dir entry");
        if !entry.file_type().expect("file type").is_dir() {
            continue;
        }
        let has_json = std::fs::read_dir(entry.path())
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
            .any(|f| f.path().extension().is_some_and(|ext| ext == "json"));
        if !has_json {
            continue;
        }
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if !registered.contains(name.as_ref()) {
            unregistered.push(name.into_owned());
        }
    }

    assert!(
        unregistered.is_empty(),
        "fixtures/{{{}}} have golden fixtures but no EngineEntry in \
         engine_entries() -- they're never conformance-tested by this harness",
        unregistered.join(", ")
    );
}
