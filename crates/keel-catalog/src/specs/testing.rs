//! `testing` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[BuiltinMethod {
    namespace: "testing",
    name: "mock",
    method_id: 0,
    params: &[BuiltinParam {
        name: "target",
        ty: TySpec::Dynamic,
        optional: false,
        binding: ParamBinding::PositionalOnly,
    }],
    result: BuiltinResult::Fixed(TySpec::Unknown),
    doc: "Create a test-local mock handle for a namespace method (e.g. `testing.mock(ai.classify)`).",
}];
