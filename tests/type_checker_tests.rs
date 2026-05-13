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
