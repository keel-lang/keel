use keel_lang::lexer::lex;
use keel_lang::parser::parse;
use keel_lang::types::checker::{self, check};
use miette::NamedSource;

fn type_errors(source: &str) -> Vec<String> {
    let named = NamedSource::new("t.keel", source.to_string());
    let tokens = lex(source, &named).expect("lex failed");
    let program = parse(tokens, source.len(), &named).expect("parse failed");
    check(&program).into_iter().map(|e| e.message).collect()
}

fn type_ok(source: &str) {
    let errs = type_errors(source);
    assert!(errs.is_empty(), "unexpected type errors: {errs:?}");
}

fn expect_error(source: &str, substring: &str) {
    let errs = type_errors(source);
    assert!(
        errs.iter().any(|e| e.contains(substring)),
        "expected error containing {substring:?}, got: {errs:?}"
    );
}

// ─── Valid programs ─────────────────────────────────────────────────────────

#[test]
fn valid_minimal_agent() {
    type_ok(
        r#"
agent Greeter {
  @role "hi"
}

run(Greeter)
"#,
    );
}

#[test]
fn valid_task_with_return_type() {
    type_ok(
        r#"
task greet(name: str) -> str {
  "hello"
}
"#,
    );
}

#[test]
fn valid_enum_and_when() {
    type_ok(
        r#"
type Urgency = low | medium | high | critical

task triage(u: Urgency) {
  when u {
    low, medium => { return }
    high, critical => { return }
  }
}
"#,
    );
}

#[test]
fn valid_self_inside_agent() {
    type_ok(
        r#"
agent Counter {
  @role "count"
  state { count: int = 0 }

  task increment() {
    self.count = self.count + 1
  }
}
"#,
    );
}

#[test]
fn valid_agent_task_calls_sibling_via_self() {
    type_ok(
        r#"
agent Bot {
  @role "x"

  task step() {
    self.other()
  }

  task other() {
    Io.notify("hi")
  }
}
"#,
    );
}

#[test]
fn error_bare_agent_task_call_is_not_in_scope() {
    expect_error(
        r#"
agent Bot {
  @role "x"

  task step() {
    other()
  }

  task other() {
    Io.notify("hi")
  }
}
"#,
        "undefined: `other`",
    );
}

#[test]
fn error_direct_agent_task_call_is_rejected() {
    expect_error(
        r#"
agent Worker {
  @role "x"

  task run() {
    Io.notify("work")
  }
}

task invoke() {
  Worker.run()
}
"#,
        "direct agent task calls",
    );
}

// ─── Errors: undefined / scope ──────────────────────────────────────────────

#[test]
fn error_undefined_variable() {
    expect_error(
        r#"
task t() {
  x = unknown_thing
}
"#,
        "undefined",
    );
}

#[test]
fn error_self_outside_agent() {
    expect_error(
        r#"
task t() {
  self.count = 1
}
"#,
        "outside an agent",
    );
}

#[test]
fn error_self_unknown_state_field() {
    expect_error(
        r#"
agent Counter {
  @role "x"
  state { count: int = 0 }

  task bad() {
    self.nope = 1
  }
}
"#,
        "no state field",
    );
}

// ─── Errors: exhaustiveness ─────────────────────────────────────────────────

#[test]
fn error_non_exhaustive_when() {
    expect_error(
        r#"
type Urgency = low | medium | high | critical

task t(u: Urgency) {
  when u {
    low => { return }
    medium => { return }
  }
}
"#,
        "non-exhaustive",
    );
}

#[test]
fn valid_when_with_wildcard() {
    type_ok(
        r#"
type Urgency = low | medium | high | critical

task t(u: Urgency) {
  when u {
    low => { return }
    _ => { return }
  }
}
"#,
    );
}

#[test]
fn error_when_on_non_enum_without_wildcard() {
    expect_error(
        r#"
task t(code: int) {
  when code {
    200 => { return }
    404 => { return }
  }
}
"#,
        "requires a `_`",
    );
}

// ─── v0.1.4: let type annotations ──────────────────────────────────────────

#[test]
fn valid_let_annotation_matching_type() {
    type_ok(
        r#"
task t() {
  x: str = "hello"
}
"#,
    );
}

#[test]
fn error_let_annotation_type_mismatch() {
    expect_error(
        r#"
task t() {
  x: int = "hello"
}
"#,
        "expected int",
    );
}

