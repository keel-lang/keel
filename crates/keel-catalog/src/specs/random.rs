//! `random` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "random",
        name: "float",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Float),
        doc: "Return a random float in the range [0, 1).",
    },
    BuiltinMethod {
        namespace: "random",
        name: "int",
        params: &[
            BuiltinParam {
                name: "min",
                ty: TySpec::Int,
                optional: false,
            },
            BuiltinParam {
                name: "max",
                ty: TySpec::Int,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Int),
        doc: "Return a random integer in the inclusive range [min, max].",
    },
    BuiltinMethod {
        namespace: "random",
        name: "bool",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Bool),
        doc: "Return a random boolean.",
    },
];
