//! Keel execution engine: the tree-walking async interpreter and the runtime
//! (host context, stdlib namespace implementations, LLM/email/human I/O).
//!
//! This is the top of the compiler stack: it depends on the syntax, catalog,
//! and compiler layers and adds the heavy I/O dependencies (tokio, reqwest,
//! rusqlite, axum, lettre, imap). The `keel-lang` facade drives it via
//! `session`, `pipeline`, the REPL, and the LSP.
#![deny(clippy::correctness)]
#![warn(clippy::suspicious)]
#![warn(clippy::perf)]
#![warn(clippy::style)]
#![warn(clippy::complexity)]

// Re-export the lower layers under their original paths so the interpreter and
// runtime modules keep using `crate::ast`, `crate::types`, `crate::modules`,
// etc. unchanged.
pub(crate) use keel_compiler::{modules, types};
pub(crate) use keel_syntax::{ast, lexer};

pub mod interpreter;
pub mod runtime;