// ─── Errors: control flow ───────────────────────────────────────────────────

#[test]
fn error_if_condition_not_bool() {
    expect_error(
        r#"
task t() {
  if "hello" {
    x = 1
  }
}
"#,
        "expected bool",
    );
}

#[test]
fn error_for_over_non_list() {
    expect_error(
        r#"
task t() {
  for x in 42 {
    y = x
  }
}
"#,
        "expects a list",
    );
}

// ─── Errors: arity ──────────────────────────────────────────────────────────

#[test]
fn error_too_many_args() {
    expect_error(
        r#"
task greet(name: str) -> str {
  "hi"
}

task call_it() {
  x = greet("a", "b", "c")
}
"#,
        "argument",
    );
}

// ─── Enum inference via Ai.classify ─────────────────────────────────────────

#[test]
fn valid_classify_inferred_enum() {
    // `Ai.classify(..., as: Mood) ?? Mood.neutral` unwraps the nullable so
    // the result is Mood and `when` on it is exhaustive.
    type_ok(
        r#"
type Mood = happy | neutral | sad

task t(text: str) {
  mood = Ai.classify(text, as: Mood) ?? Mood.neutral
  when mood {
    happy => { return }
    neutral => { return }
    sad => { return }
  }
}
"#,
    );
}

// ─── Rich enum variants ─────────────────────────────────────────────────────

#[test]
fn valid_rich_enum_variant() {
    type_ok(
        r#"
type Action =
  | reply { to: str, tone: str }
  | archive

task make() -> Action {
  Action.reply { to: "x", tone: "friendly" }
}
"#,
    );
}

#[test]
fn error_rich_variant_unknown() {
    expect_error(
        r#"
type Action =
  | reply { to: str }
  | archive

task make() -> Action {
  Action.nope { to: "x" }
}
"#,
        "no variant",
    );
}

#[test]
fn error_classify_result_missing_variant() {
    expect_error(
        r#"
type Mood = happy | neutral | sad

task t(text: str) {
  mood = Ai.classify(text, as: Mood) ?? Mood.neutral
  when mood {
    happy => { return }
    sad => { return }
  }
}
"#,
        "non-exhaustive",
    );
}

// ─── v0.1.5: nullable safety ────────────────────────────────────────────────

#[test]
fn error_nullable_passed_as_non_nullable() {
    expect_error(
        r#"
task t() {
  x: str = Env.get("KEY")
}
"#,
        "use `!` to assert non-null",
    );
}

#[test]
fn valid_nullable_unwrapped_with_assert() {
    type_ok(
        r#"
task t() {
  x: str = Env.get("KEY")!
}
"#,
    );
}

#[test]
fn valid_nullable_coalesced() {
    type_ok(
        r#"
task t() {
  x: str = Env.get("KEY") ?? "default"
}
"#,
    );
}

#[test]
fn valid_non_nullable_assigned_to_nullable() {
    type_ok(
        r#"
task t() {
  x: str? = "hello"
}
"#,
    );
}

// ─── nullable safety at call sites ───────────────────────────────────────────

#[test]
fn error_nullable_arg_at_top_level_task_call() {
    expect_error(
        r#"
task process(x: str) {}
task t() {
  val: str? = Env.get("KEY")
  process(val)
}
"#,
        "use `!` to assert non-null",
    );
}

#[test]
fn valid_nullable_arg_unwrapped_at_call_site() {
    type_ok(
        r#"
task process(x: str) {}
task t() {
  val: str? = Env.get("KEY")
  process(val!)
}
"#,
    );
}

#[test]
fn valid_nullable_arg_coalesced_at_call_site() {
    type_ok(
        r#"
task process(x: str) {}
task t() {
  val: str? = Env.get("KEY")
  process(val ?? "default")
}
"#,
    );
}

#[test]
fn error_nullable_named_arg_at_task_call() {
    expect_error(
        r#"
task process(x: str) {}
task t() {
  val: str? = Env.get("KEY")
  process(x: val)
}
"#,
        "use `!` to assert non-null",
    );
}

#[test]
fn error_wrong_type_arg_at_task_call() {
    expect_error(
        r#"
task process(x: str) {}
task t() {
  process(42)
}
"#,
        "expected str, got int",
    );
}

// ─── v0.1.5: return-type matching ──────────────────────────────────────────

