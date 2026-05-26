// Rust guideline compliant 2026-02-21
use std::fmt;
use std::future::Future;
use std::pin::Pin;

use crate::interpreter::value::Value;

/// Result type alias used throughout the DB provider abstraction.
pub type DbResult<T> = miette::Result<T>;

/// Future returned by [`DbConnectionHandle`] methods.
///
/// `'static` bound allows implementations to move owned data (Arc clones,
/// Strings, param vecs) into the future without capturing a borrow on `self`.
pub type DbFuture<T> = Pin<Box<dyn Future<Output = DbResult<T>> + Send + 'static>>;

/// A live database connection that can execute SQL.
///
/// Implement this trait to add a new database backend.  The built-in
/// implementation is [`crate::runtime::namespaces::db::SqliteConnection`].
/// Additional backends (Postgres, MySQL) are planned for v0.2.
///
/// # Contract
///
/// - `query` runs a SELECT and returns one `Value::Map` per row, keyed by
///   column name with values narrowed to the closest Keel primitive.
/// - `exec` runs a data-mutation statement and returns the row count.
/// - Both methods receive params as owned `Value`s; `Value::None` maps to SQL
///   NULL, `Value::Bool` maps to `0`/`1` integer (SQLite convention).
/// - Errors surface as `miette::Report` so the Keel runtime can display them
///   with source context.
pub trait DbConnectionHandle: fmt::Debug + Send + Sync {
    /// Run a SELECT statement and return every row as a map of column → value.
    fn query(&self, sql: String, params: Vec<Value>) -> DbFuture<Vec<Value>>;

    /// Run an INSERT / UPDATE / DELETE and return the number of rows affected.
    fn exec(&self, sql: String, params: Vec<Value>) -> DbFuture<i64>;
}
