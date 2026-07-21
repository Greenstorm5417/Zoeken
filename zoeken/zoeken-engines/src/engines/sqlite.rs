//! SQLite offline engine (upstream `searx/engines/sqlite.py`).
//!
//! This is a database engine: it is entirely settings-driven (no defaults make
//! it active) and runs an administrator-supplied `SELECT` query against a
//! local SQLite file, mapping result rows to [`KeyValue`] (or [`MainResult`])
//! results. It is an offline processor: [`Sqlite::request`] never sets
//! [`RequestParams::url`], so the executor invokes [`Sqlite::response`]
//! directly with an empty [`EngineResponse`] (see `zoeken-server/src/executor.rs`)
//! instead of making a network call. Because the [`Engine`] trait only passes
//! the response into `response()`, the query text and page number captured in
//! `request()` are threaded through via an internal mutex.

use std::collections::HashMap;
use std::sync::Mutex;
use std::thread::ThreadId;

use serde::Deserialize;
use sqlx::sqlite::{SqliteArguments, SqliteConnectOptions, SqliteRow};
use sqlx::{Arguments, Column, Connection, Row, TypeInfo, ValueRef};
use zoeken_engine_core::{
    About, Engine, EngineError, EngineMeta, EngineResponse, EngineResults, Processor,
    RequestParams, SearchQueryView,
};
use zoeken_results::{KeyValue, MainResult, Result_};

/// Engine name / identifier (upstream module name).
pub const NAME: &str = "sqlite";

/// Result mapping selected via the `result_type` setting.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum ResultType {
    #[default]
    KeyValue,
    MainResult,
}

/// Settings accepted from `settings.yml` (`EngineSettings.extra`).
#[derive(Debug, Clone, Deserialize)]
pub struct SqliteConfig {
    /// Filename of the SQLite DB. Required.
    pub database: String,
    /// SQL `SELECT` query that returns the result items. Required.
    pub query_str: String,
    /// `"MainResult"` or `"KeyValue"` (default `"KeyValue"`).
    #[serde(default)]
    pub result_type: Option<String>,
    /// Page size (default 10, matches upstream).
    #[serde(default)]
    pub limit: Option<i64>,
}

struct PendingQuery {
    query: String,
    pageno: u32,
}

/// The SQLite database engine.
pub struct Sqlite {
    meta: EngineMeta,
    database: String,
    query_str: String,
    result_type: ResultType,
    limit: i64,
    state: Mutex<HashMap<ThreadId, PendingQuery>>,
}

impl std::fmt::Debug for Sqlite {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Sqlite")
            .field("database", &self.database)
            .field("query_str", &self.query_str)
            .field("result_type", &self.result_type)
            .field("limit", &self.limit)
            .finish()
    }
}

impl Sqlite {
    /// Build the engine from validated settings.
    ///
    /// Mirrors upstream `init()`: `query_str` must be present and must start
    /// with `SELECT` (case-insensitive). Returns `Err` on invalid config so
    /// callers (the settings→engine constructor map) can treat a misconfigured
    /// entry as inactive rather than defaulting to some other database.
    pub fn new(config: SqliteConfig) -> Result<Self, String> {
        if config.query_str.trim().is_empty() {
            return Err("query_str cannot be empty".to_string());
        }
        if !config
            .query_str
            .trim_start()
            .to_lowercase()
            .starts_with("select ")
        {
            return Err("only SELECT query is supported".to_string());
        }
        if config.database.trim().is_empty() {
            return Err("database cannot be empty".to_string());
        }
        let result_type = match config.result_type.as_deref() {
            Some("MainResult") => ResultType::MainResult,
            Some("KeyValue") | None => ResultType::KeyValue,
            Some(other) => return Err(format!("unsupported result_type: {other}")),
        };
        Ok(Sqlite {
            meta: EngineMeta {
                name: NAME.to_string(),
                engine_type: Processor::Offline,
                categories: vec!["general".to_string()],
                paging: true,
                max_page: 0,
                time_range_support: false,
                safesearch: false,
                language_support: false,
                weight: 1,
                shortcut: "sqlite".to_string(),
                about: About {
                    website: Some("https://www.sqlite.org".to_string()),
                    wikidata_id: Some("Q10547406".to_string()),
                    official_api_documentation: None,
                    use_official_api: false,
                    require_api_key: false,
                    results: "".to_string(),
                },
            },
            database: config.database,
            query_str: config.query_str,
            result_type,
            limit: config.limit.unwrap_or(10),
            state: Mutex::new(HashMap::new()),
        })
    }
}