#[test]
fn error_return_stmt_type_mismatch() {
    expect_error(
        r#"
task t() -> str {
  return 42
}
"#,
        "return value: expected str",
    );
}

#[test]
fn valid_return_stmt_matches_declared() {
    type_ok(
        r#"
task t() -> str {
  return "hello"
}
"#,
    );
}

#[test]
fn valid_task_no_return_type() {
    type_ok(
        r#"
task t() {
  return 42
}
"#,
    );
}

// ─── v0.1.5: struct field checks ───────────────────────────────────────────

#[test]
fn error_missing_struct_field() {
    expect_error(
        r#"
type Person { name: str, age: int }

task t() {
  p: Person = { name: "Alice" }
}
"#,
        "missing field `age`",
    );
}

#[test]
fn valid_struct_all_fields_present() {
    type_ok(
        r#"
type Person { name: str, age: int }

task t() {
  p: Person = { name: "Alice", age: 30 }
}
"#,
    );
}

#[test]
fn valid_struct_extra_fields_allowed() {
    type_ok(
        r#"
type Person { name: str }

task t() {
  p: Person = { name: "Alice", extra: 42 }
}
"#,
    );
}

// ─── v0.1.5: generic list type inference ───────────────────────────────────

#[test]
fn valid_list_push_preserves_element_type() {
    type_ok(
        r#"
task t() {
  items: list[str] = ["a", "b"]
  more = items.push("c")
}
"#,
    );
}

#[test]
fn valid_list_concatenation_inferred() {
    type_ok(
        r#"
task t() {
  a = ["x", "y"]
  b = ["z"]
  all = a + b
  for item in all {
    Io.notify(item)
  }
}
"#,
    );
}

#[test]
fn valid_list_len_is_int() {
    type_ok(
        r#"
task t() {
  items = ["a", "b", "c"]
  n: int = items.len()
}
"#,
    );
}

// ─── v0.1.17: readonly state fields ────────────────────────────────────────

#[test]
fn valid_readonly_field_readable() {
    type_ok(
        r#"
agent Bot {
  state {
    turns: int = 0
    session_id: readonly str = "default"
  }
  task check() {
    Io.notify(self.session_id)
  }
}
"#,
    );
}

#[test]
fn error_readonly_field_assigned() {
    expect_error(
        r#"
agent Bot {
  state {
    session_id: readonly str = "default"
  }
  task reset() {
    self.session_id = "new"
  }
}
"#,
        "readonly",
    );
}

#[test]
fn valid_list_filter_preserves_type() {
    type_ok(
        r#"
task t() {
  items = ["a", "bb", "ccc"]
  short = items.filter(x => true)
  for s in short {
    Io.notify(s)
  }
}
"#,
    );
}

#[test]
fn valid_complex_type_expressions_resolve() {
    type_ok(
        r#"
type Pair = (str, int)
type Bag = dynamic

task t(pair: Pair, bag: Bag) {
  same_pair: Pair = pair
  same_bag: Bag = bag
}
"#,
    );
}

#[test]
fn error_struct_destructure_from_non_struct() {
    expect_error(
        r#"
task t() {
  {name} = 42
  Io.notify(name)
}
"#,
        "cannot destructure int as a struct",
    );
}

#[test]
fn error_tuple_destructure_from_non_tuple() {
    expect_error(
        r#"
task t() {
  (name, count) = {name: "a", count: 1}
  Io.notify(name)
}
"#,
        "cannot destructure struct as a tuple",
    );
}

#[test]
fn type_at_reports_destructured_and_nested_bindings() {
    let source = r#"
type Item = {name: str, score: int}

agent Bot {
  state { session_id: readonly str = "s1" }

  on scored({name: item_name, score: item_score}: Item) {
    for loop_score in [1] {
      try {
        copied_name = "literal"
      } catch caught_error: Error {
        recovered = "fallback"
      }
    }
  }
}
"#;

    let cases = [
        ("item_name", "str"),
        ("item_score", "int"),
        ("session_id", "str"),
        ("loop_score", "int"),
        ("copied_name", "str"),
        ("caught_error", "unknown"),
        ("recovered", "str"),
    ];

    for (needle, expected) in cases {
        let offset = source
            .find(needle)
            .unwrap_or_else(|| panic!("missing {needle} in source"))
            + 1;
        let actual = checker::type_at(source, offset)
            .unwrap_or_else(|| panic!("expected type for {needle}"));
        assert!(
            actual.contains(expected),
            "expected {needle} to contain {expected:?}, got {actual:?}"
        );
    }
}

