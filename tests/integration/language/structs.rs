use crate::common::*;

// ---------------------------------------------------------------------------
// v0.1.31 — Struct type identity (issue #16)
// ---------------------------------------------------------------------------

#[test]
fn named_struct_checker_rejects_wrong_named_type_at_call_site() {
    // The checker must reject Score where Point is expected even when field shapes match.
    let src = r#"
type Point { x: int, y: int }
type Score { x: int, y: int }
task go(p: Point) -> int { p.x }
task run_test() {
  s: Score = { x: 1, y: 2 }
  go(s)
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed — Score is not assignable to Point");
    assert!(
        stderr.contains("Point") || stderr.contains("Score") || stderr.contains("mismatch"),
        "expected a type error mentioning Point or Score: {stderr}"
    );
}

#[test]
fn nullable_struct_annotation_promotes_map_to_struct() {
    // A nullable type annotation `Score?` must still promote a map literal to Value::Struct.
    let src = r#"
use std/io
type Score { val: int }
interface Gettable { task get_val(self) -> int }
impl Gettable for Score {
  task get_val(self) -> int { self.val }
}
task run_test() {
  s: Score? = { val: 55 }
  io.show("{s.get_val()}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("55"), "expected 55: {stdout}");
}

#[test]
fn distinct_named_structs_with_same_fields_are_not_interchangeable() {
    // Two types with identical field shapes must not be assignable to each other.
    let src = r#"
type Point { x: int, y: int }
type Offset { x: int, y: int }
task use_point(p: Point) -> int { p.x }
task run_test() {
  o: Offset = { x: 1, y: 2 }
  use_point(o)
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(
        !ok,
        "should have failed — Offset is not assignable to Point"
    );
    assert!(
        stderr.contains("Point") || stderr.contains("Offset") || stderr.contains("mismatch"),
        "expected a type error mentioning Point or Offset: {stderr}"
    );
}

#[test]
fn anonymous_struct_literal_is_assignable_to_named_struct() {
    // An untyped struct literal {x:1, y:2} is assignable to a named struct type.
    let src = r#"
use std/io
type Point { x: int, y: int }
task use_point(p: Point) -> int { p.x }
task run_test() {
  io.show("{use_point({ x: 3, y: 4 })}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('3'), "expected 3: {stdout}");
}

#[test]
fn named_struct_dispatch_requires_typed_variable() {
    // Untyped map literals do not dispatch to named-struct impl methods.
    // Assigning to a typed list[TypeName] variable promotes elements so dispatch works.
    let src = r#"
use std/io
type Item { val: int }
interface Gettable { task get_val(self) -> int }
impl Gettable for Item {
  task get_val(self) -> int { self.val }
}
task run_test() {
  typed: list[Item] = [{ val: 42 }]
  io.show("{typed.first().get_val()}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("42"), "expected 42: {stdout}");
}

#[test]
fn aliased_named_struct_annotation_promotes_to_canonical_type() {
    // Alias annotations must tag the runtime value with the target struct name
    // so nominal impl dispatch can find methods declared for the canonical type.
    let src = r#"
use std/io
type Item { val: int }
type Alias = Item
interface Gettable { task get_val(self) -> int }
impl Gettable for Item {
  task get_val(self) -> int { self.val }
}
task run_test() {
  x: Alias = { val: 42 }
  io.show("{x.get_val()}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("42"), "expected 42: {stdout}");
}

#[test]
fn named_struct_describe_ty_shows_name() {
    // describe_ty now returns the declared name for named structs,
    // improving error messages throughout the checker.
    let src = r#"
type Score { val: int }
task need_str(s: str) -> str { s }
task run_test() {
  sc: Score = { val: 10 }
  need_str(sc)
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed — Score is not str");
    assert!(
        stderr.contains("Score"),
        "expected error mentioning Score by name: {stderr}"
    );
}

#[test]
fn int_keyed_map_is_not_promoted_to_struct() {
    // Regression: a map with integer keys must NOT be promoted to a struct.
    // Struct field names are always valid identifiers; "1" is not.
    // The map should pass through as Value::Map and produce normal JSON output.
    let src = r#"
use std/io
type Scores { a: int, b: int }
task run_test() {
  m: map[int, int] = {1: 10, 2: 20}
  io.show("{m.len()}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('2'), "expected len=2: {stdout}");
}

#[test]
fn map_of_struct_values_promotes_elements_at_typed_param() {
    // Regression for TypeExpr::Map arm in promote_value: when a task parameter
    // is declared map[str, TypeName], each value in the map must be promoted to
    // Value::Struct so impl dispatch works on the values.
    let src = r#"
use std/io
type Score { val: int }
interface Gettable { task get_val(self) -> int }
impl Gettable for Score {
  task get_val(self) -> int { self.val }
}
task use_scores(scores: map[str, Score]) -> int {
  scores.get("alice").get_val()
}
task run_test() {
  io.show("{use_scores({ "alice": { val: 42 } })}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("42"), "expected 42: {stdout}");
}

// ---------------------------------------------------------------------------
// v0.1.13 — Destructuring
// ---------------------------------------------------------------------------

#[test]
fn destruct_struct_shorthand() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    val = {name: "alice", age: 30}
    {name, age} = val
    io.show("{name}:{age}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("alice:30"),
        "destructure shorthand failed:\n{stdout}"
    );
}

#[test]
fn destruct_struct_rename() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    val = {urgency: "high", category: "bug"}
    {urgency: u, category: c} = val
    io.show("{u}:{c}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("high:bug"),
        "destructure rename failed:\n{stdout}"
    );
}

#[test]
fn destruct_tuple() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    pair = ("alpha", 42)
    (label, count) = pair
    io.show("{label}:{count}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("alpha:42"),
        "tuple destructure failed:\n{stdout}"
    );
}

#[test]
fn destruct_in_for_loop() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    items = [
      {name: "alice", score: 10},
      {name: "bob", score: 20},
    ]
    for {name, score} in items {
      io.show("{name}={score}")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("alice=10"),
        "for-loop destructure failed:\n{stdout}"
    );
    assert!(
        stdout.contains("bob=20"),
        "for-loop destructure failed:\n{stdout}"
    );
}

#[test]
fn destruct_in_task_param() {
    let src = r#"
use std/io
type Point = {x: int, y: int}

task show_point({x, y}: Point) {
  io.show("{x},{y}")
}

agent A {
  # @tools must cover the transitive closure: show_point uses io.
  @tools [io]
  @on_start {
    show_point({x: 3, y: 7})
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("3,7"),
        "task param destructure failed:\n{stdout}"
    );
}

#[test]
fn destruct_missing_field_type_error() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    val = {name: "alice"}
    {name, nonexistent} = val
    io.show("{name}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should fail: nonexistent field in destructure");
    assert!(
        stderr.contains("nonexistent"),
        "error should mention the missing field:\n{stderr}"
    );
}

#[test]
fn destruct_tuple_arity_mismatch_type_error() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    triple = (1, 2, 3)
    (a, b) = triple
    io.show("{a}:{b}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should fail: tuple arity mismatch");
    assert!(
        stderr.contains("tuple") || stderr.contains("element"),
        "error should mention tuple arity:\n{stderr}"
    );
}

#[test]
fn destruct_keyword_field_from() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    email = {from: "alice@example.com", subject: "hello"}
    {from, subject} = email
    io.show("{from}:{subject}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("alice@example.com:hello"),
        "keyword field 'from' destructure failed:\n{stdout}"
    );
}
