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

/// Stable id for each of the 23 stdlib namespaces, consumed by `keel-kir`'s
/// `CallTarget::Ns` lowering and `keel-rt-ffi`'s `keel_rt_call_ns(ns_id,
/// method_id, ...)` generic dispatch entry point. Assigned once and
/// append-only: never reassign or reuse an id when a namespace is removed,
/// so a compiled program's dispatch ids stay meaningful across commits.
///
/// Paired with each method's [`BuiltinMethod::method_id`] (unique within the
/// namespace) this gives every stdlib method a globally unique `(ns_id,
/// method_id)` pair. The `spec_matches_installed_methods_for_all_namespaces`
/// test in `keel-runtime` pins both halves.
pub const NAMESPACE_IDS: &[(&str, u16)] = &[
    ("ai", 0),
    ("async", 1),
    ("cache", 2),
    ("control", 3),
    ("crypto", 4),
    ("csv", 5),
    ("db", 6),
    ("email", 7),
    ("env", 8),
    ("file", 9),
    ("http", 10),
    ("io", 11),
    ("json", 12),
    ("log", 13),
    ("math", 14),
    ("memory", 15),
    ("random", 16),
    ("schedule", 17),
    ("search", 18),
    ("shell", 19),
    ("testing", 20),
    ("time", 21),
    ("uuid", 22),
];

/// Look up the stable [`NAMESPACE_IDS`] id for a namespace by name.
///
/// Returns `None` if `namespace` is not a registered stdlib namespace.
pub fn namespace_id(namespace: &str) -> Option<u16> {
    NAMESPACE_IDS
        .iter()
        .find(|(name, _)| *name == namespace)
        .map(|(_, id)| *id)
}

/// Reverse of [`namespace_id`] — the namespace name for a stable id.
///
/// Consumed by `keel-rt-ffi`'s `keel_rt_call_ns`, which only receives numeric
/// ids across the FFI boundary and needs the name to dispatch through the
/// existing (name-keyed) `Namespace.methods` registry.
pub fn namespace_by_id(ns_id: u16) -> Option<&'static str> {
    NAMESPACE_IDS
        .iter()
        .find(|(_, id)| *id == ns_id)
        .map(|(name, _)| *name)
}

/// Reverse of [`BuiltinMethod::method_id`] — the method name for a
/// `(namespace, method_id)` pair. See [`namespace_by_id`].
pub fn method_by_id(namespace: &str, method_id: u16) -> Option<&'static str> {
    catalog()
        .find(|m| m.namespace == namespace && m.method_id == method_id)
        .map(|m| m.name)
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
