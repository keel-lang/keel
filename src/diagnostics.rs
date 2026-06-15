//! Public diagnostic types for the Keel embedding API.
//!
//! Provides structured diagnostic types from the type checker and linter
//! so embedders can inspect errors without reaching into internal modules.

pub use keel_syntax::{LintWarning, Span};

pub use crate::types::diagnostics::TypeDiagnostic;
pub use crate::types::ty::{Ty, UnknownReason};
