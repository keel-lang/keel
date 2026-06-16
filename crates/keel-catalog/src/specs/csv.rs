//! `csv` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "csv",
        name: "parse",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfListOfStr),
        doc: "Parse a CSV string into a list of rows, each row a list of strings.",
    },
    BuiltinMethod {
        namespace: "csv",
        name: "parse_records",
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfMapStrStr),
        doc: "Parse a CSV string into a list of named-column records.",
    },
    BuiltinMethod {
        namespace: "csv",
        name: "stringify",
        params: &[BuiltinParam {
            name: "rows",
            ty: TySpec::Dynamic,
            optional: false,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Serialize a list of rows into a CSV string.",
    },
];
