use crate::common::*;

// ---------------------------------------------------------------------------
// Numeric namespace — abs, floor, ceil, round
// ---------------------------------------------------------------------------

#[test]
fn numeric_abs_float() {
    let src = r#"
use std/io
agent NumTest {
    @tools [io]
    @on_start {
        v = -3.75
        io.show("abs={v.abs()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("abs=3.75"), "float abs failed:\n{stdout}");
}

#[test]
fn numeric_abs_int() {
    let src = r#"
use std/io
agent NumTest {
    @tools [io]
    @on_start {
        v = -5
        io.show("abs={v.abs()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("abs=5"), "int abs failed:\n{stdout}");
}

#[test]
fn numeric_floor() {
    let src = r#"
use std/io
agent NumTest {
    @tools [io]
    @on_start {
        v = 3.7
        io.show("floor={v.floor()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("floor=3"), "floor failed:\n{stdout}");
}

#[test]
fn numeric_ceil() {
    let src = r#"
use std/io
agent NumTest {
    @tools [io]
    @on_start {
        v = 3.2
        io.show("ceil={v.ceil()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("ceil=4"), "ceil failed:\n{stdout}");
}

#[test]
fn numeric_round() {
    let src = r#"
use std/io
agent NumTest {
    @tools [io]
    @on_start {
        v = 3.5
        io.show("round={v.round()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("round=4"), "round failed:\n{stdout}");
}

#[test]
fn numeric_chain() {
    let src = r#"
use std/io
agent NumTest {
    @tools [io]
    @on_start {
        v = -3.75
        io.show("chained={v.abs().ceil()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("chained=4"),
        "chained abs().ceil() failed:\n{stdout}"
    );
}

#[test]
fn numeric_int_floor_noop() {
    let src = r#"
use std/io
agent NumTest {
    @tools [io]
    @on_start {
        v = 7
        io.show("floor={v.floor()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("floor=7"),
        "int floor no-op failed:\n{stdout}"
    );
}
