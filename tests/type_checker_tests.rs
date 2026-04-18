use keel_lang::lexer::lex;
use keel_lang::parser::parse;
use keel_lang::types::checker::check;
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
    type_ok(r#"
agent Greeter {
  @role "hi"
}

run(Greeter)
"#);
}

#[test]
fn valid_task_with_return_type() {
    type_ok(r#"
task greet(name: str) -> str {
  "hello"
}
"#);
}

#[test]
fn valid_enum_and_when() {
    type_ok(r#"
type Urgency = low | medium | high | critical

task triage(u: Urgency) {
  when u {
    low, medium => { return }
    high, critical => { return }
  }
}
"#);
}

#[test]
fn valid_self_inside_agent() {
    type_ok(r#"
agent Counter {
  @role "count"
  state { count: int = 0 }

  task increment() {
    self.count = self.count + 1
  }
}
"#);
}

#[test]
fn valid_agent_task_calls_sibling() {
    type_ok(r#"
agent Bot {
  @role "x"

  task step() {
    other()
  }

  task other() {
    Io.notify("hi")
  }
}
"#);
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
    type_ok(r#"
type Urgency = low | medium | high | critical

task t(u: Urgency) {
  when u {
    low => { return }
    _ => { return }
  }
}
"#);
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
    // `Ai.classify(..., as: Mood, fallback: Mood.neutral)` should bind
    // the result as Mood so `when` on it is exhaustive.
    type_ok(r#"
type Mood = happy | neutral | sad

task t(text: str) {
  mood = Ai.classify(text, as: Mood, fallback: Mood.neutral)
  when mood {
    happy => { return }
    neutral => { return }
    sad => { return }
  }
}
"#);
}

#[test]
fn error_classify_result_missing_variant() {
    expect_error(
        r#"
type Mood = happy | neutral | sad

task t(text: str) {
  mood = Ai.classify(text, as: Mood, fallback: Mood.neutral)
  when mood {
    happy => { return }
    sad => { return }
  }
}
"#,
        "non-exhaustive",
    );
}
