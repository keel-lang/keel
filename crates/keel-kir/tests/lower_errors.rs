//! M0's lowering is a *rejecting* subset, not a best-effort one (AGENTS.md:
//! "no silent fallbacks"). These tests pin that every construct outside the
//! scalar subset fails loudly with a `LowerError`, rather than silently
//! dropping/approximating it.

/// Pins what `keel-kir`'s own lowering rejects, independent of the type
/// checker: several fixtures below (structs, generics, agents, …) are
/// perfectly valid Keel the checker accepts today — it's the scalar-subset
/// *lowering* that hasn't caught up to the full language yet. So this
/// deliberately ignores the checker's diagnostics rather than gating on
/// them; `artifacts` is still needed (lowering's signature requires it) but
/// its accuracy on a program the checker rejects isn't this test's concern.
fn lower_err(source: &str) -> String {
    let (program, _named) = keel_syntax::parse_source(source, "t.keel").expect("must parse");
    let (_diagnostics, artifacts) =
        keel_compiler::types::checker::check_program_with_artifacts(&program, false);
    let err = keel_kir::lower(&program, "t.keel", &artifacts)
        .expect_err("must be rejected by M0 lowering");
    err.to_string()
}

#[test]
fn for_over_list_param_is_rejected_at_signature() {
    // `for` over a range lowers now (see golden fixture `for_range.kir`);
    // this pins that a `list[int]` param is still rejected — at the
    // signature, before the for-loop body is ever reached.
    let msg = lower_err(
        r#"
task sum(xs: list[int]) -> int {
  total = 0
  for x in xs {
    total += x
  }
  return total
}
"#,
    );
    assert!(msg.contains("list type"), "unexpected message: {msg}");
}

#[test]
fn for_over_non_range_iterable_is_rejected() {
    let msg = lower_err(
        r#"
task f(n: int) -> int {
  for x in n {
  }
  return 0
}
"#,
    );
    assert!(
        msg.contains("non-range iterable"),
        "unexpected message: {msg}"
    );
}

#[test]
fn for_with_where_filter_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  for x in 0..5 if x > 2 {
  }
  return 0
}
"#,
    );
    assert!(msg.contains("filter"), "unexpected message: {msg}");
}

#[test]
fn for_range_bound_type_mismatch_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  for x in 0..1.5 {
  }
  return 0
}
"#,
    );
    assert!(msg.contains("expected `int`"), "unexpected message: {msg}");
}

#[test]
fn string_interpolation_is_rejected() {
    let msg = lower_err(
        r#"
task greet(name: str) -> str {
  return "hi {name}"
}
"#,
    );
    assert!(
        msg.contains("string interpolation"),
        "unexpected message: {msg}"
    );
}

#[test]
fn struct_literal_is_rejected() {
    let msg = lower_err(
        r#"
task make() -> int {
  p = {x: 1, y: 2}
  return 1
}
"#,
    );
    assert!(msg.contains("struct literal"), "unexpected message: {msg}");
}

#[test]
fn generic_task_is_rejected() {
    let msg = lower_err(
        r#"
task first[T](xs: list[T]) -> int {
  return 0
}
"#,
    );
    assert!(msg.contains("generic task"), "unexpected message: {msg}");
}

#[test]
fn agent_declaration_is_rejected() {
    let msg = lower_err(
        r#"
agent A {
  @role "x"
}
"#,
    );
    assert!(
        msg.contains("agent declaration"),
        "unexpected message: {msg}"
    );
}

#[test]
fn type_mismatch_in_let_annotation_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  x: str = 1
  return 0
}
"#,
    );
    assert!(msg.contains("expected"), "unexpected message: {msg}");
}

#[test]
fn wrong_arg_count_is_rejected() {
    let msg = lower_err(
        r#"
task add(a: int, b: int) -> int {
  return a + b
}

x = add(1)
"#,
    );
    assert!(msg.contains("argument"), "unexpected message: {msg}");
}

#[test]
fn unknown_std_module_is_rejected() {
    let msg = lower_err("use std/bogus\n");
    assert!(
        msg.contains("unknown std module"),
        "unexpected message: {msg}"
    );
}

