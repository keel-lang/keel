//! `shell` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[BuiltinMethod {
    namespace: "shell",
    name: "run",
    method_id: 0,
    params: &[BuiltinParam {
        name: "cmd",
        ty: TySpec::Str,
        optional: false,
    }],
    result: BuiltinResult::Fixed(TySpec::Str),
    doc: "Run a shell command and return its combined stdout.",
}];
