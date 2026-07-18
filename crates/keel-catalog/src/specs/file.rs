//! `file` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "file",
        name: "read",
        method_id: 0,
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
        method_id: 1,
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
        method_id: 2,
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
        method_id: 3,
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
        method_id: 4,
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
        method_id: 5,
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
        method_id: 6,
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
        method_id: 7,
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
        method_id: 8,
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
        method_id: 9,
        params: &[],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Create a temporary file and return its path.",
    },
];
