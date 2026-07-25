//! Exit criterion for issue #159: a task whose body ends in a bare tail
//! expression (no explicit `return`), including one nested inside `if`/
//! `else` or `when` arms, compiles, links, and its output matches the
//! interpreter — rather than trapping (`SIGTRAP`) via `finish_block`'s
//! `build_unreachable` fallback, which is what happened before this
//! issue's fix.

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

// The interpreter always exits 0 (no `return`-at-top-level exit-code
// convention — see `control_flow_and_calls.rs`'s module doc), so these two
// tests assert the compiled binary's exit code directly against the
// independently computed value, same as that file's own tests, rather than
// comparing against the interpreter's exit code.

#[test]
fn bare_tail_expression_returns_instead_of_trapping() {
    // Direct repro from the issue: previously this compiled to a binary
    // that trapped (SIGTRAP) instead of exiting with code 10.
    let source = r#"
task double(x: int) -> int {
  x * 2
}

double(5)
"#;
    assert_eq!(compile_and_run(source).status.code(), Some(10));
}

#[test]
fn bare_tail_expression_inside_if_else_returns_instead_of_trapping() {
    let source = r#"
task abs(n: int) -> int {
  if n < 0 {
    0 - n
  } else {
    n
  }
}

abs(-7)
"#;
    assert_eq!(compile_and_run(source).status.code(), Some(7));

    let source = r#"
task abs(n: int) -> int {
  if n < 0 {
    0 - n
  } else {
    n
  }
}

abs(7)
"#;
    assert_eq!(compile_and_run(source).status.code(), Some(7));
}

#[test]
fn bare_tail_expression_inside_when_arms_returns_instead_of_trapping() {
    let source = r#"
use std/io

task label(n: int) -> str {
  when n {
    0 => { "zero" }
    _ => { "nonzero" }
  }
}

io.show(label(0))
io.show(label(5))
"#;
    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);
    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
}

#[test]
fn bare_tail_expression_result_matches_the_interpreter_byte_for_byte() {
    // `io.show`ing the tail-returned value (rather than using it as the
    // top-level exit code) gives a direct interpreter-vs-compiled stdout
    // comparison, closing the gap the exit-code-only tests above leave (the
    // interpreter has no return-at-top-level exit-code convention to
    // compare against — see this file's other tests).
    let source = r#"
use std/io

task double(x: int) -> int {
  x * 2
}

task abs(n: int) -> int {
  if n < 0 {
    0 - n
  } else {
    n
  }
}

io.show(double(5))
io.show(abs(-7))
io.show(abs(7))
"#;
    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);
    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, b"  10\n  7\n  7\n");
}
