// Rust guideline compliant 2026-02-21
use std::collections::HashMap;
use std::fmt;
use std::sync::Arc;

use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::Namespace;
use crate::interpreter::RuntimeErrorKind;
use crate::interpreter::value::{MapKey, Value};
use crate::runtime::args::expect_str;
use crate::runtime::db_provider::{DbConnectionHandle, DbFuture};
use crate::runtime::namespace::{make_typed_report, ns};

pub(crate) const SPEC: &[BuiltinMethod] = &[BuiltinMethod {
    namespace: "Db",
    name: "connect",
    params: &[BuiltinParam {
        name: "url",
        ty: TySpec::Str,
        optional: false,
    }],
    result: BuiltinResult::Fixed(TySpec::DbConnection),
    doc: "Open a database connection and return a DbConnection.",
}];

// ── SQLite backend ────────────────────────────────────────────────────────────

/// SQLite connection backed by `rusqlite` with the bundled SQLite library.
///
/// Implements [`DbConnectionHandle`] so it can be stored as
/// `Arc<dyn DbConnectionHandle>` inside `Value::DbConnection`.  All blocking
/// calls are dispatched via `tokio::task::spawn_blocking`.
pub struct SqliteConnection {
    conn: Arc<parking_lot::Mutex<rusqlite::Connection>>,
    url: String,
}

impl SqliteConnection {
    fn new(conn: rusqlite::Connection, url: String) -> Self {
        Self {
            conn: Arc::new(parking_lot::Mutex::new(conn)),
            url,
        }
    }
}

impl fmt::Debug for SqliteConnection {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "SqliteConnection({})", self.url)
    }
}

impl DbConnectionHandle for SqliteConnection {
    fn query(&self, sql: String, params: Vec<Value>) -> DbFuture<Vec<Value>> {
        let conn = Arc::clone(&self.conn);
        let url = self.url.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let locked = conn.lock();
                let mut stmt = locked.prepare(&sql).map_err(|e| {
                    make_typed_report(RuntimeErrorKind::Db, format!("Db.query ({url}): {e}"))
                })?;
                let col_count = stmt.column_count();
                // Detect duplicate column names up front so each occurrence
                // gets a unique key (e.g. SELECT a.id, b.id → "id_0", "id_1").
                let base_names: Vec<String> = (0..col_count)
                    .map(|i| stmt.column_name(i).unwrap_or("?column?").to_string())
                    .collect();
                let mut name_counts: HashMap<&str, usize> = HashMap::new();
                for n in &base_names {
                    *name_counts.entry(n.as_str()).or_insert(0) += 1;
                }
                let col_names: Vec<String> = base_names
                    .iter()
                    .enumerate()
                    .map(|(i, n)| {
                        if name_counts[n.as_str()] > 1 {
                            format!("{n}_{i}")
                        } else {
                            n.clone()
                        }
                    })
                    .collect();
                let rusqlite_params: Vec<rusqlite::types::Value> = params
                    .iter()
                    .map(keel_to_rusqlite)
                    .collect::<miette::Result<_>>()?;
                let mapped = stmt
                    .query_map(rusqlite::params_from_iter(rusqlite_params.iter()), |row| {
                        let mut map = HashMap::new();
                        for (i, name) in col_names.iter().enumerate() {
                            let rv: rusqlite::types::Value = row.get(i)?;
                            map.insert(MapKey::Str(name.clone()), rusqlite_to_keel(rv));
                        }
                        Ok(Value::Map(map))
                    })
                    .map_err(|e| {
                        make_typed_report(RuntimeErrorKind::Db, format!("Db.query ({url}): {e}"))
                    })?;
                let rows: rusqlite::Result<Vec<Value>> = mapped.collect();
                rows.map_err(|e| {
                    make_typed_report(
                        RuntimeErrorKind::Db,
                        format!("Db.query ({url}): row error: {e}"),
                    )
                })
            })
            .await
            .map_err(|e| miette::miette!("Db.query: task join error: {e}"))?
        })
    }

    fn exec(&self, sql: String, params: Vec<Value>) -> DbFuture<i64> {
        let conn = Arc::clone(&self.conn);
        let url = self.url.clone();
        Box::pin(async move {
            tokio::task::spawn_blocking(move || {
                let locked = conn.lock();
                let rusqlite_params: Vec<rusqlite::types::Value> = params
                    .iter()
                    .map(keel_to_rusqlite)
                    .collect::<miette::Result<_>>()?;
                locked
                    .execute(&sql, rusqlite::params_from_iter(rusqlite_params.iter()))
                    .map(|n| n as i64)
                    .map_err(|e| {
                        make_typed_report(RuntimeErrorKind::Db, format!("Db.exec ({url}): {e}"))
                    })
            })
            .await
            .map_err(|e| miette::miette!("Db.exec: task join error: {e}"))?
        })
    }
}

