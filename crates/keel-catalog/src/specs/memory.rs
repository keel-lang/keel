//! `memory` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "memory",
        name: "remember",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "value",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Store a value in agent memory.",
    },
    BuiltinMethod {
        namespace: "memory",
        name: "recall",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Retrieve a value from agent memory, returning none if absent.",
    },
    BuiltinMethod {
        namespace: "memory",
        name: "forget",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Delete a key from agent memory.",
    },
];
