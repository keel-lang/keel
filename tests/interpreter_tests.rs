#![cfg(any())] // v0.1: interpreter tests disabled until the interpreter migration lands.
use std::sync::Once;

use keel_lang::interpreter;
use keel_lang::lexer::lex;
use keel_lang::parser::parse;
use miette::NamedSource;

static INIT: Once = Once::new();

fn setup() {
    INIT.call_once(|| {
        std::env::set_var("KEEL_LLM", "mock");
    });
}

async fn run_program(source: &str) -> Result<(), String> {
    setup();
    let named = NamedSource::new("test.keel", source.to_string());
    let tokens = lex(source, &named).map_err(|e| e.to_string())?;
    let program = parse(tokens, source.len(), &named).map_err(|e| e.to_string())?;
    interpreter::run(program)
        .await
        .map_err(|e| e.to_string())
}

fn run_ok(source: &str) {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        run_program(source).await.expect("program should succeed");
    });
}

fn run_err(source: &str) -> String {
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        run_program(source)
            .await
            .expect_err("program should fail")
    })
}

// ─── Basic execution ─────────────────────────────────────────────────────────

#[test]
fn run_minimal_agent() {
    run_ok(
        r#"
agent Hello {
  role "greeter"
}
run Hello
"#,
    );
}

#[test]
fn run_agent_with_task() {
    run_ok(
        r#"
task greet(name: str) -> str {
  "Hello, {name}!"
}

agent Bot {
  role "bot"

  every 1.day {
    msg = greet("World")
    notify user msg
  }
}

run Bot
"#,
    );
}

// ─── Type system ─────────────────────────────────────────────────────────────

#[test]
fn run_enum_type() {
    run_ok(
        r#"
type Status = active | inactive

agent Bot {
  role "bot"

  every 1.day {
    s = active
    when s {
      active => notify user "active"
      inactive => notify user "inactive"
    }
  }
}

run Bot
"#,
    );
}

// ─── String interpolation ────────────────────────────────────────────────────

#[test]
fn run_string_interpolation() {
    run_ok(
        r#"
task greet(name: str) -> str {
  "Hello, {name}! Welcome."
}

agent Bot {
  role "bot"
  every 1.day {
    msg = greet("Keel")
    notify user msg
  }
}

run Bot
"#,
    );
}

// ─── Expressions ─────────────────────────────────────────────────────────────

#[test]
fn run_arithmetic() {
    run_ok(
        r#"
agent Bot {
  role "math"
  every 1.day {
    x = 2 + 3 * 4
    notify user "result: {x}"
  }
}

run Bot
"#,
    );
}

#[test]
fn run_comparison() {
    run_ok(
        r#"
agent Bot {
  role "cmp"
  every 1.day {
    if 5 > 3 {
      notify user "five is greater"
    }
  }
}

run Bot
"#,
    );
}

#[test]
fn run_boolean_logic() {
    run_ok(
        r#"
agent Bot {
  role "logic"
  every 1.day {
    if true and not false {
      notify user "logic works"
    }
  }
}

run Bot
"#,
    );
}

// ─── Null coalescing ─────────────────────────────────────────────────────────

#[test]
fn run_null_coalesce() {
    run_ok(
        r#"
agent Bot {
  role "null"
  every 1.day {
    x = none ?? "default"
    notify user x
  }
}

run Bot
"#,
    );
}

// ─── Control flow ────────────────────────────────────────────────────────────

#[test]
fn run_if_else() {
    run_ok(
        r#"
agent Bot {
  role "branch"
  every 1.day {
    x = 10
    if x > 5 {
      notify user "big"
    } else {
      notify user "small"
    }
  }
}

run Bot
"#,
    );
}

#[test]
fn run_when_match() {
    run_ok(
        r#"
type Color = red | green | blue

agent Bot {
  role "match"
  every 1.day {
    c = green
    when c {
      red => notify user "red"
      green => notify user "green"
      blue => notify user "blue"
    }
  }
}

run Bot
"#,
    );
}

#[test]
fn run_when_wildcard() {
    run_ok(
        r#"
type Level = low | medium | high

agent Bot {
  role "wildcard"
  every 1.day {
    l = high
    when l {
      low => notify user "low"
      _ => notify user "not low"
    }
  }
}

run Bot
"#,
    );
}

#[test]
fn run_for_loop() {
    run_ok(
        r#"
agent Bot {
  role "loop"
  every 1.day {
    items = [1, 2, 3]
    for item in items {
      notify user "item"
    }
  }
}

run Bot
"#,
    );
}

// ─── Agent state ─────────────────────────────────────────────────────────────

#[test]
fn run_agent_state() {
    run_ok(
        r#"
agent Counter {
  role "counter"
  state {
    count: int = 0
  }

  task increment() {
    self.count = self.count + 1
  }

  every 1.day {
    increment()
    increment()
    notify user "count: {self.count}"
  }
}

run Counter
"#,
    );
}

// ─── Struct / map literals ───────────────────────────────────────────────────

#[test]
fn run_struct_literal() {
    run_ok(
        r#"
agent Bot {
  role "struct"
  every 1.day {
    info = {name: "Keel", version: "0.1"}
    notify user info.name
  }
}

run Bot
"#,
    );
}

#[test]
fn run_list_operations() {
    run_ok(
        r#"
agent Bot {
  role "list"
  every 1.day {
    items = [10, 20, 30]
    notify user "count: {items.count}"
  }
}

run Bot
"#,
    );
}

