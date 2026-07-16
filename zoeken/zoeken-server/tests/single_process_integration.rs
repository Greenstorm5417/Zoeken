//! Single-process API + static-asset serving.

use std::borrow::Cow;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

use axum::Router;
use axum::body::{Body, to_bytes};
use axum::http::{Request, StatusCode, header};

use tower::ServiceExt;
use zoeken_engine_core::{
    Engine, EngineError, EngineMeta, EngineResponse, EngineResults, RequestParams, SearchQueryView,
};
use zoeken_results::{MainResult, Result_};
use zoeken_search::{
    EngineExecResult, EngineExecutor, EngineFuture, EngineRegistry, RegisteredEngine, Search,
    SearchConfig,
};
use zoeken_server::static_assets::{AssetSource, INDEX_HTML};
use zoeken_server::{AppState, app};

struct StubEngine {
    meta: EngineMeta,
}

impl Engine for StubEngine {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }
    fn request(&self, _q: &SearchQueryView, _p: &mut RequestParams) {}
    fn response(&self, _resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        Ok(EngineResults::new())
    }
}

struct ImmediateExecutor;

impl EngineExecutor for ImmediateExecutor {
    fn execute(&self, engine: Arc<dyn Engine>, _query: SearchQueryView) -> EngineFuture {
        let name = engine.metadata().name.clone();
        Box::pin(async move {
            let mut results = EngineResults::new();
            results.add(Result_::Main(MainResult {
                url: format!("https://{name}.test/"),
                normalized_url: format!("https://{name}.test/"),
                title: name.clone(),
                engine: name,
                ..MainResult::default()
            }));
            EngineExecResult::from_result(Ok(results))
        })
    }
}

fn stub_search() -> Search {
    let engine = StubEngine {
        meta: EngineMeta {
            name: "stub".to_string(),
            categories: vec!["general".to_string()],
            ..EngineMeta::default()
        },
    };
    let registry = EngineRegistry::from_engines([RegisteredEngine::new(Arc::new(engine))]);
    let executor: Arc<dyn EngineExecutor> = Arc::new(ImmediateExecutor);
    Search::new(registry, executor, SearchConfig::default())
}

const INDEX_BODY: &[u8] = b"<!doctype html><title>project</title>";
const ASSET_PATH: &str = "assets/main.4af1c2e9.js";
const ASSET_BODY: &[u8] = b"console.log('gs')";

struct CountingAssets {
    files: HashMap<String, Vec<u8>>,
    reads: Arc<AtomicUsize>,
}

impl CountingAssets {
    fn new(entries: &[(&str, &[u8])]) -> (Arc<Self>, Arc<AtomicUsize>) {
        let files = entries
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_vec()))
            .collect();
        let reads = Arc::new(AtomicUsize::new(0));
        let source = Arc::new(Self {
            files,
            reads: reads.clone(),
        });
        (source, reads)
    }
}

impl AssetSource for CountingAssets {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        let hit = self.files.get(path).cloned().map(Cow::Owned);
        if hit.is_some() {
            self.reads.fetch_add(1, Ordering::SeqCst);
        }
        hit
    }

    fn has_index(&self) -> bool {
        self.files.contains_key(INDEX_HTML)
    }
}

// ---------------------------------------------------------------------------
// Shared helpers.
// ---------------------------------------------------------------------------

async fn decoded_body(response: axum::response::Response) -> Vec<u8> {
    use std::io::Read;

    let is_gzip = response
        .headers()
        .get(header::CONTENT_ENCODING)
        .is_some_and(|value| value.as_bytes().eq_ignore_ascii_case(b"gzip"));
    let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
    if is_gzip {
        let mut decoder = flate2::read::GzDecoder::new(&bytes[..]);
        let mut out = Vec::new();
        decoder.read_to_end(&mut out).unwrap();
        out
    } else {
        bytes.to_vec()
    }
}

fn get(uri: &str) -> Request<Body> {
    Request::builder().uri(uri).body(Body::empty()).unwrap()
}

#[tokio::test]
async fn one_process_serves_both_api_and_assets_sequentially() {
    let (assets, reads) =
        CountingAssets::new(&[(INDEX_HTML, INDEX_BODY), (ASSET_PATH, ASSET_BODY)]);
    let asset_source: Arc<dyn AssetSource> = assets;

    let router: Router = app(AppState::from_search(stub_search()).with_assets(asset_source));

    let config = router.clone().oneshot(get("/config")).await.unwrap();
    assert_eq!(config.status(), StatusCode::OK, "the API route must answer");
    assert_eq!(
        config.headers().get(header::CONTENT_TYPE).unwrap(),
        "application/json",
        "the /config Backend_API route wins over the static fallback"
    );
    let config_body = decoded_body(config).await;
    let value: serde_json::Value = serde_json::from_slice(&config_body).unwrap();
    assert!(
        value.get("engines").is_some(),
        "the body must be the /config API JSON, not the SPA index"
    );
    assert_eq!(
        reads.load(Ordering::SeqCst),
        0,
        "an API request must not read from the static asset source"
    );

    let asset = router
        .clone()
        .oneshot(get(&format!("/{ASSET_PATH}")))
        .await
        .unwrap();
    assert_eq!(asset.status(), StatusCode::OK, "the asset must be served");
    assert_eq!(
        asset.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/javascript; charset=utf-8",
        "the asset content type is derived from its extension (Req 1.4)"
    );
    let asset_body = decoded_body(asset).await;
    assert_eq!(
        &asset_body[..],
        ASSET_BODY,
        "the asset bytes must round-trip"
    );
    let reads_after_asset = reads.load(Ordering::SeqCst);
    assert!(
        reads_after_asset > 0,
        "the asset must have been read from the one shared source"
    );

    let spa = router.oneshot(get("/some/client/route")).await.unwrap();
    assert_eq!(spa.status(), StatusCode::OK);
    assert_eq!(
        spa.headers().get(header::CONTENT_TYPE).unwrap(),
        "text/html; charset=utf-8"
    );
    let spa_body = decoded_body(spa).await;
    assert_eq!(
        &spa_body[..],
        INDEX_BODY,
        "the SPA entry document is served"
    );

    assert!(
        reads.load(Ordering::SeqCst) > reads_after_asset,
        "the SPA entry document must have been read from the one shared source"
    );
}

/// Default app uses the embedded asset source.
#[tokio::test]
async fn default_app_uses_the_embedded_single_binary_source() {
    let default_router: Router = app(AppState::from_search(stub_search()));

    let config = default_router.oneshot(get("/config")).await.unwrap();
    assert_eq!(config.status(), StatusCode::OK);
}
