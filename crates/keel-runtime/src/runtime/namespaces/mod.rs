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

/// Registers every stdlib namespace's method closures on `host` via
/// [`Host::register_namespace`]. Registration only inserts each namespace
/// into `host`'s own registry — it never calls a closure — so this is safe
/// to run against any `Host` implementation, including one whose other
/// methods aren't implemented yet (`keel-rt-ffi`'s `CompiledHost`, which
/// reuses this exact set of closures per `designs/llvm-compilation.md`
/// §2.7 instead of re-implementing 23 namespaces for the compiled path).
pub fn install(host: &mut dyn Host) {
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

    /// Every namespace source file below, keyed by the `ns!("name", { .. })`
    /// name it actually registers under (not its filename — `asynchronous.rs`
    /// registers as `"async"`). `include_str!` embeds each file's source at
    /// compile time so this test has no filesystem dependency at runtime.
    fn namespace_sources() -> &'static [&'static str] {
        &[
            include_str!("ai.rs"),
            include_str!("asynchronous.rs"),
            include_str!("cache.rs"),
            include_str!("control.rs"),
            include_str!("crypto.rs"),
            include_str!("csv.rs"),
            include_str!("db.rs"),
            include_str!("email.rs"),
            include_str!("env.rs"),
            include_str!("file.rs"),
            include_str!("http.rs"),
            include_str!("io.rs"),
            include_str!("json.rs"),
            include_str!("log.rs"),
            include_str!("math.rs"),
            include_str!("memory.rs"),
            include_str!("random.rs"),
            include_str!("schedule.rs"),
            include_str!("search.rs"),
            include_str!("shell.rs"),
            include_str!("testing.rs"),
            include_str!("time.rs"),
            include_str!("uuid.rs"),
        ]
    }

    /// Issue #236: catalog `params` drifted from the runtime's actual
    /// `find_arg`/`expect_*_named` argument-name lookups (undeclared params
    /// silently accepted, e.g. `shell.run`'s `stdin:`/`cwd:` before this
    /// fix). This test regexes each namespace source for the literal name in
    /// every named-argument lookup call and asserts it is declared
    /// *somewhere* in that namespace's catalog params (and vice versa: every
    /// catalog param bound `NamedOnly`/`Either` must be looked up somewhere
    /// in that namespace's source) — catching root-cause #3 ("undeclared
    /// params entirely") without parsing Rust or tying a name to one
    /// specific method. It cannot catch a purely positional/binding-mode
    /// mismatch (e.g. required-vs-optional); the `examples/` corpus check
    /// under #222 is the backstop for those.
    #[test]
    fn catalog_named_params_match_runtime_named_arg_lookups() {
        let lookup_re = regex::Regex::new(
            r#"(?:find_arg|expect_str_named|expect_bool_named|expect_duration_named)\(\s*&?args,\s*"([A-Za-z_][A-Za-z0-9_]*)""#,
        )
        .expect("valid regex");
        let ns_name_re = regex::Regex::new(r#"ns!\(\s*"([A-Za-z_]+)""#).expect("valid regex");

        let mut catalog_named: HashMap<&str, HashSet<&str>> = HashMap::new();
        for m in keel_catalog::catalog() {
            for p in m.params {
                if !matches!(
                    p.binding,
                    keel_catalog::builtins::ParamBinding::PositionalOnly
                ) {
                    catalog_named.entry(m.namespace).or_default().insert(p.name);
                }
            }
        }

        for src in namespace_sources() {
            let Some(ns_name) = ns_name_re.captures(src).map(|c| c[1].to_string()) else {
                continue;
            };
            let runtime_named: HashSet<&str> = lookup_re
                .captures_iter(src)
                .map(|c| c.get(1).unwrap().as_str())
                .collect();
            let declared = catalog_named
                .get(ns_name.as_str())
                .cloned()
                .unwrap_or_default();

            let undeclared: Vec<&str> = runtime_named.difference(&declared).copied().collect();
            assert!(
                undeclared.is_empty(),
                "{ns_name}: runtime looks up named argument(s) {undeclared:?} that no \
                 catalog param declares (add them to specs/{ns_name}.rs, or to the file \
                 matching this namespace if the filename differs)"
            );

            let stale: Vec<&str> = declared
                .difference(&runtime_named)
                .copied()
                .filter(|name| !KNOWN_GAPS.contains(&(ns_name.as_str(), *name)))
                .collect();
            assert!(
                stale.is_empty(),
                "{ns_name}: catalog declares named param(s) {stale:?} that the runtime \
                 never looks up by name — stale entry, or the runtime reads it through a \
                 helper this test doesn't recognize"
            );
        }
    }

    /// `(namespace, param)` pairs where the catalog correctly declares a
    /// named param a real `examples/` call site passes, but this test's
    /// `find_arg`/`expect_*_named`-only regex can't confirm the runtime
    /// reads it — either because it's a genuine no-op (tracked separately;
    /// not a documentation slip) or because the runtime reads it through a
    /// different mechanism this test doesn't parse for.
    const KNOWN_GAPS: &[(&str, &str)] = &[
        // examples/multi_agent_inbox.keel passes `context:`; ai.draft never
        // folds it into the prompt — genuine no-op.
        ("ai", "context"),
        // examples/data_pipeline.keel and examples/struct_types.keel pass
        // `format:`; ai.draft never reads it — genuine no-op.
        ("ai", "format"),
        // examples/multi_agent_inbox.keel passes `limit:`; memory.recall has
        // no result-limiting behavior at all — genuine no-op.
        ("memory", "limit"),
        // http.request reads these via `cfg.get(&MapKey::Str("name".into()))`
        // against a map assembled from `args` (SPEC.md §17.2's all-named
        // calling convention), not `find_arg`/`expect_*_named` — read, just
        // not through a pattern this regex recognizes.
        ("http", "method"),
        ("http", "url"),
        // SPEC.md §17.2 documents `timeout:` for http.request; the runtime
        // never reads it at all — genuine no-op.
        ("http", "timeout"),
    ];
}
