//! Typed AST wrapper that pairs a node value with its source byte range.
//!
//! [`Node<T>`] replaces the ad-hoc `(T, Span)` tuple pattern used throughout
//! the pipeline.  Every AST position that previously stored a `Spanned<T>`
//! tuple now stores a `Node<T>` whose fields have meaningful names:
//!
//! - `.kind`  — the node value (expression, statement, type, …)
//! - `.span`  — the source byte range, suitable for diagnostics and IDE hover
//!
//! The lexer keeps its own `Spanned<Token> = (Token, Span)` tuple because
//! chumsky's stream API consumes that exact shape; the AST layer does not.
//!
//! # Migration note
//!
//! `SpannedExpr`, `Block`, `Program::declarations`, and every
//! `Spanned<TypeExpr>` field have been updated to use `Node<T>`.  The type
//! alias `pub type SpannedExpr = Node<Expr>` provides source compatibility for
//! the large number of call sites that name the alias rather than the inner
//! type.

use crate::lexer::Span;
use std::fmt;

/// An AST node paired with its source byte range.
///
/// Replaces the ad-hoc `(T, Span)` tuples used in earlier pipeline stages.
/// Fields are named rather than positional so that accesses are self-documenting
/// (`node.kind` and `node.span` vs. `node.0` and `node.1`).
#[derive(Clone, PartialEq)]
pub struct Node<T> {
    /// The node value.
    pub kind: T,
    /// Source byte range (`start..end`) corresponding to this node.
    pub span: Span,
}

impl<T> Node<T> {
    /// Creates a new node from a value and its source span.
    #[inline]
    pub fn new(kind: T, span: Span) -> Self {
        Self { kind, span }
    }

    /// Creates a synthetic node with the sentinel `0..0` span.
    ///
    /// Use only for internally-constructed AST nodes that have no source
    /// position (prelude builtins, test helpers, lowering artifacts).
    #[inline]
    pub fn synthetic(kind: T) -> Self {
        Self { kind, span: 0..0 }
    }

    /// Applies a mapping function to the inner value, preserving the span.
    #[inline]
    pub fn map<U, F: FnOnce(T) -> U>(self, f: F) -> Node<U> {
        Node {
            kind: f(self.kind),
            span: self.span,
        }
    }

    /// Borrows the inner value.
    #[inline]
    pub fn as_ref(&self) -> Node<&T> {
        Node {
            kind: &self.kind,
            span: self.span.clone(),
        }
    }
}

impl<T: fmt::Debug> fmt::Debug for Node<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Omit the span in debug output so snapshots stay concise.
        write!(f, "{:?}", self.kind)
    }
}
