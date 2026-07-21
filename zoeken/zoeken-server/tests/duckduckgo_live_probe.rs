//! Live DuckDuckGo probe — ignored by default.
//!
//!   .\run.ps1 cargo test -p zoeken-server --test duckduckgo_live_probe '--' '--ignored' '--nocapture'

use std::sync::Arc;

use zoeken_engine_core::SearchQueryView;
use zoeken_engines::DuckDuckGo;
use zoeken_network::NetworkManager;
use zoeken_search::EngineExecutor;
use zoeken_server::executor::NetworkExecutor;
use zoeken_settings::OutgoingSettings;

#[tokio::test]
#[ignore = "makes a real outbound request to DuckDuckGo"]
async fn probe_duckduckgo_live() {
    let networks =
        Arc::new(NetworkManager::from_settings(&OutgoingSettings::default()).expect("networks"));
    let executor = NetworkExecutor::new(networks);
    let engine = Arc::new(DuckDuckGo::new());
    let query = SearchQueryView {
        query: "rust programming".to_string(),
        pageno: 1,
        locale: "all".to_string(),
        ..SearchQueryView::default()
    };

    let outcome = executor.execute(engine, query).await;
    match outcome.result {
        Ok(results) => {
            println!(
                "DDG OK  results={} answers={} http={:?}",
                results.results.len(),
                results.answers.len(),
                outcome.http_duration,
            );
            for (i, r) in results.results.iter().take(5).enumerate() {
                if let zoeken_results::Result_::Main(m) = r {
                    println!("  [{i}] {} — {}", m.title, m.url);
                }
            }
            assert!(
                !results.results.is_empty(),
                "DuckDuckGo returned zero results (possible silent challenge / selector drift)"
            );
        }
        Err(err) => {
            println!("DDG ERR {err}");
            panic!("DuckDuckGo live probe failed: {err}");
        }
    }
}
