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
fn list_of_struct_element_type_is_rejected() {
    // `list[int]` lowers now (see golden fixture `lists.kir`, #147) — this
    // pins that a struct-element list is still rejected at the type
    // annotation (struct/enum elements need `Value` marshaling that
    // doesn't exist yet).
    let msg = lower_err(
        r#"
type Point { x: int, y: int }

task f(xs: list[Point]) -> int {
  return 0
}
"#,
    );
    assert!(
        msg.contains("list element type other than int/float/bool/str"),
        "unexpected message: {msg}"
    );
}

#[test]
fn for_over_non_range_non_list_iterable_is_rejected() {
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
        msg.contains("non-range, non-list iterable"),
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
fn type_alias_declaration_is_rejected() {
    // `type Color = red | green | blue` is a simple enum, not an alias — and
    // is no longer rejected as of #146 (see `enum_when` fixtures). Aliases
    // (`type Timestamp = datetime`) remain unsupported.
    let msg = lower_err("type Timestamp = datetime\n");
    assert!(
        msg.contains("rich enum or type-alias declaration"),
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

#[test]
fn rich_enum_type_declaration_is_rejected() {
    let msg = lower_err(
        r#"
type Action =
  | reply { to: str }
  | archive
"#,
    );
    assert!(
        msg.contains("rich enum or type-alias declaration"),
        "unexpected message: {msg}"
    );
}

#[test]
fn generic_enum_type_is_rejected() {
    let msg = lower_err("type Pair[T] = a | b\n");
    assert!(
        msg.contains("generic enum type"),
        "unexpected message: {msg}"
    );
}

#[test]
fn unknown_enum_variant_in_construction_is_rejected() {
    let msg = lower_err(
        r#"
type Priority = low | medium | high

task f() -> Priority {
  return Priority.urgent
}
"#,
    );
    assert!(
        msg.contains("has no variant `urgent`"),
        "unexpected message: {msg}"
    );
}

#[test]
fn when_over_a_non_identifier_subject_is_rejected() {
    let msg = lower_err(
        r#"
type Priority = low | medium | high

task make() -> Priority {
  return Priority.low
}

task f() -> str {
  when make() {
    low => { return "low" }
    medium => { return "medium" }
    high => { return "high" }
  }
}
"#,
    );
    assert!(
        msg.contains("non-identifier subject"),
        "unexpected message: {msg}"
    );
}

#[test]
fn when_arm_guard_is_rejected() {
    let msg = lower_err(
        r#"
task f(n: int) -> str {
  when n {
    x where x > 0 => { return "positive" }
    _ => { return "other" }
  }
}
"#,
    );
    assert!(msg.contains("arm guard"), "unexpected message: {msg}");
}

#[test]
fn identifier_pattern_on_a_non_enum_scrutinee_is_rejected() {
    let msg = lower_err(
        r#"
task f(n: int) -> str {
  when n {
    x => { return "bound" }
    _ => { return "other" }
  }
}
"#,
    );
    assert!(
        msg.contains("identifier pattern on a non-enum scrutinee"),
        "unexpected message: {msg}"
    );
}

#[test]
fn when_expression_as_a_call_argument_is_still_rejected() {
    // `when` as an expression lowers in `let`/`return` position (issue
    // #160), but a call-argument or other nested-sub-expression position
    // would need every `lower_expr`/`lower_expr_expecting` call site to
    // thread out hoisted statements — out of scope for that issue, see
    // `lower/stmt.rs`'s `lower_when_expr_let` doc. Plain `lower_expr` still
    // rejects a bare `WhenExpr` outright, so this position still errors.
    let msg = lower_err(
        r#"
task describe(n: int) -> str {
  return f(when n {
    0 => "zero"
    _ => "nonzero"
  })
}

task f(s: str) -> str {
  return s
}
"#,
    );
    assert!(
        msg.contains("when` expression"),
        "unexpected message: {msg}"
    );
}

#[test]
fn when_expression_with_mismatched_arm_types_is_rejected() {
    let msg = lower_err(
        r#"
task f(n: int) -> str {
  result = when n {
    0 => "zero"
    _ => 1
  }
  return result
}
"#,
    );
    assert!(msg.contains("expected"), "unexpected message: {msg}");
}

#[test]
fn empty_list_literal_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  xs = []
  return 0
}
"#,
    );
    assert!(
        msg.contains("empty list literal"),
        "unexpected message: {msg}"
    );
}

#[test]
fn list_literal_with_mixed_element_types_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  xs = [1, "two"]
  return 0
}
"#,
    );
    assert!(
        msg.contains("mixed element types"),
        "unexpected message: {msg}"
    );
}

#[test]
fn unknown_list_method_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  xs = [1, 2, 3]
  ys = xs.reverse()
  return 0
}
"#,
    );
    assert!(msg.contains("list method"), "unexpected message: {msg}");
}

