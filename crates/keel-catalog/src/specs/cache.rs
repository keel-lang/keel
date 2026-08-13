//! `cache` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "cache",
        name: "set",
        method_id: 0,
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "value",
                ty: TySpec::Dynamic,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "ttl",
                ty: TySpec::Duration,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Store a value in the in-process cache, with an optional TTL duration.",
    },
    BuiltinMethod {
        namespace: "cache",
        name: "get",
        method_id: 1,
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableDynamic),
        doc: "Retrieve a cached value, returning none if absent.",
    },
    BuiltinMethod {
        namespace: "cache",
        name: "delete",
        method_id: 2,
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Remove a key from the cache.",
    },
    BuiltinMethod {
        namespace: "cache",
        name: "clear",
        method_id: 3,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Clear all entries from the cache.",
    },
];