#[test]
fn file_path_use_import_is_rejected() {
    let msg = lower_err("use \"./other.keel\"\n");
    assert!(msg.contains("file-path"), "unexpected message: {msg}");
}

#[test]
fn symbol_list_use_import_is_rejected() {
    let msg = lower_err("use read from std/file\n");
    assert!(msg.contains("symbol-list"), "unexpected message: {msg}");
}

#[test]
fn unknown_namespace_method_is_rejected() {
    let msg = lower_err(
        r#"
use std/io

io.nonexistent("hi")
"#,
    );
    assert!(msg.contains("has no method"), "unexpected message: {msg}");
}

#[test]
fn namespace_call_wrong_arg_count_is_rejected() {
    let msg = lower_err(
        r#"
use std/io

io.show()
"#,
    );
    assert!(msg.contains("argument"), "unexpected message: {msg}");
}

#[test]
fn named_argument_to_namespace_call_is_rejected() {
    let msg = lower_err(
        r#"
use std/io

io.show(value: "hi")
"#,
    );
    assert!(msg.contains("named or spread"), "unexpected message: {msg}");
}

#[test]
fn namespace_method_with_dynamic_result_is_rejected() {
    // `env.get` returns `TySpec::NullableStr`, which has no `KirType`
    // equivalent until nullable types land (M2+).
    let msg = lower_err(
        r#"
use std/env

x = env.get("HOME")
"#,
    );
    assert!(msg.contains("M2+ types"), "unexpected message: {msg}");
}

#[test]
fn local_shadowing_a_namespace_binding_is_not_lowered_as_a_namespace_call() {
    // Mirrors the checker's "lexical locals shadow globals" rule
    // (`db = db.connect(...)` rebinds `db`) — once `io` is a local, `io.foo()`
    // is an ordinary (unsupported) value method call, not a namespace call.
    let msg = lower_err(
        r#"
use std/io

io = 5
io.show("hi")
"#,
    );
    assert!(msg.contains("method call"), "unexpected message: {msg}");
}

#[test]
fn struct_literal_missing_a_field_is_rejected() {
    let msg = lower_err(
        r#"
type Point { x: int, y: int }

p: Point = { x: 1 }
"#,
    );
    assert!(
        msg.contains("missing field `y`"),
        "unexpected message: {msg}"
    );
}

#[test]
fn struct_literal_with_an_unknown_field_is_rejected() {
    let msg = lower_err(
        r#"
type Point { x: int, y: int }

p: Point = { x: 1, y: 2, z: 3 }
"#,
    );
    assert!(
        msg.contains("has no field `z`"),
        "unexpected message: {msg}"
    );
}

#[test]
fn struct_literal_field_type_mismatch_is_rejected() {
    let msg = lower_err(
        r#"
type Point { x: int, y: int }

p: Point = { x: 1, y: "two" }
"#,
    );
    assert!(msg.contains("expected"), "unexpected message: {msg}");
}

#[test]
fn field_access_on_a_non_struct_value_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  x = 1
  return x.y
}
"#,
    );
    assert!(
        msg.contains("field access on a non-struct value"),
        "unexpected message: {msg}"
    );
}

#[test]
fn field_access_on_an_unknown_field_is_rejected() {
    let msg = lower_err(
        r#"
type Point { x: int, y: int }

task f(p: Point) -> int {
  return p.z
}
"#,
    );
    assert!(
        msg.contains("has no field `z`"),
        "unexpected message: {msg}"
    );
}

#[test]
fn struct_spread_update_over_a_non_identifier_base_is_rejected() {
    let msg = lower_err(
        r#"
type Point { x: int, y: int }

task make() -> Point {
  return { x: 1, y: 2 }
}

p: Point = { ...make(), x: 3 }
"#,
    );
    assert!(
        msg.contains("non-identifier base"),
        "unexpected message: {msg}"
    );
}

#[test]
fn non_struct_type_declaration_is_rejected() {
    let msg = lower_err("type Color = red | green | blue\n");
    assert!(
        msg.contains("non-struct type declaration"),
        "unexpected message: {msg}"
    );
}

#[test]
fn generic_struct_type_is_rejected() {
    let msg = lower_err("type Box[T] { value: int }\n");
    assert!(
        msg.contains("generic struct type"),
        "unexpected message: {msg}"
    );
}
