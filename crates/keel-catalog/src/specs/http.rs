//! `http` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "http",
        name: "get",
        method_id: 0,
        params: &[
            BuiltinParam {
                name: "url",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "headers",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP GET request and return an HttpResponse.",
    },
    BuiltinMethod {
        namespace: "http",
        name: "post",
        method_id: 1,
        params: &[
            BuiltinParam {
                name: "url",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "headers",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "body",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "json",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP POST request and return an HttpResponse. `json` is JSON-encoded and takes precedence over `body` if both are given.",
    },
    BuiltinMethod {
        namespace: "http",
        name: "request",
        method_id: 2,
        params: &[
            BuiltinParam {
                name: "url",
                ty: TySpec::Str,
                optional: false,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "method",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "headers",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "body",
                ty: TySpec::Str,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "json",
                ty: TySpec::Dynamic,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
            BuiltinParam {
                name: "timeout",
                ty: TySpec::Duration,
                optional: true,
                binding: ParamBinding::NamedOnly,
            },
        ],
        result: BuiltinResult::Unknown,
        doc: "Make an HTTP request with full control and return an HttpResponse. `method` defaults to GET.",
    },
    BuiltinMethod {
        namespace: "http",
        name: "serve",
        method_id: 3,
        params: &[
            BuiltinParam {
                name: "port",
                ty: TySpec::Int,
                optional: true,
                binding: ParamBinding::PositionalOnly,
            },
            BuiltinParam {
                name: "fn",
                ty: TySpec::Callback,
                optional: false,
                binding: ParamBinding::PositionalOnly,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Start an HTTP server on the given port (default 8080).",
    },
];