// ── Db namespace ──────────────────────────────────────────────────────────────

pub(crate) fn namespace() -> Namespace {
    ns!("Db", {
        "connect" => |_i, args| Box::pin(async move {
            let url = expect_str(&args, 0, "Db.connect")?.to_owned();

            let path = parse_sqlite_url(&url)?;
            let path_for_open = path.clone();

            let conn = tokio::task::spawn_blocking(move || {
                rusqlite::Connection::open(&path_for_open)
                    .map_err(|e| make_typed_report(RuntimeErrorKind::Db, format!("Db.connect: cannot open `{path_for_open}`: {e}")))
            })
            .await
            .map_err(|e| miette::miette!("Db.connect: task error: {e}"))??;

            let handle: Arc<dyn DbConnectionHandle> =
                Arc::new(SqliteConnection::new(conn, url.clone()));
            Ok(Value::DbConnection(url, handle))
        }),
    })
}

/// Parse a `sqlite://…` connection URL and return the file-system path.
///
/// Accepted forms:
/// - `sqlite://relative/path.db`  → `relative/path.db`
/// - `sqlite:///absolute/path.db` → `/absolute/path.db`
/// - `sqlite://:memory:`          → `:memory:` (rusqlite in-memory DB)
fn parse_sqlite_url(url: &str) -> miette::Result<String> {
    if let Some(path) = url.strip_prefix("sqlite://") {
        if path.is_empty() {
            return Err(make_typed_report(
                RuntimeErrorKind::Db,
                format!(
                    "Db.connect: missing path in URL `{url}`; \
                     use sqlite://path.db, sqlite:///abs/path.db, or sqlite://:memory:"
                ),
            ));
        }
        Ok(path.to_string())
    } else if url.starts_with("postgres://") || url.starts_with("mysql://") {
        Err(make_typed_report(
            RuntimeErrorKind::Db,
            format!("Db.connect: `{url}` — only sqlite:// is supported in v0.1"),
        ))
    } else {
        Err(make_typed_report(
            RuntimeErrorKind::Db,
            format!(
                "Db.connect: unrecognized URL `{url}`; expected sqlite://path or sqlite://:memory:"
            ),
        ))
    }
}

// ── Value conversion helpers ──────────────────────────────────────────────────

/// Convert a Keel [`Value`] to its rusqlite SQL parameter equivalent.
///
/// Only scalar types are valid SQL parameters. Passing a complex value
/// (list, map, struct, closure, …) is a type error at the call site.
pub(crate) fn keel_to_rusqlite(v: &Value) -> miette::Result<rusqlite::types::Value> {
    match v {
        Value::Integer(n) => Ok(rusqlite::types::Value::Integer(*n)),
        Value::Float(f) => Ok(rusqlite::types::Value::Real(*f)),
        Value::String(s) => Ok(rusqlite::types::Value::Text(s.clone())),
        // SQLite has no native bool — store as 0/1 integer.
        Value::Bool(b) => Ok(rusqlite::types::Value::Integer(*b as i64)),
        Value::None => Ok(rusqlite::types::Value::Null),
        other => Err(miette::miette!(
            "SQL parameter must be a scalar (int, float, str, bool, or none), \
             got {}",
            other.type_name()
        )),
    }
}

