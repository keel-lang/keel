//! Exit criterion for issue #160: `when` used as an expression in `let`
//! position (`result = when ... { ... }`) and `return` position compiles,
//! links, and matches the interpreter — including
//! `examples/when_expression.keel`'s `grade` task verbatim.
//!
//! And for issue #170: the same in an arbitrary *nested* position — a call
//! argument, a stdlib namespace argument, a binary-op operand — where the
//! `when` desugars to a declare+`if`-chain hoisted ahead of the enclosing
//! statement, with any sibling sub-expression to its left spilled into a
//! temp so evaluation order is preserved (`lower/mod.rs`'s
//! `FnCtx::keep_order`).

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

#[test]
fn when_expression_as_a_call_argument_matches_the_interpreter() {
    // Issue #170's exit criterion, verbatim.
    let source = r#"
use std/io

task f(s: str) -> str {
  return s
}

task g(n: int) -> str {
  return f(when n {
    0 => "zero"
    _ => "nonzero"
  })
}

io.show(g(0))
io.show(g(7))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  zero\n  nonzero\n");
}

#[test]
fn when_expression_as_a_binary_operand_matches_the_interpreter() {
    let source = r#"
use std/io

task rank(n: int) -> int {
  return 100 + when n {
    0 => 1
    _ => 2
  }
}

io.show("{rank(0)} {rank(7)}")
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  101 102\n");
}

#[test]
fn when_expression_as_a_namespace_call_argument_matches_the_interpreter() {
    // A stdlib namespace argument lowers through plain `lower_expr` (the
    // catalog's params are `dynamic`), so the result type comes from the
    // discarded first-arm probe rather than an expected type.
    let source = r#"
use std/io

task describe(n: int) {
  io.show(when n {
    0 => "zero"
    _ => "nonzero"
  })
}

describe(0)
describe(7)
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  zero\n  nonzero\n");
}

#[test]
fn a_sibling_left_of_a_when_expression_keeps_its_evaluation_order() {
    // `note(...)` runs before the `when`, and the `when`'s own arms print
    // too — so a hoist that moved the chain ahead of `note(...)` would show
    // up as reordered output here, not just as a different value.
    let source = r#"
use std/io

task note(s: str) -> str {
  io.show(s)
  return s
}

task both(n: int) -> str {
  return note("left") + note(when n {
    0 => note("zero")
    _ => note("nonzero")
  })
}

io.show(both(0))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  left\n  zero\n  zero\n  leftzero\n");
}

#[test]
fn when_expression_widened_into_a_nullable_argument_matches_the_interpreter() {
    // The result temp takes the pinned `int?` type, so each arm's own `int`
    // tail value goes through `lower_expr_expecting`'s nullable-widening
    // path on its way into it.
    let source = r#"
use std/io

task takes_opt(x: int?) -> int {
  return x ?? 0
}

task pick(n: int) -> int {
  return takes_opt(when n {
    0 => 1
    _ => 2
  })
}

io.show("{pick(0)} {pick(9)}")
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  1 2\n");
}

#[test]
fn a_when_expression_nested_in_another_when_expression_matches_the_interpreter() {
    let source = r#"
use std/io

task f(s: str) -> str {
  return s
}

task classify(n: int, m: int) -> str {
  return f(when n {
    0 => f(when m {
      0 => "both zero"
      _ => "n zero"
    })
    _ => "n nonzero"
  })
}

io.show(classify(0, 0))
io.show(classify(0, 3))
io.show(classify(3, 0))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  both zero\n  n zero\n  n nonzero\n");
}
