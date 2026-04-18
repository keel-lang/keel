//! VM — placeholder module for v0.1.
//!
//! `keel build` / `.keelc` execution is deferred: v0.1 ships with the
//! tree-walking interpreter only. A bytecode compiler and register-based
//! VM will land in a later release.

pub mod bytecode;
pub mod compiler;
pub mod machine;