/// Convert a rusqlite column value to the corresponding Keel [`Value`].
///
/// `BLOB` columns are returned as `list[int]` (raw byte values 0–255) because
/// Keel has no binary type. This is lossless; the caller can base-64-encode or
/// interpret the bytes as needed.
pub(crate) fn rusqlite_to_keel(v: rusqlite::types::Value) -> Value {
    match v {
        rusqlite::types::Value::Integer(n) => Value::Integer(n),
        rusqlite::types::Value::Real(f) => Value::Float(f),
        rusqlite::types::Value::Text(s) => Value::String(s),
        rusqlite::types::Value::Blob(b) => Value::List(
            b.into_iter()
                .map(|byte| Value::Integer(byte as i64))
                .collect(),
        ),
        rusqlite::types::Value::Null => Value::None,
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{CallArgValue, Interpreter};

    fn arg(v: Value) -> CallArgValue {
        CallArgValue {
            name: None,
            value: v,
        }
    }

    #[test]
    fn namespace_exposes_connect() {
        let ns = namespace();
        assert_eq!(ns.name, "Db");
        assert!(ns.methods.contains_key("connect"));
    }

    #[test]
    fn parse_sqlite_url_relative() {
        assert_eq!(parse_sqlite_url("sqlite://trades.db").unwrap(), "trades.db");
    }

    #[test]
    fn parse_sqlite_url_absolute() {
        assert_eq!(
            parse_sqlite_url("sqlite:///tmp/trades.db").unwrap(),
            "/tmp/trades.db"
        );
    }

    #[test]
    fn parse_sqlite_url_memory() {
        assert_eq!(parse_sqlite_url("sqlite://:memory:").unwrap(), ":memory:");
    }

    #[test]
    fn parse_sqlite_url_rejects_postgres() {
        let err = parse_sqlite_url("postgres://localhost/db").unwrap_err();
        assert!(err.to_string().contains("only sqlite://"));
    }

    #[test]
    fn parse_sqlite_url_rejects_unknown_scheme() {
        let err = parse_sqlite_url("notadb://foo").unwrap_err();
        assert!(err.to_string().contains("unrecognized URL"));
    }

    #[tokio::test]
    async fn connect_memory_returns_db_connection() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let connect = ns.methods.get("connect").unwrap();
        let result = connect(
            &mut interp,
            vec![arg(Value::String("sqlite://:memory:".into()))],
        )
        .await
        .unwrap();
        assert!(matches!(result, Value::DbConnection(_, _)));
    }

    #[tokio::test]
    async fn connect_missing_arg_returns_error() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let connect = ns.methods.get("connect").unwrap();
        let err = connect(&mut interp, vec![]).await.unwrap_err();
        assert!(err.to_string().contains("missing argument at position 0"));
    }

    #[tokio::test]
    async fn connect_bad_scheme_returns_error() {
        let ns = namespace();
        let mut interp = Interpreter::default();
        let connect = ns.methods.get("connect").unwrap();
        let err = connect(
            &mut interp,
            vec![arg(Value::String("postgres://localhost/db".into()))],
        )
        .await
        .unwrap_err();
        assert!(err.to_string().contains("only sqlite://"));
    }

    #[tokio::test]
    async fn sqlite_query_and_exec_roundtrip() {
        let conn = rusqlite::Connection::open(":memory:").unwrap();
        let handle = SqliteConnection::new(conn, "sqlite://:memory:".into());

        handle
            .exec("CREATE TABLE t (k TEXT, v INTEGER)".into(), vec![])
            .await
            .unwrap();
        handle
            .exec(
                "INSERT INTO t VALUES (?, ?)".into(),
                vec![Value::String("hello".into()), Value::Integer(42)],
            )
            .await
            .unwrap();

        let rows = handle
            .query("SELECT k, v FROM t".into(), vec![])
            .await
            .unwrap();

        assert_eq!(rows.len(), 1);
        if let Value::Map(row) = &rows[0] {
            assert_eq!(
                row.get(&MapKey::Str("k".into())),
                Some(&Value::String("hello".into()))
            );
            assert_eq!(row.get(&MapKey::Str("v".into())), Some(&Value::Integer(42)));
        } else {
            panic!("expected a map row");
        }
    }
}
