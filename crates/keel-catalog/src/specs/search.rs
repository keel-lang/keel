//! `search` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "search",
        name: "web",
        params: &[BuiltinParam {
            name: "query",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Search the web and return a list of SearchResult values.",
    },
    BuiltinMethod {
        namespace: "search",
        name: "news",
        params: &[BuiltinParam {
            name: "query",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Search for recent news and return a list of SearchResult values.",
    },
];
