//! Type checker for the Keel language.
//!
//! The type checker walks the AST to detect undefined identifiers,
//! scope violations, non-exhaustive `when` matches, arity mismatches,
//! readonly field assignments, struct subtyping errors, and basic
//! nullable-safety issues. When a type cannot be resolved cheaply
//! (generics, closures, complex prelude signatures), it falls back to
//! [`Ty::Unknown`] without reporting an error. Use `keel check --strict`
//! to reject bindings whose type the checker cannot resolve.

pub mod checker;
pub mod interface;
pub mod prelude;
pub(crate) mod resolve;
pub mod scope;
pub mod ty;
