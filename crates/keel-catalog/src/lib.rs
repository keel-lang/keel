//! Keel stdlib namespace catalog.
//!
//! A neutral leaf crate holding the built-in method *descriptors* (names,
//! signatures, return types) and capability metadata. Both the type checker
//! and the runtime depend on this crate, so neither has to depend on the other
//! to know what the stdlib surface looks like.
//!
//! This crate contains no executable namespace code — only data.
#![deny(clippy::correctness)]
#![warn(clippy::suspicious)]
#![warn(clippy::perf)]
#![warn(clippy::style)]
#![warn(clippy::complexity)]

pub mod builtins;
pub mod specs;

pub use specs::{catalog, module_requires_capability};
