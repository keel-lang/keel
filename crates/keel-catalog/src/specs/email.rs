//! `email` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "email",
        name: "fetch",
        method_id: 0,
        params: &[],
        result: BuiltinResult::Unknown,
        doc: "Fetch messages from the configured email inbox.",
    },
    BuiltinMethod {
        namespace: "email",
        name: "send",
        method_id: 1,
        params: &[
            BuiltinParam {
                name: "to",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "subject",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "body",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Send an email.",
    },
    BuiltinMethod {
        namespace: "email",
        name: "archive",
        method_id: 2,
        params: &[BuiltinParam {
            name: "id",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Archive an email by its ID.",
    },
];
