//! On-demand live diagnostic: run every default engine through the real
//! network executor and print each raw outcome (result count or error).
//!
//! This makes real outbound requests, so it is `#[ignore]`d by default. Run it
//! explicitly with:
//!
//!   cargo test -p zoeken-server --test engine_live_diagnostic -- --ignored --nocapture

use std::sync::Arc;

use zoeken_engine_core::{Engine, SearchQueryView};
use zoeken_engines::{
    AppleAppStore, Arxiv, Bandcamp, Bing, Brave, Crates, Crossref, Dailymotion, DockerHub, Dogpile,
    DuckDuckGo, GenericEngineConfig, GenericHtmlEngine, GenericJsonEngine, Genius, Github, Gitlab,
    Google, Hackernews, Imdb, Lemmy, Mastodon, Mojeek, NineGag, Nyaa, Openstreetmap, Openverse,
    Peertube, Photon, Piratebay, Pypi, Qwant, Reddit, SemanticScholar, SensCritique, SepiaSearch,
    SolidTorrents, Soundcloud, Stackexchange, Startpage, Swisscows, SwisscowsConfig, Tootfinder,
    Unsplash, Wikibooks, Wikidata, Wikipedia, builtin_generic_config, builtin_generic_ids,
};
use zoeken_network::NetworkManager;
use zoeken_search::EngineExecutor;
use zoeken_server::executor::NetworkExecutor;
use zoeken_settings::OutgoingSettings;

fn default_engines() -> Vec<Arc<dyn Engine>> {
    vec![
        Arc::new(DuckDuckGo::new()),
        Arc::new(Google::new()),
        Arc::new(Bing::new()),
        Arc::new(Brave::new()),
        Arc::new(Startpage::new()),
        Arc::new(Mojeek::new()),
        Arc::new(Qwant::new()),
        Arc::new(Dogpile::default()),
        Arc::new(Swisscows::default()),
        Arc::new(
            Swisscows::new(SwisscowsConfig {
                base_url: "https://api.swisscows.com".to_string(),
                swisscows_category: "news".to_string(),
                results_per_page: 20,
            })
            .expect("swisscows news"),
        ),
        Arc::new(Wikipedia::new()),
        Arc::new(Wikidata::new()),
        Arc::new(Wikibooks::new()),
        Arc::new(Arxiv::new()),
        Arc::new(Crates::new()),
        Arc::new(DockerHub::new()),
        Arc::new(Github::new()),
        Arc::new(Gitlab::new()),
        Arc::new(Pypi::new()),
        Arc::new(Hackernews::new()),
        Arc::new(Reddit::new()),
        Arc::new(Lemmy::new()),
        Arc::new(Mastodon::accounts()),
        Arc::new(Stackexchange::stackoverflow()),
        Arc::new(Bandcamp::new()),
        Arc::new(Soundcloud::new()),
        Arc::new(Openverse::new()),
        Arc::new(SepiaSearch::new()),
        Arc::new(Openstreetmap::new()),
        Arc::new(Peertube::new()),
        Arc::new(Dailymotion::new()),
        Arc::new(Unsplash::new()),
        Arc::new(Genius::new()),
        Arc::new(SemanticScholar::new()),
        Arc::new(Crossref::new()),
        Arc::new(Piratebay::new()),
        Arc::new(Nyaa::new()),
        Arc::new(SolidTorrents::new()),
        Arc::new(Photon::new()),
        Arc::new(Imdb::new()),
        Arc::new(AppleAppStore::new()),
        Arc::new(Tootfinder::new()),
        Arc::new(SensCritique::new()),
        Arc::new(NineGag::new()),
    ]
}

fn generic_catalog_engines() -> Vec<Arc<dyn Engine>> {
    builtin_generic_ids()
        .filter_map(|id| {
            let config = builtin_generic_config(id)?;
            if generic_config_search_url(&config).contains(".onion") {
                return None;
            }
            match config {
                GenericEngineConfig::Html(config) => {
                    Some(Arc::new(GenericHtmlEngine::new(config).ok()?) as Arc<dyn Engine>)
                }
                GenericEngineConfig::Json(config) => {
                    Some(Arc::new(GenericJsonEngine::new(config).ok()?) as Arc<dyn Engine>)
                }
            }
        })
        .collect()
}

