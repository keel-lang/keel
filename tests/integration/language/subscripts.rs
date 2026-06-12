use crate::common::*;

// ─── Subscript access — list[i], str[i], map[k] ─────────────────────────────

#[test]
fn subscript_list_in_bounds() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        items = [10, 20, 30]
        v = items[1]
        io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("20"), "expected 20, got:\n{stdout}");
}

#[test]
fn subscript_list_out_of_bounds_errors() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        items = [10, 20, 30]
        v = items[99]
        io.show(v)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error on OOB");
    assert!(
        stderr.contains("out of bounds"),
        "expected 'out of bounds' error, got:\n{stderr}"
    );
}

#[test]
fn subscript_list_negative_errors() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        items = [10, 20, 30]
        v = items[-1]
        io.show(v)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error on negative index");
    assert!(
        stderr.contains("out of bounds"),
        "expected 'out of bounds' error, got:\n{stderr}"
    );
}

#[test]
fn subscript_string_in_bounds() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        word = "hello"
        ch = word[1]
        io.show(ch)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("e"), "expected 'e', got:\n{stdout}");
}

#[test]
fn subscript_string_out_of_bounds_errors() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        word = "hi"
        ch = word[99]
        io.show(ch)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error on OOB string index");
    assert!(
        stderr.contains("out of bounds"),
        "expected 'out of bounds' error, got:\n{stderr}"
    );
}

#[test]
fn subscript_list_first_element() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        items = ["alpha", "beta", "gamma"]
        v = items[0]
        io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("alpha"), "expected 'alpha', got:\n{stdout}");
}

#[test]
fn subscript_map_hit() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        scores: map[str, int] = {alice: 90, bob: 85}
        v = scores["alice"]
        io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("90"), "expected 90, got:\n{stdout}");
}

#[test]
fn subscript_map_miss_returns_none() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        scores: map[str, int] = {alice: 90}
        v = scores["nobody"]
        io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("none"),
        "expected none for missing key, got:\n{stdout}"
    );
}
