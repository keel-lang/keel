use crate::builtins::BuiltinMethod;
use crate::interpreter::{Host, Namespace};

pub(crate) mod agent;
mod ai;
mod asynchronous;
mod cache;
mod control;
mod crypto;
mod csv;
mod db;
mod email;
mod env;
mod file;
mod http;
mod io;
mod json;
mod log;
mod math;
pub(crate) mod memory;
mod random;
mod schedule;
mod search;
mod shell;
mod testing;
mod time;
pub(crate) mod uuid;

/// Return an iterator over every [`BuiltinMethod`] declared across all
/// runtime namespaces. This is the authoritative runtime catalog — the
/// checker's `types::prelude::catalog()` delegates here.
///
/// NOTE: `types::prelude` depends on this function, creating a
/// `types → runtime` dependency. Within a single crate this compiles
/// cleanly. A future crate split would require extracting `BuiltinMethod`
/// to a neutral leaf crate first.
pub(crate) fn catalog() -> impl Iterator<Item = &'static BuiltinMethod> {
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
pub(crate) fn module_requires_capability(namespace: &str) -> bool {
    CAPABILITY_GATED.contains(&namespace)
}

pub(crate) fn install(host: &mut dyn Host) {
    for namespace in namespaces() {
        host.register_namespace(namespace);
    }
}

fn namespaces() -> [Namespace; 23] {
    [
        io::namespace(),
        schedule::namespace(),
        ai::namespace(),
        email::namespace(),
        env::namespace(),
        memory::namespace(),
        log::namespace(),
        control::namespace(),
        asynchronous::namespace(),
        http::namespace(),
        search::namespace(),
        db::namespace(),
        time::namespace(),
        file::namespace(),
        json::namespace(),
        cache::namespace(),
        random::namespace(),
        uuid::namespace(),
        crypto::namespace(),
        math::namespace(),
        shell::namespace(),
        testing::namespace(),
        csv::namespace(),
    ]
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    /// Every method name declared in a namespace's SPEC must be installed by
    /// its `namespace()` function, and every installed method must appear in
    /// SPEC. This catches additions to one side without the other.
    #[test]
    fn spec_matches_installed_methods_for_all_namespaces() {
        let pairs: &[(&[crate::builtins::BuiltinMethod], Namespace)] = &[
            (ai::SPEC, ai::namespace()),
            (asynchronous::SPEC, asynchronous::namespace()),
            (cache::SPEC, cache::namespace()),
            (control::SPEC, control::namespace()),
            (crypto::SPEC, crypto::namespace()),
            (csv::SPEC, csv::namespace()),
            (db::SPEC, db::namespace()),
            (email::SPEC, email::namespace()),
            (env::SPEC, env::namespace()),
            (file::SPEC, file::namespace()),
            (http::SPEC, http::namespace()),
            (io::SPEC, io::namespace()),
            (json::SPEC, json::namespace()),
            (log::SPEC, log::namespace()),
            (math::SPEC, math::namespace()),
            (memory::SPEC, memory::namespace()),
            (random::SPEC, random::namespace()),
            (schedule::SPEC, schedule::namespace()),
            (search::SPEC, search::namespace()),
            (shell::SPEC, shell::namespace()),
            (testing::SPEC, testing::namespace()),
            (time::SPEC, time::namespace()),
            (uuid::SPEC, uuid::namespace()),
        ];

        for (spec, ns) in pairs {
            let ns_name = spec.first().map_or("?", |m| m.namespace);
            let spec_names: HashSet<&str> = spec.iter().map(|m| m.name).collect();
            let installed_names: HashSet<&str> = ns.methods.keys().map(|s| s.as_str()).collect();

            let in_spec_not_installed: Vec<&str> =
                spec_names.difference(&installed_names).copied().collect();
            assert!(
                in_spec_not_installed.is_empty(),
                "{ns_name}: SPEC declares {:?} but namespace() does not install them",
                in_spec_not_installed
            );

            let installed_not_in_spec: Vec<&str> =
                installed_names.difference(&spec_names).copied().collect();
            assert!(
                installed_not_in_spec.is_empty(),
                "{ns_name}: namespace() installs {:?} but SPEC does not declare them",
                installed_not_in_spec
            );
        }
    }

    #[test]
    fn catalog_has_no_duplicate_entries() {
        let mut seen = HashSet::new();
        for entry in catalog() {
            let key = format!("{}.{}", entry.namespace, entry.name);
            assert!(
                seen.insert(key),
                "duplicate catalog entry: {}.{}",
                entry.namespace,
                entry.name
            );
        }
    }
}
