//! Types — placeholder module for v0.1.
//!
//! The full type checker is being rewritten to use the new AST shape
//! (interfaces, attributes, removed AI/IO/scheduling primitives). Until
//! then, this module exposes a `checker::check` function that always
//! returns no errors, so the pipeline can be exercised end-to-end.

pub mod checker;