#[test]
fn ident_helpers_decline_non_identifier_offsets() {
    let source = "task greet() -> str { \"hello\" }\n";
    let quote = source.find('"').expect("string literal quote");

    assert_eq!(checker::ident_at_offset(source, quote), None);
    assert_eq!(checker::ident_span_at_offset(source, quote), None);
    assert_eq!(checker::definition_of(source, quote), None);
    assert_eq!(checker::type_at("task t( {", 2), None);
}

// ─── v0.1.19 additive checker fixes ─────────────────────────────────────────

#[test]
fn valid_set_literal_typed_as_set() {
    // set[] literal — checker must not error; inferred as set[int]
    type_ok(
        r#"
task go() {
  s = set[1, 2, 3]
}
"#,
    );
}

#[test]
fn valid_null_field_access_propagates_nullable() {
    type_ok(
        r#"
type Info = { name: str, score: int }

task go(x: Info?) {
  n = x?.name
  s = x?.score
}
"#,
    );
}

#[test]
fn valid_null_coalesce_unwraps_nullable() {
    type_ok(
        r#"
task go(x: str?) {
  result: str = x ?? "default"
}
"#,
    );
}

#[test]
fn valid_lambda_block_body_return_type_inferred() {
    type_ok(
        r#"
task go() {
  items = [1, 2, 3]
  doubled = items.map(x => {
    x * 2
  })
}
"#,
    );
}

#[test]
fn valid_ai_extract_as_resolves_struct_type() {
    type_ok(
        r#"
type Contact = { name: str, email: str }

task go(text: str) {
  result = Ai.extract(text, as: Contact)
  name = result?.name
}
"#,
    );
}

#[test]
fn valid_ai_decide_as_resolves_enum_type() {
    type_ok(
        r#"
type Priority = low | medium | high

task go(text: str) {
  p = Ai.decide(text, as: Priority)
}
"#,
    );
}

#[test]
fn valid_implicit_return_expression_matches_declared() {
    type_ok(
        r#"
task double(n: int) -> int {
  n * 2
}
"#,
    );
}

#[test]
fn error_implicit_return_type_mismatch() {
    expect_error(
        r#"
task greet() -> int {
  "hello"
}
"#,
        "implicit return",
    );
}

#[test]
fn valid_implicit_return_skipped_for_return_stmt() {
    // A task ending in `return` must not trigger the implicit-return check.
    type_ok(
        r#"
task greet() -> str {
  return "hello"
}
"#,
    );
}

#[test]
fn valid_implicit_return_skipped_for_when_stmt() {
    // A task ending in `when` must not trigger the implicit-return check.
    type_ok(
        r#"
type Color = red | green | blue

task name(c: Color) -> str {
  when c {
    red => { return "red" }
    green => { return "green" }
    blue => { return "blue" }
  }
}
"#,
    );
}

#[test]
fn valid_if_expr_branches_same_type() {
    type_ok(
        r#"
task go(x: int) -> int {
  if x > 0 { x } else { 0 }
}
"#,
    );
}

#[test]
fn error_if_expr_branches_type_mismatch() {
    expect_error(
        r#"
task go(flag: bool) {
  result = if flag { 1 } else { "oops" }
}
"#,
        "branches must have the same type",
    );
}

#[test]
fn valid_if_expr_return_branch_propagates_other_type() {
    // When one branch exits via `return`, the if-expr takes the other branch's type.
    type_ok(
        r#"
task classify(n: int) -> str {
  label = if n > 0 { return "positive" } else { "other" }
  label
}
"#,
    );
}

// ─── v0.1.20: generic type declarations ────────────────────────────────────

#[test]
fn valid_generic_struct_instantiation() {
    type_ok(
        r#"
type Paginated[T] {
  items: list[T]
  page: int
  has_more: bool
}

task t(p: Paginated[str]) {
  items: list[str] = p.items
}
"#,
    );
}

#[test]
fn valid_generic_struct_nested_params() {
    // T flows through nested list inside a generic struct.
    type_ok(
        r#"
type Wrapper[T] {
  value: T
}

task t(w: Wrapper[int]) {
  v: int = w.value
}
"#,
    );
}

