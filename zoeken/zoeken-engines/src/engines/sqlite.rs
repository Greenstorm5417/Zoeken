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

use std::sync::Mutex;

use rusqlite::{Connection, OpenFlags, named_params};
use serde::Deserialize;
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
    state: Mutex<Option<PendingQuery>>,
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
            state: Mutex::new(None),
        })
    }

    fn open_ro(&self) -> Result<Connection, EngineError> {
        let uri = format!("file:{}?mode=ro", self.database);
        Connection::open_with_flags(
            uri,
            OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_URI,
        )
        .map_err(|e| EngineError::Unexpected(format!("sqlite: failed to open database: {e}")))
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
        *state = Some(PendingQuery {
            query: q.query.clone(),
            pageno: q.pageno.max(1),
        });
    }

    fn response(&self, _resp: &EngineResponse) -> Result<EngineResults, EngineError> {
        let pending = {
            let mut state = self.state.lock().unwrap_or_else(|e| e.into_inner());
            state.take()
        };
        let PendingQuery { query, pageno } = pending.unwrap_or(PendingQuery {
            query: String::new(),
            pageno: 1,
        });

        let wildcard = format!("%{}%", query.replace(' ', "%"));
        let offset = (i64::from(pageno) - 1) * self.limit;
        let query_to_run = format!("{} LIMIT :limit OFFSET :offset", self.query_str);

        let conn = self.open_ro()?;
        let mut stmt = conn.prepare(&query_to_run).map_err(|e| {
            EngineError::Unexpected(format!("sqlite: failed to prepare query: {e}"))
        })?;
        let col_names: Vec<String> = stmt.column_names().iter().map(|s| s.to_string()).collect();

        let mut res = EngineResults::new();
        let uses_query = query_to_run.contains(":query");
        let uses_wildcard = query_to_run.contains(":wildcard");
        let mut rows = match (uses_query, uses_wildcard) {
            (true, true) => stmt.query(named_params! {
                ":query": query,
                ":wildcard": wildcard,
                ":limit": self.limit,
                ":offset": offset,
            }),
            (true, false) => stmt.query(named_params! {
                ":query": query,
                ":limit": self.limit,
                ":offset": offset,
            }),
            (false, true) => stmt.query(named_params! {
                ":wildcard": wildcard,
                ":limit": self.limit,
                ":offset": offset,
            }),
            (false, false) => stmt.query(named_params! {
                ":limit": self.limit,
                ":offset": offset,
            }),
        }
        .map_err(|e| EngineError::Unexpected(format!("sqlite: query failed: {e}")))?;

        while let Some(row) = rows
            .next()
            .map_err(|e| EngineError::Unexpected(format!("sqlite: failed to read row: {e}")))?
        {
            let mut kvmap: Vec<(String, String)> = Vec::with_capacity(col_names.len());
            for (idx, name) in col_names.iter().enumerate() {
                let value: rusqlite::types::Value = row.get(idx).map_err(|e| {
                    EngineError::Unexpected(format!("sqlite: column read error: {e}"))
                })?;
                kvmap.push((name.clone(), sqlite_value_to_string(&value)));
            }

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

/// Render a SQLite cell the way Python's `str(value)` would for the types
/// `sqlite3.Row` can produce. NULL maps to the literal `"None"` to match the
/// upstream `map(str, row)` behavior; BLOBs are decoded lossily (upstream's
/// `str(bytes)` produces a `b'...'` repr, which is not reproduced here).
fn sqlite_value_to_string(value: &rusqlite::types::Value) -> String {
    match value {
        rusqlite::types::Value::Null => "None".to_string(),
        rusqlite::types::Value::Integer(i) => i.to_string(),
        rusqlite::types::Value::Real(f) => f.to_string(),
        rusqlite::types::Value::Text(s) => s.clone(),
        rusqlite::types::Value::Blob(b) => String::from_utf8_lossy(b).to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_db(sql: &[&str]) -> tempfile::TempPath {
        let file = tempfile::NamedTempFile::new().unwrap();
        let path = file.into_temp_path();
        let conn = Connection::open(&path).unwrap();
        for stmt in sql {
            conn.execute(stmt, []).unwrap();
        }
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
}
