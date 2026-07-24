//! Exit criterion for issue #150 (raise/try/catch result calling
//! convention): a program where a task `raise`s, a caller catches it via
//! `try`/`catch`, and a separate call site uses automatic propagation
//! (an uncaught call to a `can_raise` function) to propagate the same
//! error compiles, links, and matches the interpreter's error message.

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
fn raise_propagates_through_an_uncaught_caller_and_is_caught_further_up_matching_the_interpreter() {
    let source = r#"
use std/io

task a() -> int {
  raise "boom"
}

task b() -> int {
  return a()
}

task c() -> int {
  try {
    result = b()
    return result
  } catch e: Error {
    io.show(e.message)
    return -1
  }
}

io.show(c())
"#;
    assert_matches_interpreter(source, b"  boom\n  -1\n");
}

#[test]
fn try_catch_around_a_direct_raise_matches_the_interpreter() {
    let source = r#"
use std/io

task risky(should_raise: bool) -> int {
  if should_raise {
    raise "nope"
  }
  return 42
}

task run_it(should_raise: bool) -> int {
  try {
    return risky(should_raise)
  } catch e: Error {
    io.show(e.message)
    return -1
  }
}

io.show(run_it(false))
io.show(run_it(true))
"#;
    assert_matches_interpreter(source, b"  42\n  nope\n  -1\n");
}

#[test]
fn a_unit_returning_can_raise_task_falling_through_matches_the_interpreter() {
    // Exercises the success (non-error) path of a `none`-returning
    // `can_raise` function — falling off the end without an explicit
    // `return` still has to produce a well-formed result-ABI value
    // (`func.rs`'s `finish_block` can_raise+Unit branch), not just the
    // error path the other two tests already cover.
    let source = r#"
use std/io

task maybe_raise(should_raise: bool) {
  if should_raise {
    raise "unit-raise"
  }
}

task use_it(should_raise: bool) {
  try {
    maybe_raise(should_raise)
    io.show("no error")
  } catch e: Error {
    io.show(e.message)
  }
}

use_it(false)
use_it(true)
"#;
    assert_matches_interpreter(source, b"  no error\n  unit-raise\n");
}
