//! `testing` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[BuiltinMethod {
    namespace: "testing",
    name: "mock",
    params: &[],
    result: BuiltinResult::Fixed(TySpec::Unknown),
    doc: "Create a test-local mock handle for a namespace method.",
}];
