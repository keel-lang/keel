//! Exit criterion for issue #232: plain `break`/`continue` in `while` and
//! both `for` shapes, previously rejected outright by `keel build
//! --emit=kir` regardless of position — confirmed by a direct test before
//! this issue was filed (any `break` anywhere errored the whole build).

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
fn break_exits_a_while_loop_immediately() {
    let source = r#"
use std/io

task f(n: int) -> int {
  i = 0
  while i < n {
    if i == 3 {
      break
    }
    i += 1
  }
  return i
}

io.show("{f(10)}")
"#;
    assert_matches_interpreter(source, b"  3\n");
}

#[test]
fn continue_skips_the_rest_of_a_while_body() {
    let source = r#"
use std/io

task f(n: int) -> int {
  i = 0
  total = 0
  while i < n {
    i += 1
    if i % 2 == 0 {
      continue
    }
    total += i
  }
  return total
}

io.show("{f(6)}")
"#;
    // Sums the odd numbers 1..=6: 1 + 3 + 5 = 9.
    assert_matches_interpreter(source, b"  9\n");
}

#[test]
fn break_exits_an_indexed_for_loop() {
    let source = r#"
use std/io

task f() -> int {
  total = 0
  for i in 1..10 {
    if i == 5 {
      break
    }
    total += i
  }
  return total
}

io.show("{f()}")
"#;
    // 1 + 2 + 3 + 4 = 10, stops before adding 5.
    assert_matches_interpreter(source, b"  10\n");
}

#[test]
fn continue_still_runs_the_indexed_for_loops_increment() {
    let source = r#"
use std/io

task f() -> int {
  seen = 0
  for i in 1..5 {
    if i == 3 {
      continue
    }
    seen += 1
  }
  return seen
}

io.show("{f()}")
"#;
    // Every i in 1..=5 visits the loop once; only i == 3 skips the body via
    // `continue`. If `continue` skipped the increment instead, this would
    // never terminate.
    assert_matches_interpreter(source, b"  4\n");
}

#[test]
fn break_exits_a_for_each_loop_over_a_list() {
    let source = r#"
use std/io

task f() -> int {
  total = 0
  for x in [1, 2, 3, 4, 5] {
    if x == 3 {
      break
    }
    total += x
  }
  return total
}

io.show("{f()}")
"#;
    assert_matches_interpreter(source, b"  3\n");
}

#[test]
fn continue_still_advances_the_for_each_loops_index() {
    let source = r#"
use std/io

task f() -> int {
  seen = 0
  for x in [1, 2, 3, 4, 5] {
    if x == 3 {
      continue
    }
    seen += 1
  }
  return seen
}

io.show("{f()}")
"#;
    assert_matches_interpreter(source, b"  4\n");
}

#[test]
fn break_inside_a_nested_loop_only_exits_the_innermost_one() {
    let source = r#"
use std/io

task f() -> int {
  total = 0
  for i in 1..3 {
    for j in 1..3 {
      if j == 2 {
        break
      }
      total += 1
    }
  }
  return total
}

io.show("{f()}")
"#;
    // Outer runs 3 times, inner adds 1 each time before breaking at j == 2.
    assert_matches_interpreter(source, b"  3\n");
}

#[test]
fn break_inside_a_try_body_still_exits_the_enclosing_loop() {
    let source = r#"
use std/io

task f() -> int {
  total = 0
  for i in 1..5 {
    try {
      if i == 3 {
        break
      }
      total += i
    } catch e: Error {
      total += 0
    }
  }
  return total
}

io.show("{f()}")
"#;
    // 1 + 2 = 3, then breaks at i == 3 from inside the try body.
    assert_matches_interpreter(source, b"  3\n");
}
