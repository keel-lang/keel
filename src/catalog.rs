//! Public catalog API for tooling that enumerates stdlib namespace methods.
//!
//! Consumers of this API (docs generators, IDE plugins, CI validators) use
//! [`catalog()`] to iterate every built-in method and inspect its name,
//! namespace, parameters, and return type.
//!
//! # Example
//! ```
//! for method in keel_lang::catalog::catalog() {
//!     println!("{}.{}", method.namespace, method.name);
//! }
//! ```

pub use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};

/// Iterate every built-in namespace method registered in the stdlib catalog.
///
/// The returned iterator visits each method exactly once in an unspecified
/// but stable order within a single binary build.
pub fn catalog() -> impl Iterator<Item = &'static BuiltinMethod> {
    crate::types::prelude::catalog()
}
