use crate::builtins::{BuiltinMethod, BuiltinParam, BuiltinResult, TySpec};
use crate::interpreter::value::{MapKey, Value};
use crate::interpreter::{Namespace, RuntimeErrorKind};
use crate::runtime::args::{expect_list, expect_str};
use crate::runtime::namespace::{make_typed_report, ns};

pub(crate) const SPEC: &[BuiltinMethod] = &[
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

fn parse_csv(text: &str) -> miette::Result<Vec<Vec<String>>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_reader(text.as_bytes());
    let mut rows = Vec::new();
    for result in rdr.records() {
        let record = result.map_err(|e| {
            make_typed_report(
                RuntimeErrorKind::Csv,
                format!("failed to read CSV record: {e}"),
            )
        })?;
        rows.push(record.iter().map(str::to_owned).collect());
    }
    Ok(rows)
}

pub(crate) fn namespace() -> Namespace {
    ns!("csv", {
        // Csv.parse(text) — parse a CSV string into list[list[str]].
        // Each inner list is one row; all values are strings.
        "parse" => |_i, args| Box::pin(async move {
            let text = expect_str(&args, 0, "csv.parse")?;

            let rows = parse_csv(text)?;
            Ok(Value::List(
                rows.into_iter()
                    .map(|row| {
                        Value::List(row.into_iter().map(Value::String).collect())
                    })
                    .collect(),
            ))
        }),

        // Csv.parse_records(text) — parse CSV using the first row as header names.
        // Returns list[map[str, str]]; absent fields default to "".
        "parse_records" => |_i, args| Box::pin(async move {
            let text = expect_str(&args, 0, "csv.parse_records")?;

            let rows = parse_csv(text)?;
            if rows.is_empty() {
                return Ok(Value::List(vec![]));
            }
            let headers = rows[0].clone();
            for header in headers.iter() {
                if header.is_empty() {
                    return Err(make_typed_report(
                        RuntimeErrorKind::Csv,
                        "csv.parse_records: header names must not be empty",
                    ));
                }
            }
            let mut seen = std::collections::HashSet::new();
            for header in headers.iter() {
                if !seen.insert(header.as_str()) {
                    return Err(make_typed_report(
                        RuntimeErrorKind::Csv,
                        format!("csv.parse_records: duplicate header name \"{header}\""),
                    ));
                }
            }
            let mut records: Vec<Value> = Vec::new();
            for row in &rows[1..] {
                if row.len() > headers.len() {
                    return Err(make_typed_report(
                        RuntimeErrorKind::Csv,
                        format!(
                            "csv.parse_records: row has {} cells but header defines {} columns",
                            row.len(),
                            headers.len()
                        ),
                    ));
                }
                let mut map = std::collections::HashMap::new();
                for (i, header) in headers.iter().enumerate() {
                    let cell = row.get(i).cloned().unwrap_or_default();
                    map.insert(MapKey::Str(header.clone()), Value::String(cell));
                }
                records.push(Value::Map(map));
            }
            Ok(Value::List(records))
        }),

        // Csv.stringify(rows) — convert list[list[str]] to a CSV string.
        // Each inner list is a row; every cell must be a string.
        // Include a header row as the first element if desired.
        "stringify" => |_i, args| Box::pin(async move {
            let rows = expect_list(&args, 0, "csv.stringify")?;

            let mut wtr = csv::WriterBuilder::new()
                .terminator(csv::Terminator::CRLF)
                .from_writer(Vec::new());

            for row_val in rows {
                let cells = match row_val {
                    Value::List(cells) => cells,
                    other => {
                        return Err(make_typed_report(
                            RuntimeErrorKind::Csv,
                            format!(
                                "csv.stringify expects each row to be a list, got {}",
                                other.type_name()
                            ),
                        ));
                    }
                };
                let mut record: Vec<String> = Vec::new();
                for (col, v) in cells.iter().enumerate() {
                    match v {
                        Value::String(s) => record.push(s.clone()),
                        other => {
                            return Err(make_typed_report(
                                RuntimeErrorKind::Csv,
                                format!(
                                    "csv.stringify cell at column {} is {}, expected str",
                                    col,
                                    other.type_name()
                                ),
                            ));
                        }
                    }
                }
                wtr.write_record(&record).map_err(|e| {
                    make_typed_report(RuntimeErrorKind::Csv, format!("csv.stringify write failed: {e}"))
                })?;
            }

            let bytes = wtr
                .into_inner()
                .map_err(|e| make_typed_report(RuntimeErrorKind::Csv, format!("csv.stringify flush failed: {e}")))?;
            let text = String::from_utf8(bytes)
                .map_err(|e| make_typed_report(RuntimeErrorKind::Csv, format!("csv.stringify encoding error: {e}")))?;
            Ok(Value::String(text))
        }),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::interpreter::{CallArgValue, Interpreter};

    fn pos(v: Value) -> CallArgValue {
        CallArgValue {
            name: None,
            value: v,
        }
    }

    fn str_val(s: &str) -> Value {
        Value::String(s.to_string())
    }

    #[tokio::test]
    async fn namespace_has_all_methods() {
        let ns = namespace();
        assert_eq!(ns.name, "csv");
        for m in ["parse", "parse_records", "stringify"] {
            assert!(ns.methods.contains_key(m), "missing method: {m}");
        }
    }

    #[tokio::test]
    async fn parse_returns_list_of_list_of_str() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let result = ns.methods["parse"](&mut i, vec![pos(str_val("a,b,c\n1,2,3"))])
            .await
            .unwrap();
        let Value::List(rows) = result else {
            panic!("expected list")
        };
        assert_eq!(rows.len(), 2);
        let Value::List(ref row0) = rows[0] else {
            panic!("expected list row")
        };
        assert_eq!(row0.len(), 3);
        assert_eq!(row0[0], str_val("a"));
    }

    #[tokio::test]
    async fn parse_handles_quoted_fields() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let result = ns.methods["parse"](&mut i, vec![pos(str_val("\"hello, world\",plain"))])
            .await
            .unwrap();
        let Value::List(rows) = result else {
            panic!("expected list")
        };
        let Value::List(ref row) = rows[0] else {
            panic!()
        };
        assert_eq!(row[0], str_val("hello, world"));
        assert_eq!(row[1], str_val("plain"));
    }

    #[tokio::test]
    async fn parse_empty_string_returns_empty_list() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let result = ns.methods["parse"](&mut i, vec![pos(str_val(""))])
            .await
            .unwrap();
        let Value::List(rows) = result else { panic!() };
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn parse_records_keys_from_first_row() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let result =
            ns.methods["parse_records"](&mut i, vec![pos(str_val("name,score\nAlice,10\nBob,20"))])
                .await
                .unwrap();
        let Value::List(rows) = result else { panic!() };
        assert_eq!(rows.len(), 2);
        let Value::Map(ref m) = rows[0] else { panic!() };
        assert_eq!(m[&MapKey::Str("name".into())], str_val("Alice"));
        assert_eq!(m[&MapKey::Str("score".into())], str_val("10"));
    }

    #[tokio::test]
    async fn parse_records_only_row_returns_empty_list() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let result = ns.methods["parse_records"](&mut i, vec![pos(str_val("name,score"))])
            .await
            .unwrap();
        let Value::List(rows) = result else { panic!() };
        assert!(rows.is_empty());
    }

    #[tokio::test]
    async fn stringify_produces_csv_string() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let rows = Value::List(vec![
            Value::List(vec![str_val("symbol"), str_val("price")]),
            Value::List(vec![str_val("BTC"), str_val("67000")]),
        ]);
        let result = ns.methods["stringify"](&mut i, vec![pos(rows)])
            .await
            .unwrap();
        let Value::String(s) = result else { panic!() };
        assert!(s.contains("symbol"), "missing header: {s}");
        assert!(s.contains("BTC"), "missing data: {s}");
    }

    #[tokio::test]
    async fn stringify_quotes_fields_with_commas() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let rows = Value::List(vec![Value::List(vec![str_val("a,b"), str_val("plain")])]);
        let result = ns.methods["stringify"](&mut i, vec![pos(rows)])
            .await
            .unwrap();
        let Value::String(s) = result else { panic!() };
        assert!(s.contains("\"a,b\""), "expected quoting in: {s}");
    }

    #[tokio::test]
    async fn stringify_non_list_row_raises() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let rows = Value::List(vec![Value::String("not a row".into())]);
        let err = ns.methods["stringify"](&mut i, vec![pos(rows)])
            .await
            .unwrap_err();
        assert!(format!("{err:?}").contains("CsvError"));
    }

    #[tokio::test]
    async fn parse_records_duplicate_header_raises() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let err = ns.methods["parse_records"](
            &mut i,
            vec![pos(str_val("name,name,score\nAlice,Bob,10"))],
        )
        .await
        .unwrap_err();
        assert!(
            format!("{err:?}").contains("duplicate header"),
            "expected duplicate error: {err:?}"
        );
    }

    #[tokio::test]
    async fn parse_records_empty_header_name_raises() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let err = ns.methods["parse_records"](&mut i, vec![pos(str_val("name,,score\nAlice,,10"))])
            .await
            .unwrap_err();
        assert!(
            format!("{err:?}").contains("must not be empty"),
            "expected empty-name error: {err:?}"
        );
    }

    #[tokio::test]
    async fn parse_records_extra_cells_in_row_raises() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let err = ns.methods["parse_records"](&mut i, vec![pos(str_val("a,b\n1,2,3"))])
            .await
            .unwrap_err();
        assert!(
            format!("{err:?}").contains("row has"),
            "expected over-wide row error: {err:?}"
        );
    }

    #[tokio::test]
    async fn stringify_non_string_cell_raises() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let rows = Value::List(vec![Value::List(vec![Value::Integer(42), str_val("ok")])]);
        let err = ns.methods["stringify"](&mut i, vec![pos(rows)])
            .await
            .unwrap_err();
        assert!(
            format!("{err:?}").contains("expected str"),
            "expected type error: {err:?}"
        );
    }

    #[tokio::test]
    async fn parse_then_stringify_roundtrip() {
        let ns = namespace();
        let mut i = Interpreter::new();
        let original = "name,score\r\nAlice,10\r\nBob,20\r\n";
        let parsed = ns.methods["parse"](&mut i, vec![pos(str_val(original))])
            .await
            .unwrap();
        let stringified = ns.methods["stringify"](&mut i, vec![pos(parsed)])
            .await
            .unwrap();
        let Value::String(s) = stringified else {
            panic!()
        };
        assert!(s.contains("Alice"), "roundtrip lost data: {s}");
        assert!(s.contains("name"), "roundtrip lost header: {s}");
    }
}
