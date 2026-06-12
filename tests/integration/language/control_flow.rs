use crate::common::*;

// ---------------------------------------------------------------------------
// v0.1.4 — if / else expressions
// ---------------------------------------------------------------------------

#[test]
fn if_expr_on_rhs_of_binding() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        score = 0.9
        label = if score > 0.8 { "high" } else { "low" }
        io.show(label)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("high"),
        "expected 'high' branch, stdout:\n{stdout}"
    );
}

#[test]
fn if_expr_else_branch_selected() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        score = 0.3
        label = if score > 0.8 { "high" } else { "low" }
        io.show(label)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("low"),
        "expected 'low' branch, stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// v0.1.12 — Range operator `..`
// ---------------------------------------------------------------------------

#[test]
fn range_basic_for_loop() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    for i in 1..3 {
      io.show("{i}")
    }
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('1'), "expected 1 in output:\n{stdout}");
    assert!(stdout.contains('2'), "expected 2 in output:\n{stdout}");
    assert!(stdout.contains('3'), "expected 3 in output:\n{stdout}");
}

#[test]
fn range_assigned_to_variable() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    xs = 1..4
    io.show("{xs.count()}")
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('4'), "expected count 4 for 1..4:\n{stdout}");
}

#[test]
fn range_type_error_non_int() {
    let src = r#"
agent A {
  @on_start {
    xs = 1.0..3.0
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected type error for float range\nstderr: {stderr}");
    assert!(
        stderr.contains("int") || stderr.contains("range"),
        "error should mention int or range:\n{stderr}"
    );
}

#[test]
fn range_empty() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    xs = 5..3
    io.show("{xs.count()}")
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains('0'),
        "expected empty range to have count 0:\n{stdout}"
    );
}

#[test]
fn range_single() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    xs = 4..4
    io.show("{xs.count()}")
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains('1'),
        "expected single-element range to have count 1:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// v0.1.14 — if guards (for loops and when arms)
// ---------------------------------------------------------------------------

#[test]
fn if_guard_for_filters_elements() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [1, 2, 3, 4, 5]
    for n in nums if n % 2 == 0 {
      io.show("even:{n}")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("even:2"), "2 should pass filter:\n{stdout}");
    assert!(stdout.contains("even:4"), "4 should pass filter:\n{stdout}");
    assert!(
        !stdout.contains("even:1"),
        "1 should be filtered:\n{stdout}"
    );
    assert!(
        !stdout.contains("even:3"),
        "3 should be filtered:\n{stdout}"
    );
}

#[test]
fn if_guard_for_range() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    for x in 1..5 if x != 3 {
      io.show("x:{x}")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("x:1"), "1 should appear:\n{stdout}");
    assert!(stdout.contains("x:2"), "2 should appear:\n{stdout}");
    assert!(!stdout.contains("x:3"), "3 should be filtered:\n{stdout}");
    assert!(stdout.contains("x:4"), "4 should appear:\n{stdout}");
    assert!(stdout.contains("x:5"), "5 should appear:\n{stdout}");
}

#[test]
fn when_arm_where_guard() {
    // Guard must be a non-trivial expression (not a bare ident) to avoid
    // the lambda ambiguity: `ident => body` parses as a lambda.
    let src = r#"
use std/io
type Status = active | inactive
agent A {
  @tools [io]
  @on_start {
    s = Status.active
    level = 5
    when s {
      active where level > 3 => io.show("admin-active")
      active                 => io.show("user-active")
      _                      => io.show("inactive")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("admin-active"),
        "guard should match:\n{stdout}"
    );
}

#[test]
fn when_arm_where_guard_falls_through() {
    let src = r#"
use std/io
type Status = active | inactive
agent A {
  @tools [io]
  @on_start {
    s = Status.active
    level = 1
    when s {
      active where level > 3 => io.show("admin-active")
      active                 => io.show("user-active")
      _                      => io.show("inactive")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("user-active"),
        "guard false should fall through:\n{stdout}"
    );
    assert!(
        !stdout.contains("admin-active"),
        "admin branch should not fire:\n{stdout}"
    );
}

// ── when as expression ────────────────────────────────────────────────────────

#[test]
fn when_expr_evaluates_to_matched_arm_value() {
    let src = r#"
use std/io
task grade(score: str) -> str {
  when score {
    "A" => "excellent"
    "B" => "good"
    _   => "needs work"
  }
}

io.show(grade("A"))
io.show(grade("B"))
io.show(grade("C"))
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("excellent"), "stdout: {stdout}");
    assert!(stdout.contains("good"), "stdout: {stdout}");
    assert!(stdout.contains("needs work"), "stdout: {stdout}");
}

#[test]
fn when_expr_result_assigned_to_variable() {
    let src = r#"
use std/io
task label(n: int) -> str {
  result = when n {
    0 => "zero"
    1 => "one"
    _ => "many"
  }
  result
}

io.show(label(0))
io.show(label(1))
io.show(label(5))
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("zero"), "stdout: {stdout}");
    assert!(stdout.contains("one"), "stdout: {stdout}");
    assert!(stdout.contains("many"), "stdout: {stdout}");
}

// ---------------------------------------------------------------------------
// Named and mixed positional/named arguments
// ---------------------------------------------------------------------------

#[test]
fn named_args_bind_by_label_for_user_tasks() {
    let src = r#"
use std/io
task greet(greeting: str, name: str) -> str {
    "{greeting}, {name}!"
}
agent A {
    @tools [io]
    @on_start {
        io.show(greet(name: "Alice", greeting: "Hello"))
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("Hello, Alice!"),
        "named args should bind by label regardless of call order:\n{stdout}"
    );
}

#[test]
fn mixed_named_and_positional_args() {
    let src = r#"
use std/io
task add(a: int, b: int, c: int) -> int {
    a + b + c
}
agent A {
    @tools [io]
    @on_start {
        io.show("{add(1, c: 30, b: 20)}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("51"),
        "mixed named+positional should sum to 51:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Issue #13 — else-if at statement position (was a parse error before dedup)
// ---------------------------------------------------------------------------

#[test]
fn if_else_if_chain_statement_form_executes() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        x = 2
        if x == 1 {
            io.show("one")
        } else if x == 2 {
            io.show("two")
        } else {
            io.show("other")
        }
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("two"),
        "expected 'two' branch for x == 2, stdout:\n{stdout}"
    );
}

#[test]
fn if_else_if_chain_three_branches_statement_form() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        x = 3
        if x == 1 {
            io.show("one")
        } else if x == 2 {
            io.show("two")
        } else if x == 3 {
            io.show("three")
        } else {
            io.show("other")
        }
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("three"),
        "expected 'three' branch for x == 3, stdout:\n{stdout}"
    );
}
