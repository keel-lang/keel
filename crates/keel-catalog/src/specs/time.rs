//! `time` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "time",
        name: "now",
        method_id: 0,
        params: &[BuiltinParam {
            name: "tz",
            ty: TySpec::Str,
            optional: true,
            binding: ParamBinding::NamedOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::Datetime),
        doc: "Return the current datetime, optionally offset-shifted to an IANA timezone.",
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
        params: &[
            BuiltinParam {
                name: "s",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "tz",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::NullableDatetime),
        doc: "Parse a datetime string, returning none on failure. `tz` coerces a naive (no-offset) string into the given timezone.",
    },
];
