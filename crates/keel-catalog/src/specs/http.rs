//! `http` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "http",
        name: "get",
        params: &[BuiltinParam {
            name: "url",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP GET request and return an HttpResponse.",
    },
    BuiltinMethod {
        namespace: "http",
        name: "post",
        params: &[
            BuiltinParam {
                name: "url",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "body",
                ty: TySpec::Str,
                optional: true,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP POST request and return an HttpResponse.",
    },
    BuiltinMethod {
        namespace: "http",
        name: "request",
        params: &[
            BuiltinParam {
                name: "method",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "url",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP request with full control and return an HttpResponse.",
    },
    BuiltinMethod {
        namespace: "http",
        name: "serve",
        params: &[BuiltinParam {
            name: "port",
            ty: TySpec::Int,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Start an HTTP server on the given port.",
    },
];
