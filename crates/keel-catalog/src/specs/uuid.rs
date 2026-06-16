//! `uuid` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "uuid",
        name: "v4",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Uuid),
        doc: "Generate a random UUID v4.",
    },
    BuiltinMethod {
        namespace: "uuid",
        name: "v7",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Uuid),
        doc: "Generate a time-sortable UUID v7.",
    },
    BuiltinMethod {
        namespace: "uuid",
        name: "v5",
        params: &[
            BuiltinParam {
                name: "ns",
                ty: TySpec::Uuid,
                optional: false,
            },
            BuiltinParam {
                name: "name",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Uuid),
        doc: "Generate a deterministic UUID v5 from a namespace UUID and a name.",
    },
    BuiltinMethod {
        namespace: "uuid",
        name: "parse",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableUuid),
        doc: "Parse a UUID string, returning none on failure.",
    },
];