impl Engine for Sqlite {
    fn metadata(&self) -> &EngineMeta {
        &self.meta
    }

    fn request(&self, q: &SearchQueryView, _p: &mut RequestParams) {
        // Offline processor: no HTTP request is made. Stash the query text and
        // page number so `response()` (invoked with an empty EngineResponse)
        // can run the configured SELECT.
        let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
        state.insert(
            std::thread::current().id(),
            PendingQuery {
                query: q.query.clone(),
                pageno: q.pageno.max(1),
            },
        );
    }

    fn response(&self, _resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let pending = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.remove(&std::thread::current().id())
        };
        let PendingQuery { query, pageno } = pending.unwrap_or(PendingQuery {
            query: String::new(),
            pageno: 1,
        });

        let offset = (i64::from(pageno) - 1) * self.limit;
        let query_to_run = format!("{} LIMIT :limit OFFSET :offset", self.query_str);
        let rows = execute_query(&self.database, &query_to_run, &query, self.limit, offset)?;
        let mut res = EngineResults::new();
        for kvmap in rows {
            match self.result_type {
                ResultType::MainResult => {
                    let get = |key: &str| -> String {
                        kvmap
                            .iter()
                            .find(|(k, _)| k == key)
                            .map(|(_, v)| v.clone())
                            .unwrap_or_default()
                    };
                    let url = get("url");
                    res.add(Result_::Main(MainResult {
                        url: url.clone(),
                        normalized_url: url,
                        title: get("title"),
                        content: get("content"),
                        engine: NAME.to_string(),
                        ..MainResult::default()
                    }));
                }
                ResultType::KeyValue => {
                    res.add(Result_::KeyValue(KeyValue {
                        kvmap,
                        engine: NAME.to_string(),
                        ..KeyValue::default()
                    }));
                }
            }
        }

        Ok(res)
    }
}

fn execute_query(
    database: &str,
    statement: &str,
    search_query: &str,
    limit: i64,
    offset: i64,
) -> Result<Vec<Vec<(String, String)>>, EngineError> {
    let database = database.to_string();
    let statement = statement.to_string();
    let search_query = search_query.to_string();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|error| EngineError::Unexpected(format!("sqlite runtime: {error}")))?;
        runtime.block_on(async move {
            let options = SqliteConnectOptions::new()
                .filename(database)
                .read_only(true);
            let mut connection = sqlx::SqliteConnection::connect_with(&options)
                .await
                .map_err(sqlite_error("failed to open database"))?;
            let wildcard = format!("%{}%", search_query.replace(' ', "%"));
            let (sql, arguments) =
                bind_named_arguments(&statement, &search_query, &wildcard, limit, offset)?;
            let rows = sqlx::query_with(&sql, arguments)
                .fetch_all(&mut connection)
                .await
                .map_err(sqlite_error("query failed"))?;
            rows.iter().map(row_to_pairs).collect()
        })
    })
    .join()
    .map_err(|_| EngineError::Unexpected("sqlite worker panicked".to_string()))?
}

fn bind_named_arguments(
    statement: &str,
    search_query: &str,
    wildcard: &str,
    limit: i64,
    offset: i64,
) -> Result<(String, SqliteArguments<'static>), EngineError> {
    const NAMES: [&str; 4] = [":query", ":wildcard", ":limit", ":offset"];
    let mut sql = String::with_capacity(statement.len());
    let mut remaining = statement;
    let mut arguments = SqliteArguments::default();
    while let Some((position, name)) = NAMES
        .iter()
        .filter_map(|name| remaining.find(name).map(|position| (position, *name)))
        .min_by_key(|(position, _)| *position)
    {
        sql.push_str(&remaining[..position]);
        sql.push('?');
        match name {
            ":query" => arguments.add(search_query.to_string()),
            ":wildcard" => arguments.add(wildcard.to_string()),
            ":limit" => arguments.add(limit),
            ":offset" => arguments.add(offset),
            _ => unreachable!(),
        }
        .map_err(|error| {
            EngineError::Unexpected(format!("sqlite: failed to bind query parameter: {error}"))
        })?;
        remaining = &remaining[position + name.len()..];
    }
    sql.push_str(remaining);
    Ok((sql, arguments))
}

