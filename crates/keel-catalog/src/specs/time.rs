//! `time` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "time",
        name: "now",
        method_id: 0,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Datetime),
        doc: "Return the current UTC datetime.",
    },
    BuiltinMethod {
        namespace: "time",
        name: "epoch_ms",
        method_id: 1,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Int),
        doc: "Return the current Unix timestamp in milliseconds.",
    },
    BuiltinMethod {
        namespace: "time",
        name: "parse",
        method_id: 2,
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableDatetime),
        doc: "Parse a datetime string, returning none on failure.",
    },
];
