use keel_lang::formatter::format_program;
use keel_lang::lexer::lex;
use keel_lang::parser::parse;
use miette::NamedSource;
use std::path::PathBuf;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn format_source(src: &str) -> String {
    let named = NamedSource::new("t.keel", src.to_string());
    let tokens = lex(src, &named).expect("lex");
    let program = parse(tokens, src.len(), &named).expect("parse");
    format_program(&program)
}

/// Format twice and confirm idempotence: fmt(src) == fmt(fmt(src)).
fn assert_idempotent(src: &str) {
    let once = format_source(src);
    let twice = format_source(&once);
    assert_eq!(once, twice, "formatter not idempotent.\n--- once ---\n{once}\n--- twice ---\n{twice}");
}

#[test]
fn format_minimal_program() {
    let src = r#"agent G {
  @role "hi"
}
run(G)
"#;
    let out = format_source(src);
    assert!(out.contains("agent G {"), "output:\n{out}");
    assert!(out.contains("@role"));
    assert!(out.contains("run(G)"));
}

#[test]
fn idempotent_on_every_example() {
    let examples_dir = project_root().join("examples");
    let mut count = 0;
    for entry in std::fs::read_dir(&examples_dir).expect("read examples dir") {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.extension().map(|e| e != "keel").unwrap_or(true) { continue; }
        let src = std::fs::read_to_string(&path).expect("read keel file");
        let once = format_source(&src);
        let twice = format_source(&once);
        assert_eq!(
            once, twice,
            "formatter not idempotent on {}\n--- once ---\n{once}\n--- twice ---\n{twice}",
            path.display()
        );
        count += 1;
    }
    assert!(count > 5, "expected many example files, found {count}");
}

#[test]
fn idempotent_rich_enum_construction() {
    assert_idempotent(r#"
type Action =
  | reply { to: str, tone: str }
  | archive

task t() {
  a = Action.reply { to: "x", tone: "y" }
}
"#);
}

#[test]
fn idempotent_when_arms() {
    assert_idempotent(r#"
type U = low | medium | high

task t(u: U) -> str {
  when u {
    low => "lo"
    medium, high => "hi"
  }
}
"#);
}

#[test]
fn idempotent_nested_blocks() {
    assert_idempotent(r#"
agent Bot {
  @role "..."

  @on_start {
    if true {
      Io.notify("a")
    } else {
      Io.notify("b")
    }
    for x in [1, 2, 3] {
      Io.notify(x.to_str())
    }
  }
}

run(Bot)
"#);
}
