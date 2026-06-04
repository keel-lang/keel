//! Prelude catalog — identifiers and built-in interfaces known to the checker
//! before user code is examined.
//!
//! # Namespace method catalog
//!
//! [`catalog()`] and [`catalog_method()`] delegate to
//! `runtime::namespaces::catalog()`, which aggregates the per-module
//! `SPEC` constants co-located with each namespace implementation.
//! Every surface that needs to know about stdlib methods — the checker,
//! the LSP, and the docs generator — must derive that knowledge from
//! [`catalog()`] rather than maintaining its own independent list.
//!
//! NOTE: this module depends on `runtime::namespaces`, creating a
//! `types → runtime` dependency. Within a single crate this compiles
//! cleanly. A future crate split would require extracting `BuiltinMethod`
//! to a neutral leaf crate first.
//!
//! To convert a [`TySpec`] into the runtime [`Ty`] used by the checker,
//! call [`ty_from_spec`].

use std::collections::{HashMap, HashSet};

use crate::ast::{Binding, Node, Param, TaskSig, TypeExpr};
use crate::types::ty::{Ty, UnknownReason};

// Re-export the descriptor types so callers only need to import from here.
pub use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};

/// Convert a [`TySpec`] into the full [`Ty`] used by the checker.
pub(crate) fn ty_from_spec(spec: TySpec) -> Ty {
    match spec {
        TySpec::Int => Ty::Int,
        TySpec::Float => Ty::Float,
        TySpec::Str => Ty::Str,
        TySpec::Bool => Ty::Bool,
        TySpec::None_ => Ty::None_,
        TySpec::Datetime => Ty::Datetime,
        TySpec::Duration => Ty::Duration,
        TySpec::Uuid => Ty::Uuid,
        TySpec::Dynamic => Ty::Dynamic,
        TySpec::DbConnection => Ty::DbConnection,
        TySpec::NullableStr => Ty::Nullable(Box::new(Ty::Str)),
        TySpec::NullableInt => Ty::Nullable(Box::new(Ty::Int)),
        TySpec::NullableFloat => Ty::Nullable(Box::new(Ty::Float)),
        TySpec::NullableUuid => Ty::Nullable(Box::new(Ty::Uuid)),
        TySpec::NullableDatetime => Ty::Nullable(Box::new(Ty::Datetime)),
        TySpec::NullableDynamic => Ty::Nullable(Box::new(Ty::Dynamic)),
        TySpec::ListOfStr => Ty::List(Box::new(Ty::Str)),
        TySpec::ListOfInt => Ty::List(Box::new(Ty::Int)),
        TySpec::ListOfListOfStr => Ty::List(Box::new(Ty::List(Box::new(Ty::Str)))),
        TySpec::ListOfMapStrStr => {
            Ty::List(Box::new(Ty::Map(Box::new(Ty::Str), Box::new(Ty::Str))))
        }
        TySpec::ListOfMapStrDynamic => {
            Ty::List(Box::new(Ty::Map(Box::new(Ty::Str), Box::new(Ty::Dynamic))))
        }
        // Runtime-dynamic: the return type depends on external data (JSON
        // payloads, LLM outputs, etc.) and cannot be determined statically.
        TySpec::Unknown => Ty::Unknown(UnknownReason::ExternalDynamic),
    }
}

// ---------------------------------------------------------------------------
// Public catalog API
// ---------------------------------------------------------------------------

/// Return an iterator over the complete built-in namespace method catalog.
///
/// Delegates to `runtime::namespaces::catalog()`, which aggregates the
/// per-module `SPEC` constants co-located with each namespace implementation.
/// The checker, the LSP completion provider, and the docs generator must all
/// derive their method lists from this function.
pub fn catalog() -> impl Iterator<Item = &'static BuiltinMethod> {
    crate::runtime::namespaces::catalog()
}

/// Look up a built-in method by namespace and name.
///
/// Returns `None` if the pair is not registered in the catalog.
pub(crate) fn catalog_method(namespace: &str, name: &str) -> Option<&'static BuiltinMethod> {
    crate::runtime::namespaces::catalog().find(|m| m.namespace == namespace && m.name == name)
}

// ---------------------------------------------------------------------------
// Prelude names
// ---------------------------------------------------------------------------

