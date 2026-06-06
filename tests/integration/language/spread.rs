use crate::common::*;

// ---------------------------------------------------------------------------
// Struct spread-update  { ...base, field: new }
// ---------------------------------------------------------------------------

#[test]
fn struct_spread_update_single_field() {
    let src = r#"
type Order { id: str, status: str, amount: float }
task run_test() {
  o: Order = { id: "ord-1", status: "pending", amount: 9.99 }
  filled = { ...o, status: "filled" }
  Io.show(filled.id)
  Io.show(filled.status)
  Io.show("{filled.amount}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("ord-1"), "id preserved: {stdout}");
    assert!(stdout.contains("filled"), "status updated: {stdout}");
    assert!(stdout.contains("9.99"), "amount preserved: {stdout}");
}

#[test]
fn struct_spread_update_multiple_overrides() {
    let src = r#"
type Point { x: int, y: int, z: int }
task run_test() {
  p: Point = { x: 1, y: 2, z: 3 }
  q = { ...p, x: 10, z: 30 }
  Io.show("{q.x}")
  Io.show("{q.y}")
  Io.show("{q.z}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("10"), "x updated: {stdout}");
    assert!(stdout.contains('2'), "y preserved: {stdout}");
    assert!(stdout.contains("30"), "z updated: {stdout}");
}

#[test]
fn struct_spread_update_no_overrides_is_copy() {
    let src = r#"
type Rec { a: int, b: str }
task run_test() {
  r: Rec = { a: 7, b: "hello" }
  r2 = { ...r }
  Io.show("{r2.a}")
  Io.show(r2.b)
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('7'), "a: {stdout}");
    assert!(stdout.contains("hello"), "b: {stdout}");
}

#[test]
fn struct_spread_update_preserves_type_tag() {
    let src = r#"
type Item { name: str, price: float }
task run_test() {
  item: Item = { name: "Widget", price: 9.99 }
  updated = { ...item, price: 4.99 }
  Io.show(typeof(updated))
  Io.show(updated.name)
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("Item"), "type tag preserved: {stdout}");
    assert!(stdout.contains("Widget"), "name preserved: {stdout}");
}

#[test]
fn struct_spread_update_chained() {
    let src = r#"
type Config { host: str, port: int, debug: bool }
task run_test() {
  base: Config = { host: "localhost", port: 8080, debug: false }
  dev = { ...base, debug: true }
  prod = { ...dev, host: "prod.example.com", debug: false }
  Io.show(prod.host)
  Io.show("{prod.port}")
  Io.show("{prod.debug}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("prod.example.com"), "host: {stdout}");
    assert!(stdout.contains("8080"), "port: {stdout}");
    assert!(stdout.contains("false"), "debug: {stdout}");
}

#[test]
fn struct_spread_update_unknown_field_is_type_error() {
    let src = r#"
type Rec { a: int }
task run_test() {
  r: Rec = { a: 1 }
  bad = { ...r, nonexistent: 99 }
  Io.show("{bad.a}")
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected type error for unknown field");
    assert!(
        stderr.contains("nonexistent") || stderr.contains("unknown field"),
        "got: {stderr}"
    );
}

#[test]
fn struct_spread_update_formatter_roundtrip() {
    // Format a program containing spread-update twice; formatter must be idempotent.
    let src = r#"
type Point { x: int, y: int }
task run_test() {
  p: Point = { x: 1, y: 2 }
  q = { ...p, x: 10 }
  Io.show("{q.x}")
}
run_test()
"#;
    let once = keel_lang::session::fmt_source(src, "t.keel").expect("fmt once");
    let twice = keel_lang::session::fmt_source(&once, "t.keel").expect("fmt twice");
    assert_eq!(
        once, twice,
        "formatter not idempotent:\n--- once ---\n{once}\n--- twice ---\n{twice}"
    );
    assert!(
        once.contains("...p"),
        "spread not in formatted output: {once}"
    );
}

#[test]
fn struct_spread_update_untyped_map_base() {
    // Untyped struct literals are Value::Map at runtime (not Value::Struct).
    // Spread-update must work through the Value::Map branch, not just Value::Struct.
    let src = r#"
task run_test() {
  r = { a: 1, b: "hello" }
  q = { ...r, a: 99 }
  Io.show("{q.a}")
  Io.show(q.b)
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("99"), "a overridden: {stdout}");
    assert!(stdout.contains("hello"), "b preserved: {stdout}");
}

#[test]
fn struct_spread_update_duplicate_override_is_type_error() {
    let src = r#"
type Rec { x: int, y: int }
task run_test() {
  r: Rec = { x: 1, y: 2 }
  bad = { ...r, x: 10, x: 20 }
  Io.show("{bad.x}")
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected type error for duplicate override field");
    assert!(
        stderr.contains("duplicate") || stderr.contains('x'),
        "got: {stderr}"
    );
}

#[test]
fn struct_spread_update_dynamic_base_unknown_field_is_runtime_error() {
    // When the base is typed `dynamic` the checker skips field validation.
    // The runtime guard must reject an unknown override field.
    let src = r#"
type Config { host: str, port: int }
task apply(cfg: dynamic) -> dynamic {
  return { ...cfg, phantom: true }
}
task run_test() {
  c: Config = { host: "localhost", port: 8080 }
  apply(c)
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(
        !ok,
        "expected runtime error for unknown field on dynamic base"
    );
    assert!(
        stderr.contains("phantom") || stderr.contains("unknown field"),
        "got: {stderr}"
    );
}

#[test]
fn struct_spread_update_map_base_works() {
    // Spread-update on an explicit map[str, int] variable — keys are unrestricted.
    let src = r#"
task run_test() {
  m: map[str, int] = { "a": 1, "b": 2 }
  m2 = { ...m, "c": 3 }
  Io.show("{m2.a}")
  Io.show("{m2.b}")
  Io.show("{m2.c}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('1'), "a: {stdout}");
    assert!(stdout.contains('2'), "b: {stdout}");
    assert!(stdout.contains('3'), "c: {stdout}");
}

#[test]
fn struct_spread_update_map_base_wrong_value_type_is_error() {
    // Override value type must match the map's declared value type.
    let src = r#"
task run_test() {
  m: map[str, int] = { "a": 1 }
  bad = { ...m, "b": "not-an-int" }
  Io.show("{bad.b}")
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(
        !ok,
        "expected type error for wrong value type in map spread-update"
    );
    assert!(
        stderr.contains("str") || stderr.contains("int") || stderr.contains("expected"),
        "got: {stderr}"
    );
}
