//! Exit criterion for issue #193: a `when`/`if`-expression as a `while`
//! condition, previously rejected outright by `keel build --emit=kir` (the
//! condition is re-evaluated once per iteration, but a bare hoist ahead of
//! the loop would run the chain exactly once, ahead of it) — the guarded
//! expression must run once *per iteration*, not once total.

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
fn a_when_expression_while_condition_is_re_evaluated_every_iteration() {
    let source = r#"
use std/io

task check(n: int) -> bool {
  io.show("check {n}")
  return when n {
    0 => true
    1 => true
    _ => false
  }
}

task f() -> int {
  n = 0
  while check(n) {
    n += 1
  }
  return n
}

io.show("{f()}")
"#;
    // Checked at n=0 (true), n=1 (true), n=2 (false) — three checks, not
    // one hoisted ahead of the loop.
    assert_matches_interpreter(source, b"  check 0\n  check 1\n  check 2\n  2\n");
}

#[test]
fn an_if_expression_while_condition_composes_the_same_way() {
    let source = r#"
use std/io

task f() -> int {
  n = 0
  while if true { n < 3 } else { false } {
    n += 1
  }
  return n
}

io.show("{f()}")
"#;
    assert_matches_interpreter(source, b"  3\n");
}
