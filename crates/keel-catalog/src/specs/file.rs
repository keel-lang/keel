//! `file` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "file",
        name: "read",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Read a file and return its contents as a string.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "write",
        params: &[
            BuiltinParam {
                name: "path",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "content",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Write a string to a file, creating or overwriting it.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "exists",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Bool),
        doc: "Return true if the path exists on the filesystem.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "list",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfStr),
        doc: "List the entries in a directory.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "mkdir",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Create a directory and all intermediate parents.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "remove",
        params: &[BuiltinParam {
            name: "path",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Remove a file or directory.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "copy",
        params: &[
            BuiltinParam {
                name: "src",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "dst",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Copy a file from src to dst.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "glob",
        params: &[BuiltinParam {
            name: "pattern",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfStr),
        doc: "Return file paths that match a glob pattern.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "move",
        params: &[
            BuiltinParam {
                name: "src",
                ty: TySpec::Str,
                optional: false,
            },
            BuiltinParam {
                name: "dst",
                ty: TySpec::Str,
                optional: false,
            },
        ],
        result: BuiltinResult::Fixed(TySpec::None_),
        doc: "Move (rename) a file from src to dst.",
    },
    BuiltinMethod {
        namespace: "file",
        name: "mktemp",
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Create a temporary file and return its path.",
    },
];