fn row_to_pairs(row: &SqliteRow) -> Result<Vec<(String, String)>, EngineError> {
    row.columns()
        .iter()
        .enumerate()
        .map(|(index, column)| {
            let raw = row
                .try_get_raw(index)
                .map_err(sqlite_error("column read failed"))?;
            let value = if raw.is_null() {
                "None".to_string()
            } else {
                match raw.type_info().name() {
                    "INTEGER" => row.try_get::<i64, _>(index).map(|value| value.to_string()),
                    "REAL" => row.try_get::<f64, _>(index).map(|value| value.to_string()),
                    "BLOB" => row
                        .try_get::<Vec<u8>, _>(index)
                        .map(|value| String::from_utf8_lossy(&value).to_string()),
                    _ => row.try_get::<String, _>(index),
                }
                .map_err(sqlite_error("column decode failed"))?
            };
            Ok((column.name().to_string(), value))
        })
        .collect()
}

fn sqlite_error(context: &'static str) -> impl FnOnce(sqlx::Error) -> EngineError {
    move |error| EngineError::Unexpected(format!("sqlite: {context}: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(sql: &[&str]) -> tempfile::TempPath {
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.into_temp_path();
        let database = path.to_path_buf();
        let statements = sql
            .iter()
            .map(|value| value.to_string())
            .collect::<Vec<_>>();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            runtime.block_on(async move {
                let options = SqliteConnectOptions::new()
                    .filename(database)
                    .create_if_missing(true);
                let mut connection = sqlx::SqliteConnection::connect_with(&options)
                    .await
                    .unwrap();
                for statement in statements {
                    sqlx::raw_sql(&statement)
                        .execute(&mut connection)
                        .await
                        .unwrap();
                }
            });
        })
        .join()
        .unwrap();
        path
    }

    fn query(q: &str, pageno: u32) -> SearchQueryView {
        SearchQueryView {
            query: q.to_string(),
            pageno,
            ..SearchQueryView::default()
        }
    }

    #[test]
    fn rejects_missing_query_str() {
        let err = Sqlite::new(SqliteConfig {
            database: "test.db".to_string(),
            query_str: String::new(),
            result_type: None,
            limit: None,
        })
        .unwrap_err();
        assert!(err.contains("query_str"));
    }

    #[test]
    fn rejects_non_select_query() {
        let err = Sqlite::new(SqliteConfig {
            database: "test.db".to_string(),
            query_str: "DELETE FROM t".to_string(),
            result_type: None,
            limit: None,
        })
        .unwrap_err();
        assert!(err.contains("SELECT"));
    }

    #[test]
    fn rejects_empty_database() {
        let err = Sqlite::new(SqliteConfig {
            database: String::new(),
            query_str: "SELECT 1".to_string(),
            result_type: None,
            limit: None,
        })
        .unwrap_err();
        assert!(err.contains("database"));
    }

    #[test]
    fn keyvalue_result_type_maps_rows() {
        let path = temp_db(&[
            "CREATE TABLE film (title TEXT, description TEXT)",
            "INSERT INTO film VALUES ('rust in action', 'a book about rust')",
            "INSERT INTO film VALUES ('unrelated', 'nothing to do with the query')",
        ]);
        let engine = Sqlite::new(SqliteConfig {
            database: path.to_string_lossy().to_string(),
            query_str: "SELECT title, description AS content FROM film WHERE title LIKE :wildcard"
                .to_string(),
            result_type: None,
            limit: Some(10),
        })
        .unwrap();

        let q = query("rust", 1);
        let mut params = RequestParams::default();
        engine.request(&q, &mut params);
        assert!(params.url.is_none(), "offline engine must not set a URL");

        let results = engine.response(&EngineResponse::default()).unwrap();
        assert_eq!(results.results.len(), 1);
        match &results.results[0] {
            Result_::KeyValue(kv) => {
                assert_eq!(kv.engine, NAME);
                assert_eq!(
                    kv.kvmap,
                    vec![
                        ("title".to_string(), "rust in action".to_string()),
                        ("content".to_string(), "a book about rust".to_string()),
                    ]
                );
            }
            other => panic!("expected KeyValue result, got {other:?}"),
        }
    }

    #[test]
    fn main_result_type_maps_url_title_content() {
        let path = temp_db(&[
            "CREATE TABLE film (title TEXT, url TEXT, description TEXT)",
            "INSERT INTO film VALUES ('rust in action', 'https://example.com/rust', 'a book')",
        ]);
        let engine = Sqlite::new(SqliteConfig {
            database: path.to_string_lossy().to_string(),
            query_str:
                "SELECT title, url, description AS content FROM film WHERE title LIKE :wildcard"
                    .to_string(),
            result_type: Some("MainResult".to_string()),
            limit: Some(10),
        })
        .unwrap();

        let q = query("rust", 1);
        let mut params = RequestParams::default();
        engine.request(&q, &mut params);
        let results = engine.response(&EngineResponse::default()).unwrap();
        assert_eq!(results.results.len(), 1);
        match &results.results[0] {
            Result_::Main(m) => {
                assert_eq!(m.title, "rust in action");
                assert_eq!(m.url, "https://example.com/rust");
                assert_eq!(m.content, "a book");
                assert_eq!(m.engine, NAME);
            }
            other => panic!("expected Main result, got {other:?}"),
        }
    }

    #[test]
    fn paging_applies_limit_and_offset() {
        let path = temp_db(&[
            "CREATE TABLE t (title TEXT)",
            "INSERT INTO t VALUES ('a')",
            "INSERT INTO t VALUES ('b')",
            "INSERT INTO t VALUES ('c')",
        ]);
        let engine = Sqlite::new(SqliteConfig {
            database: path.to_string_lossy().to_string(),
            query_str: "SELECT title FROM t ORDER BY title".to_string(),
            result_type: None,
            limit: Some(2),
        })
        .unwrap();

        // page 2 with limit 2 should return only the 3rd row.
        let q = query("", 2);
        let mut params = RequestParams::default();
        engine.request(&q, &mut params);
        let results = engine.response(&EngineResponse::default()).unwrap();
        assert_eq!(results.results.len(), 1);
        match &results.results[0] {
            Result_::KeyValue(kv) => {
                assert_eq!(kv.kvmap, vec![("title".to_string(), "c".to_string())]);
            }
            other => panic!("expected KeyValue result, got {other:?}"),
        }
    }

    #[test]
    fn null_column_renders_as_python_str_none() {
        let path = temp_db(&[
            "CREATE TABLE t (title TEXT, note TEXT)",
            "INSERT INTO t VALUES ('a', NULL)",
        ]);
        let engine = Sqlite::new(SqliteConfig {
            database: path.to_string_lossy().to_string(),
            query_str: "SELECT title, note FROM t".to_string(),
            result_type: None,
            limit: Some(10),
        })
        .unwrap();
        let q = query("", 1);
        let mut params = RequestParams::default();
        engine.request(&q, &mut params);
        let results = engine.response(&EngineResponse::default()).unwrap();
        match &results.results[0] {
            Result_::KeyValue(kv) => {
                assert_eq!(
                    kv.kvmap,
                    vec![
                        ("title".to_string(), "a".to_string()),
                        ("note".to_string(), "None".to_string()),
                    ]
                );
            }
            other => panic!("expected KeyValue result, got {other:?}"),
        }
    }

    #[test]
    fn fixture_queries_remain_isolated_across_threads() {
        let path = temp_db(&[include_str!("../../fixtures/sqlite/search.sql")]);
        let engine = std::sync::Arc::new(
            Sqlite::new(SqliteConfig {
                database: path.to_string_lossy().to_string(),
                query_str: "SELECT title FROM documents WHERE title LIKE :wildcard ORDER BY title"
                    .to_string(),
                result_type: None,
                limit: Some(10),
            })
            .unwrap(),
        );
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let handles: Vec<_> = ["rust", "privacy"]
            .into_iter()
            .map(|term| {
                let engine = std::sync::Arc::clone(&engine);
                let barrier = std::sync::Arc::clone(&barrier);
                std::thread::spawn(move || {
                    engine.request(&query(term, 1), &mut RequestParams::default());
                    barrier.wait();
                    engine.response(&EngineResponse::default()).unwrap()
                })
            })
            .collect();
        let titles: Vec<String> = handles
            .into_iter()
            .map(|handle| {
                let results = handle.join().unwrap();
                match &results.results[0] {
                    Result_::KeyValue(row) => row.kvmap[0].1.clone(),
                    other => panic!("expected KeyValue result, got {other:?}"),
                }
            })
            .collect();
        assert_eq!(titles, ["rust search", "privacy search"]);
    }
}
