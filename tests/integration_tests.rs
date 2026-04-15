use std::fs;
use std::path::Path;
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

fn check_file(path: &str) {
    setup();
    let source = fs::read_to_string(path).expect("could not read file");
    let filename = Path::new(path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let named = NamedSource::new(&filename, source.clone());

    let tokens = lex(&source, &named).expect(&format!("lexer failed on {path}"));
    let _program = parse(tokens, source.len(), &named).expect(&format!("parser failed on {path}"));
}

fn run_file(path: &str) {
    setup();
    let source = fs::read_to_string(path).expect("could not read file");
    let filename = Path::new(path)
        .file_name()
        .unwrap()
        .to_string_lossy()
        .to_string();
    let named = NamedSource::new(&filename, source.clone());

    let tokens = lex(&source, &named).expect("lexer failed");
    let program = parse(tokens, source.len(), &named).expect("parser failed");

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        interpreter::run(program)
            .await
            .expect(&format!("interpreter failed on {path}"));
    });
}

// ─── Parse-check all example files ───────────────────────────────────────────

#[test]
fn check_minimal_keel() {
    check_file("examples/minimal.keel");
}

#[test]
fn check_hello_world_keel() {
    check_file("examples/hello_world.keel");
}

#[test]
fn check_email_agent_keel() {
    check_file("examples/email_agent.keel");
}

#[test]
fn check_multi_agent_inbox_keel() {
    check_file("examples/multi_agent_inbox.keel");
}

// ─── Run examples that don't require user input ──────────────────────────────

#[test]
fn run_minimal_keel() {
    run_file("examples/minimal.keel");
}

#[test]
fn run_email_agent_keel() {
    // The email agent fetches (mock returns empty list) and runs without
    // requiring any human input since there are 0 emails to handle.
    run_file("examples/email_agent.keel");
}

// ─── Inline programs — end-to-end ────────────────────────────────────────────

#[test]
fn e2e_type_task_agent_run() {
    setup();
    let source = r#"
type Priority = low | medium | high

task label(p: Priority) -> str {
  when p {
    low => "Low priority"
    medium => "Medium priority"
    high => "High priority"
  }
}

agent Labeler {
  role "Labels things by priority"
  model "claude-haiku"

  state {
    processed: int = 0
  }

  every 1.day {
    msg = label(medium)
    notify user msg
    self.processed = self.processed + 1
  }
}

run Labeler
"#;
    let named = NamedSource::new("e2e.keel", source.to_string());
    let tokens = lex(source, &named).unwrap();
    let program = parse(tokens, source.len(), &named).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        interpreter::run(program).await.unwrap();
    });
}

#[test]
fn e2e_nested_task_calls() {
    setup();
    let source = r#"
task add(a: int, b: int) -> int { a + b }
task mul(a: int, b: int) -> int { a * b }

task calc(x: int) -> int {
  y = add(x, 10)
  mul(y, 2)
}

agent Math {
  role "calculator"
  every 1.day {
    result = calc(5)
    notify user "calc(5) = {result}"
  }
}

run Math
"#;
    let named = NamedSource::new("e2e.keel", source.to_string());
    let tokens = lex(source, &named).unwrap();
    let program = parse(tokens, source.len(), &named).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        interpreter::run(program).await.unwrap();
    });
}

#[test]
fn e2e_struct_field_access() {
    setup();
    let source = r#"
agent Bot {
  role "struct test"
  every 1.day {
    person = {name: "Alice", age: 30}
    notify user "Name: {person.name}, Age: {person.age}"
  }
}

run Bot
"#;
    let named = NamedSource::new("e2e.keel", source.to_string());
    let tokens = lex(source, &named).unwrap();
    let program = parse(tokens, source.len(), &named).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        interpreter::run(program).await.unwrap();
    });
}

#[test]
fn e2e_for_loop_with_state() {
    setup();
    let source = r#"
agent Summer {
  role "sums a list"
  state {
    total: int = 0
  }

  every 1.day {
    items = [10, 20, 30]
    for item in items {
      self.total = self.total + item
    }
    notify user "total = {self.total}"
  }
}

run Summer
"#;
    let named = NamedSource::new("e2e.keel", source.to_string());
    let tokens = lex(source, &named).unwrap();
    let program = parse(tokens, source.len(), &named).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        interpreter::run(program).await.unwrap();
    });
}

#[test]
fn e2e_classify_with_when() {
    setup();
    let source = r#"
type Mood = happy | sad | neutral

task analyze(text: str) -> Mood {
  classify text as Mood fallback neutral
}

agent MoodBot {
  role "mood analyzer"
  every 1.day {
    mood = analyze("I love Keel!")
    when mood {
      happy => notify user "Great mood!"
      sad => notify user "Cheer up!"
      neutral => notify user "Meh."
    }
  }
}

run MoodBot
"#;
    let named = NamedSource::new("e2e.keel", source.to_string());
    let tokens = lex(source, &named).unwrap();
    let program = parse(tokens, source.len(), &named).unwrap();

    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(async {
        interpreter::run(program).await.unwrap();
    });
}

// ─── Error diagnostics ──────────────────────────────────────────────────────

#[test]
fn e2e_lex_error_reports_location() {
    let source = "agent A { $ }";
    let named = NamedSource::new("bad.keel", source.to_string());
    let result = lex(source, &named);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Unexpected character"));
}

#[test]
fn e2e_parse_error_reports_location() {
    let source = "task t( { }";
    let named = NamedSource::new("bad.keel", source.to_string());
    let tokens = lex(source, &named).unwrap();
    let result = parse(tokens, source.len(), &named);
    assert!(result.is_err());
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("Parse error"));
}