#[test]
fn list_push_wrong_arg_count_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  xs = [1, 2, 3]
  ys = xs.push(1, 2)
  return 0
}
"#,
    );
    assert!(msg.contains("`push`"), "unexpected message: {msg}");
}

#[test]
fn list_push_wrong_element_type_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  xs = [1, 2, 3]
  ys = xs.push("four")
  return 0
}
"#,
    );
    assert!(msg.contains("expected"), "unexpected message: {msg}");
}

#[test]
fn non_int_list_index_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  xs = [1, 2, 3]
  return xs["0"]
}
"#,
    );
    assert!(msg.contains("list index"), "unexpected message: {msg}");
}

#[test]
fn map_literal_with_a_non_str_key_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  stock: map[str, int] = {1: 2}
  return 0
}
"#,
    );
    assert!(msg.contains("non-str key"), "unexpected message: {msg}");
}

#[test]
fn duplicate_key_in_map_literal_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  stock: map[str, int] = {apples: 1, apples: 2}
  return 0
}
"#,
    );
    assert!(
        msg.contains("duplicate key `apples`"),
        "unexpected message: {msg}"
    );
}

#[test]
fn unknown_map_method_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  stock: map[str, int] = {apples: 1}
  ys = stock.pop()
  return 0
}
"#,
    );
    assert!(msg.contains("map method"), "unexpected message: {msg}");
}

#[test]
fn map_get_wrong_arg_count_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  stock: map[str, int] = {apples: 1}
  ys = stock.get()
  return 0
}
"#,
    );
    assert!(msg.contains("`get`"), "unexpected message: {msg}");
}

#[test]
fn set_value_method_call_is_rejected() {
    // Issue #162 scopes construct/pass-through only for `set[T]` (the
    // interpreter has no `Value::Set` variant and no set methods to match
    // against yet — see `KirType::Set`'s doc) — a method call on a set
    // falls through to the generic "method call" rejection, same as any
    // other unsupported receiver type.
    let msg = lower_err(
        r#"
task f() -> int {
  nums = set[1, 2, 3]
  return nums.len()
}
"#,
    );
    assert!(msg.contains("method call"), "unexpected message: {msg}");
}

#[test]
fn index_access_on_a_non_list_value_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  x = 1
  return x[0]
}
"#,
    );
    assert!(
        msg.contains("index access on a non-list value"),
        "unexpected message: {msg}"
    );
}

#[test]
fn non_default_parameter_after_a_defaulted_one_is_rejected() {
    let msg = lower_err(
        r#"
task f(a: int = 1, b: int) -> int {
  return a + b
}
"#,
    );
    assert!(
        msg.contains("defaults must be trailing"),
        "unexpected message: {msg}"
    );
}

#[test]
fn call_omitting_a_non_default_argument_is_rejected() {
    let msg = lower_err(
        r#"
task f(a: int, b: int = 2) -> int {
  return a + b
}

x = f()
"#,
    );
    assert!(msg.contains("takes"), "unexpected message: {msg}");
}

#[test]
fn call_with_too_many_arguments_is_still_rejected() {
    let msg = lower_err(
        r#"
task f(a: int, b: int = 2) -> int {
  return a + b
}

x = f(1, 2, 3)
"#,
    );
    assert!(msg.contains("takes"), "unexpected message: {msg}");
}

#[test]
fn null_safe_field_access_on_a_non_nullable_value_is_rejected() {
    let msg = lower_err(
        r#"
type Email { subject: str }

task f(email: Email) -> str? {
  return email?.subject
}
"#,
    );
    assert!(
        msg.contains("non-nullable value"),
        "unexpected message: {msg}"
    );
}

#[test]
fn null_coalesce_on_a_non_nullable_left_hand_side_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  return 1 ?? 0
}
"#,
    );
    assert!(
        msg.contains("non-nullable left-hand side"),
        "unexpected message: {msg}"
    );
}

#[test]
fn nullable_enum_inner_type_is_rejected() {
    let msg = lower_err(
        r#"
type Priority = low | medium | high

task f(p: Priority? = none) -> Priority {
  return p ?? Priority.low
}
"#,
    );
    assert!(
        msg.contains("nullable inner type"),
        "unexpected message: {msg}"
    );
}

#[test]
fn equality_comparison_between_nullable_values_is_rejected() {
    // `T?` has no single-value representation to compare bitwise/structurally
    // for scalar inner types (the `{i1, T}` pair) — scope this issue to the
    // nullable-value operators (`?.`, `??`) only; unwrap via `??` before
    // comparing.
    let msg = lower_err(
        r#"
task f(a: int? = none, b: int? = none) -> bool {
  return a == b
}
"#,
    );
    assert!(
        msg.contains("cannot compare nullable"),
        "unexpected message: {msg}"
    );
}