#[test]
fn valid_generic_alias() {
    // Generic alias that expands to a concrete list type.
    type_ok(
        r#"
type Bag[T] = list[T]

task t(items: Bag[str]) {
  n: int = items.len()
}
"#,
    );
}

#[test]
fn valid_generic_enum_variant_exhaustive() {
    // Generic enums register variant names; exhaustiveness check still works.
    type_ok(
        r#"
type Pair[A, B] =
  | both { first: A, second: B }
  | only_first { value: A }
  | only_second { value: B }

task t(p: Pair[str, int]) {
  when p {
    both => { Io.notify("both") }
    only_first => { Io.notify("first") }
    only_second => { Io.notify("second") }
  }
}
"#,
    );
}

#[test]
fn valid_generic_struct_multi_param() {
    type_ok(
        r#"
type Pair[A, B] {
  first: A
  second: B
}

task t(p: Pair[str, int]) {
  a: str = p.first
  b: int = p.second
}
"#,
    );
}

// ─── v0.1.20: function type syntax ─────────────────────────────────────────

#[test]
fn valid_func_type_alias_used_as_param() {
    type_ok(
        r#"
type Handler = (str) -> bool

task t(h: Handler) {
  ok: bool = h("hello")
}
"#,
    );
}

#[test]
fn valid_func_type_multi_param() {
    type_ok(
        r#"
type Reducer = (str, int) -> str

task t(r: Reducer) {
  result: str = r("x", 1)
}
"#,
    );
}

#[test]
fn valid_generic_func_type_alias() {
    // type Predicate[T] = (T) -> bool — from SPEC §2.6
    type_ok(
        r#"
type Predicate[T] = (T) -> bool

task t(pred: Predicate[str]) {
  ok: bool = pred("hello")
}
"#,
    );
}

// ─── v0.1.20: generic enum variant field types ──────────────────────────────

#[test]
fn valid_generic_enum_variant_fields_typed() {
    // Variant bindings resolve to substituted field types, not Unknown.
    type_ok(
        r#"
type Pair[A, B] =
  | both { first: A, second: B }
  | only_first { value: A }
  | only_second { value: B }

task t(p: Pair[str, int]) {
  when p {
    both { first, second } => {
      f: str = first
      s: int = second
    }
    only_first { value } => {
      v: str = value
    }
    only_second { value } => {
      v: int = value
    }
  }
}
"#,
    );
}

#[test]
fn valid_generic_enum_variant_nested_type() {
    // Field type itself is a generic instantiation.
    type_ok(
        r#"
type Box[T] {
  value: T
}

type Wrapped[T] =
  | some { inner: Box[T] }
  | none_val

task t(w: Wrapped[str]) {
  when w {
    some { inner } => {
      b: Box[str] = inner
    }
    none_val => { Io.notify("empty") }
  }
}
"#,
    );
}

#[test]
fn error_generic_enum_variant_field_wrong_type() {
    // Assigning a variant field binding to the wrong type must be caught.
    expect_error(
        r#"
type Pair[A, B] =
  | both { first: A, second: B }

task t(p: Pair[str, int]) {
  when p {
    both { first, second } => {
      wrong: int = first
    }
  }
}
"#,
        "expected int, got str",
    );
}

// ─── Generic tasks ───────────────────────────────────────────────────────────

#[test]
fn valid_generic_task_identity_inferred() {
    type_ok(
        r#"
task identity[T](x: T) -> T { x }

task main() {
  s: str = identity("hello")
  n: int = identity(42)
}
"#,
    );
}

#[test]
fn valid_generic_task_return_type_inferred() {
    type_ok(
        r#"
task wrap[T](x: T) -> list[T] { [x] }

task main() {
  xs: list[int] = wrap(1)
}
"#,
    );
}

#[test]
fn valid_generic_task_multi_param_inferred() {
    type_ok(
        r#"
task first[A, B](a: A, b: B) -> A { a }

task main() {
  s: str = first("hi", 99)
}
"#,
    );
}

#[test]
fn error_generic_task_return_type_mismatch() {
    expect_error(
        r#"
task identity[T](x: T) -> T { x }

task main() {
  n: int = identity("oops")
}
"#,
        "expected int, got str",
    );
}

// ─── when as expression ─────────────────────────────────────────────────────

