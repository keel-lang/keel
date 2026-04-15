use keel_lang::lexer::lex;
use keel_lang::parser::parse;
use keel_lang::types::checker::check;
use miette::NamedSource;

fn type_errors(source: &str) -> Vec<String> {
    let named = NamedSource::new("test.keel", source.to_string());
    let tokens = lex(source, &named).expect("lexer failed");
    let program = parse(tokens, source.len(), &named).expect("parser failed");
    check(&program)
        .into_iter()
        .map(|e| e.message)
        .collect()
}

fn type_ok(source: &str) {
    let errors = type_errors(source);
    assert!(
        errors.is_empty(),
        "Expected no type errors, got: {:?}",
        errors
    );
}

fn expect_error(source: &str, substring: &str) {
    let errors = type_errors(source);
    assert!(
        errors.iter().any(|e| e.contains(substring)),
        "Expected error containing '{}', got: {:?}",
        substring,
        errors
    );
}

// ─── Valid programs ──────────────────────────────────────────────────────────

#[test]
fn valid_minimal() {
    type_ok(
        r#"
agent A {
  role "test"
}
run A
"#,
    );
}

#[test]
fn valid_task_with_return_type() {
    type_ok(
        r#"
task greet(name: str) -> str {
  "Hello!"
}
agent A { role "a" }
run A
"#,
    );
}

#[test]
fn valid_enum_and_when() {
    type_ok(
        r#"
type Color = red | green | blue

agent A {
  role "a"
  every 1.day {
    c = red
    when c {
      red => notify user "red"
      green => notify user "green"
      blue => notify user "blue"
    }
  }
}
run A
"#,
    );
}

#[test]
fn valid_classify_with_fallback() {
    type_ok(
        r#"
type Mood = happy | sad

task analyze(text: str) -> Mood {
  classify text as Mood fallback happy
}

agent A { role "a" }
run A
"#,
    );
}

#[test]
fn valid_state_mutation() {
    type_ok(
        r#"
agent Counter {
  role "counter"
  state {
    count: int = 0
  }
  task inc() {
    self.count = self.count + 1
  }
}
run Counter
"#,
    );
}

#[test]
fn valid_for_loop() {
    type_ok(
        r#"
agent A {
  role "a"
  every 1.day {
    items = [1, 2, 3]
    for item in items {
      notify user "item"
    }
  }
}
run A
"#,
    );
}

#[test]
fn valid_null_coalesce() {
    type_ok(
        r#"
agent A {
  role "a"
  every 1.day {
    x = none ?? "default"
    notify user x
  }
}
run A
"#,
    );
}

// ─── Type errors: non-exhaustive when ────────────────────────────────────────

#[test]
fn error_non_exhaustive_when() {
    expect_error(
        r#"
type Status = active | paused | stopped

agent A {
  role "a"
  every 1.day {
    s = active
    when s {
      active => notify user "on"
      paused => notify user "paused"
    }
  }
}
run A
"#,
        "Non-exhaustive",
    );
}

#[test]
fn valid_when_with_wildcard() {
    type_ok(
        r#"
type Status = active | paused | stopped

agent A {
  role "a"
  every 1.day {
    s = active
    when s {
      active => notify user "on"
      _ => notify user "other"
    }
  }
}
run A
"#,
    );
}

// ─── Type errors: if condition ───────────────────────────────────────────────

#[test]
fn error_if_condition_not_bool() {
    expect_error(
        r#"
agent A {
  role "a"
  every 1.day {
    if "hello" {
      notify user "bad"
    }
  }
}
run A
"#,
        "if condition must be bool",
    );
}

// ─── Type errors: for loop ───────────────────────────────────────────────────

#[test]
fn error_for_over_non_list() {
    expect_error(
        r#"
agent A {
  role "a"
  every 1.day {
    for x in 42 {
      notify user "bad"
    }
  }
}
run A
"#,
        "for loop requires a list",
    );
}

// ─── Type errors: undefined variable ─────────────────────────────────────────

#[test]
fn error_undefined_variable() {
    expect_error(
        r#"
agent A {
  role "a"
  every 1.day {
    notify user unknown_var
  }
}
run A
"#,
        "Undefined variable",
    );
}

// ─── Type errors: self outside agent ─────────────────────────────────────────

#[test]
fn error_self_outside_agent() {
    expect_error(
        r#"
task bad() {
  self.count = 1
}
agent A { role "a" }
run A
"#,
        "self can only be used inside an agent",
    );
}

// ─── Type errors: wrong argument type ────────────────────────────────────────

#[test]
fn error_wrong_argument_type() {
    expect_error(
        r#"
task greet(name: str) -> str {
  "Hello!"
}

agent A {
  role "a"
  every 1.day {
    greet(42)
  }
}
run A
"#,
        "expected str, got int",
    );
}

// ─── Type errors: too many arguments ─────────────────────────────────────────

#[test]
fn error_too_many_args() {
    expect_error(
        r#"
task greet(name: str) -> str {
  "Hello!"
}

agent A {
  role "a"
  every 1.day {
    greet("a", "b", "c")
  }
}
run A
"#,
        "expects 1 arguments, got 3",
    );
}

// ─── Type errors: arithmetic ─────────────────────────────────────────────────

#[test]
fn error_add_incompatible_types() {
    expect_error(
        r#"
agent A {
  role "a"
  every 1.day {
    x = "hello" + 42
  }
}
run A
"#,
        "Cannot add",
    );
}

// ─── Type errors: negation ───────────────────────────────────────────────────

#[test]
fn error_negate_string() {
    expect_error(
        r#"
agent A {
  role "a"
  every 1.day {
    x = -"hello"
  }
}
run A
"#,
        "Cannot negate",
    );
}

// ─── Type errors: list element mismatch ──────────────────────────────────────

#[test]
fn error_mixed_list_types() {
    expect_error(
        r#"
agent A {
  role "a"
  every 1.day {
    items = [1, "two", 3]
  }
}
run A
"#,
        "List element type mismatch",
    );
}

// ─── Type errors: state field ────────────────────────────────────────────────

#[test]
fn error_assign_wrong_type_to_state() {
    expect_error(
        r#"
agent A {
  role "a"
  state {
    count: int = 0
  }
  task bad() {
    self.count = "not a number"
  }
}
run A
"#,
        "Cannot assign str to state field 'count' of type int",
    );
}

#[test]
fn error_unknown_state_field() {
    expect_error(
        r#"
agent A {
  role "a"
  state {
    count: int = 0
  }
  task bad() {
    self.unknown = 1
  }
}
run A
"#,
        "Unknown state field",
    );
}

// ─── Classify returns nullable without fallback ──────────────────────────────

#[test]
fn valid_classify_nullable() {
    type_ok(
        r#"
type Mood = happy | sad

task maybe(text: str) -> Mood {
  classify text as Mood fallback happy
}

agent A { role "a" }
run A
"#,
    );
}

// ─── After requires duration ─────────────────────────────────────────────────

#[test]
fn error_after_non_duration() {
    expect_error(
        r#"
agent A {
  role "a"
  every 1.day {
    after "not a duration" {
      notify user "bad"
    }
  }
}
run A
"#,
        "after requires a duration",
    );
}
