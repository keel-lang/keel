//! `asynchronous` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "async",
        name: "spawn",
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Spawn a concurrent task and return a handle.",
    },
    BuiltinMethod {
        namespace: "async",
        name: "join_all",
        params: &[BuiltinParam {
            name: "handles",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Wait for all async task handles to complete.",
    },
    BuiltinMethod {
        namespace: "async",
        name: "select",
        params: &[BuiltinParam {
            name: "handles",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Return the result of the first completed task handle.",
    },
    BuiltinMethod {
        namespace: "async",
        name: "sleep",
        params: &[BuiltinParam {
            name: "duration",
            ty: TySpec::Duration,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Pause execution for the given duration.",
    },
];
