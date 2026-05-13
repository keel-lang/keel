//! AST node types for the Keel language.
//!
//! The AST is produced by the parser and consumed by the type checker,
//! interpreter, formatter, and linter. Every node carries a [`Span`] so that
//! diagnostics can point back to source positions.

pub mod decl;
pub mod expr;
pub mod stmt;
pub mod ty;
pub mod visit;

use crate::lexer::Span;

pub use decl::*;
pub use expr::*;
pub use stmt::*;
pub use ty::*;

#[derive(Debug, Clone)]
pub struct Program {
    pub declarations: Vec<Spanned<Decl>>,
}

pub type Spanned<T> = (T, Span);
