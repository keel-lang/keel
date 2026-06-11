use crate::common::*;

// ---------------------------------------------------------------------------
// break / continue
// ---------------------------------------------------------------------------

#[test]
fn break_exits_loop_early() {
    let src = r#"
use std/io
agent A {
    @on_start {
        count = 0
        for i in 1..10 {
            if i > 3 {
                break
            }
            count += 1
        }
        io.show("{count}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("3"), "expected count=3, stdout: {stdout}");
}

#[test]
fn continue_skips_current_iteration() {
    let src = r#"
use std/io
agent A {
    @on_start {
        sum = 0
        for i in 1..6 {
            if i == 3 {
                continue
            }
            sum += i
        }
        io.show("{sum}")
    }
}
run(A)
"#;
    // 1+2+4+5+6 = 18 (3 is skipped)
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("18"), "expected sum=18, stdout: {stdout}");
}

#[test]
fn break_inside_if_stmt_in_loop() {
    // Verifies break inside an `if` inside a `for` exits the loop.
    // Items before 99 are counted; break fires at 99 so count stays at 2.
    let src = r#"
use std/io
agent A {
    @on_start {
        items = [10, 20, 99, 30, 40]
        count = 0
        for item in items {
            if item == 99 {
                break
            }
            count += 1
        }
        io.show("{count}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("2"), "expected count=2, stdout: {stdout}");
}

#[test]
fn continue_and_break_together() {
    let src = r#"
use std/io
agent A {
    @on_start {
        result = 0
        for i in 1..10 {
            if i == 7 {
                break
            }
            if i % 2 == 0 {
                continue
            }
            result += i
        }
        io.show("{result}")
    }
}
run(A)
"#;
    // odd numbers 1..6: 1+3+5 = 9 (7 triggers break, evens are skipped)
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("9"), "expected result=9, stdout: {stdout}");
}

#[test]
fn break_in_nested_loop_only_exits_inner() {
    let src = r#"
use std/io
agent A {
    @on_start {
        outer = 0
        for i in 1..3 {
            outer += 1
            for j in 1..10 {
                if j > 2 {
                    break
                }
            }
        }
        io.show("{outer}")
    }
}
run(A)
"#;
    // outer loop runs 3 times; inner break only exits the inner loop
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("3"), "expected outer=3, stdout: {stdout}");
}

// ─── while loop (v0.1.27) ────────────────────────────────────────────────────

#[test]
fn while_basic_countdown() {
    let src = r#"
use std/io
agent A {
    @on_start {
        n = 3
        while n > 0 {
            io.show("tick:{n}")
            n -= 1
        }
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("tick:3"), "tick:3 expected:\n{stdout}");
    assert!(stdout.contains("tick:2"), "tick:2 expected:\n{stdout}");
    assert!(stdout.contains("tick:1"), "tick:1 expected:\n{stdout}");
    assert!(
        !stdout.contains("tick:0"),
        "tick:0 should not appear:\n{stdout}"
    );
}

#[test]
fn while_break_exits_loop() {
    let src = r#"
use std/io
agent A {
    @on_start {
        count = 0
        while true {
            count += 1
            if count >= 3 {
                break
            }
        }
        io.show("{count}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("3"), "expected count=3:\n{stdout}");
}

#[test]
fn while_continue_skips_iteration() {
    let src = r#"
use std/io
agent A {
    @on_start {
        x = 0
        sum = 0
        while x < 6 {
            x += 1
            if x % 2 == 0 {
                continue
            }
            sum += x
        }
        io.show("{sum}")
    }
}
run(A)
"#;
    // odd numbers 1,3,5 => sum = 9
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("9"), "expected sum=9:\n{stdout}");
}

#[test]
fn while_nested_in_for() {
    let src = r#"
use std/io
agent A {
    @on_start {
        total = 0
        for i in 1..3 {
            j = 0
            while j < i {
                total += 1
                j += 1
            }
        }
        io.show("{total}")
    }
}
run(A)
"#;
    // i=1: 1 iter, i=2: 2 iters, i=3: 3 iters => total=6
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("6"), "expected total=6:\n{stdout}");
}

#[test]
fn while_non_bool_condition_type_error() {
    let src = r#"
task t() {
    while "oops" {
        break
    }
}
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected type error for non-bool while condition");
    assert!(
        stderr.contains("while") || stderr.contains("bool"),
        "expected while/bool in error:\n{stderr}"
    );
}
