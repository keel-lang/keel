//! Keel compiler middle-end: HIR, type checker, module graph, and IDE queries.
//!
//! Depends only on the syntax layer (`keel-syntax`) and the neutral stdlib
//! catalog (`keel-catalog`). It has no dependency on the interpreter or the
//! runtime — the type checker reads the stdlib surface from `keel-catalog`.

// Re-export the syntax and catalog layers under their original paths so the
// modules below keep using `crate::ast`, `crate::lexer`, `crate::parser`, and
// `crate::builtins` unchanged.
pub(crate) use keel_catalog::builtins;
pub(crate) use keel_syntax::{ast, lexer, parser};

pub mod hir;
pub mod ide;
pub mod modules;
pub mod types;