#[test]
fn valid_when_expr_string_arms() {
    type_ok(
        r#"
task grade(score: str) -> str {
  result: str = when score {
    "A" => "excellent"
    "B" => "good"
    _   => "needs work"
  }
  result
}
"#,
    );
}

#[test]
fn valid_when_expr_enum_subject() {
    type_ok(
        r#"
type Priority = | low | medium | high

task label(p: Priority) -> str {
  when p {
    low    => "low"
    medium => "med"
    high   => "high"
  }
}
"#,
    );
}

#[test]
fn valid_when_expr_int_arms() {
    type_ok(
        r#"
task classify(n: int) -> str {
  when n {
    0 => "zero"
    1 => "one"
    _ => "many"
  }
}
"#,
    );
}

#[test]
fn error_when_expr_mismatched_arm_types() {
    expect_error(
        r#"
task t(x: str) -> str {
  result = when x {
    "a" => "ok"
    _   => 42
  }
  result
}
"#,
        "`when` expression arms must all have the same type",
    );
}

#[test]
fn valid_when_expr_as_return_value() {
    type_ok(
        r#"
type Mood = | happy | sad

task describe(m: Mood) -> str {
  when m {
    happy => "great"
    sad   => "meh"
  }
}
"#,
    );
}

#[test]
fn invalid_zip_non_list_arg_is_type_error() {
    expect_error(
        r#"
task t() {
  result = [1, 2, 3].zip("hello")
}
"#,
        "`.zip()` expects a list argument, got str",
    );
}

// ─── operator type compatibility ───────────────────────────────────────────

#[test]
fn binop_str_plus_int_is_error() {
    expect_error(
        r#"
agent A {
    @on_start {
        x = "hi" + 5
    }
}
run(A)
"#,
        "cannot apply `+`",
    );
}

#[test]
fn binop_str_minus_int_is_error() {
    expect_error(
        r#"
agent A {
    @on_start {
        x = "hi" - 1
    }
}
run(A)
"#,
        "cannot apply `-`",
    );
}

#[test]
fn binop_str_lt_int_is_error() {
    expect_error(
        r#"
agent A {
    @on_start {
        x = "hi" < 5
    }
}
run(A)
"#,
        "cannot apply `<`",
    );
}

#[test]
fn binop_bool_plus_int_is_error() {
    expect_error(
        r#"
agent A {
    @on_start {
        x = true + 1
    }
}
run(A)
"#,
        "cannot apply `+`",
    );
}

#[test]
fn binop_list_minus_int_is_error() {
    expect_error(
        r#"
agent A {
    @on_start {
        x = [1, 2] - 1
    }
}
run(A)
"#,
        "cannot apply `-`",
    );
}

#[test]
fn aug_assign_type_mismatch_is_error() {
    expect_error(
        r#"
agent A {
    @on_start {
        x = 0
        x += "oops"
    }
}
run(A)
"#,
        "cannot apply `+`",
    );
}

#[test]
fn binop_valid_numeric_combos() {
    type_ok(
        r#"
agent A {
    @on_start {
        a = 1 + 1
        b = 1.0 + 2
        c = 1 + 2.0
        d = 3.0 - 1.0
    }
}
run(A)
"#,
    );
}

#[test]
fn binop_valid_str_concat() {
    type_ok(
        r#"
agent A {
    @on_start {
        x = "a" + "b"
    }
}
run(A)
"#,
    );
}

#[test]
fn binop_valid_list_concat() {
    type_ok(
        r#"
agent A {
    @on_start {
        x = [1] + [2]
    }
}
run(A)
"#,
    );
}

#[test]
fn binop_valid_comparisons() {
    type_ok(
        r#"
agent A {
    @on_start {
        a = 1 < 2
        b = "a" < "b"
        c = 1.0 >= 0
    }
}
run(A)
"#,
    );
}

#[test]
fn binop_equality_is_always_valid() {
    type_ok(
        r#"
agent A {
    @on_start {
        x = 1 == "hello"
    }
}
run(A)
"#,
    );
}

#[test]
fn binop_unknown_operand_skips_check() {
    // list.reduce() returns Unknown — should not trigger a type error when used as operand
    type_ok(
        r#"
agent A {
    @on_start {
        v = [1, 2, 3].reduce()
        x = v + 1
    }
}
run(A)
"#,
    );
}