/// Return the set of identifiers that are always in scope — prelude
/// namespaces, built-in type names, top-level builtins, symbol hint
/// keywords, and built-in interface names.
///
/// The checker pre-seeds its identifier table with this set so that
/// references to `Ai`, `str`, `run`, `json`, `Stringable`, etc. do not
/// produce spurious "undefined identifier" errors.
pub(crate) fn prelude_names() -> HashSet<String> {
    let mut prelude = HashSet::new();

    // Prelude namespaces — derived from the catalog to stay in sync.
    let mut seen = HashSet::new();
    for entry in catalog() {
        if seen.insert(entry.namespace) {
            prelude.insert(entry.namespace.to_string());
        }
    }

    // Top-level builtins
    for n in ["run", "stop", "min", "max", "uuid", "typeof"] {
        prelude.insert(n.to_string());
    }

    // Built-in type names
    for n in [
        "int",
        "float",
        "str",
        "bool",
        "none",
        "datetime",
        "duration",
        "Uuid",
        "dynamic",
        "list",
        "map",
        "set",
        "Result",
        "Message",
        "SearchResult",
        "Memory",
        "HttpResponse",
        "Decision",
        "Error",
        "AiError",
        "AiSchemaError",
        "NetworkError",
        "TimeoutError",
        "NullError",
        "TypeError",
        "ParseError",
    ] {
        prelude.insert(n.to_string());
    }

    // Symbol identifiers used as hint args (see runtime::SYMBOL_IDENTS)
    // and attribute-value keywords (`@memory persistent`, etc.).
    for n in [
        "sentence",
        "sentences",
        "line",
        "lines",
        "word",
        "words",
        "paragraph",
        "paragraphs",
        "bullets",
        "prose",
        "json",
        "exponential",
        "linear",
        "fixed",
        "google",
        "bing",
        "arxiv",
        "text",
        "html",
        "markdown",
        "persistent",
        "session",
    ] {
        prelude.insert(n.to_string());
    }

    // Built-in interface names (Stringable, Comparable, …) are not keywords —
    // they're identifiers resolved at runtime.  Adding them to the prelude
    // prevents spurious "undefined identifier" errors when the checker
    // encounters `impl Stringable for Foo` before seeing any declaration of
    // `Stringable` in the source file.
    for iface in [
        "Stringable",
        "Comparable",
        "Equatable",
        "Serializable",
        "Iterable",
    ] {
        prelude.insert(iface.to_string());
    }

    prelude
}

// ---------------------------------------------------------------------------
// Built-in interface definitions
// ---------------------------------------------------------------------------

