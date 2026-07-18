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
    use std::collections::{HashMap, HashSet};

    use super::*;

    /// Every method name declared in the catalog's SPEC for a namespace must be
    /// installed by that namespace's `namespace()` function, and every installed
    /// method must appear in the catalog. Driven off the production
    /// `namespaces()` array and `keel_catalog::catalog()` so adding a namespace
    /// or a method on either side without the other fails here.
    #[test]
    fn spec_matches_installed_methods_for_all_namespaces() {
        let mut spec_by_ns: HashMap<&str, HashSet<&str>> = HashMap::new();
        for m in keel_catalog::catalog() {
            spec_by_ns.entry(m.namespace).or_default().insert(m.name);
        }

        let installed = namespaces();
        let installed_ns_names: HashSet<&str> =
            installed.iter().map(|ns| ns.name.as_str()).collect();
        let spec_ns_names: HashSet<&str> = spec_by_ns.keys().copied().collect();
        assert_eq!(
            installed_ns_names, spec_ns_names,
            "installed namespaces and catalog namespaces disagree"
        );

        for ns in &installed {
            let spec_names = &spec_by_ns[ns.name.as_str()];
            let installed_names: HashSet<&str> = ns.methods.keys().map(|s| s.as_str()).collect();

            let in_spec_not_installed: Vec<&str> =
                spec_names.difference(&installed_names).copied().collect();
            assert!(
                in_spec_not_installed.is_empty(),
                "{}: SPEC declares {:?} but namespace() does not install them",
                ns.name,
                in_spec_not_installed
            );

            let installed_not_in_spec: Vec<&str> =
                installed_names.difference(spec_names).copied().collect();
            assert!(
                installed_not_in_spec.is_empty(),
                "{}: namespace() installs {:?} but SPEC does not declare them",
                ns.name,
                installed_not_in_spec
            );
        }
    }

    /// Every namespace installed by `namespaces()` must have a stable
    /// `ns_id` in `keel_catalog::specs::NAMESPACE_IDS`, ids must be unique,
    /// and there must be no orphaned entries for namespaces that no longer
    /// exist. Pins the compiled path's `keel_rt_call_ns(ns_id, ..)` dispatch
    /// table against accidental drift (see designs/llvm-compilation.md §2.7).
    #[test]
    fn namespace_ids_are_stable_and_complete() {
        let installed = namespaces();
        let installed_ns_names: HashSet<&str> =
            installed.iter().map(|ns| ns.name.as_str()).collect();

        let mut seen_ids = HashSet::new();
        let mut ided_ns_names = HashSet::new();
        for (name, id) in keel_catalog::specs::NAMESPACE_IDS {
            assert!(
                seen_ids.insert(*id),
                "duplicate ns_id {id} (namespace {name:?})"
            );
            ided_ns_names.insert(*name);
        }

        assert_eq!(
            installed_ns_names, ided_ns_names,
            "installed namespaces and NAMESPACE_IDS disagree"
        );
    }

    /// Every method in the catalog must have a `method_id` unique within its
    /// namespace, so `(ns_id, method_id)` is a globally unique dispatch key.
    #[test]
    fn method_ids_are_unique_within_each_namespace() {
        let mut seen: HashMap<&str, HashSet<u16>> = HashMap::new();
        for m in keel_catalog::catalog() {
            assert!(
                seen.entry(m.namespace).or_default().insert(m.method_id),
                "{}: duplicate method_id {} ({})",
                m.namespace,
                m.method_id,
                m.name
            );
        }
    }
}
