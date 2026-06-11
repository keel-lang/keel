//! Keel — a programming language where AI agents are first-class citizens.
//!
//! This crate provides the compiler pipeline (lexer → parser → HIR → type checker),
//! the tree-walking async interpreter, the runtime prelude namespaces, and
//! the formatter, linter, REPL, and LSP server.
//!
//! # Public embedding API
//!
//! External consumers (embedding, tooling) should use only these three modules:
//!
//! - [`ast`] — AST node types
//! - [`session`] — `parse_source`, `check_source`, `run_source`, `fmt_source`, `lint_source`
//! - [`diagnostics`] — [`diagnostics::TypeDiagnostic`], [`diagnostics::LintWarning`], [`diagnostics::Ty`]
#![deny(clippy::correctness)]
#![warn(clippy::suspicious)]
#![warn(clippy::perf)]
#![warn(clippy::style)]
#![warn(clippy::complexity)]

// ── Public embedding API ─────────────────────────────────────────────────────
pub mod ast;
pub mod catalog;
pub mod diagnostics;
pub mod modules;
pub mod session;

// ── Internal implementation modules ─────────────────────────────────────────
pub(crate) mod builtins;
pub(crate) mod cli;
pub(crate) mod formatter;
pub(crate) mod hir;
pub(crate) mod ide;
pub(crate) mod interpreter;
pub(crate) mod lexer;
pub(crate) mod lint;
pub(crate) mod lsp;
pub(crate) mod parser;
pub(crate) mod pipeline;
pub(crate) mod repl;
pub(crate) mod runtime;
pub(crate) mod types;
pub(crate) mod vm;

/// Entry point for the `keel` CLI binary.
///
/// # Runtime
///
/// Must be called from within a Tokio async runtime (the binary uses
/// `#[tokio::main]`). Calling this from a non-Tokio executor will panic.
///
/// # Process exit
///
/// Spawns a background task that calls [`std::process::exit(130)`] when
/// Ctrl-C is received. This is intentional for the interactive CLI but
/// means this function should **not** be called from a host binary that
/// needs to manage its own SIGINT lifecycle.
pub async fn run() -> miette::Result<()> {
    cli::run().await
}
