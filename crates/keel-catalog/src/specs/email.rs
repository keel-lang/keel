//! `email` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "email",
        name: "fetch",
        method_id: 0,
        params: &[BuiltinParam {
            name: "unread",
            ty: TySpec::Bool,
            optional: true,
            binding: ParamBinding::NamedOnly,
        }],
        result: BuiltinResult::Unknown,
        doc: "Fetch messages from the configured email inbox.",
    },
    BuiltinMethod {
        namespace: "email",
        name: "send",
        method_id: 1,
        params: &[
            BuiltinParam {
                name: "body",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "to",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "subject",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Send an email. `subject` defaults to \"(no subject)\" if omitted.",
    },
    BuiltinMethod {
        namespace: "email",
        name: "archive",
        method_id: 2,
        params: &[BuiltinParam {
            name: "email",
            ty: TySpec::Dynamic,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Archive a fetched email (the map returned by `email.fetch`).",
    },
];
