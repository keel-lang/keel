//! `random` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "random",
        name: "float",
        method_id: 0,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return a random float in the range [0, 1).",
    },
    BuiltinMethod {
        namespace: "random",
        name: "int",
        method_id: 1,
        params: &[
            BuiltinParam {
                name: "min",
                ty: TySpec::Int,
                optional: false,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "max",
                ty: TySpec::Int,
                optional: false,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Int),
        doc: "Return a random integer in the inclusive range [min, max].",
    },
    BuiltinMethod {
        namespace: "random",
        name: "bool",
        method_id: 2,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Bool),
        doc: "Return a random boolean.",
    },
];
