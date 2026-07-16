//! Static-asset routing, cache headers, and SPA fallback.

use std::borrow::Cow;
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use axum::body::Body;
use axum::extract::{Request, State};
use axum::http::{HeaderValue, Method, StatusCode, header};
use axum::response::{IntoResponse, Response};

pub const SERVED_EXTENSIONS: &[&str] = &[
    "html", "js", "mjs", "css", "map", "json", "txt", "xml", "xsl", "svg", "ico", "png", "jpg",
    "jpeg", "gif", "webp", "avif", "woff", "woff2", "ttf", "eot",
];

const CACHE_IMMUTABLE: &str = "public, max-age=31536000, immutable";

const CACHE_REVALIDATE: &str = "no-cache, must-revalidate";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssetDecision {
    /// Serve the named asset.
    ServeAsset {
        path: String,
    },
    /// Serve the SPA entry document.
    ServeIndex,
    MethodNotAllowed,
    NotFound,
}

pub fn decide(method: &Method, path: &str, asset_exists: impl Fn(&str) -> bool) -> AssetDecision {
    if method != Method::GET && method != Method::HEAD {
        return AssetDecision::MethodNotAllowed;
    }
    if has_served_extension(path) {
        return if asset_exists(path) {
            AssetDecision::ServeAsset {
                path: path.to_string(),
            }
        } else {
            AssetDecision::NotFound
        };
    }
    AssetDecision::ServeIndex
}

pub fn cache_control_for(path: &str) -> &'static str {
    if is_fingerprinted(path) {
        CACHE_IMMUTABLE
    } else {
        CACHE_REVALIDATE
    }
}

pub fn is_fingerprinted(path: &str) -> bool {
    let file = path.rsplit('/').next().unwrap_or(path);
    let parts: Vec<&str> = file.split('.').collect();
    if parts.len() < 3 {
        return false;
    }
    let hash = parts[parts.len() - 2];
    hash.len() >= 8 && hash.chars().all(is_hash_char)
}

pub fn content_type_for(path: &str) -> HeaderValue {
    let content_type = match extension(path).as_deref() {
        Some("html") => "text/html; charset=utf-8",
        Some("js") | Some("mjs") => "text/javascript; charset=utf-8",
        Some("css") => "text/css; charset=utf-8",
        Some("map") | Some("json") => "application/json",
        Some("txt") => "text/plain; charset=utf-8",
        Some("xml") | Some("xsl") => "application/xml",
        Some("svg") => "image/svg+xml",
        Some("ico") => "image/x-icon",
        Some("png") => "image/png",
        Some("jpg") | Some("jpeg") => "image/jpeg",
        Some("gif") => "image/gif",
        Some("webp") => "image/webp",
        Some("avif") => "image/avif",
        Some("woff") => "font/woff",
        Some("woff2") => "font/woff2",
        Some("ttf") => "font/ttf",
        Some("eot") => "application/vnd.ms-fontobject",
        _ => "application/octet-stream",
    };
    HeaderValue::from_static(content_type)
}

fn is_hash_char(c: char) -> bool {
    c.is_ascii_digit() || c.is_ascii_lowercase()
}

fn extension(path: &str) -> Option<String> {
    let file = path.rsplit('/').next().unwrap_or(path);
    let dot = file.rfind('.')?;
    if dot == 0 || dot + 1 == file.len() {
        return None;
    }
    Some(file[dot + 1..].to_ascii_lowercase())
}

fn has_served_extension(path: &str) -> bool {
    match extension(path) {
        Some(ext) => SERVED_EXTENSIONS.contains(&ext.as_str()),
        None => false,
    }
}

pub const INDEX_HTML: &str = "index.html";

pub fn startup_asset_check(assets: &dyn AssetSource, location: &str) -> Result<(), String> {
    if assets.has_index() {
        Ok(())
    } else {
        Err(format!(
            "startup aborted: frontend assets missing: no `{INDEX_HTML}` found in {location}; \
             build the frontend into the asset source before starting (Req 1.3)"
        ))
    }
}

pub trait AssetSource: Send + Sync {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>>;

    fn has_index(&self) -> bool;
}

