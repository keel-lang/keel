//! Keel front-end: lexer, AST, parser, formatter, and linter.
//!
//! This crate is the syntax layer of the Keel compiler. It has no dependency on
//! the type checker, interpreter, or runtime — it turns source text into an AST
//! and provides the pretty-printer and AST-only lints.

pub mod ast;
pub mod diagnostics;
pub mod formatter;
pub mod lexer;
pub mod lint;
pub mod parser;

pub use diagnostics::LintWarning;
pub use lexer::Span;

use ast::Program;
use miette::{NamedSource, Result};

/// Lex and parse source text into an AST.
///
/// Returns the parsed [`Program`] together with the [`NamedSource`] built
/// during lexing, so callers can pass it straight to the type checker without
/// allocating a second copy of the source string.
///
/// # Errors
///
/// Returns an error if the source cannot be lexed or parsed.
pub fn parse_source(src: &str, name: &str) -> Result<(Program, NamedSource<String>)> {
    let named = NamedSource::new(name, src.to_string());
    let tokens = lexer::lex(src, &named)?;
    let program = parser::parse(tokens, src.len(), &named)?;
    Ok((program, named))
}
