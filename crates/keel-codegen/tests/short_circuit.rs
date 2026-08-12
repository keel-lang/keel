//! Exit criterion for issue #225: `and`/`or` short-circuit in compiled code
//! the same way #224 made them short-circuit in the interpreter — a
//! side-effecting right operand must not run when the left operand already
//! decides the result. Also covers #228: a `when`/`if`-expression as the
//! right operand of `and`/`or`, previously rejected outright by
//! `keel build --emit=kir`.

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

fn assert_matches_interpreter(source: &str, expected_stdout: &[u8]) {
    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, expected_stdout);
}

#[test]
fn and_does_not_run_the_right_operand_when_the_left_is_false() {
    let source = r#"
use std/io

task side_effect() -> bool {
  io.show("evaluated")
  true
}

flag = false
result = flag and side_effect()
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  false\n");
}

#[test]
fn or_does_not_run_the_right_operand_when_the_left_is_true() {
    let source = r#"
use std/io

task side_effect() -> bool {
  io.show("evaluated")
  true
}

flag = true
result = flag or side_effect()
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  true\n");
}

#[test]
fn and_still_runs_the_right_operand_when_the_left_is_true() {
    let source = r#"
use std/io

task side_effect() -> bool {
  io.show("evaluated")
  false
}

flag = true
result = flag and side_effect()
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  evaluated\n  false\n");
}

#[test]
fn or_still_runs_the_right_operand_when_the_left_is_false() {
    let source = r#"
use std/io

task side_effect() -> bool {
  io.show("evaluated")
  true
}

flag = false
result = flag or side_effect()
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  evaluated\n  true\n");
}

#[test]
fn nested_short_circuit_skips_the_inner_expression_entirely() {
    // The outer `and` short-circuits on `flag = false`, so the right
    // operand — itself a short-circuiting `or` over two calls — must never
    // run at all: neither `check_a` nor `check_b` should execute. This is
    // the case `emit_short_circuit`'s alloca-over-phi design targets: the
    // inner `or`'s own codegen builds its own basic blocks, so if it ran,
    // the builder would be left positioned somewhere other than the block
    // this outer call created.
    let source = r#"
use std/io

task check_a() -> bool {
  io.show("a")
  true
}

task check_b() -> bool {
  io.show("b")
  true
}

flag = false
result = flag and (check_a() or check_b())
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  false\n");
}

#[test]
fn nested_short_circuit_runs_the_inner_expression_when_the_outer_does_not_short_circuit() {
    // The outer `and`'s left is true, so the right operand runs — and it
    // is itself a short-circuiting `or`: `check_a` returns true, so
    // `check_b` must never run. Proves the outer `and` correctly picks up
    // the inner `or`'s result from whichever block its own short-circuit
    // left the builder in, not the block this function created.
    let source = r#"
use std/io

task check_a() -> bool {
  io.show("a")
  true
}

task check_b() -> bool {
  io.show("b")
  true
}

flag = true
result = flag and (check_a() or check_b())
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  a\n  true\n");
}

#[test]
fn a_when_expression_as_the_right_operand_of_and_does_not_run_when_short_circuited() {
    // Exit criterion for #228: `keel build --emit=kir` used to reject a
    // `when`/`if`-expression as the right operand of `and`/`or` outright.
    // `n` is chosen so the `when`'s side-effecting arm *would* fire if it
    // ran — proving it doesn't run at all when `flag` short-circuits `and`.
    let source = r#"
use std/io

flag = false
n = 0
result = flag and when n {
  0 => {
    io.show("evaluated")
    true
  }
  _ => false
}
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  false\n");
}

#[test]
fn a_when_expression_as_the_right_operand_of_and_runs_when_not_short_circuited() {
    let source = r#"
use std/io

flag = true
n = 0
result = flag and when n {
  0 => {
    io.show("evaluated")
    true
  }
  _ => false
}
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  evaluated\n  true\n");
}
