//! `control` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "control",
        name: "retry",
        method_id: 0,
        params: &[
            BuiltinParam {
                name: "attempts",
                ty: TySpec::Int,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "fn",
                ty: TySpec::Callback,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Retry a task on failure with configurable backoff.",
    },
    BuiltinMethod {
        namespace: "control",
        name: "with_timeout",
        method_id: 1,
        params: &[
            BuiltinParam {
                name: "duration",
                ty: TySpec::Duration,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "fn",
                ty: TySpec::Callback,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Run a task, raising an error if it exceeds the timeout.",
    },
    BuiltinMethod {
        namespace: "control",
        name: "with_deadline",
        method_id: 2,
        params: &[
            BuiltinParam {
                name: "deadline",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "fn",
                ty: TySpec::Callback,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Run a task, raising an error if it runs past the ISO 8601 deadline.",
    },
];
