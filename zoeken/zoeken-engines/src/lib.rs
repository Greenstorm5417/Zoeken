//! zoeken-engines: ported search engines and the conformance harness.
//!
//! The `conformance` module loads fixtures and compares parsed engine output
//! against recorded golden results.

pub mod conformance;
pub mod engines;

pub use conformance::{
    ConformanceMismatch, Fixture, FixtureError, load_fixture, load_fixtures, load_fixtures_for,
    run_all, run_conformance, run_request_conformance, run_response_conformance,
};
pub use engines::{
    AppleAppStore, Arxiv, Bandcamp, Bing, Brave, Core, Crates, Crossref, Dailymotion, DockerHub,
    Dogpile, DogpileConfig, DuckDuckGo, Elasticsearch, ElasticsearchConfig, GenericEngineConfig,
    GenericHtmlConfig, GenericHtmlEngine, GenericJsonConfig, GenericJsonEngine, Genius, Github,
    GithubCode, Gitlab, Google, Hackernews, Imdb, Invidious, Lemmy, LemmyType, Marginalia,
    MarginaliaConfig, Mastodon, MastodonType, Meilisearch, MeilisearchConfig, Mojeek, NineGag,
    Nyaa, Openstreetmap, Openverse, Peertube, Photon, Piped, Piratebay, Pypi, Qwant, Reddit,
    SemanticScholar, SensCritique, SepiaSearch, SolidTorrents, Soundcloud, Sqlite, SqliteConfig,
    Stackexchange, Startpage, Swisscows, SwisscowsConfig, Tootfinder, Unsplash, Vimeo, Wikibooks,
    Wikidata, Wikipedia, Yacy, YacyConfig, all_generic_ids, builtin_generic_config,
    builtin_generic_html_config, builtin_generic_ids,
};
