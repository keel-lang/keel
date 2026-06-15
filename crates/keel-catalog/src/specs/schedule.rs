//! `schedule` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "schedule",
        name: "every",
        params: &[BuiltinParam {
            name: "interval",
            ty: TySpec::Duration,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Schedule a task to run on a recurring interval.",
    },
    BuiltinMethod {
        namespace: "schedule",
        name: "after",
        params: &[BuiltinParam {
            name: "delay",
            ty: TySpec::Duration,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Schedule a task to run after a delay.",
    },
    BuiltinMethod {
        namespace: "schedule",
        name: "at",
        params: &[BuiltinParam {
            name: "time",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Schedule a task to run at a specific wall-clock time.",
    },
    BuiltinMethod {
        namespace: "schedule",
        name: "cron",
        params: &[BuiltinParam {
            name: "expr",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Schedule a task using a cron expression.",
    },
    BuiltinMethod {
        namespace: "schedule",
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
