//! IDE integration helpers — shared by the language server and other
//! tooling that needs to answer questions about source positions.
//!
//! # Sub-modules
//!
//! - [`symbols`] — token-level navigation: identifier lookup, go-to-definition, find-usages.
//! - [`hover`] — type-inference hover: resolve the type of the identifier under the cursor.

pub mod hover;
pub mod symbols;
