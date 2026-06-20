//! Keel stdlib namespace catalog.
//!
//! A neutral leaf crate holding the built-in method *descriptors* (names,
//! signatures, return types) and capability metadata. Both the type checker
//! and the runtime depend on this crate, so neither has to depend on the other
//! to know what the stdlib surface looks like.
//!
//! This crate contains no executable namespace code — only data.

pub mod builtins;
pub mod providers;
pub mod specs;

pub use providers::{BUILTIN_LLM_PROVIDERS, is_builtin_llm_provider};
pub use specs::{catalog, catalog_method, module_requires_capability};
