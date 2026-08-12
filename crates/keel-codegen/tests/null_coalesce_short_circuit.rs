//! Exit criterion for issue #230: a `when`/`if`-expression as the fallback
//! of `??`, previously rejected outright by `keel build --emit=kir` — the
//! fallback must not run at all when the left-hand side is non-`none`.

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
fn a_when_expression_fallback_does_not_run_when_the_left_side_is_not_none() {
    let source = r#"
use std/io

task side_effect() -> int {
  io.show("evaluated")
  99
}

maybe: int? = 5
n = 0
result = maybe ?? when n {
  0 => side_effect()
  _ => 0
}
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  5\n");
}

#[test]
fn a_when_expression_fallback_runs_when_the_left_side_is_none() {
    let source = r#"
use std/io

task side_effect() -> int {
  io.show("evaluated")
  99
}

maybe: int? = none
n = 0
result = maybe ?? when n {
  0 => side_effect()
  _ => 0
}
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  evaluated\n  99\n");
}

#[test]
fn an_if_expression_fallback_composes_the_same_way() {
    let source = r#"
use std/io

maybe: int? = none
n = 0
result = maybe ?? if n == 0 { 1 } else { 2 }
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  1\n");
}

#[test]
fn a_side_effecting_nullable_left_side_is_evaluated_exactly_once() {
    // The nullable operand is spilled into a temp because the fallback
    // hoists (needing it twice: the `IsNone` test and the `UnwrapSome`
    // unwrap). If that spill were wrong, `side_effect` would run twice.
    let source = r#"
use std/io

task side_effect() -> int? {
  io.show("evaluated")
  5
}

n = 0
result = side_effect() ?? when n {
  0 => 1
  _ => 2
}
io.show("{result}")
"#;
    assert_matches_interpreter(source, b"  evaluated\n  5\n");
}
