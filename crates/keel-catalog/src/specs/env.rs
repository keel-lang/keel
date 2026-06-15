//! `env` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "env",
        name: "get",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableStr),
        doc: "Get an environment variable, returning none if it is unset.",
    },
    BuiltinMethod {
        namespace: "env",
        name: "require",
        params: &[BuiltinParam {
            name: "key",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Get an environment variable, raising an error if it is unset.",
    },
];