fn generic_config_search_url(config: &GenericEngineConfig) -> &str {
    match config {
        GenericEngineConfig::Html(config) => &config.search_url,
        GenericEngineConfig::Json(config) => &config.search_url,
    }
}

fn query() -> SearchQueryView {
    SearchQueryView {
        query: "rust".to_string(),
        pageno: 1,
        locale: "all".to_string(),
        ..SearchQueryView::default()
    }
}

fn query_for_engine(name: &str) -> SearchQueryView {
    let mut q = query();
    q.query = match name {
        "anaconda" => "numpy",
        "bitbucket" => "django",
        "erowid" => "cannabis",
        "etymonline" => "love",
        "fastbot" => "test",
        "habrahabr" => "python",
        "tmdb" => "inception",
        "swisscows news" => "berlin",
        "imdb" => "inception",
        "apple app store" => "signal",
        "senscritique" => "inception",
        "9gag" => "cat",
        "semantic scholar" => "rust programming language",
        "crossref" => "rust programming language",
        "genius" => "hello",
        "openstreetmap" => "berlin",
        "photon" => "berlin",
        "solidtorrents" => "ubuntu",
        "rubygems" => "rails",
        "woxikon.de synonyme" => "gut",
        _ => "rust",
    }
    .to_string();
    q
}

#[tokio::test]
#[ignore = "makes real outbound network requests; run manually to diagnose engines"]
async fn run_all_engines_live() {
    let networks =
        Arc::new(NetworkManager::from_settings(&OutgoingSettings::default()).expect("networks"));
    let executor = NetworkExecutor::new(networks);

    println!("\n=== live engine diagnostic ===");
    for engine in default_engines() {
        let name = engine.metadata().name.clone();
        match executor.execute(engine, query()).await.result {
            Ok(results) => {
                println!(
                    "{name:20} OK  results={} answers={} suggestions={} infoboxes={}",
                    results.results.len(),
                    results.answers.len(),
                    results.suggestions.len(),
                    results.infoboxes.len(),
                );
                if let Some(ib) = results.infoboxes.first() {
                    let content: String = ib.content.chars().take(100).collect();
                    println!("{:20}   infobox: {} — {}", "", ib.infobox, content);
                }
                if let Some(ans) = results.answers.first() {
                    let answer: String = ans.answer.chars().take(100).collect();
                    println!("{:20}   answer: {}", "", answer);
                }
            }
            Err(err) => {
                println!("{name:20} ERR {err}");
            }
        }
    }
    println!("=== end diagnostic ===\n");
}

#[tokio::test]
#[ignore = "makes real outbound network requests; run manually to diagnose generic catalog engines"]
async fn run_generic_catalog_engines_live() {
    let networks =
        Arc::new(NetworkManager::from_settings(&OutgoingSettings::default()).expect("networks"));
    let executor = NetworkExecutor::new(networks);

    println!("\n=== live generic catalog diagnostic ===");
    let mut zero_result_engines = Vec::new();
    for engine in generic_catalog_engines() {
        let name = engine.metadata().name.clone();
        match executor
            .execute(engine, query_for_engine(&name))
            .await
            .result
        {
            Ok(results) => {
                println!(
                    "{name:24} OK  results={} answers={} suggestions={} infoboxes={}",
                    results.results.len(),
                    results.answers.len(),
                    results.suggestions.len(),
                    results.infoboxes.len(),
                );
                if results.results.is_empty()
                    && results.answers.is_empty()
                    && results.suggestions.is_empty()
                    && results.infoboxes.is_empty()
                {
                    zero_result_engines.push(name);
                }
            }
            Err(err) => {
                println!("{name:24} ERR {err}");
            }
        }
    }
    println!("=== end generic catalog diagnostic ===\n");
    assert!(
        zero_result_engines.is_empty(),
        "generic engines returned zero results without an error: {}",
        zero_result_engines.join(", ")
    );
}
