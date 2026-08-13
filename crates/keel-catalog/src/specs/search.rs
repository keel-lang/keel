//! `search` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "search",
        name: "web",
        method_id: 0,
        params: &[BuiltinParam {
            name: "query",
            ty: TySpec::Str,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Unknown,
        doc: "Search the web and return a list of SearchResult values.",
    },
    BuiltinMethod {
        namespace: "search",
        name: "news",
        method_id: 1,
        params: &[BuiltinParam {
            name: "query",
            ty: TySpec::Str,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Unknown,
        doc: "Search for recent news and return a list of SearchResult values.",
    },
];