#[test]
fn string_interpolation_format_spec_is_rejected() {
    // `{expr:spec}` alignment/precision formatting is a distinct feature
    // from the concat-chain desugaring this issue implements — deferred,
    // not silently ignored.
    let msg = lower_err(
        r#"
task f(pi: float) -> str {
  return "{pi:.2f}"
}
"#,
    );
    assert!(msg.contains("format spec"), "unexpected message: {msg}");
}

#[test]
fn string_interpolation_of_a_struct_value_is_rejected() {
    // Struct/enum-to-string needs `Value` marshaling that doesn't exist
    // yet (coordinate with #145/#146 rather than block on them) — only
    // int/float/bool/str slots lower in this issue.
    let msg = lower_err(
        r#"
type Point { x: int, y: int }

task f(p: Point) -> str {
  return "point: {p}"
}
"#,
    );
    assert!(
        msg.contains("struct/enum to-string"),
        "unexpected message: {msg}"
    );
}

#[test]
fn multiple_catch_clauses_are_rejected() {
    // Only a single `catch e: Error`/`catch e: UserRaised` clause is
    // supported — both bind the same synthetic `UserRaised` shape, since
    // `raise` only ever produces one; per-namespace error kinds (a second,
    // distinct clause could meaningfully match) aren't modeled yet.
    let msg = lower_err(
        r#"
task f() -> int {
  try {
    raise "x"
  } catch e: UserRaised {
    return 1
  } catch e: Error {
    return 2
  }
  return 0
}
"#,
    );
    assert!(
        msg.contains("multiple catch clauses"),
        "unexpected message: {msg}"
    );
}

#[test]
fn catch_clause_over_a_non_error_type_is_rejected() {
    // Per-namespace error kinds (FileError, HttpError, ...) aren't modeled
    // by the compiled backend yet — only `Error`/`UserRaised` bind the
    // synthetic shape this issue's `raise` produces.
    let msg = lower_err(
        r#"
task f() -> int {
  try {
    raise "x"
  } catch e: FileError {
    return 1
  }
  return 0
}
"#,
    );
    assert!(
        msg.contains("error type other than `Error`/`UserRaised`"),
        "unexpected message: {msg}"
    );
}

#[test]
fn raise_of_a_non_str_value_is_rejected() {
    // The interpreter's non-`str` `Display`-coercion path (`raise 42`
    // becomes `"42"`) is a later M2/M3 concern — only a `str` message
    // lowers in this issue.
    let msg = lower_err(
        r#"
task f() -> int {
  raise 42
}
"#,
    );
    assert!(msg.contains("`raise` of a"), "unexpected message: {msg}");
}

#[test]
fn a_can_raise_task_returning_a_struct_is_rejected() {
    // The result-ABI's success payload is uniformly boxed via
    // `keel_box_*`/`unbox_value` — struct/enum/nullable return types need
    // `Value` marshaling that doesn't exist yet.
    let msg = lower_err(
        r#"
type Point { x: int, y: int }

task f() -> Point {
  raise "x"
}
"#,
    );
    assert!(
        msg.contains("can raise and returns"),
        "unexpected message: {msg}"
    );
}

#[test]
fn an_uncaught_raise_reaching_the_top_level_is_rejected() {
    // Propagating past the top level would need to change
    // `keel_user_toplevel`'s fixed `-> i32` entry-point signature — a later
    // M2/M3 concern; wrap in `try`/`catch` instead.
    let msg = lower_err(
        r#"
task a() -> int {
  raise "x"
}

x = a()
"#,
    );
    assert!(
        msg.contains("reaches the top level"),
        "unexpected message: {msg}"
    );
}

#[test]
fn empty_body_with_a_non_unit_return_type_is_rejected() {
    // Issue #159: implicit-tail-return desugaring only rewrites a *bare
    // expression* in tail position — an empty body has no tail statement
    // at all to rewrite, so this must still be rejected rather than
    // silently lowering a function whose fallthrough is unreachable in
    // name only.
    let msg = lower_err(
        r#"
task f() -> int {
}
"#,
    );
    assert!(
        msg.contains("does not return on every path"),
        "unexpected message: {msg}"
    );
}

#[test]
fn body_ending_in_a_while_loop_with_a_non_unit_return_type_is_rejected() {
    // A `while` can't be in tail position (loops aren't exhaustive) — even
    // after tail-desugaring, a body whose last statement is a `while` still
    // doesn't return a value on the loop's `false`-condition path.
    let msg = lower_err(
        r#"
task f(n: int) -> int {
  while n > 0 {
    n -= 1
  }
}
"#,
    );
    assert!(
        msg.contains("does not return on every path"),
        "unexpected message: {msg}"
    );
}
