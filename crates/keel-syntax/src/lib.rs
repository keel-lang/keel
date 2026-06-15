//! Keel front-end: lexer, AST, parser, formatter, and linter.
//!
//! This crate is the syntax layer of the Keel compiler. It has no dependency on
//! the type checker, interpreter, or runtime — it turns source text into an AST
//! and provides the pretty-printer and AST-only lints.
#![deny(clippy::correctness)]
#![warn(clippy::suspicious)]
#![warn(clippy::perf)]
#![warn(clippy::style)]
#![warn(clippy::complexity)]

pub mod ast;
pub mod diagnostics;
pub mod formatter;
pub mod lexer;
pub mod lint;
pub mod parser;

pub use diagnostics::LintWarning;
pub use lexer::Span;
