//! Prelude catalog — identifiers and built-in interfaces known to the checker
//! before user code is examined.
//!
//! Keeping this in one place prevents checker, runtime, LSP, and docs from
//! drifting independently.  When a new stdlib namespace or built-in type name
//! is added, this file is the single source of truth.

use std::collections::{HashMap, HashSet};

use crate::ast::{Binding, Param, TaskSig, TypeExpr};

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

    // Prelude namespaces
    for n in [
        "Ai", "Io", "Http", "Email", "Search", "Db", "Memory", "Schedule", "Async", "Control",
        "Env", "Time", "Log", "Agent", "Cache", "File", "Json", "Random", "Uuid", "Crypto",
        "Math", "Shell", "Csv",
    ] {
        prelude.insert(n.to_string());
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
        "AIError",
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

    let self_param = || Param {
        name: Binding::Ident("self".to_string()),
        ty: TypeExpr::Named("__impl_self__".to_string()),
        default: None,
        variadic: false,
    };
    let dynamic_param = |name: &str| Param {
        name: Binding::Ident(name.to_string()),
        ty: TypeExpr::Dynamic,
        default: None,
        variadic: false,
    };

    map.insert(
        "Stringable".to_string(),
        vec![TaskSig {
            name: "to_str".to_string(),
            params: vec![self_param()],
            return_type: Some(TypeExpr::Named("str".to_string())),
        }],
    );
    map.insert(
        "Serializable".to_string(),
        vec![TaskSig {
            name: "to_json".to_string(),
            params: vec![self_param()],
            return_type: Some(TypeExpr::Named("str".to_string())),
        }],
    );
    map.insert(
        "Comparable".to_string(),
        vec![TaskSig {
            name: "compare".to_string(),
            params: vec![self_param(), dynamic_param("other")],
            return_type: Some(TypeExpr::Named("int".to_string())),
        }],
    );
    map.insert(
        "Equatable".to_string(),
        vec![TaskSig {
            name: "equals".to_string(),
            params: vec![self_param(), dynamic_param("other")],
            return_type: Some(TypeExpr::Named("bool".to_string())),
        }],
    );
    map.insert(
        "Iterable".to_string(),
        vec![TaskSig {
            name: "items".to_string(),
            params: vec![self_param()],
            return_type: Some(TypeExpr::List(Box::new(TypeExpr::Dynamic))),
        }],
    );

    map
}
