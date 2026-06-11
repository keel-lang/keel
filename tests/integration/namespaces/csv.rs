use crate::common::*;

// ---------------------------------------------------------------------------
// Csv namespace
// ---------------------------------------------------------------------------

#[test]
fn csv_parse_returns_rows_of_strings() {
    let src = r#"
use std/csv
use std/io
agent A {
    @on_start {
        rows = csv.parse("name,score\nAlice,10\nBob,20")
        io.show("rows={rows.len()}")
        io.show("col0={rows[0][0]}")
        io.show("val={rows[2][1]}")
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
use std/csv
use std/io
agent A {
    @on_start {
        rows = csv.parse("\"hello, world\",plain")
        io.show("field={rows[0][0]}")
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
use std/csv
use std/io
agent A {
    @on_start {
        rows = csv.parse_records("symbol,price\nBTC,67000\nETH,3500")
        io.show("count={rows.len()}")
        io.show("sym={rows[0]["symbol"]}")
        io.show("price={rows[1]["price"]}")
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
use std/csv
use std/io
agent A {
    @on_start {
        rows = [["symbol", "price"], ["BTC", "67000"], ["ETH", "3500"]]
        text = csv.stringify(rows)
        io.show(text)
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
use std/csv
use std/io
agent A {
    @on_start {
        rows = [["a,b", "plain"]]
        text = csv.stringify(rows)
        io.show(text)
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
use std/csv
use std/io
agent A {
    @on_start {
        raw = "name,score\nAlice,10\nBob,20"
        rows = csv.parse(raw)
        text = csv.stringify(rows)
        reparsed = csv.parse(text)
        io.show("rows={reparsed.len()}")
        io.show("name={reparsed[1][0]}")
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
use std/csv
use std/io
agent A {
    @on_start {
        try {
            csv.stringify(["not a row"])
        } catch e: Error {
            io.show("caught={e.message.len() > 0}")
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
use std/csv
use std/io
agent A {
    @on_start {
        try {
            csv.parse_records("name,name,score\nAlice,Bob,10")
        } catch e: Error {
            io.show("caught={e.message.len() > 0}")
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
use std/csv
use std/io
agent A {
    @on_start {
        try {
            csv.parse_records("name,,score\nAlice,,10")
        } catch e: Error {
            io.show("caught={e.message.len() > 0}")
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
use std/csv
use std/io
agent A {
    @on_start {
        try {
            csv.parse_records("a,b\n1,2,3")
        } catch e: Error {
            io.show("caught={e.message.len() > 0}")
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
use std/csv
use std/io
agent A {
    @on_start {
        try {
            rows = [[1, "ok"]]
            csv.stringify(rows)
        } catch e: Error {
            io.show("caught={e.message.len() > 0}")
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

#[test]
fn csv_error_is_catchable_by_specific_type() {
    let src = r#"
use std/csv
use std/io
agent A {
    @on_start {
        try {
            csv.parse_records("name,name\nAlice,Bob")
        } catch e: CsvError {
            io.show("kind=CsvError")
            io.show("msg={e.message.len() > 0}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("kind=CsvError"),
        "expected CsvError to be caught by specific type:\n{stdout}"
    );
    assert!(
        stdout.contains("msg=true"),
        "expected message field to be non-empty:\n{stdout}"
    );
}

#[test]
fn csv_error_is_also_caught_by_error_fallback() {
    let src = r#"
use std/csv
use std/io
agent A {
    @on_start {
        try {
            csv.stringify(["not a row"])
        } catch e: CsvError {
            io.show("specific=true")
        } catch e: Error {
            io.show("specific=false")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("specific=true"),
        "CsvError should match the specific clause before the fallback:\n{stdout}"
    );
}