/// Return the built-in interface catalog.
///
/// Each entry maps an interface name to the list of method signatures that
/// implementing types must satisfy.  These are used by `check_impl_conformance`
/// to validate `impl X for Y` blocks.
pub(crate) fn builtin_interfaces() -> HashMap<String, Vec<TaskSig>> {
    let mut map = HashMap::new();

    // Synthetic params have no source position; use the 0..0 sentinel span.
    let self_param = || Param {
        name: Binding::Ident("self".to_string()),
        name_span: 0..0,
        ty: Node::synthetic(TypeExpr::SelfType),
        default: None,
        variadic: false,
    };
    let dynamic_param = |name: &str| Param {
        name: Binding::Ident(name.to_string()),
        name_span: 0..0,
        ty: Node::synthetic(TypeExpr::Dynamic),
        default: None,
        variadic: false,
    };

    map.insert(
        "Stringable".to_string(),
        vec![TaskSig {
            name: "to_str".to_string(),
            name_span: 0..0,
            params: vec![self_param()],
            return_type: Some(Node::synthetic(TypeExpr::Named("str".to_string()))),
        }],
    );
    map.insert(
        "Serializable".to_string(),
        vec![TaskSig {
            name: "to_json".to_string(),
            name_span: 0..0,
            params: vec![self_param()],
            return_type: Some(Node::synthetic(TypeExpr::Named("str".to_string()))),
        }],
    );
    map.insert(
        "Comparable".to_string(),
        vec![TaskSig {
            name: "compare".to_string(),
            name_span: 0..0,
            params: vec![self_param(), dynamic_param("other")],
            return_type: Some(Node::synthetic(TypeExpr::Named("int".to_string()))),
        }],
    );
    map.insert(
        "Equatable".to_string(),
        vec![TaskSig {
            name: "equals".to_string(),
            name_span: 0..0,
            params: vec![self_param(), dynamic_param("other")],
            return_type: Some(Node::synthetic(TypeExpr::Named("bool".to_string()))),
        }],
    );
    map.insert(
        "Iterable".to_string(),
        vec![TaskSig {
            name: "items".to_string(),
            name_span: 0..0,
            params: vec![self_param()],
            return_type: Some(Node::synthetic(TypeExpr::List(Box::new(TypeExpr::Dynamic)))),
        }],
    );

    map
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_catalog_namespaces_are_in_prelude_names() {
        let names = prelude_names();
        for entry in super::catalog() {
            assert!(
                names.contains(entry.namespace),
                "catalog namespace `{}` missing from prelude_names()",
                entry.namespace
            );
        }
    }

    /// Every `Namespace.method` in the catalog must be mentioned in docs.
    ///
    /// A method is considered mentioned if either:
    /// - The literal string `"Namespace.method"` appears in a `docs/src/**/*.md` file, OR
    /// - A `{{#catalog Namespace}}` directive appears in any `docs/src/**/*.md` file
    ///   (the preprocessor will expand it to a table covering all methods of that namespace).
    ///
    /// If this test fails after you add a catalog entry, either add a literal mention or
    /// ensure the appropriate `{{#catalog Ns}}` directive is present in a guide page.
    #[test]
    fn catalog_methods_are_mentioned_in_docs() {
        use std::collections::HashSet;
        use std::path::Path;

        let docs_root = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/src");
        let mut docs = String::new();
        fn collect(dir: &Path, out: &mut String) {
            if let Ok(entries) = std::fs::read_dir(dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_dir() {
                        collect(&path, out);
                    } else if path.extension().and_then(|e| e.to_str()) == Some("md")
                        && let Ok(text) = std::fs::read_to_string(&path)
                    {
                        out.push_str(&text);
                    }
                }
            }
        }
        collect(&docs_root, &mut docs);

        // Collect namespaces covered by a {{#catalog Ns}} directive anywhere in the docs.
        let covered_by_directive: HashSet<&str> = {
            let mut set = HashSet::new();
            for line in docs.lines() {
                let trimmed = line.trim();
                if let Some(rest) = trimmed.strip_prefix("{{#catalog ") {
                    if let Some(ns) = rest.strip_suffix("}}") {
                        set.insert(ns.trim());
                    }
                }
            }
            set
        };

        let mut missing: Vec<String> = Vec::new();
        for entry in super::catalog() {
            if covered_by_directive.contains(entry.namespace) {
                continue;
            }
            let key = format!("{}.{}", entry.namespace, entry.name);
            if !docs.contains(&key) {
                missing.push(key);
            }
        }

        assert!(
            missing.is_empty(),
            "The following catalog methods are not mentioned in any docs/src/**/*.md file\n\
             and their namespace has no {{{{#catalog Ns}}}} directive.\n\
             Add a literal mention or a {{{{#catalog Ns}}}} directive to the appropriate guide page:\n  {}",
            missing.join("\n  ")
        );
    }

    #[test]
    fn catalog_covers_all_runtime_namespaces() {
        // Every namespace registered by the runtime must appear in the catalog.
        // The bidirectional check (SPEC vs installed methods) lives in
        // runtime::namespaces tests.
        let runtime_namespaces = [
            "Io", "Schedule", "Ai", "Email", "Env", "Memory", "Log", "Agent", "Control", "Async",
            "Http", "Search", "Db", "Time", "File", "Json", "Cache", "Random", "Uuid", "Crypto",
            "Math", "Shell", "Csv",
        ];
        let catalog_namespaces: std::collections::HashSet<&str> =
            super::catalog().map(|m| m.namespace).collect();
        for ns in runtime_namespaces {
            assert!(
                catalog_namespaces.contains(ns),
                "runtime namespace `{ns}` is not represented in the catalog"
            );
        }
    }

    #[test]
    fn ty_from_spec_round_trips_for_all_variants() {
        // Smoke-test that every TySpec variant produces a non-panicking Ty.
        let specs = [
            TySpec::Int,
            TySpec::Float,
            TySpec::Str,
            TySpec::Bool,
            TySpec::None_,
            TySpec::Datetime,
            TySpec::Duration,
            TySpec::Uuid,
            TySpec::Dynamic,
            TySpec::DbConnection,
            TySpec::NullableStr,
            TySpec::NullableInt,
            TySpec::NullableFloat,
            TySpec::NullableUuid,
            TySpec::NullableDatetime,
            TySpec::NullableDynamic,
            TySpec::ListOfStr,
            TySpec::ListOfInt,
            TySpec::ListOfListOfStr,
            TySpec::ListOfMapStrStr,
            TySpec::ListOfMapStrDynamic,
            TySpec::Unknown,
        ];
        for spec in specs {
            let _ = ty_from_spec(spec); // must not panic
        }
    }
}
