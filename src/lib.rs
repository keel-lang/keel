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

// ── Syntax layer (extracted into the `keel-syntax` crate) ────────────────────
// Re-exported under their original paths so internal modules keep using
// `crate::ast`, `crate::lexer`, `crate::parser`, etc. unchanged.
pub use keel_syntax::ast;
pub(crate) use keel_syntax::{formatter, lexer, lint, parser};

// Compiler middle-end (HIR, type checker, module graph, IDE queries),
// extracted into the `keel-compiler` crate. Re-exported under their original
// paths so internal modules keep using `crate::types`, `crate::hir`,
// `crate::ide`, and `crate::modules` unchanged.
pub use keel_compiler::modules;
pub(crate) use keel_compiler::{hir, ide, types};

// Execution engine (interpreter + runtime), extracted into the `keel-runtime`
// crate. Re-exported as crate-private so `session`, `pipeline`, the REPL, and
// the CLI keep using `crate::interpreter` and `crate::runtime` unchanged — and
// so the external embedding API is unchanged (these stay non-public).
pub(crate) use keel_runtime::{interpreter, runtime};

// ── Public embedding API ─────────────────────────────────────────────────────
pub mod catalog;
pub mod diagnostics;
pub mod session;

// ── Internal implementation modules ─────────────────────────────────────────
pub(crate) mod cli;
pub(crate) mod lsp;
pub(crate) mod pipeline;
pub(crate) mod repl;
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
