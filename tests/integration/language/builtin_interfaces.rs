use crate::common::*;

// ---------------------------------------------------------------------------
// Built-in interfaces — Serializable, Equatable, Comparable, Iterable
// ---------------------------------------------------------------------------

#[test]
fn serializable_to_json_on_untagged_map_uses_custom_impl() {
    // Regression: Json.stringify on a struct literal not bound to a typed variable
    // must still invoke the Serializable impl when there is exactly one struct type
    // whose field set exactly matches the map's keys.
    let src = r#"
type Event { name: str, score: int }
impl Serializable for Event {
  task to_json(self) -> str {
    "name={self.name};score={self.score}"
  }
}
task run_test() {
  Io.show(Json.stringify({ name: "click", score: 7 }))
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("name=click;score=7"),
        "expected custom to_json output: {stdout}"
    );
}

#[test]
fn serializable_ambiguous_field_set_falls_back_to_builtin() {
    // When two struct types share the exact same field set, Json.stringify must
    // not guess and must fall back to the built-in JSON serializer.
    let src = r#"
type A { x: int }
type B { x: int }
impl Serializable for A { task to_json(self) -> str { "A" } }
impl Serializable for B { task to_json(self) -> str { "B" } }
task run_test() {
  out = Json.stringify({ x: 1 })
  Io.show(out)
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    // Built-in serializer produces JSON, not "A" or "B".
    assert!(
        stdout.contains('{') || stdout.contains("null"),
        "expected built-in JSON output: {stdout}"
    );
    // Must not contain the custom impl output.
    assert!(
        !stdout.contains("\"A\"") && !stdout.trim().eq("A"),
        "ambiguous dispatch fired: {stdout}"
    );
    assert!(
        !stdout.contains("\"B\"") && !stdout.trim().eq("B"),
        "ambiguous dispatch fired: {stdout}"
    );
}

#[test]
fn serializable_to_json_used_by_json_stringify() {
    let src = r#"
type Event { name: str, score: int }
impl Serializable for Event {
  task to_json(self) -> str {
    "name={self.name};score={self.score}"
  }
}
task run_test() {
  e: Event = { name: "goal", score: 3 }
  Io.show(Json.stringify(e))
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("goal"), "got: {stdout}");
    assert!(stdout.contains('3'), "score: {stdout}");
}

#[test]
fn equatable_equals_method_is_callable() {
    let src = r#"
type Point { x: int, y: int }
impl Equatable for Point {
  task equals(self, other: Point) -> bool {
    self.x == other.x and self.y == other.y
  }
}
task run_test() {
  a: Point = { x: 1, y: 2 }
  b: Point = { x: 1, y: 2 }
  c: Point = { x: 9, y: 0 }
  Io.show("{a.equals(b)}")
  Io.show("{a.equals(c)}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("true"), "equals true: {stdout}");
    assert!(stdout.contains("false"), "equals false: {stdout}");
}

#[test]
fn comparable_sort_orders_structs_ascending() {
    let src = r#"
type Score { val: int }
impl Comparable for Score {
  task compare(self, other: Score) -> int {
    self.val - other.val
  }
}
task run_test() {
  items: list[Score] = [{ val: 30 }, { val: 10 }, { val: 20 }]
  sorted = items.sort()
  for s in sorted {
    Io.show("{s.val}")
  }
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    let vals: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(vals, vec!["10", "20", "30"], "sorted: {stdout}");
}

#[test]
fn comparable_min_max_on_structs() {
    let src = r#"
type Score { val: int }
impl Comparable for Score {
  task compare(self, other: Score) -> int {
    self.val - other.val
  }
}
task run_test() {
  items: list[Score] = [{ val: 30 }, { val: 10 }, { val: 20 }]
  lo = items.min()
  hi = items.max()
  Io.show("{lo.val}")
  Io.show("{hi.val}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("10"), "min: {stdout}");
    assert!(stdout.contains("30"), "max: {stdout}");
}

#[test]
fn iterable_struct_via_typed_binding_dispatches_items() {
    // Regression for the dead Value::Map guard bug: a struct returned from a task
    // with a declared return type is promoted to Value::Struct at the call boundary,
    // so find_impl_task("items") fires correctly in the for loop.
    let src = r#"
type Range { lo: int, hi: int }
impl Iterable for Range {
  task items(self) -> list[int] {
    result: list[int] = []
    i = self.lo
    while i <= self.hi {
      result += [i]
      i += 1
    }
    result
  }
}
task make_range() -> Range { { lo: 2, hi: 4 } }
task run_test() {
  for n in make_range() {
    Io.show("{n}")
  }
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('2'), "2: {stdout}");
    assert!(stdout.contains('3'), "3: {stdout}");
    assert!(stdout.contains('4'), "4: {stdout}");
}

#[test]
fn iterable_items_used_in_for_loop() {
    let src = r#"
type Range { lo: int, hi: int }
impl Iterable for Range {
  task items(self) -> list[int] {
    result: list[int] = []
    i = self.lo
    while i <= self.hi {
      result += [i]
      i += 1
    }
    result
  }
}
task run_test() {
  r: Range = { lo: 1, hi: 4 }
  for n in r {
    Io.show("{n}")
  }
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('1'), "1: {stdout}");
    assert!(stdout.contains('2'), "2: {stdout}");
    assert!(stdout.contains('3'), "3: {stdout}");
    assert!(stdout.contains('4'), "4: {stdout}");
}

#[test]
fn builtin_interfaces_cannot_be_redeclared() {
    for iface in [
        "Stringable",
        "Comparable",
        "Equatable",
        "Serializable",
        "Iterable",
    ] {
        let src = format!(
            "interface {iface} {{ task dummy(self) -> str }}\ntask run_test() {{ Io.show(\"ok\") }}\nrun_test()"
        );
        let (ok, _stdout, stderr) = run_inline(&src, false);
        assert!(!ok, "{iface} should be rejected");
        assert!(
            stderr.contains("built-in"),
            "{iface}: expected 'built-in' in stderr, got: {stderr}"
        );
    }
}

#[test]
fn iterable_return_type_can_be_concrete_list() {
    let src = r#"
type Pair { a: int, b: int }
impl Iterable for Pair {
  task items(self) -> list[int] {
    [self.a, self.b]
  }
}
task run_test() {
  p: Pair = { a: 7, b: 8 }
  for n in p {
    Io.show("{n}")
  }
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('7'), "7: {stdout}");
    assert!(stdout.contains('8'), "8: {stdout}");
}
