//! `crypto` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "crypto",
        name: "sha224",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-224 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "sha256",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-256 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "sha384",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-384 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "sha512",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-512 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "sha512_224",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-512/224 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "sha512_256",
        params: &[BuiltinParam {
            name: "data",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the SHA-512/256 hex digest of a string.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "hmac_sha224",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-224 hex digest.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "hmac_sha256",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-256 hex digest.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "hmac_sha384",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-384 hex digest.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "hmac_sha512",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-512 hex digest.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "hmac_sha512_224",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-512/224 hex digest.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "hmac_sha512_256",
        params: &[
            BuiltinParam {
                name: "key",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "data",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Return the HMAC-SHA-512/256 hex digest.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "token",
        params: &[BuiltinParam {
            name: "len",
            ty: TySpec::Int,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Generate a random URL-safe token of the given byte length.",
    },
    BuiltinMethod {
        namespace: "crypto",
        name: "random_bytes",
        params: &[BuiltinParam {
            name: "len",
            ty: TySpec::Int,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfInt),
        doc: "Generate cryptographically random bytes as a list of integers.",
    },
];
