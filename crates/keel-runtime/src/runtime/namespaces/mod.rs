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

// The authoritative stdlib catalog and the `@tools` capability metadata live
// in the neutral `keel-catalog` crate (`keel_catalog::catalog()` and
// `keel_catalog::module_requires_capability()`) so the type checker can read
// the stdlib surface without depending on the runtime. The per-namespace SPEC
// tables moved there too; `namespace()` installs the matching executable
// implementations below, cross-checked against the catalog in tests.

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
        let pairs: &[(&[keel_catalog::builtins::BuiltinMethod], Namespace)] = &[
            (keel_catalog::specs::ai::SPEC, ai::namespace()),
            (
                keel_catalog::specs::asynchronous::SPEC,
                asynchronous::namespace(),
            ),
            (keel_catalog::specs::cache::SPEC, cache::namespace()),
            (keel_catalog::specs::control::SPEC, control::namespace()),
            (keel_catalog::specs::crypto::SPEC, crypto::namespace()),
            (keel_catalog::specs::csv::SPEC, csv::namespace()),
            (keel_catalog::specs::db::SPEC, db::namespace()),
            (keel_catalog::specs::email::SPEC, email::namespace()),
            (keel_catalog::specs::env::SPEC, env::namespace()),
            (keel_catalog::specs::file::SPEC, file::namespace()),
            (keel_catalog::specs::http::SPEC, http::namespace()),
            (keel_catalog::specs::io::SPEC, io::namespace()),
            (keel_catalog::specs::json::SPEC, json::namespace()),
            (keel_catalog::specs::log::SPEC, log::namespace()),
            (keel_catalog::specs::math::SPEC, math::namespace()),
            (keel_catalog::specs::memory::SPEC, memory::namespace()),
            (keel_catalog::specs::random::SPEC, random::namespace()),
            (keel_catalog::specs::schedule::SPEC, schedule::namespace()),
            (keel_catalog::specs::search::SPEC, search::namespace()),
            (keel_catalog::specs::shell::SPEC, shell::namespace()),
            (keel_catalog::specs::testing::SPEC, testing::namespace()),
            (keel_catalog::specs::time::SPEC, time::namespace()),
            (keel_catalog::specs::uuid::SPEC, uuid::namespace()),
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
}
