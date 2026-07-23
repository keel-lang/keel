//! Exit criterion for issue #146: a program declaring a simple enum and
//! matching it exhaustively via `when` (each arm returning directly, side-
//! effect style — `when` as an *expression*, e.g. `x = when ... {...}`,
//! isn't lowered yet), plus a `when` over a `str` scrutinee with a wildcard
//! arm, compiles, links, and matches the interpreter byte-for-byte
//! (`examples/when_expression.keel`'s shape, minus its implicit-tail-return
//! style — see #159).

use std::process::Command;

use keel_codegen::BuildOptions;

#[path = "support/mod.rs"]
mod support;

fn compile_and_run(source: &str) -> std::process::Output {
    let kir = support::parse_check_and_lower(source);

    let out_dir = tempfile::tempdir().expect("create temp out dir");
    let opts = BuildOptions {
        out_dir: out_dir.path().to_path_buf(),
        runtime_link_args: support::runtime_link_args().clone(),
    };
    let bin = keel_codegen::compile(&kir, &opts).expect("compile must succeed");
    Command::new(&bin).output().expect("run compiled binary")
}

const SOURCE: &str = r#"
use std/io

type Priority = low | medium | high

task label(p: Priority) -> str {
  when p {
    low => { return "low priority" }
    medium => { return "medium priority" }
    high => { return "high priority" }
  }
}

task grade(score: str) -> str {
  when score {
    "A" => { return "excellent" }
    "B" => { return "good" }
    _ => { return "needs work" }
  }
}

io.show(label(Priority.low))
io.show(label(Priority.high))
io.show(grade("A"))
io.show(grade("D"))
"#;

#[test]
fn enum_when_and_str_when_with_wildcard_match_the_interpreter() {
    let compiled = compile_and_run(SOURCE);
    let interpreted = support::run_interpreter(SOURCE);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(
        compiled.stdout,
        b"  low priority\n  high priority\n  excellent\n  needs work\n"
    );
}

#[test]
fn non_returning_when_statement_falls_through_to_a_later_statement() {
    // Every arm above terminates via `return` (each branch's LLVM block
    // already has a terminator, so nothing merges). This exercises the
    // other shape: an arm that falls off the end of its block (`io.show`,
    // no `return`) merges back into the enclosing function and execution
    // continues past the `when` — a different codegen path (`emit_if`'s
    // per-branch unconditional-branch-to-merge case) than every other test
    // in this file exhausts.
    let source = r#"
use std/io

type Priority = low | medium | high

task announce(p: Priority) {
  when p {
    low => { io.show("low seen") }
    medium => { io.show("medium seen") }
    high => { io.show("high seen") }
  }
  io.show("done")
}

announce(Priority.low)
announce(Priority.high)
"#;

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(
        compiled.stdout,
        b"  low seen\n  done\n  high seen\n  done\n"
    );
}

#[test]
fn when_arm_with_multiple_comma_separated_patterns_matches_the_interpreter() {
    let source = r#"
use std/io

task grade(score: str) -> str {
  when score {
    "A", "B" => { return "good" }
    _ => { return "needs work" }
  }
}

io.show(grade("A"))
io.show(grade("B"))
io.show(grade("C"))
"#;

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, b"  good\n  good\n  needs work\n");
}
