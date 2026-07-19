//! Exit criterion for issue #133: a scalar-only `.keel` example using
//! `if`/`else`, `while`, `for` over a range, and calls between compiled
//! tasks compiles, links, and its exit code matches the independently
//! computed value. See `func.rs`'s module doc: real top-level statements
//! can't produce a computed exit code directly (no `return` at top level),
//! so every fixture here ends with a *bare call* to a task — the call's
//! `int` result becomes the exit code via the same convention #132
//! established for bare arithmetic.

use std::process::Command;

use keel_codegen::BuildOptions;

#[path = "support/mod.rs"]
mod support;

fn compile_and_run(source: &str) -> i32 {
    let (program, _named) =
        keel_syntax::parse_source(source, "t.keel").expect("fixture must parse");
    let kir = keel_kir::lower(&program, "t.keel").expect("fixture must lower to KIR");

    let out_dir = tempfile::tempdir().expect("create temp out dir");
    let opts = BuildOptions {
        out_dir: out_dir.path().to_path_buf(),
        runtime_link_args: support::runtime_link_args().clone(),
    };
    let bin = keel_codegen::compile(&kir, &opts).expect("compile must succeed");

    let run = Command::new(&bin).output().expect("run compiled binary");
    run.status.code().expect("process exited via a signal")
}

#[test]
fn if_else_selects_the_correct_branch() {
    let source = r#"
task classify(n: int) -> int {
  if n < 0 {
    return 1
  } else {
    return 2
  }
}

classify(-5)
"#;
    assert_eq!(compile_and_run(source), 1);

    let source = r#"
task classify(n: int) -> int {
  if n < 0 {
    return 1
  } else {
    return 2
  }
}

classify(5)
"#;
    assert_eq!(compile_and_run(source), 2);
}

#[test]
fn if_without_else_falls_through_correctly() {
    // No `else` — the non-taken-branch path must still reach `return total`
    // (proves the `if.merge` block wiring works when `else_branch` is empty).
    let source = r#"
task bump(n: int) -> int {
  total = n
  if n < 10 {
    total += 100
  }
  return total
}

bump(3)
"#;
    assert_eq!(compile_and_run(source), 103);

    let source = r#"
task bump(n: int) -> int {
  total = n
  if n < 10 {
    total += 100
  }
  return total
}

bump(20)
"#;
    assert_eq!(compile_and_run(source), 20);
}

#[test]
fn while_loop_computes_correct_sum() {
    let source = r#"
task sum_upto(n: int) -> int {
  total = 0
  i = 0
  while i < n {
    total += i
    i += 1
  }
  return total
}

sum_upto(5)
"#;
    // 0 + 1 + 2 + 3 + 4 = 10
    assert_eq!(compile_and_run(source), 10);
}

#[test]
fn for_over_range_computes_correct_sum() {
    let source = r#"
task sum_range(n: int) -> int {
  total = 0
  for i in 0..n {
    total += i
  }
  return total
}

sum_range(5)
"#;
    // Inclusive range: 0 + 1 + 2 + 3 + 4 + 5 = 15
    assert_eq!(compile_and_run(source), 15);
}

#[test]
fn calls_between_compiled_tasks_compose() {
    let source = r#"
task double(x: int) -> int {
  return x * 2
}

task quadruple(x: int) -> int {
  return double(double(x))
}

quadruple(3)
"#;
    assert_eq!(compile_and_run(source), 12);
}

#[test]
fn forward_reference_to_a_later_declared_task_resolves() {
    // `caller` is declared (and calls `callee`) before `callee`'s own
    // declaration — proves declare_functions's up-front pass, not emission
    // order, is what makes the call resolve.
    let source = r#"
task caller(x: int) -> int {
  return callee(x) + 1
}

task callee(x: int) -> int {
  return x * 10
}

caller(4)
"#;
    assert_eq!(compile_and_run(source), 41);
}

#[test]
fn everything_together_matches_the_independently_computed_value() {
    let source = r#"
task classify(n: int) -> int {
  if n < 0 {
    return 1
  } else {
    return 2
  }
}

task sum_upto(n: int) -> int {
  total = 0
  i = 0
  while i < n {
    total += i
    i += 1
  }
  return total
}

task sum_range(n: int) -> int {
  total = 0
  for i in 0..n {
    total += i
  }
  return total
}

task double(x: int) -> int {
  return x * 2
}

task quadruple(x: int) -> int {
  return double(double(x))
}

classify(-5) + sum_upto(5) + sum_range(5) + quadruple(3)
"#;
    // 1 + 10 + 15 + 12 = 38
    assert_eq!(compile_and_run(source), 38);
}

#[test]
fn bare_return_in_top_level_code_exits_zero() {
    // Real top-level statements always lower to a Unit-returning function
    // (keel-kir forces this), so a bare top-level `return` never carries a
    // value — `keel_user_toplevel` treats it as "stop now, exit 0" (see
    // func.rs's module doc; issue #134 made toplevel a real function with
    // its own exit-code convention instead of inlining into `main`).
    let code = compile_and_run("return\n5\n");
    assert_eq!(code, 0);
}

// CallTarget::Ns dispatch (io.show/log.* wired through keel_rt_call_ns) is
// covered by tests/namespace_calls.rs, which also proves byte-identical
// output against the interpreter — the exit criterion for issue #135.
