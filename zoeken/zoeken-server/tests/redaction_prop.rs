// Property 15: sensitive-value redaction.

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::{Method, Request, StatusCode, header};
use axum::response::{IntoResponse, Response};
use axum::routing::any as any_method;
use proptest::prelude::*;
use tower::ServiceExt;
use zoeken_server::middleware::trace_layer;

use tracing::Subscriber;
use tracing::field::{Field, Visit};
use tracing::span::{Attributes, Id, Record};
use tracing_subscriber::layer::{Context, Layer};
use tracing_subscriber::prelude::*;
use tracing_subscriber::registry::LookupSpan;

const ALLOWED_FIELDS: &[&str] = &["method", "path", "status", "latency_ms"];

struct FieldVisitor<'a>(&'a mut BTreeMap<String, String>);

impl Visit for FieldVisitor<'_> {
    fn record_debug(&mut self, field: &Field, value: &dyn std::fmt::Debug) {
        self.0
            .insert(field.name().to_string(), format!("{value:?}"));
    }
    fn record_str(&mut self, field: &Field, value: &str) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_u64(&mut self, field: &Field, value: u64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
    fn record_i64(&mut self, field: &Field, value: i64) {
        self.0.insert(field.name().to_string(), value.to_string());
    }
}

#[derive(Clone)]
struct CaptureLayer(Arc<Mutex<BTreeMap<String, String>>>);

impl<S> Layer<S> for CaptureLayer
where
    S: Subscriber + for<'a> LookupSpan<'a>,
{
    fn on_new_span(&self, attrs: &Attributes<'_>, _id: &Id, _ctx: Context<'_, S>) {
        let mut fields = self.0.lock().unwrap();
        attrs.record(&mut FieldVisitor(&mut fields));
    }

    fn on_record(&self, _id: &Id, values: &Record<'_>, _ctx: Context<'_, S>) {
        let mut fields = self.0.lock().unwrap();
        values.record(&mut FieldVisitor(&mut fields));
    }
}

#[derive(Debug, Clone)]
struct Secrets {
    cookie_value: String,
    set_cookie_value: String,
    secret_key: String,
    query_value: String,
}

fn token_strategy(marker: &'static str) -> impl Strategy<Value = String> {
    "[A-Z0-9]{1,12}".prop_map(move |suffix| format!("{marker}{suffix}"))
}

fn secrets_strategy() -> impl Strategy<Value = Secrets> {
    (
        token_strategy("COOKIE"),
        token_strategy("SETCOOKIE"),
        token_strategy("SECRETKEY"),
        token_strategy("QUERYSECRET"),
    )
        .prop_map(
            |(cookie_value, set_cookie_value, secret_key, query_value)| Secrets {
                cookie_value,
                set_cookie_value,
                secret_key,
                query_value,
            },
        )
}

fn path_strategy() -> impl Strategy<Value = String> {
    prop::collection::vec("[a-z]{1,8}", 0..3).prop_map(|segs| format!("/{}", segs.join("/")))
}

fn method_strategy() -> impl Strategy<Value = Method> {
    prop_oneof![Just(Method::GET), Just(Method::POST)].boxed()
}

async fn set_cookie_handler(secret: String) -> Response {
    (
        StatusCode::OK,
        [(header::SET_COOKIE, format!("session={secret}; HttpOnly"))],
        "ok",
    )
        .into_response()
}

fn build_app(set_cookie_value: String) -> Router {
    Router::new()
        .route(
            "/{*rest}",
            any_method(move || set_cookie_handler(set_cookie_value.clone())),
        )
        .layer(trace_layer())
}

fn build_request(method: &Method, path: &str, secrets: &Secrets) -> Request<Body> {
    let uri = format!(
        "{path}?q={q}&token={q}",
        path = if path == "/" { "/root" } else { path },
        q = secrets.query_value,
    );
    Request::builder()
        .method(method.clone())
        .uri(uri)
        .header(
            header::COOKIE,
            format!("prefs={}; session=alsohidden", secrets.cookie_value),
        )
        .header(
            header::AUTHORIZATION,
            format!("Bearer {}", secrets.secret_key),
        )
        .header("x-secret-key", secrets.secret_key.clone())
        .body(Body::empty())
        .unwrap()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(256))]

    /// Property 15: sensitive-value redaction.
    #[test]
    fn tracing_span_never_records_cookie_or_secret_values(
        method in method_strategy(),
        path in path_strategy(),
        secrets in secrets_strategy(),
    ) {
        let captured = Arc::new(Mutex::new(BTreeMap::<String, String>::new()));
        let subscriber =
            tracing_subscriber::registry().with(CaptureLayer(captured.clone()));

        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build current-thread runtime");

        let response = tracing::subscriber::with_default(subscriber, || {
            rt.block_on(async {
                build_app(secrets.set_cookie_value.clone())
                    .oneshot(build_request(&method, &path, &secrets))
                    .await
                    .expect("router is infallible")
            })
        });

        prop_assert_eq!(response.status(), StatusCode::OK);
        prop_assert!(response.headers().contains_key(header::SET_COOKIE));

        let fields = captured.lock().unwrap().clone();

        for name in ALLOWED_FIELDS {
            prop_assert!(
                fields.contains_key(*name),
                "expected allowlisted field {name:?} to be recorded, got fields: {fields:?}",
            );
        }

        for name in fields.keys() {
            prop_assert!(
                ALLOWED_FIELDS.contains(&name.as_str()),
                "span recorded a non-allowlisted field {name:?} (only {ALLOWED_FIELDS:?} permitted)",
            );
        }

        let leaked = [
            ("cookie value (Req 12.5)", &secrets.cookie_value),
            ("Set-Cookie value (Req 12.5)", &secrets.set_cookie_value),
            ("secret key (Req 11.4)", &secrets.secret_key),
            ("query string", &secrets.query_value),
        ];
        for (field_name, value) in &fields {
            for (label, sensitive) in &leaked {
                prop_assert!(
                    !value.contains(sensitive.as_str()),
                    "recorded field {field_name:?} = {value:?} leaked the {label}: {sensitive:?}",
                );
            }
        }
    }
}
