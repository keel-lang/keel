//! Keel — a programming language where AI agents are first-class citizens.
//!
//! This crate provides the compiler pipeline (lexer → parser → type checker),
//! the tree-walking async interpreter, the runtime prelude namespaces, and
//! the formatter, linter, REPL, and LSP server.
#![deny(clippy::correctness)]
#![warn(clippy::suspicious)]
#![warn(clippy::perf)]
#![warn(clippy::style)]
#![warn(clippy::complexity)]

pub mod ast;
pub mod formatter;
pub mod interpreter;
pub mod lexer;
pub mod lint;
pub mod lsp;
pub mod parser;
pub mod pipeline;
pub mod repl;
pub mod runtime;
pub mod types;
pub mod vm;

#[doc(inline)]
pub use ast::visit as ast_visit;
