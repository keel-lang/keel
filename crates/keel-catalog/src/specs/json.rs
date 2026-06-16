//! `json` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "json",
        name: "parse",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        // Unknown (ExternalDynamic): the JSON structure depends on runtime input and is
        // not statically knowable. `keel check --strict` will flag unannotated bindings;
        // users silence this with an explicit `let x: dynamic = Json.parse(...)`.
        result: BuiltinResult::Unknown,
        doc: "Parse a JSON string into a dynamic value.",
    },
    BuiltinMethod {
        namespace: "json",
        name: "stringify",
        params: &[BuiltinParam {
            name: "value",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Serialize a value to a JSON string.",
    },
];
