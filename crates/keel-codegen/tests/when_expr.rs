//! Exit criterion for issue #160: `when` used as an expression in `let`
//! position (`result = when ... { ... }`) and `return` position compiles,
//! links, and matches the interpreter — including
//! `examples/when_expression.keel`'s `grade` task verbatim.

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

fn assert_stdout_matches_interpreter(source: &str) -> Vec<u8> {
    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);
    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    compiled.stdout
}

#[test]
fn when_expression_in_let_position_matches_the_interpreter() {
    let source = r#"
use std/io

task grade(score: str) -> str {
  result = when score {
    "A" => "excellent"
    "B" => "good"
    "C" => "fair"
    _ => "needs work"
  }
  return result
}

io.show(grade("A"))
io.show(grade("D"))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  excellent\n  needs work\n");
}

#[test]
fn when_expression_in_return_position_matches_the_interpreter() {
    let source = r#"
use std/io

task grade(score: str) -> str {
  return when score {
    "A" => "excellent"
    _ => "needs work"
  }
}

io.show(grade("A"))
io.show(grade("Z"))
"#;
    assert_stdout_matches_interpreter(source);
}

#[test]
fn when_expression_example_grade_task_matches_the_interpreter() {
    // `examples/when_expression.keel`'s `grade` task verbatim.
    let source = r#"
use std/io

task grade(score: str) -> str {
  result = when score {
    "A" => "excellent"
    "B" => "good"
    "C" => "fair"
    _ => "needs work"
  }
  result
}

io.show(grade("A"))
io.show(grade("D"))
"#;
    assert_stdout_matches_interpreter(source);
}
