use crate::common::*;

// ---------------------------------------------------------------------------
// Csv namespace
// ---------------------------------------------------------------------------

#[test]
fn csv_parse_returns_rows_of_strings() {
    let src = r#"
agent A {
    @on_start {
        rows = Csv.parse("name,score\nAlice,10\nBob,20")
        Io.show("rows={rows.len()}")
        Io.show("col0={rows[0][0]}")
        Io.show("val={rows[2][1]}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("rows=3"), "expected 3 rows: {stdout}");
    assert!(stdout.contains("col0=name"), "header row: {stdout}");
    assert!(stdout.contains("val=20"), "data value: {stdout}");
}

#[test]
fn csv_parse_handles_quoted_fields_with_commas() {
    let src = r#"
agent A {
    @on_start {
        rows = Csv.parse("\"hello, world\",plain")
        Io.show("field={rows[0][0]}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("field=hello, world"),
        "quoted field: {stdout}"
    );
}

#[test]
fn csv_parse_records_returns_list_of_maps() {
    let src = r#"
agent A {
    @on_start {
        rows = Csv.parse_records("symbol,price\nBTC,67000\nETH,3500")
        Io.show("count={rows.len()}")
        Io.show("sym={rows[0]["symbol"]}")
        Io.show("price={rows[1]["price"]}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("count=2"), "row count: {stdout}");
    assert!(stdout.contains("sym=BTC"), "symbol field: {stdout}");
    assert!(stdout.contains("price=3500"), "price field: {stdout}");
}

#[test]
fn csv_stringify_produces_valid_csv() {
    let src = r#"
agent A {
    @on_start {
        rows = [["symbol", "price"], ["BTC", "67000"], ["ETH", "3500"]]
        text = Csv.stringify(rows)
        Io.show(text)
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("symbol"), "missing header: {stdout}");
    assert!(stdout.contains("BTC"), "missing BTC row: {stdout}");
    assert!(stdout.contains("ETH"), "missing ETH row: {stdout}");
}

#[test]
fn csv_stringify_quotes_fields_with_commas() {
    let src = r#"
agent A {
    @on_start {
        rows = [["a,b", "plain"]]
        text = Csv.stringify(rows)
        Io.show(text)
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("\"a,b\""),
        "expected quoted field: {stdout}"
    );
}

#[test]
fn csv_parse_then_stringify_roundtrip() {
    let src = r#"
agent A {
    @on_start {
        raw = "name,score\nAlice,10\nBob,20"
        rows = Csv.parse(raw)
        text = Csv.stringify(rows)
        reparsed = Csv.parse(text)
        Io.show("rows={reparsed.len()}")
        Io.show("name={reparsed[1][0]}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("rows=3"), "roundtrip row count: {stdout}");
    assert!(stdout.contains("name=Alice"), "roundtrip data: {stdout}");
}

#[test]
fn csv_stringify_invalid_row_type_raises() {
    let src = r#"
agent A {
    @on_start {
        try {
            Csv.stringify(["not a row"])
        } catch e: Error {
            Io.show("caught={e.message.len() > 0}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("caught=true"),
        "expected error to be caught: {stdout}"
    );
}

#[test]
fn csv_parse_records_duplicate_header_raises() {
    let src = r#"
agent A {
    @on_start {
        try {
            Csv.parse_records("name,name,score\nAlice,Bob,10")
        } catch e: Error {
            Io.show("caught={e.message.len() > 0}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("caught=true"),
        "expected duplicate-header error: {stdout}"
    );
}

#[test]
fn csv_parse_records_empty_header_name_raises() {
    let src = r#"
agent A {
    @on_start {
        try {
            Csv.parse_records("name,,score\nAlice,,10")
        } catch e: Error {
            Io.show("caught={e.message.len() > 0}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("caught=true"),
        "expected empty-header error: {stdout}"
    );
}

#[test]
fn csv_parse_records_extra_cells_raises() {
    let src = r#"
agent A {
    @on_start {
        try {
            Csv.parse_records("a,b\n1,2,3")
        } catch e: Error {
            Io.show("caught={e.message.len() > 0}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("caught=true"),
        "expected over-wide row error: {stdout}"
    );
}

#[test]
fn csv_stringify_non_string_cell_raises() {
    let src = r#"
agent A {
    @on_start {
        try {
            rows = [[1, "ok"]]
            Csv.stringify(rows)
        } catch e: Error {
            Io.show("caught={e.message.len() > 0}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("caught=true"),
        "expected non-string cell error: {stdout}"
    );
}
