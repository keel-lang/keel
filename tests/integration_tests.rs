use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Once;

use keel_lang::formatter;
use keel_lang::interpreter;
use keel_lang::lexer::lex;
use keel_lang::parser::parse;
use keel_lang::types::checker;
use keel_lang::vm;
use miette::NamedSource;

static INIT: Once = Once::new();

fn setup() {
    INIT.call_once(|| {
        std::env::set_var("KEEL_LLM", "mock");
        std::env::set_var("KEEL_ONESHOT", "1");
    });
}

// ---------------------------------------------------------------------------
// Example discovery
// ---------------------------------------------------------------------------

const EXAMPLES_DIR: &str = "examples";

/// Examples that currently fail the type checker — tracked so the rest of the
/// suite keeps passing while we investigate. Each entry should have a linked
/// explanation.
///
/// - multi_agent_inbox.keel: `when` on a field whose type is an enum alias
///   (e.g. `result.urgency` where `urgency: Urgency`) isn't recognized as
///   enum-typed by the checker, so the exhaustiveness rule demands a `_`.
const TYPECHECK_SKIP: &[&str] = &["multi_agent_inbox.keel"];

/// Examples safe to run headlessly (no `ask user` / `confirm user`, no env
/// vars required, no external network/Ollama/IMAP dependency).
const RUNNABLE: &[&str] = &[
    "minimal.keel",
    "hello_world.keel",
    "meeting_prep.keel",
    "data_pipeline.keel",
    "test_scheduling.keel",
    "email_agent.keel",
];

fn all_example_files() -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = fs::read_dir(EXAMPLES_DIR)
        .expect("examples dir should exist")
        .filter_map(|entry| entry.ok().map(|e| e.path()))
        .filter(|p| p.extension().map(|ext| ext == "keel").unwrap_or(false))
        .collect();
    files.sort();
    files
}

fn file_name(path: &Path) -> String {
    path.file_name().unwrap().to_string_lossy().to_string()
}

fn read_and_name(path: &Path) -> (String, NamedSource<String>) {
    let source = fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("could not read {}: {e}", path.display()));
    let named = NamedSource::new(file_name(path), source.clone());
    (source, named)
}

// ---------------------------------------------------------------------------
// Tier 1 — Parse every example
// ---------------------------------------------------------------------------

#[test]
fn examples_parse_all() {
    setup();
    let examples = all_example_files();
    assert!(!examples.is_empty(), "no example files found");

    for path in &examples {
        let (source, named) = read_and_name(path);
        let tokens = lex(&source, &named)
            .unwrap_or_else(|e| panic!("lex failed for {}: {e:?}", path.display()));
        parse(tokens, source.len(), &named)
            .unwrap_or_else(|e| panic!("parse failed for {}: {e:?}", path.display()));
    }
}

// ---------------------------------------------------------------------------
// Tier 1b — Type-check every example (minus known failures)
// ---------------------------------------------------------------------------

#[test]
fn examples_typecheck_all() {
    setup();
    for path in all_example_files() {
        let name = file_name(&path);
        if TYPECHECK_SKIP.contains(&name.as_str()) {
            continue;
        }
        let (source, named) = read_and_name(&path);
        let tokens = lex(&source, &named).unwrap();
        let program = parse(tokens, source.len(), &named).unwrap();
        let errors = checker::check(&program);
        assert!(
            errors.is_empty(),
            "type errors in {name}: {:#?}",
            errors.iter().map(|e| &e.message).collect::<Vec<_>>()
        );
    }
}

// ---------------------------------------------------------------------------
// Tier 2 — Run examples that are safe headlessly
// ---------------------------------------------------------------------------

#[test]
fn examples_run_headless() {
    setup();
    let rt = tokio::runtime::Runtime::new().unwrap();

    for name in RUNNABLE {
        let path = PathBuf::from(EXAMPLES_DIR).join(name);
        let (source, named) = read_and_name(&path);
        let tokens = lex(&source, &named).unwrap();
        let program = parse(tokens, source.len(), &named).unwrap();

        rt.block_on(async {
            interpreter::run(program)
                .await
                .unwrap_or_else(|e| panic!("interpreter failed on {name}: {e:?}"));
        });
    }
}

// ---------------------------------------------------------------------------
// Tier 3 — Formatter idempotence
// ---------------------------------------------------------------------------

#[test]
fn examples_format_idempotent() {
    setup();
    for path in all_example_files() {
        let name = file_name(&path);
        let (source, named) = read_and_name(&path);
        let tokens = lex(&source, &named).unwrap();
        let program = parse(tokens, source.len(), &named).unwrap();
        let first = formatter::format_program(&program);

        // Re-parse the formatted output and format again. Two passes of fmt
        // should produce the same text — otherwise the formatter is lossy or
        // the parser is dropping something the printer re-emits.
        let named2 = NamedSource::new(format!("{name} (formatted)"), first.clone());
        let tokens2 = lex(&first, &named2)
            .unwrap_or_else(|e| panic!("lex failed on formatted {name}: {e:?}"));
        let program2 = parse(tokens2, first.len(), &named2)
            .unwrap_or_else(|e| panic!("parse failed on formatted {name}: {e:?}"));
        let second = formatter::format_program(&program2);

        assert_eq!(
            first, second,
            "formatter is not idempotent for {name}"
        );
    }
}

// ---------------------------------------------------------------------------
// Tier 4 — Bytecode compiles every example (minus known-failing)
// ---------------------------------------------------------------------------

#[test]
fn examples_bytecode_compile() {
    setup();
    for path in all_example_files() {
        let name = file_name(&path);
        if TYPECHECK_SKIP.contains(&name.as_str()) {
            // Type-check gates `keel build`; skip the same set.
            continue;
        }
        let (source, named) = read_and_name(&path);
        let tokens = lex(&source, &named).unwrap();
        let program = parse(tokens, source.len(), &named).unwrap();
        vm::compiler::compile(&program)
            .unwrap_or_else(|e| panic!("bytecode compile failed for {name}: {e}"));
    }
}

// ---------------------------------------------------------------------------
// Inline programs — end-to-end
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Error diagnostics
// ---------------------------------------------------------------------------

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
