//! `io` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "io",
        name: "ask",
        method_id: 0,
        params: &[BuiltinParam {
            name: "prompt",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Ask the user a question and return the response as a string.",
    },
    BuiltinMethod {
        namespace: "io",
        name: "confirm",
        method_id: 1,
        params: &[BuiltinParam {
            name: "prompt",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Bool),
        doc: "Ask the user to confirm a choice and return true or false.",
    },
    BuiltinMethod {
        namespace: "io",
        name: "notify",
        method_id: 2,
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Show a notification to the user.",
    },
    BuiltinMethod {
        namespace: "io",
        name: "show",
        method_id: 3,
        params: &[BuiltinParam {
            name: "value",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Display a value to the user.",
    },
];
