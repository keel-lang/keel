//! Parser for the Keel language.
//!
//! Built on [`chumsky`] 0.9. All sub-parsers return [`BoxedParser`] to avoid
//! the macOS linker crash caused by deeply nested chumsky type parameters.
//! Newlines serve as statement separators — the grammar is newline-sensitive
//! rather than semicolon-delimited.
#![allow(clippy::result_large_err)]

mod common;
mod decl;
mod error;
mod expr;
mod stmt;
mod strings;
mod types;

use chumsky::Stream;
use chumsky::prelude::*;
use miette::NamedSource;

use crate::ast::*;
use crate::lexer::{Span, Spanned, Token};

use common::*;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Parse a complete Keel program from a token stream.
///
/// # Errors
///
/// Returns a miette error with source-span labels if the token stream does not
/// form a valid Keel program.
pub fn parse(
    tokens: Vec<(Token, Span)>,
    source_len: usize,
    named_src: &NamedSource<String>,
) -> miette::Result<Program> {
    let eoi = source_len..source_len + 1;
    let stream = Stream::from_iter(eoi, tokens.into_iter());

    decl::program_parser()
        .parse(stream)
        .map_err(|errors| error::into_miette(errors, named_src))
}

/// Parse a sequence of statements (REPL mode).
///
/// # Errors
///
/// Returns a miette error if the token stream does not form valid statements.
pub fn parse_stmts(
    tokens: Vec<(Token, Span)>,
    source_len: usize,
    named_src: &NamedSource<String>,
) -> miette::Result<Vec<Spanned<Stmt>>> {
    let eoi = source_len..source_len + 1;
    let stream = Stream::from_iter(eoi, tokens.into_iter());

    let parser = newlines()
        .ignore_then(stmt::stmt_parser().separated_by(sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(end());

    parser
        .parse(stream)
        .map_err(|errors| error::into_miette(errors, named_src))
}
