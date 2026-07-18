//! `log` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "log",
        name: "info",
        method_id: 0,
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Emit an info-level log message.",
    },
    BuiltinMethod {
        namespace: "log",
        name: "warn",
        method_id: 1,
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Emit a warning-level log message.",
    },
    BuiltinMethod {
        namespace: "log",
        name: "error",
        method_id: 2,
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Emit an error-level log message.",
    },
    BuiltinMethod {
        namespace: "log",
        name: "debug",
        method_id: 3,
        params: &[BuiltinParam {
            name: "message",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Emit a debug-level log message.",
    },
    BuiltinMethod {
        namespace: "log",
        name: "set_level",
        method_id: 4,
        params: &[BuiltinParam {
            name: "level",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Set the minimum log level (\"debug\", \"info\", \"warn\", or \"error\").",
    },
    BuiltinMethod {
        namespace: "log",
        name: "level",
        method_id: 5,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the current log level as a string.",
    },
];
