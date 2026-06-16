//! `cache` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "cache",
        name: "set",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "value",
                ty: TySpec::Dynamic,
                optional: false,
            },
            BuiltinParam {
                name: "ttl",
                ty: TySpec::Duration,
                optional: true,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Store a value in the in-process cache, with an optional TTL duration.",
    },
    BuiltinMethod {
        namespace: "cache",
        name: "get",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableDynamic),
        doc: "Retrieve a cached value, returning none if absent.",
    },
    BuiltinMethod {
        namespace: "cache",
        name: "delete",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Remove a key from the cache.",
    },
    BuiltinMethod {
        namespace: "cache",
        name: "clear",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Clear all entries from the cache.",
    },
];