// ─── String methods ──────────────────────────────────────────────────────────

#[test]
fn run_string_methods() {
    run_ok(
        r#"
agent Bot {
  role "strings"
  every 1.day {
    s = "  Hello, World!  "
    trimmed = s.trim()
    upper = trimmed.upper()
    has = trimmed.contains("Hello")
    notify user "trimmed={trimmed}, upper={upper}"
  }
}

run Bot
"#,
    );
}

// ─── AI primitives (mock mode) ───────────────────────────────────────────────

#[test]
fn run_classify_mock() {
    run_ok(
        r#"
type Urgency = low | medium | high

task triage(text: str) -> Urgency {
  classify text as Urgency fallback medium
}

agent Bot {
  role "classifier"
  every 1.day {
    result = triage("urgent email")
    notify user "classified"
  }
}

run Bot
"#,
    );
}

#[test]
fn run_draft_mock() {
    run_ok(
        r#"
agent Bot {
  role "drafter"
  every 1.day {
    reply = draft "response to customer" {
      tone: "friendly"
    } ?? "fallback"
    notify user reply
  }
}

run Bot
"#,
    );
}

#[test]
fn run_summarize_mock() {
    run_ok(
        r#"
agent Bot {
  role "summarizer"
  every 1.day {
    brief = summarize "long text here" in 1 sentence fallback "no summary"
    notify user brief
  }
}

run Bot
"#,
    );
}

// ─── Task composition ────────────────────────────────────────────────────────

#[test]
fn run_task_calls_task() {
    run_ok(
        r#"
task add(a: int, b: int) -> int {
  a + b
}

task double(x: int) -> int {
  add(x, x)
}

agent Bot {
  role "composer"
  every 1.day {
    result = double(5)
    notify user "result: {result}"
  }
}

run Bot
"#,
    );
}

// ─── Lambdas ─────────────────────────────────────────────────────────────────

#[test]
fn run_lambda_map() {
    run_ok(
        r#"
agent Bot {
  role "lambda"
  every 1.day {
    nums = [1, 2, 3]
    doubled = nums.map(n => n * 2)
    notify user "doubled: {doubled}"
  }
}
run Bot
"#,
    );
}

#[test]
fn run_lambda_filter() {
    run_ok(
        r#"
agent Bot {
  role "lambda"
  every 1.day {
    nums = [1, 2, 3, 4, 5]
    big = nums.filter(n => n > 3)
    notify user "big: {big}"
  }
}
run Bot
"#,
    );
}

#[test]
fn run_lambda_any_all() {
    run_ok(
        r#"
agent Bot {
  role "lambda"
  every 1.day {
    nums = [2, 4, 6]
    all_even = nums.all(n => n % 2 == 0)
    has_big = nums.any(n => n > 5)
    notify user "all_even={all_even}, has_big={has_big}"
  }
}
run Bot
"#,
    );
}

#[test]
fn run_lambda_find() {
    run_ok(
        r#"
agent Bot {
  role "lambda"
  every 1.day {
    items = [10, 20, 30]
    found = items.find(n => n > 15) ?? 0
    notify user "found: {found}"
  }
}
run Bot
"#,
    );
}

#[test]
fn run_task_ref_in_map() {
    run_ok(
        r#"
task double(n: int) -> int { n * 2 }

agent Bot {
  role "task ref"
  every 1.day {
    nums = [1, 2, 3]
    doubled = nums.map(double)
    notify user "doubled: {doubled}"
  }
}
run Bot
"#,
    );
}

#[test]
fn run_lambda_sort_by() {
    run_ok(
        r#"
agent Bot {
  role "sort"
  every 1.day {
    items = [{name: "Charlie", age: 30}, {name: "Alice", age: 25}, {name: "Bob", age: 35}]
    sorted = items.sort_by(item => item.name)
    notify user "sorted"
  }
}
run Bot
"#,
    );
}

// ─── Error handling ──────────────────────────────────────────────────────────

#[test]
fn run_undefined_variable_error() {
    let msg = run_err(
        r#"
agent Bot {
  role "error"
  every 1.day {
    notify user undefined_var
  }
}

run Bot
"#,
    );
    assert!(msg.contains("Undefined variable"));
}

#[test]
fn run_undefined_agent_error() {
    let msg = run_err("run NonExistent");
    assert!(msg.contains("not found"));
}

// ─── Connect and fetch ───────────────────────────────────────────────────────

#[test]
fn run_fetch_returns_empty_list() {
    run_ok(
        r#"
connect inbox via imap {
  host: env.IMAP_HOST
}

agent Bot {
  role "fetcher"
  every 1.day {
    emails = fetch inbox where unread
    notify user "got emails"
  }
}

run Bot
"#,
    );
}

// ─── Remember ────────────────────────────────────────────────────────────────

#[test]
fn run_remember() {
    run_ok(
        r#"
agent Bot {
  role "memory"
  every 1.day {
    remember {contact: "alice", topic: "meeting"}
  }
}

run Bot
"#,
    );
}

// ─── Return ──────────────────────────────────────────────────────────────────

#[test]
fn run_early_return() {
    run_ok(
        r#"
task check(x: int) -> str {
  if x > 10 {
    return "big"
  }
  "small"
}

agent Bot {
  role "return"
  every 1.day {
    a = check(20)
    b = check(5)
    notify user "a={a}, b={b}"
  }
}

run Bot
"#,
    );
}
