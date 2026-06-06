use crate::common::*;

// ---------------------------------------------------------------------------
// Augmented assignment (+=, -=, *=, /=, %=)
// ---------------------------------------------------------------------------

#[test]
fn aug_assign_plus_eq_local() {
    let src = r#"
agent A {
    @on_start {
        x = 10
        x += 5
        Io.show("{x}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("15"), "expected 15, stdout: {stdout}");
}

#[test]
fn aug_assign_minus_eq_local() {
    let src = r#"
agent A {
    @on_start {
        x = 10
        x -= 3
        Io.show("{x}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("7"), "expected 7, stdout: {stdout}");
}

#[test]
fn aug_assign_star_eq_local() {
    let src = r#"
agent A {
    @on_start {
        x = 3
        x *= 4
        Io.show("{x}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("12"), "expected 12, stdout: {stdout}");
}

#[test]
fn aug_assign_slash_eq_local() {
    let src = r#"
agent A {
    @on_start {
        x = 20
        x /= 4
        Io.show("{x}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("5"), "expected 5, stdout: {stdout}");
}

#[test]
fn aug_assign_self_field() {
    let src = r#"
agent A {
    @role "aug"
    state { count: int = 0 }
    @on_start {
        self.count += 3
        self.count += 2
        Io.show("{self.count}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("5"), "expected 5, stdout: {stdout}");
}

#[test]
fn aug_assign_percent_eq_local() {
    let src = r#"
agent A {
    @on_start {
        x = 17
        x %= 5
        Io.show("{x}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("2"), "expected 2, stdout: {stdout}");
}

#[test]
fn aug_assign_chained_in_loop() {
    let src = r#"
agent A {
    @on_start {
        total = 0
        for i in 1..5 {
            total += i
        }
        Io.show("{total}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("15"), "expected 15, stdout: {stdout}");
}

// ─── Variadic parameters (v0.1.25) ───────────────────────────────────────────

#[test]
fn variadic_basic_collect() {
    // The variadic param collects all positional args into a list.
    let src = r#"
task join_words(...words: str) -> str {
    result = ""
    for w in words { result += w }
    result
}

agent A {
    @on_start {
        out = join_words("hello", " ", "world")
        Io.show(out)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("hello world"),
        "expected 'hello world', stdout: {stdout}"
    );
}

#[test]
fn variadic_zero_args_yields_empty_list() {
    let src = r#"
task count_args(...items: str) -> int {
    total = 0
    for _ in items { total += 1 }
    total
}

agent A {
    @on_start {
        n = count_args()
        Io.show("{n}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("0"), "expected 0, stdout: {stdout}");
}

#[test]
fn variadic_spread_expands_list() {
    let src = r#"
task sum_ints(...nums: int) -> int {
    total = 0
    for n in nums { total += n }
    total
}

agent A {
    @on_start {
        xs = [1, 2, 3]
        result = sum_ints(...xs, 4)
        Io.show("{result}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("10"), "expected 10, stdout: {stdout}");
}

#[test]
fn variadic_with_fixed_prefix() {
    let src = r#"
task labeled(prefix: str, ...items: str) -> str {
    result = prefix + ":"
    for item in items { result += " " + item }
    result
}

agent A {
    @on_start {
        out = labeled("tags", "rust", "keel", "lang")
        Io.show(out)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("tags: rust keel lang"),
        "expected 'tags: rust keel lang', stdout: {stdout}"
    );
}

// ─── min / max prelude free functions (v0.1.26) ──────────────────────────────

#[test]
fn min_variadic_integers() {
    let src = r#"
agent A {
    @on_start {
        result = min(3, 1, 4, 1, 5, 9)
        Io.show("{result}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("1"), "expected 1, stdout: {stdout}");
}

#[test]
fn max_variadic_integers() {
    let src = r#"
agent A {
    @on_start {
        result = max(3, 1, 4, 1, 5, 9)
        Io.show("{result}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("9"), "expected 9, stdout: {stdout}");
}

#[test]
fn min_with_list_spread() {
    let src = r#"
agent A {
    @on_start {
        scores = [7, 2, 9, 4]
        lo = min(scores)
        hi = max(scores)
        Io.show("{lo}")
        Io.show("{hi}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("2"), "expected lo=2, stdout: {stdout}");
    assert!(stdout.contains("9"), "expected hi=9, stdout: {stdout}");
}

#[test]
fn min_empty_returns_none() {
    let src = r#"
agent A {
    @on_start {
        result = min()
        Io.show("{result}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("none"), "expected none, stdout: {stdout}");
}

#[test]
fn min_with_by_key_selector() {
    let src = r#"
agent A {
    @on_start {
        people = [
            {name: "Alice", age: 30},
            {name: "Bob", age: 25},
            {name: "Carol", age: 35},
        ]
        youngest = min(people, by: p => p.age)
        Io.show(youngest.name)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("Bob"), "expected Bob, stdout: {stdout}");
}

#[test]
fn max_with_by_key_selector() {
    let src = r#"
agent A {
    @on_start {
        people = [
            {name: "Alice", age: 30},
            {name: "Bob", age: 25},
            {name: "Carol", age: 35},
        ]
        oldest = max(people, by: p => p.age)
        Io.show(oldest.name)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("Carol"), "expected Carol, stdout: {stdout}");
}

#[test]
fn min_single_item_returns_it() {
    let src = r#"
agent A {
    @on_start {
        result = min(42)
        Io.show("{result}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("42"), "expected 42, stdout: {stdout}");
}

#[test]
fn min_max_strings() {
    let src = r#"
agent A {
    @on_start {
        lo = min("banana", "apple", "cherry")
        hi = max("banana", "apple", "cherry")
        Io.show(lo)
        Io.show(hi)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("apple"), "expected apple, stdout: {stdout}");
    assert!(
        stdout.contains("cherry"),
        "expected cherry, stdout: {stdout}"
    );
}

#[test]
fn max_spread_plus_extra_scalar() {
    let src = r#"
agent A {
    @on_start {
        scores = [4, 9, 2, 7]
        result = max(...scores, 99)
        Io.show("{result}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("99"), "expected 99, stdout: {stdout}");
}

#[test]
fn min_multi_spread() {
    let src = r#"
agent A {
    @on_start {
        a = [4, 9]
        b = [2, 7]
        result = min(...a, ...b)
        Io.show("{result}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("2"), "expected 2, stdout: {stdout}");
}

#[test]
fn spread_on_fixed_arity_task_is_runtime_error() {
    let src = r#"
task greet(name: str) -> str { name }
agent A {
    @on_start {
        xs = ["Alice", "Bob"]
        greet(...xs)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error for spread on fixed-arity task");
    assert!(
        stderr.contains("spread") || stderr.contains("variadic"),
        "expected spread/variadic error:\n{stderr}"
    );
}
