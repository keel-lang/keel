use crate::common::*;

// ---------------------------------------------------------------------------
// Map operations
// ---------------------------------------------------------------------------

#[test]
fn map_get_method_inferred_as_nullable_value() {
    // map.get returns T?, so assigning to a non-nullable should fail check.
    let src = r#"
task t() {
    m: map[str, int] = {a: 1}
    n: int = m.get("a")
}
"#;
    let (ok, stdout, stderr) = check_inline_output(src);
    let combined = format!("{stdout}{stderr}");
    assert!(!ok, "expected check to fail on map.get assignment");
    assert!(
        combined.contains("int?") || combined.contains("nullable"),
        "expected nullable-mismatch diagnostic:\n{combined}"
    );
}

#[test]
fn map_keys_method_inferred_as_list_of_keys() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        m: map[str, int] = {a: 1, b: 2}
        ks: list[str] = m.keys()
        io.show("keys-count={ks.count()}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("keys-count=2"),
        "expected 2 keys:\n{stdout}"
    );
}

#[test]
fn map_float_key_rejected_at_compile_time() {
    let src = r#"
task t() {
    m: map[float, str] = {a: "x"}
}
t()
"#;
    let (ok, stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected type error for float map key");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("float") && combined.contains("NaN"),
        "expected float/NaN diagnostic:\n{combined}"
    );
}

#[test]
fn map_nullable_key_rejected_at_compile_time() {
    let src = r#"
task t() {
    m: map[str?, int] = {a: 1}
}
t()
"#;
    let (ok, stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected type error for nullable map key");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("nullable"),
        "expected nullable diagnostic:\n{combined}"
    );
}

#[test]
fn map_struct_key_rejected_at_compile_time() {
    let src = r#"
type Point { x: int, y: int }
task t() {
    m: map[Point, str] = {a: "x"}
}
t()
"#;
    let (ok, stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected type error for struct map key");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("Hashable") || combined.contains("struct"),
        "expected Hashable/struct diagnostic:\n{combined}"
    );
}

#[test]
fn map_int_key_literal_parses_and_runs() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        m: map[int, str] = {1: "one", 2: "two"}
        v = m[1]
        io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("one"), "expected 'one', got:\n{stdout}");
}

#[test]
fn map_bool_key_literal_parses_and_runs() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        m: map[bool, str] = {true: "on", false: "off"}
        v = m[true]
        io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("on"), "expected 'on', got:\n{stdout}");
}

#[test]
fn map_insert_returns_a_new_map_and_leaves_the_receiver_alone() {
    // The value-method contract shared with `list.push` and `set.add`: the
    // result has to be rebound, and an aliased binding never observes it.
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        a: map[str, int] = {apples: 1}
        b = a
        a.insert("discarded", 9)
        a = a.insert("pears", 2)
        io.show("a={a.len()} b={b.len()} dropped={a.contains("discarded")}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("a=2 b=1 dropped=false"),
        "expected .insert to return a fresh map:\n{stdout}"
    );
}

#[test]
fn map_insert_overwrites_an_existing_key() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        m: map[str, int] = {apples: 1}
        m = m.insert("apples", 9)
        io.show("len={m.len()} value={m.get("apples") ?? -1}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("len=1 value=9"),
        "expected an overwrite, not a second entry:\n{stdout}"
    );
}

#[test]
fn map_insert_accepts_int_and_bool_keys() {
    // `MapKey` admits str/int/bool; `.insert` must not narrow that to `str`
    // just because the compiled backend only models `map[str, V]` today.
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        m: map[int, str] = {1: "one"}
        m = m.insert(2, "two")
        io.show("len={m.len()} value={m.get(2) ?? "?"}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("len=2 value=two"),
        "expected an int-keyed insert to work:\n{stdout}"
    );
}
