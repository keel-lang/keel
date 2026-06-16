//! Syntax-level diagnostic types.

use crate::lexer::Span;

/// A linter warning emitted by `session::lint_source` or `keel lint`.
#[derive(Debug)]
pub struct LintWarning {
    pub message: String,
    pub span: Option<Span>,
    pub hint: Option<String>,
    /// Whether `keel lint --fix` can automatically remove this warning's source.
    pub fixable: bool,
}
