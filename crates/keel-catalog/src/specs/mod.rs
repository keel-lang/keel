//! Aggregated stdlib namespace method catalog and capability metadata.

use crate::builtins::BuiltinMethod;

pub mod ai;
pub mod asynchronous;
pub mod cache;
pub mod control;
pub mod crypto;
pub mod csv;
pub mod db;
pub mod email;
pub mod env;
pub mod file;
pub mod http;
pub mod io;
pub mod json;
pub mod log;
pub mod math;
pub mod memory;
pub mod random;
pub mod schedule;
pub mod search;
pub mod shell;
pub mod testing;
pub mod time;
pub mod uuid;

/// Return an iterator over every [`BuiltinMethod`] declared across all stdlib
/// namespaces. This is the authoritative catalog: the type checker, the LSP,
/// and the docs generator all derive their method lists from here, and the
/// runtime cross-checks its installed methods against it.
pub fn catalog() -> impl Iterator<Item = &'static BuiltinMethod> {
    const ALL: &[&[BuiltinMethod]] = &[
        ai::SPEC,
        asynchronous::SPEC,
        cache::SPEC,
        control::SPEC,
        crypto::SPEC,
        csv::SPEC,
        db::SPEC,
        email::SPEC,
        env::SPEC,
        file::SPEC,
        http::SPEC,
        io::SPEC,
        json::SPEC,
        log::SPEC,
        math::SPEC,
        memory::SPEC,
        random::SPEC,
        schedule::SPEC,
        search::SPEC,
        shell::SPEC,
        testing::SPEC,
        time::SPEC,
        uuid::SPEC,
    ];
    ALL.iter().flat_map(|s| s.iter())
}

/// Look up a built-in method by namespace and name.
///
/// Returns `None` if the pair is not registered in the catalog.
pub fn catalog_method(namespace: &str, name: &str) -> Option<&'static BuiltinMethod> {
    catalog().find(|m| m.namespace == namespace && m.name == name)
}

/// Modules whose entry points exercise authority over the world outside the
/// process — network, filesystem, subprocesses, external services, ambient
/// secrets, humans, and LLMs. Only these require an `@tools` capability.
///
/// The rule: a capability guards *effects*; pure computation and internal
/// control flow (json, math, time, schedule, …) are never gated.
const CAPABILITY_GATED: &[&str] = &[
    "ai", "io", "http", "email", "file", "shell", "db", "search", "env",
];

/// Whether calls to `namespace` require an `@tools` capability inside an
/// agent turn. Consulted by both the type checker and the runtime gate.
pub fn module_requires_capability(namespace: &str) -> bool {
    CAPABILITY_GATED.contains(&namespace)
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    #[test]
    fn catalog_has_no_duplicate_entries() {
        let mut seen = HashSet::new();
        for m in catalog() {
            assert!(
                seen.insert((m.namespace, m.name)),
                "duplicate catalog entry: {}.{}",
                m.namespace,
                m.name
            );
        }
    }
}
