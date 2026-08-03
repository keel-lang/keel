//! Exit criterion for issue #192: `if` used as an expression compiles,
//! links, and matches the interpreter — in `let` position, `return`
//! position, and arbitrary *nested* positions (a call argument, a stdlib
//! namespace argument, a binary-op operand), where it desugars to a
//! declare+`if` pair hoisted ahead of the enclosing statement with any
//! sibling sub-expression to its left spilled into a temp so evaluation
//! order is preserved (`lower/mod.rs`'s `FnCtx::keep_order`).
//!
//! The sibling of `when_expr.rs`, deliberately mirroring its case list:
//! the two constructs share every mechanism (`TailSink::Assign`, the hoist
//! buffer, the type probe), so divergence between these two files is itself
//! a signal worth looking at.

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
fn if_expression_in_let_position_matches_the_interpreter() {
    let source = r#"
use std/io

task grade(score: int) -> str {
  result = if score > 90 { "excellent" } else { "needs work" }
  return result
}

io.show(grade(95))
io.show(grade(40))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  excellent\n  needs work\n");
}

#[test]
fn if_expression_in_return_position_matches_the_interpreter() {
    // No result temp on this path — each branch returns directly.
    let source = r#"
use std/io

task grade(score: int) -> str {
  return if score > 90 { "excellent" } else { "needs work" }
}

io.show(grade(95))
io.show(grade(40))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  excellent\n  needs work\n");
}

#[test]
fn else_if_chain_matches_the_interpreter() {
    // Each `else if` level nests its own `<if.result>` temp inside the
    // enclosing `else` branch — the parser spells its own recursion as a
    // one-statement block wrapping the next `if`, so no special case is
    // needed to lower it. All three arms must still be reachable.
    let source = r#"
use std/io

task band(score: int) -> str {
  label = if score > 90 { "A" } else if score > 80 { "B" } else { "C" }
  return label
}

io.show(band(95))
io.show(band(85))
io.show(band(10))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  A\n  B\n  C\n");
}

#[test]
fn if_expression_as_a_call_argument_matches_the_interpreter() {
    // Issue #192's own exit-criterion program.
    let source = r#"
use std/io

task shout(s: str) -> str {
  return s + "!"
}

task pick(n: int) -> str {
  return shout(if n == 0 { "zero" } else { "other" })
}

io.show(pick(0))
io.show(pick(7))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  zero!\n  other!\n");
}

#[test]
fn if_expression_as_a_binary_operand_matches_the_interpreter() {
    let source = r#"
use std/io

task rank(n: int) -> int {
  return 100 + if n == 0 { 1 } else { 2 }
}

io.show("{rank(0)} {rank(7)}")
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  101 102\n");
}

#[test]
fn if_expression_as_a_namespace_argument_matches_the_interpreter() {
    // A stdlib namespace param is `dynamic`, so nothing pins the result
    // type — it comes from the discarded `then`-branch probe instead.
    let source = r#"
use std/io

io.show(if 1 > 0 { "yes" } else { "no" })
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  yes\n");
}

#[test]
fn sibling_evaluation_order_is_preserved_around_a_hoisted_if() {
    // `label` is read *before* the `if` is evaluated, so hoisting the
    // declare+chain ahead of the `return` has to spill `label` into a temp
    // first — otherwise the concatenation would read it after the chain ran.
    let source = r#"
use std/io

task shout(s: str) -> str {
  return s + "!"
}

task tag(n: int, label: str) -> str {
  return label + shout(if n == 0 { "zero" } else { "nonzero" })
}

io.show(tag(0, "a:"))
io.show(tag(7, "b:"))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  a:zero!\n  b:nonzero!\n");
}

#[test]
fn if_expression_nesting_a_when_expression_matches_the_interpreter() {
    // The two constructs compose. A statement-position `when` in the `if`'s
    // `else` tail assigns straight into the same `<if.result>` local with no
    // second temp — that flattening is `lower_stmt`'s existing `Stmt::When`
    // arm propagating the sink, not anything new here. See
    // `if_expression_in_a_when_arm_matches_the_interpreter` for the reverse
    // nesting, which is the direction that goes through the new dispatch.
    let source = r#"
use std/io

task shout(s: str) -> str {
  return s + "!"
}

task mix(n: int) -> str {
  return shout(if n == 0 { "zero" } else { when n {
    1 => "one"
    _ => "many"
  } })
}

io.show(mix(0))
io.show(mix(1))
io.show(mix(5))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  zero!\n  one!\n  many!\n");
}

#[test]
fn if_expression_in_a_when_arm_matches_the_interpreter() {
    // The reverse nesting, and the one that exercises the new dispatch: the
    // arm body's tail is an `if`-expression under `TailSink::Assign`, so it
    // reaches `lower_expr_expecting`'s new `IfExpr` arm and hoists its
    // declare+branch pair *inside* the arm block rather than ahead of the
    // whole `when`.
    let source = r#"
use std/io

task f(n: int, c: bool) -> str {
  return when n {
    0 => if c { "a" } else { "b" }
    _ => "z"
  }
}

io.show(f(0, true))
io.show(f(0, false))
io.show(f(9, true))
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  a\n  b\n  z\n");
}

#[test]
fn annotated_if_expression_with_a_returning_branch_matches_the_interpreter() {
    // An annotation pins the result type, so the `then` branch is free to
    // `return` out of the enclosing task instead of producing a value —
    // only the *unannotated* form needs a probe-able tail value.
    let source = r#"
use std/io

task clamp(n: int) -> int {
  bounded: int = if n > 10 { return 10 } else { n }
  return bounded
}

io.show("{clamp(50)} {clamp(3)}")
"#;
    let stdout = assert_stdout_matches_interpreter(source);
    assert_eq!(stdout, b"  10 3\n");
}
