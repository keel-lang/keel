//! Public diagnostic types for the Keel embedding API.
//!
//! Provides structured diagnostic types from the type checker and linter
//! so embedders can inspect errors without reaching into internal modules.

pub use crate::lexer::Span;
pub use crate::types::diagnostics::TypeDiagnostic;
pub use crate::types::ty::{Ty, UnknownReason};

/// A linter warning emitted by `session::lint_source` or `keel lint`.
#[derive(Debug)]
pub struct LintWarning {
    pub message: String,
    pub span: Option<Span>,
    pub hint: Option<String>,
    /// Whether `keel lint --fix` can automatically remove this warning's source.
    pub fixable: bool,
}