pub struct DirAssets {
    root: PathBuf,
}

impl DirAssets {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn resolve(&self, path: &str) -> Option<PathBuf> {
        if !is_safe_relative(path) {
            return None;
        }
        Some(self.root.join(path))
    }
}

impl AssetSource for DirAssets {
    fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
        let full = self.resolve(path)?;
        if !full.is_file() {
            return None;
        }
        std::fs::read(full).ok().map(Cow::Owned)
    }

    fn has_index(&self) -> bool {
        self.root.join(INDEX_HTML).is_file()
    }
}

fn is_safe_relative(path: &str) -> bool {
    if path.starts_with('/') {
        return false;
    }
    Path::new(path)
        .components()
        .all(|component| matches!(component, Component::Normal(_) | Component::CurDir))
}

pub async fn static_fallback(State(assets): State<Arc<dyn AssetSource>>, req: Request) -> Response {
    let path = req.uri().path().trim_start_matches('/').to_string();
    let decision = decide(req.method(), &path, |p| assets.get(p).is_some());
    build_response(&decision, assets.as_ref())
}

pub fn build_response(decision: &AssetDecision, assets: &dyn AssetSource) -> Response {
    match decision {
        AssetDecision::ServeAsset { path } => match assets.get(path) {
            Some(bytes) => asset_response(content_type_for(path), cache_control_for(path), bytes),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        AssetDecision::ServeIndex => match assets.get(INDEX_HTML) {
            Some(bytes) => asset_response(
                content_type_for(INDEX_HTML),
                cache_control_for(INDEX_HTML),
                bytes,
            ),
            None => StatusCode::NOT_FOUND.into_response(),
        },
        AssetDecision::MethodNotAllowed => StatusCode::METHOD_NOT_ALLOWED.into_response(),
        AssetDecision::NotFound => StatusCode::NOT_FOUND.into_response(),
    }
}

/// Build a `200 OK` asset response carrying `bytes` with the given content-type
/// and cache-control headers.
fn asset_response(
    content_type: HeaderValue,
    cache_control: &'static str,
    bytes: Cow<'static, [u8]>,
) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, content_type),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static(cache_control),
            ),
        ],
        Body::from(bytes.into_owned()),
    )
        .into_response()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A served-extension path with a matching asset is served (Req 1.4).
    #[test]
    fn get_existing_asset_serves_it() {
        let decision = decide(&Method::GET, "assets/main.4af1c2e9.js", |_| true);
        assert_eq!(
            decision,
            AssetDecision::ServeAsset {
                path: "assets/main.4af1c2e9.js".to_string()
            }
        );
    }

    /// A served-extension path with no matching asset is a 404, never an index
    /// fallback (Req 1.8).
    #[test]
    fn get_missing_asset_extension_is_not_found() {
        let decision = decide(&Method::GET, "assets/missing.js", |_| false);
        assert_eq!(decision, AssetDecision::NotFound);
    }

    #[test]
    fn get_extensionless_path_serves_index() {
        assert_eq!(
            decide(&Method::GET, "search", |_| false),
            AssetDecision::ServeIndex
        );
        assert_eq!(
            decide(&Method::GET, "preferences/nested", |_| true),
            AssetDecision::ServeIndex
        );
        assert_eq!(
            decide(&Method::GET, "", |_| false),
            AssetDecision::ServeIndex
        );
    }

    #[test]
    fn head_is_permitted() {
        assert_eq!(
            decide(&Method::HEAD, "assets/app.css", |_| true),
            AssetDecision::ServeAsset {
                path: "assets/app.css".to_string()
            }
        );
        assert_eq!(
            decide(&Method::HEAD, "about", |_| false),
            AssetDecision::ServeIndex
        );
    }

    #[test]
    fn non_get_head_methods_are_method_not_allowed() {
        for method in [Method::POST, Method::PUT, Method::DELETE, Method::PATCH] {
            assert_eq!(
                decide(&method, "assets/main.4af1c2e9.js", |_| true),
                AssetDecision::MethodNotAllowed
            );
            assert_eq!(
                decide(&method, "search", |_| true),
                AssetDecision::MethodNotAllowed
            );
        }
    }

    #[test]
    fn fingerprinted_filenames_are_detected() {
        assert!(is_fingerprinted("main.4af1c2e9.js"));
        assert!(is_fingerprinted("assets/index.0a1b2c3d.css"));
        assert!(is_fingerprinted("vendor.deadbeefcafe.mjs"));
    }

    #[test]
    fn non_fingerprinted_filenames_are_not_detected() {
        assert!(!is_fingerprinted("index.html"));
        assert!(!is_fingerprinted("app.js"));
        assert!(!is_fingerprinted("main.abc123.js"));
        assert!(!is_fingerprinted("robots.txt"));
    }

    #[test]
    fn cache_control_matches_fingerprinting() {
        assert_eq!(cache_control_for("main.4af1c2e9.js"), CACHE_IMMUTABLE);
        assert!(cache_control_for("main.4af1c2e9.js").contains("max-age=31536000"));
        assert!(cache_control_for("main.4af1c2e9.js").contains("immutable"));

        assert_eq!(cache_control_for("index.html"), CACHE_REVALIDATE);
        assert_eq!(cache_control_for("app.js"), CACHE_REVALIDATE);
    }

    #[test]
    fn content_type_is_derived_from_extension() {
        assert_eq!(content_type_for("index.html"), "text/html; charset=utf-8");
        assert_eq!(
            content_type_for("main.js"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            content_type_for("chunk.mjs"),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(content_type_for("styles.css"), "text/css; charset=utf-8");
        assert_eq!(content_type_for("icon.SVG"), "image/svg+xml");
        assert_eq!(content_type_for("photo.JPEG"), "image/jpeg");
        assert_eq!(content_type_for("font.woff2"), "font/woff2");
        assert_eq!(content_type_for("app.js.map"), "application/json");
        assert_eq!(content_type_for("data.bin"), "application/octet-stream");
        assert_eq!(content_type_for("noext"), "application/octet-stream");
    }

    use std::io::Write;

    #[test]
    fn safe_relative_rejects_traversal_and_absolute() {
        assert!(is_safe_relative("index.html"));
        assert!(is_safe_relative("assets/main.4af1c2e9.js"));
        assert!(is_safe_relative("./assets/app.css"));

        assert!(!is_safe_relative("/etc/passwd"));
        assert!(!is_safe_relative("../secret"));
        assert!(!is_safe_relative("assets/../../secret"));
    }

    #[test]
    fn dir_assets_reads_files_and_detects_index() {
        let dir = std::env::temp_dir().join(format!("zoeken-dir-assets-{}", std::process::id()));
        let nested = dir.join("assets");
        std::fs::create_dir_all(&nested).unwrap();

        let mut index = std::fs::File::create(dir.join("index.html")).unwrap();
        index
            .write_all(b"<!doctype html><title>ok</title>")
            .unwrap();

        let mut js = std::fs::File::create(nested.join("app.js")).unwrap();
        js.write_all(b"console.log(1)").unwrap();

        let source = DirAssets::new(&dir);

        assert!(source.has_index());
        assert_eq!(
            source.get("index.html").as_deref(),
            Some(&b"<!doctype html><title>ok</title>"[..])
        );
        assert_eq!(
            source.get("assets/app.js").as_deref(),
            Some(&b"console.log(1)"[..])
        );

        assert!(source.get("assets/missing.js").is_none());
        assert!(source.get("assets").is_none());
        assert!(source.get("../secret").is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn dir_assets_without_index_reports_absent() {
        let dir = std::env::temp_dir().join(format!("zoeken-dir-noindex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();

        let source = DirAssets::new(&dir);
        assert!(!source.has_index());
        assert!(source.get("index.html").is_none());

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn startup_asset_check_aborts_when_index_absent() {
        let dir =
            std::env::temp_dir().join(format!("zoeken-startup-noindex-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let dir_source = DirAssets::new(&dir);
        let location = format!("directory '{}'", dir.display());
        let err = startup_asset_check(&dir_source, &location)
            .expect_err("check must abort when index.html is absent");
        assert!(
            err.contains(INDEX_HTML),
            "error must name the missing entry document"
        );
        assert!(
            err.contains(&location),
            "error must identify the asset location"
        );
        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn startup_asset_check_passes_when_index_present() {
        let dir = std::env::temp_dir().join(format!("zoeken-startup-index-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        std::fs::write(dir.join(INDEX_HTML), b"<!doctype html>ok").unwrap();
        let source = DirAssets::new(&dir);
        assert!(startup_asset_check(&source, "directory override").is_ok());
        std::fs::remove_dir_all(&dir).ok();
    }

    use axum::body::to_bytes;
    use std::collections::HashMap;

    struct MockAssets {
        files: HashMap<String, Vec<u8>>,
    }

    impl MockAssets {
        fn new(entries: &[(&str, &[u8])]) -> Self {
            let files = entries
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_vec()))
                .collect();
            Self { files }
        }
    }

    impl AssetSource for MockAssets {
        fn get(&self, path: &str) -> Option<Cow<'static, [u8]>> {
            self.files.get(path).cloned().map(Cow::Owned)
        }

        fn has_index(&self) -> bool {
            self.files.contains_key(INDEX_HTML)
        }
    }

    async fn call(assets: Arc<dyn AssetSource>, method: Method, uri: &str) -> Response {
        let req = Request::builder()
            .method(method)
            .uri(uri)
            .body(Body::empty())
            .unwrap();
        static_fallback(State(assets), req).await
    }

    fn mock() -> Arc<dyn AssetSource> {
        Arc::new(MockAssets::new(&[
            ("index.html", b"<!doctype html><title>gs</title>"),
            ("assets/main.4af1c2e9.js", b"console.log(1)"),
            ("app.css", b"body{}"),
        ]))
    }

    #[tokio::test]
    async fn serve_asset_returns_bytes_content_type_and_immutable_cache() {
        let response = call(mock(), Method::GET, "/assets/main.4af1c2e9.js").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/javascript; charset=utf-8"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            CACHE_IMMUTABLE
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"console.log(1)");
    }

    #[tokio::test]
    async fn serve_non_fingerprinted_asset_revalidates() {
        let response = call(mock(), Method::GET, "/app.css").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/css; charset=utf-8"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            CACHE_REVALIDATE
        );
    }

    #[tokio::test]
    async fn extensionless_path_serves_index_html() {
        let response = call(mock(), Method::GET, "/search").await;

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            CACHE_REVALIDATE
        );

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&body[..], b"<!doctype html><title>gs</title>");
    }

    #[tokio::test]
    async fn missing_asset_extension_is_not_found() {
        let response = call(mock(), Method::GET, "/assets/missing.js").await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn non_get_head_method_is_method_not_allowed() {
        let response = call(mock(), Method::POST, "/assets/main.4af1c2e9.js").await;
        assert_eq!(response.status(), StatusCode::METHOD_NOT_ALLOWED);

        let body = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert!(body.is_empty(), "405 must not carry the asset body");
    }

    #[tokio::test]
    async fn head_serves_asset_headers() {
        let response = call(mock(), Method::HEAD, "/assets/main.4af1c2e9.js").await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            CACHE_IMMUTABLE
        );
    }

    #[tokio::test]
    async fn router_fallback_serves_from_dir_assets() {
        use axum::Router;
        use std::io::Write;
        use tower::ServiceExt;

        let dir = std::env::temp_dir().join(format!("zoeken-fallback-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let mut index = std::fs::File::create(dir.join("index.html")).unwrap();
        index.write_all(b"<!doctype html>ok").unwrap();

        let assets: Arc<dyn AssetSource> = Arc::new(DirAssets::new(&dir));
        let app: Router = Router::new().fallback(static_fallback).with_state(assets);

        let response = app
            .oneshot(
                Request::builder()
                    .uri("/some/client/route")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CONTENT_TYPE).unwrap(),
            "text/html; charset=utf-8"
        );

        std::fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn build_response_degrades_to_not_found_when_asset_missing() {
        let empty = MockAssets::new(&[]);
        let decision = AssetDecision::ServeAsset {
            path: "gone.js".to_string(),
        };
        let response = build_response(&decision, &empty);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);

        let index = AssetDecision::ServeIndex;
        let response = build_response(&index, &empty);
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
