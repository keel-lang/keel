//! `memory` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "memory",
        name: "remember",
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
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Store a value in agent memory.",
    },
    BuiltinMethod {
        namespace: "memory",
        name: "recall",
        method_id: 1,
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "limit",
                ty: TySpec::Int,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Retrieve a value from agent memory, returning none if absent.",
    },
    BuiltinMethod {
        namespace: "memory",
        name: "forget",
        method_id: 2,
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Delete a key from agent memory.",
    },
];
