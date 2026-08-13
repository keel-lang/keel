//! `csv` namespace method descriptors.

use crate::builtins::*;

pub const SPEC: &[BuiltinMethod] = &[
    BuiltinMethod {
        namespace: "csv",
        name: "parse",
        method_id: 0,
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfListOfStr),
        doc: "Parse a CSV string into a list of rows, each row a list of strings.",
    },
    BuiltinMethod {
        namespace: "csv",
        name: "parse_records",
        method_id: 1,
        params: &[BuiltinParam {
            name: "s",
            ty: TySpec::Str,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::ListOfMapStrStr),
        doc: "Parse a CSV string into a list of named-column records.",
    },
    BuiltinMethod {
        namespace: "csv",
        name: "stringify",
        method_id: 2,
        params: &[BuiltinParam {
            name: "rows",
            ty: TySpec::ListOfListOfStr,
            optional: false,
            binding: ParamBinding::PositionalOnly,
        }],
        result: BuiltinResult::Fixed(TySpec::Str),
        doc: "Serialize a list of rows (each row a list of strings) into a CSV string.",
    },
];
