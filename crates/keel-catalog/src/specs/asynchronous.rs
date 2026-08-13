//! `asynchronous` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "async",
        name: "spawn",
        method_id: 0,
        params: &[BuiltinParam {
            name: "fn",
            ty: TySpec::Callback,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Unknown,
        doc: "Spawn a concurrent task and return a handle.",
    },
    BuiltinMethod {
        namespace: "async",
        name: "join_all",
        method_id: 1,
        params: &[BuiltinParam {
            name: "handles",
            ty: TySpec::Dynamic,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Unknown,
        doc: "Wait for all async task handles to complete.",
    },
    BuiltinMethod {
        namespace: "async",
        name: "select",
        method_id: 2,
        params: &[BuiltinParam {
            name: "handles",
            ty: TySpec::Dynamic,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Unknown,
        doc: "Return the result of the first completed task handle.",
    },
    BuiltinMethod {
        namespace: "async",
        name: "sleep",
        method_id: 3,
        params: &[BuiltinParam {
            name: "duration",
            ty: TySpec::Duration,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Pause execution for the given duration.",
    },
];
