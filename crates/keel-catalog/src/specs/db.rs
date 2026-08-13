//! `db` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[BuiltinMethod {
    namespace: "db",
    name: "connect",
    method_id: 0,
    params: &[BuiltinParam {
        name: "url",
        ty: TySpec::Str,
        optional: false,
        binding: ParamBinding::PositionalOnly,
    }],
    result: BuiltinResult::Fixed(TySpec::DbConnection),
    doc: "Open a database connection and return a DbConnection.",
}];
