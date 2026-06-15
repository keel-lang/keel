//! AST node types for the Keel language.
//!
//! The AST is produced by the parser and lowered into HIR for the type checker
//! and IDE. The interpreter, formatter, and linter still consume AST directly.
//! Every node carries a [`crate::lexer::Span`] so that
//! diagnostics can point back to source positions.
//!
//! ## Node wrapper vs. lexer tuples
//!
//! AST nodes use [`Node<T>`] (`.kind` + `.span`) rather than the old
//! `(T, Span)` tuple.  The lexer continues to expose `Spanned<Token>` tuples
//! because chumsky 0.9's stream API expects that shape.

pub mod decl;
pub mod expr;
pub mod node;
pub mod stmt;
pub mod ty;
pub mod visit;

pub use decl::*;
pub use expr::*;
pub use node::Node;
pub use stmt::*;
pub use ty::*;

/// The top-level program node.
#[derive(Debug, Clone)]
pub struct Program {
    pub declarations: Vec<Node<Decl>>,
}
