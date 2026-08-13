//! `uuid` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "uuid",
        name: "v4",
        method_id: 0,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Uuid),
        doc: "Generate a random UUID v4.",
    },
    BuiltinMethod {
        namespace: "uuid",
        name: "v7",
        method_id: 1,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Uuid),
        doc: "Generate a time-sortable UUID v7.",
    },
    BuiltinMethod {
        namespace: "uuid",
        name: "v5",
        method_id: 2,
        params: &[
            BuiltinParam {
                name: "ns",
                ty: TySpec::Uuid,
                optional: false,
                binding: ParamBinding::Either,
            },
            BuiltinParam {
                name: "name",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::Either,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Uuid),
        doc: "Generate a deterministic UUID v5 from a namespace UUID and a name.",
    },
    BuiltinMethod {
        namespace: "uuid",
        name: "parse",
        method_id: 3,
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::NullableUuid),
        doc: "Parse a UUID string, returning none on failure.",
    },
];
