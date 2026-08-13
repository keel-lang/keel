//! `shell` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[BuiltinMethod {
    namespace: "shell",
    name: "run",
    method_id: 0,
    params: &[
        BuiltinParam {
            name: "cmd",
            ty: TySpec::Str,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        },
        BuiltinParam {
            name: "stdin",
            ty: TySpec::Str,
            optional: true,
            binding: ParamBinding::NamedOnly,
        },
        BuiltinParam {
            name: "cwd",
            ty: TySpec::Str,
            optional: true,
            binding: ParamBinding::NamedOnly,
        },
    ],
    result: BuiltinResult::Fixed(TySpec::Str),
    doc: "Run a shell command and return its combined stdout.",
}];
