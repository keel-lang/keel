use crate::common::*;

// ---------------------------------------------------------------------------
// Null assertions
// ---------------------------------------------------------------------------

#[test]
fn null_assert_on_none_raises_runtime_error() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        x = none
        val = x!
        io.show(val)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit when unwrapping none");
    assert!(
        stderr.contains("NullError") || stderr.contains("none"),
        "expected NullError in stderr:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Type error diagnostics
// ---------------------------------------------------------------------------

#[test]
fn type_error_includes_source_span() {
    let src = r#"
agent A {
  @on_start {
    x: int = "not an int"
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected type error");
    // miette renders spans as ╭─[file:line:col]
    assert!(
        stderr.contains('╭') || stderr.contains('['),
        "type error should include source location:\n{stderr}"
    );
}

#[test]
fn type_error_arity_includes_param_names() {
    let src = r#"
task greet(name: str, title: str) -> str {
  name + title
}
agent A {
  @on_start {
    r = greet("a", "b", "c", "d")
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected arity type error");
    assert!(
        stderr.contains("name") || stderr.contains("title"),
        "arity error should list param names:\n{stderr}"
    );
}

// Issue #235: a call omitting a required (no-default) parameter used to pass
// `keel check` silently, with the interpreter binding the missing param to
// `none` rather than erroring — `keel run` would then either crash on an
// unrelated type error deep inside the task body, or (worse, with no
// observable effect at all) succeed silently.
#[test]
fn missing_required_arg_is_a_check_error_not_a_silent_none() {
    let src = r#"
task f(a: int, b: int) -> int {
  return a + b
}
agent A {
  @on_start {
    r = f(1)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected a missing-argument type error");
    assert!(
        stderr.contains('b'),
        "error should name the missing param `b`:\n{stderr}"
    );
}

// Issue #238: same bug class as #235 above, but for a lambda call — a
// different code path (`Ty::Func`-typed callee, not a named task lookup)
// with its own checker story, since `LambdaParam` has no default field at
// all: every lambda parameter is unconditionally required.
#[test]
fn closure_call_missing_arg_is_a_check_error_not_a_silent_none() {
    let src = r#"
use std/io

task main() {
  add = (a, b) => a + b
  io.show("{add(1)}")
}
main()
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected a closure-call arity type error");
    assert!(
        stderr.contains("expected 2 argument(s), got 1"),
        "error should report the expected/actual arg counts:\n{stderr}"
    );
}

// A spread arg's expanded length isn't known at check time, so the checker
// deliberately skips the arity check for `add(...pair)` — this reaches
// `call_closure_inner` with too few args despite passing `keel check`,
// exercising its own defense-in-depth guard (issue #238) rather than the
// checker's.
#[test]
fn closure_call_missing_arg_via_undersized_spread_is_a_runtime_error() {
    let src = r#"
task main() {
  add = (a, b) => a + b
  pair = [1]
  x = add(...pair)
}
main()
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(
        ok,
        "checker should not flag an undersized spread:\n{stderr}"
    );

    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected a runtime error, not a silent none binding");
    assert!(
        stderr.contains("expected 2 argument(s), got 1"),
        "error should report the expected/actual arg counts:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Regressions
// ---------------------------------------------------------------------------

#[test]
fn modulo_by_zero_is_runtime_error() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        x = 5 % 0
        io.show(x)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "5 % 0 should exit non-zero");
    assert!(
        stderr.contains("zero") || stderr.contains("modulo") || stderr.contains("Modulo"),
        "expected modulo-by-zero error:\n{stderr}"
    );
}

#[test]
fn return_inside_if_expr_propagates_out_of_task() {
    let src = r#"
use std/io
task classify(n: int) -> str {
    label = if n > 0 { return "positive" } else { "other" }
    label
}
agent A {
    @tools [io]
    @on_start {
        io.show(classify(5))
        io.show(classify(-1))
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("positive"),
        "early return inside if-expr should yield 'positive':\n{stdout}"
    );
    assert!(
        stdout.contains("other"),
        "else branch should yield 'other':\n{stdout}"
    );
}

#[test]
fn return_inside_if_expr_else_branch_propagates() {
    // Exercises the else-body path of the IfExpr EarlyReturn fix.
    let src = r#"
use std/io
task classify(n: int) -> str {
    label = if n > 0 { "positive" } else { return "non-positive" }
    label
}
agent A {
    @tools [io]
    @on_start {
        io.show(classify(5))
        io.show(classify(-3))
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("positive"),
        "then branch should yield 'positive':\n{stdout}"
    );
    assert!(
        stdout.contains("non-positive"),
        "early return inside else-expr should yield 'non-positive':\n{stdout}"
    );
}

#[test]
fn return_inside_list_literal_propagates() {
    // Before ExprFlow, `return` nested in a list literal element was silently
    // dropped into the list as a stray EarlyReturn value instead of exiting
    // the enclosing task.
    let src = r#"
use std/io
task get_early(flag: bool) -> int {
    nums = [1, if flag { return 42 } else { 0 }, 3]
    nums[0]
}
agent A {
    @tools [io]
    @on_start {
        io.show(get_early(true))
        io.show(get_early(false))
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("42"),
        "return inside list element should exit task with 42:\n{stdout}"
    );
    assert!(
        stdout.contains("1"),
        "no early return: task should return nums[0] = 1:\n{stdout}"
    );
}

#[test]
fn limits_max_cost_per_request_raises_error() {
    let src = r#"
agent Bot {
    @limits { max_cost_per_request: 0.50, timeout: 30.seconds }
    @on_start { stop(self) }
}
run(Bot)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "unsupported @limits field should cause non-zero exit");
    assert!(
        stderr.contains("max_cost_per_request") || stderr.contains("not supported"),
        "expected error about unsupported @limits field:\n{stderr}"
    );
}

#[test]
fn limits_require_confirmation_raises_error() {
    let src = r#"
agent Bot {
    @limits { require_confirmation: [Io], timeout: 10.seconds }
    @on_start { stop(self) }
}
run(Bot)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "unsupported @limits field should cause non-zero exit");
    assert!(
        stderr.contains("require_confirmation") || stderr.contains("not supported"),
        "expected error about unsupported @limits field:\n{stderr}"
    );
}

#[test]
fn limits_supported_fields_are_accepted() {
    let src = r#"
agent Bot {
    @limits { timeout: 30.seconds, max_tokens: 1024, max_cost: 0.10 }
    @on_start { stop(self) }
}
run(Bot)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "supported @limits fields should not error:\nstderr: {stderr}"
    );
}

// ---------------------------------------------------------------------------
// raise
// ---------------------------------------------------------------------------

#[test]
fn raise_string_is_caught_by_error() {
    let src = r#"
use std/io
try {
    raise "something went wrong"
} catch err: Error {
    io.show("caught: {err.message}")
}
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("caught: something went wrong"),
        "expected catch message, stdout: {stdout}"
    );
}

#[test]
fn raise_stops_execution_in_block() {
    let src = r#"
use std/io
try {
    io.show("before")
    raise "stop"
    io.show("after")
} catch err: Error {
    io.show("caught")
}
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("before"), "stdout: {stdout}");
    assert!(
        !stdout.contains("after"),
        "execution should stop at raise, stdout: {stdout}"
    );
    assert!(stdout.contains("caught"), "stdout: {stdout}");
}

#[test]
fn raise_without_catch_exits_nonzero() {
    let src = r#"
raise "unhandled error"
"#;
    let (ok, _stdout, _stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for unhandled raise");
}

#[test]
fn raise_inside_task_propagates() {
    let src = r#"
use std/io
task validate(x: int) {
    if x < 0 {
        raise "x must be non-negative"
    }
}

try {
    validate(-1)
} catch err: Error {
    io.show("task raised: {err.message}")
}
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("task raised: x must be non-negative"),
        "stdout: {stdout}"
    );
}
