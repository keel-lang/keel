use crate::common::*;
use std::io::Write as _;
use std::process::Command;

// ---------------------------------------------------------------------------
// v0.1.4 parser hardening — one test per feature
// ---------------------------------------------------------------------------

#[test]
fn if_expr_on_rhs_of_binding() {
    let src = r#"
agent A {
    @on_start {
        score = 0.9
        label = if score > 0.8 { "high" } else { "low" }
        Io.show(label)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("high"),
        "expected 'high' branch, stdout:\n{stdout}"
    );
}

#[test]
fn if_expr_else_branch_selected() {
    let src = r#"
agent A {
    @on_start {
        score = 0.3
        label = if score > 0.8 { "high" } else { "low" }
        Io.show(label)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("low"),
        "expected 'low' branch, stdout:\n{stdout}"
    );
}

#[test]
fn let_annotation_valid_runs() {
    let src = r#"
agent A {
    @on_start {
        greeting: str = "hello annotated"
        Io.show(greeting)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("hello annotated"), "stdout:\n{stdout}");
}

#[test]
fn null_assert_unwraps_non_none_value() {
    let src = r#"
agent A {
    @on_start {
        x = "present"
        val = x!
        Io.show(val)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("present"), "stdout:\n{stdout}");
}

#[test]
fn null_assert_on_none_raises_runtime_error() {
    let src = r#"
agent A {
    @on_start {
        x = none
        val = x!
        Io.show(val)
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

#[test]
fn list_concat_with_plus() {
    let src = r#"
agent A {
    @on_start {
        a = ["x", "y"]
        b = ["z"]
        all = a + b
        for item in all {
            Io.show(item)
        }
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("x"), "stdout:\n{stdout}");
    assert!(stdout.contains("y"), "stdout:\n{stdout}");
    assert!(stdout.contains("z"), "stdout:\n{stdout}");
}

#[test]
fn list_push_returns_extended_list() {
    let src = r#"
agent A {
    @on_start {
        items = ["a", "b"]
        items = items.push("c")
        for item in items {
            Io.show(item)
        }
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("a"), "stdout:\n{stdout}");
    assert!(stdout.contains("b"), "stdout:\n{stdout}");
    assert!(stdout.contains("c"), "stdout:\n{stdout}");
}

#[test]
fn string_interp_method_call() {
    let src = r#"
agent A {
    @on_start {
        items = [1, 2, 3]
        msg = "size={items.count()}"
        Io.show(msg)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("size=3"),
        "expected 'size=3' in stdout:\n{stdout}"
    );
}

#[test]
fn string_interp_binary_expr() {
    let src = r#"
agent A {
    @on_start {
        x = 5
        msg = "doubled={x * 2}"
        Io.show(msg)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("doubled=10"),
        "expected 'doubled=10' in stdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// v0.1.6 — wiring & ergonomics
// ---------------------------------------------------------------------------

#[test]
fn nested_string_in_interpolation_slot() {
    let src = r#"
agent A {
    @on_start {
        score = 0.9
        msg = "label={if score > 0.8 { "high" } else { "low" }}"
        Io.show(msg)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("label=high"),
        "expected 'label=high' in:\n{stdout}"
    );
}

#[test]
fn nested_string_double_layer() {
    let src = r#"
agent A {
    @on_start {
        x = "world"
        msg = "hi {"there {x}"}"
        Io.show(msg)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("hi there world"),
        "expected nested interp resolution:\n{stdout}"
    );
}

#[test]
fn map_get_method_inferred_as_nullable_value() {
    // map.get returns T?, so assigning to a non-nullable should fail check.
    let bin = keel_binary();
    let src = r#"
task t() {
    m: map[str, int] = {a: 1}
    n: int = m.get("a")
}
"#;
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write");
    let path = tmp.path().to_owned();
    let output = Command::new(&bin)
        .arg("check")
        .arg(&path)
        .output()
        .expect("run keel check");
    assert!(
        !output.status.success(),
        "expected check to fail on map.get assignment"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("int?") || combined.contains("nullable"),
        "expected nullable-mismatch diagnostic:\n{combined}"
    );
}

#[test]
fn map_keys_method_inferred_as_list_of_keys() {
    let src = r#"
agent A {
    @on_start {
        m: map[str, int] = {a: 1, b: 2}
        ks: list[str] = m.keys()
        Io.show("keys-count={ks.count()}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("keys-count=2"),
        "expected 2 keys:\n{stdout}"
    );
}

#[test]
fn map_float_key_rejected_at_compile_time() {
    let src = r#"
task t() {
    m: map[float, str] = {a: "x"}
}
t()
"#;
    let (ok, stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected type error for float map key");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("float") && combined.contains("NaN"),
        "expected float/NaN diagnostic:\n{combined}"
    );
}

#[test]
fn map_nullable_key_rejected_at_compile_time() {
    let src = r#"
task t() {
    m: map[str?, int] = {a: 1}
}
t()
"#;
    let (ok, stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected type error for nullable map key");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("nullable"),
        "expected nullable diagnostic:\n{combined}"
    );
}

#[test]
fn map_struct_key_rejected_at_compile_time() {
    let src = r#"
type Point { x: int, y: int }
task t() {
    m: map[Point, str] = {a: "x"}
}
t()
"#;
    let (ok, stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected type error for struct map key");
    let combined = format!("{stdout}{stderr}");
    assert!(
        combined.contains("Hashable") || combined.contains("struct"),
        "expected Hashable/struct diagnostic:\n{combined}"
    );
}

#[test]
fn map_int_key_literal_parses_and_runs() {
    let src = r#"
agent A {
    @on_start {
        m: map[int, str] = {1: "one", 2: "two"}
        v = m[1]
        Io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
    assert!(stdout.contains("one"), "expected 'one', got:\n{stdout}");
}

#[test]
fn map_bool_key_literal_parses_and_runs() {
    let src = r#"
agent A {
    @on_start {
        m: map[bool, str] = {true: "on", false: "off"}
        v = m[true]
        Io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
    assert!(stdout.contains("on"), "expected 'on', got:\n{stdout}");
}

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

// ---------------------------------------------------------------------------
// v0.1.12 — Range operator `..`
// ---------------------------------------------------------------------------

#[test]
fn range_basic_for_loop() {
    let src = r#"
agent A {
  @on_start {
    for i in 1..3 {
      Io.show("{i}")
    }
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains('1'), "expected 1 in output:\n{stdout}");
    assert!(stdout.contains('2'), "expected 2 in output:\n{stdout}");
    assert!(stdout.contains('3'), "expected 3 in output:\n{stdout}");
}

#[test]
fn range_assigned_to_variable() {
    let src = r#"
agent A {
  @on_start {
    xs = 1..4
    Io.show("{xs.count()}")
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
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
agent A {
  @on_start {
    xs = 5..3
    Io.show("{xs.count()}")
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains('0'),
        "expected empty range to have count 0:\n{stdout}"
    );
}

#[test]
fn range_single() {
    let src = r#"
agent A {
  @on_start {
    xs = 4..4
    Io.show("{xs.count()}")
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains('1'),
        "expected single-element range to have count 1:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// v0.1.13 — Destructuring
// ---------------------------------------------------------------------------

#[test]
fn destruct_struct_shorthand() {
    let src = r#"
agent A {
  @on_start {
    val = {name: "alice", age: 30}
    {name, age} = val
    Io.show("{name}:{age}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("alice:30"),
        "destructure shorthand failed:\n{stdout}"
    );
}

#[test]
fn destruct_struct_rename() {
    let src = r#"
agent A {
  @on_start {
    val = {urgency: "high", category: "bug"}
    {urgency: u, category: c} = val
    Io.show("{u}:{c}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("high:bug"),
        "destructure rename failed:\n{stdout}"
    );
}

#[test]
fn destruct_tuple() {
    let src = r#"
agent A {
  @on_start {
    pair = ("alpha", 42)
    (label, count) = pair
    Io.show("{label}:{count}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("alpha:42"),
        "tuple destructure failed:\n{stdout}"
    );
}

#[test]
fn destruct_in_for_loop() {
    let src = r#"
agent A {
  @on_start {
    items = [
      {name: "alice", score: 10},
      {name: "bob", score: 20},
    ]
    for {name, score} in items {
      Io.show("{name}={score}")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("alice=10"),
        "for-loop destructure failed:\n{stdout}"
    );
    assert!(
        stdout.contains("bob=20"),
        "for-loop destructure failed:\n{stdout}"
    );
}

#[test]
fn destruct_in_task_param() {
    let src = r#"
type Point = {x: int, y: int}

task show_point({x, y}: Point) {
  Io.show("{x},{y}")
}

agent A {
  @on_start {
    show_point({x: 3, y: 7})
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("3,7"),
        "task param destructure failed:\n{stdout}"
    );
}

#[test]
fn destruct_missing_field_type_error() {
    let src = r#"
agent A {
  @on_start {
    val = {name: "alice"}
    {name, nonexistent} = val
    Io.show("{name}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should fail: nonexistent field in destructure");
    assert!(
        stderr.contains("nonexistent"),
        "error should mention the missing field:\n{stderr}"
    );
}

#[test]
fn destruct_tuple_arity_mismatch_type_error() {
    let src = r#"
agent A {
  @on_start {
    triple = (1, 2, 3)
    (a, b) = triple
    Io.show("{a}:{b}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should fail: tuple arity mismatch");
    assert!(
        stderr.contains("tuple") || stderr.contains("element"),
        "error should mention tuple arity:\n{stderr}"
    );
}

#[test]
fn destruct_keyword_field_from() {
    let src = r#"
agent A {
  @on_start {
    email = {from: "alice@example.com", subject: "hello"}
    {from, subject} = email
    Io.show("{from}:{subject}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("alice@example.com:hello"),
        "keyword field 'from' destructure failed:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// v0.1.14 — if guards (for loops and when arms)
// ---------------------------------------------------------------------------

#[test]
fn if_guard_for_filters_elements() {
    let src = r#"
agent A {
  @on_start {
    nums = [1, 2, 3, 4, 5]
    for n in nums if n % 2 == 0 {
      Io.show("even:{n}")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
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
agent A {
  @on_start {
    for x in 1..5 if x != 3 {
      Io.show("x:{x}")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
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
type Status = active | inactive
agent A {
  @on_start {
    s = Status.active
    level = 5
    when s {
      active where level > 3 => Io.show("admin-active")
      active                 => Io.show("user-active")
      _                      => Io.show("inactive")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("admin-active"),
        "guard should match:\n{stdout}"
    );
}

#[test]
fn when_arm_where_guard_falls_through() {
    let src = r#"
type Status = active | inactive
agent A {
  @on_start {
    s = Status.active
    level = 1
    when s {
      active where level > 3 => Io.show("admin-active")
      active                 => Io.show("user-active")
      _                      => Io.show("inactive")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("user-active"),
        "guard false should fall through:\n{stdout}"
    );
    assert!(
        !stdout.contains("admin-active"),
        "admin branch should not fire:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// List operations — any, all, find, reduce, sum, min, max, join, sort,
//                   reverse, flatten, take, skip
// ---------------------------------------------------------------------------

#[test]
fn list_any_returns_true_when_predicate_matches() {
    let src = r#"
agent A {
  @on_start {
    nums = [1, 5, 10, 15]
    Io.show("{nums.any(n => n > 8)}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("true"), "any: {stdout}");
}

#[test]
fn list_all_returns_false_when_one_fails() {
    let src = r#"
agent A {
  @on_start {
    nums = [1, 5, 10, 15]
    Io.show("{nums.all(n => n > 8)}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("false"), "all: {stdout}");
}

#[test]
fn list_find_returns_first_match_or_none() {
    let src = r#"
agent A {
  @on_start {
    nums = [3, 7, 12, 20]
    found = nums.find(n => n > 10)
    Io.show("{found}")
    missing = nums.find(n => n > 100)
    Io.show("{missing}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("12"), "find match: {stdout}");
    assert!(stdout.contains("none"), "find none: {stdout}");
}

#[test]
fn list_reduce_sums_with_accumulator() {
    let src = r#"
agent A {
  @on_start {
    nums = [1, 2, 3, 4, 5]
    total = nums.reduce((acc, x) => acc + x, 0)
    Io.show("{total}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("15"), "reduce: {stdout}");
}

#[test]
fn list_sum_min_max_on_integers() {
    let src = r#"
agent A {
  @on_start {
    nums = [4, 1, 9, 2, 7]
    Io.show("{nums.sum()}")
    Io.show("{nums.min()}")
    Io.show("{nums.max()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("23"), "sum: {stdout}");
    assert!(stdout.contains("1"), "min: {stdout}");
    assert!(stdout.contains("9"), "max: {stdout}");
}

#[test]
fn list_min_max_with_by_key() {
    let src = r#"
type Item { name: str, score: int }
task run_test() {
  items: list[Item] = [
    { name: "b", score: 5 },
    { name: "a", score: 1 },
    { name: "c", score: 9 },
  ]
  Io.show(items.min(by: x => x.score).name)
  Io.show(items.max(by: x => x.score).name)
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    let names: Vec<&str> = stdout.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    assert_eq!(names, vec!["a", "c"], "min/max by key: {stdout}");
}

#[test]
fn list_join_produces_delimited_string() {
    let src = r#"
agent A {
  @on_start {
    tags = ["a", "b", "c"]
    Io.show("{tags.join(", ")}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("a, b, c"), "join: {stdout}");
}

#[test]
fn list_sort_and_reverse() {
    let src = r#"
agent A {
  @on_start {
    nums = [3, 1, 4, 1, 5]
    Io.show("{nums.sort().join(" ")}")
    Io.show("{nums.sort().reverse().first()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("1 1 3 4 5"), "sort: {stdout}");
    assert!(stdout.contains("5"), "reverse first: {stdout}");
}

#[test]
fn list_sort_by_int_key() {
    let src = r#"
agent A {
  @on_start {
    nums = [3, 1, 4, 1, 5, 9, 2]
    sorted = nums.sort(by: x => x)
    Io.show(sorted.join(" "))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("1 1 2 3 4 5 9"), "sort_by identity: {stdout}");
}

#[test]
fn list_sort_by_field() {
    let src = r#"
type Item { name: str, score: int }
task run_test() {
  items: list[Item] = [
    { name: "c", score: 3 },
    { name: "a", score: 1 },
    { name: "b", score: 2 },
  ]
  sorted = items.sort(by: x => x.score)
  for item in sorted {
    Io.show(item.name)
  }
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    let names: Vec<&str> = stdout.lines().map(|l| l.trim()).filter(|l| !l.is_empty()).collect();
    assert_eq!(names, vec!["a", "b", "c"], "sort_by score: {stdout}");
}

#[test]
fn list_sort_by_string_key() {
    let src = r#"
agent A {
  @on_start {
    words = ["banana", "apple", "cherry"]
    sorted = words.sort(by: w => w)
    Io.show(sorted.join(" "))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("apple banana cherry"), "sort_by str: {stdout}");
}

#[test]
fn list_sort_by_descending_via_negation() {
    let src = r#"
agent A {
  @on_start {
    nums = [3, 1, 4, 1, 5]
    sorted = nums.sort(by: x => 0 - x)
    Io.show(sorted.join(" "))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("5 4 3 1 1"), "sort_by desc: {stdout}");
}

#[test]
fn list_sort_by_empty_list_is_ok() {
    let src = r#"
agent A {
  @on_start {
    nums = [1]
    empty = nums.filter(x => x > 999)
    Io.show("{empty.sort(by: x => x).count()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('0'), "empty sort_by: {stdout}");
}

#[test]
fn list_sort_by_non_function_arg_is_error() {
    let src = r#"
agent A {
  @on_start {
    nums = [3, 1, 2]
    Io.show("{nums.sort(by: 42).count()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, true);
    assert!(!ok, "expected runtime error for non-function by: arg");
    assert!(
        stderr.contains("must be a function"),
        "expected 'must be a function' in stderr:\n{stderr}"
    );
}

#[test]
fn list_sort_by_invalid_key_type_is_error() {
    let src = r#"
type Item { name: str, flag: bool }
task run_test() {
  items: list[Item] = [{ name: "a", flag: true }, { name: "b", flag: false }]
  items.sort(by: x => x.flag)
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error for bool key type");
    assert!(
        stderr.contains("key function must return int, float, or str"),
        "expected key type error in stderr:\n{stderr}"
    );
}

#[test]
fn list_flatten_merges_nested_lists() {
    let src = r#"
agent A {
  @on_start {
    nested = [[1, 2], [3], [4, 5]]
    Io.show("{nested.flatten().join(" ")}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("1 2 3 4 5"), "flatten: {stdout}");
}

#[test]
fn list_take_and_skip() {
    let src = r#"
agent A {
  @on_start {
    nums = [10, 20, 30, 40, 50]
    Io.show("{nums.take(3).join(" ")}")
    Io.show("{nums.skip(3).join(" ")}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("10 20 30"), "take: {stdout}");
    assert!(stdout.contains("40 50"), "skip: {stdout}");
}

// ---------------------------------------------------------------------------
// List zip (deferred — open design questions)
// ---------------------------------------------------------------------------

#[test]
#[ignore]
fn list_zip_pairs_elements() {
    let src = r#"
agent A {
  @on_start {
    a = [1, 2, 3]
    b = ["x", "y", "z"]
    pairs = a.zip(b)
    Io.show("{pairs.len()}")
    Io.show("{pairs[1][0]}")
    Io.show("{pairs[1][1]}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains('3'), "len: {stdout}");
    assert!(stdout.contains('2'), "pair[1][0]: {stdout}");
    assert!(stdout.contains('y'), "pair[1][1]: {stdout}");
}

#[test]
#[ignore]
fn list_zip_stops_at_shorter_list() {
    let src = r#"
agent A {
  @on_start {
    pairs = [1, 2, 3].zip(["a", "b"])
    Io.show("{pairs.len()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains('2'), "should stop at shorter: {stdout}");
}

#[test]
#[ignore]
fn list_zip_destructuring_in_for_loop() {
    let src = r#"
agent A {
  @on_start {
    names = ["alice", "bob"]
    scores = [90, 85]
    for (name, score) in names.zip(scores) {
        Io.show("{name}:{score}")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("alice:90"), "destructure: {stdout}");
    assert!(stdout.contains("bob:85"), "destructure: {stdout}");
}

// ---------------------------------------------------------------------------
// String methods — repeat, slice, index_of, trim_start, trim_end,
//                  to_int, to_float
// ---------------------------------------------------------------------------

#[test]
fn string_repeat_produces_n_copies() {
    let src = r#"
agent A {
  @on_start {
    Io.show("{"ha".repeat(3)}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("hahaha"), "repeat: {stdout}");
}

#[test]
fn string_slice_extracts_chars() {
    let src = r#"
agent A {
  @on_start {
    Io.show("{"hello world".slice(6, 11)}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("world"), "slice: {stdout}");
}

#[test]
fn string_index_of_returns_position_or_none() {
    let src = r#"
agent A {
  @on_start {
    Io.show("{"hello world".index_of("world")}")
    Io.show("{"hello world".index_of("xyz")}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("6"), "index_of found: {stdout}");
    assert!(stdout.contains("none"), "index_of miss: {stdout}");
}

#[test]
fn string_trim_start_and_trim_end() {
    let src = r#"
agent A {
  @on_start {
    s = "  hi  "
    Io.show("{s.trim_start()}")
    Io.show("{s.trim_end()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("hi  "), "trim_start: {stdout}");
    assert!(stdout.contains("  hi"), "trim_end: {stdout}");
}

#[test]
fn string_to_int_parses_or_returns_none() {
    let src = r#"
agent A {
  @on_start {
    Io.show("{"42".to_int()}")
    Io.show("{"bad".to_int()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("42"), "to_int parse: {stdout}");
    assert!(stdout.contains("none"), "to_int fail: {stdout}");
}

#[test]
fn string_to_float_parses_or_returns_none() {
    let src = r#"
agent A {
  @on_start {
    Io.show("{"3.14".to_float()}")
    Io.show("{"nope".to_float()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("3.14"), "to_float parse: {stdout}");
    assert!(stdout.contains("none"), "to_float fail: {stdout}");
}

// ── Regression: modulo by zero (B1) ──────────────────────────────────────────

#[test]
fn modulo_by_zero_is_runtime_error() {
    let src = r#"
agent A {
    @on_start {
        x = 5 % 0
        Io.show(x)
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

// ── Regression: return inside expression-position if/when (B2) ───────────────

#[test]
fn return_inside_if_expr_propagates_out_of_task() {
    let src = r#"
task classify(n: int) -> str {
    label = if n > 0 { return "positive" } else { "other" }
    label
}
agent A {
    @on_start {
        Io.show(classify(5))
        Io.show(classify(-1))
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program exited non-zero\nstderr: {stderr}");
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
task classify(n: int) -> str {
    label = if n > 0 { "positive" } else { return "non-positive" }
    label
}
agent A {
    @on_start {
        Io.show(classify(5))
        Io.show(classify(-3))
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program exited non-zero\nstderr: {stderr}");
    assert!(
        stdout.contains("positive"),
        "then branch should yield 'positive':\n{stdout}"
    );
    assert!(
        stdout.contains("non-positive"),
        "early return inside else-expr should yield 'non-positive':\n{stdout}"
    );
}

// ── Regression: named args for user-defined tasks (S1) ───────────────────────

#[test]
fn named_args_bind_by_label_for_user_tasks() {
    let src = r#"
task greet(greeting: str, name: str) -> str {
    "{greeting}, {name}!"
}
agent A {
    @on_start {
        Io.show(greet(name: "Alice", greeting: "Hello"))
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program exited non-zero\nstderr: {stderr}");
    assert!(
        stdout.contains("Hello, Alice!"),
        "named args should bind by label regardless of call order:\n{stdout}"
    );
}

#[test]
fn mixed_named_and_positional_args() {
    let src = r#"
task add(a: int, b: int, c: int) -> int {
    a + b + c
}
agent A {
    @on_start {
        Io.show("{add(1, c: 30, b: 20)}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program exited non-zero\nstderr: {stderr}");
    assert!(
        stdout.contains("51"),
        "mixed named+positional should sum to 51:\n{stdout}"
    );
}

// ── Regression: @limits unimplemented fields raise an error (S3) ─────────────

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

// ── when as expression ────────────────────────────────────────────────────────

#[test]
fn when_expr_evaluates_to_matched_arm_value() {
    let src = r#"
task grade(score: str) -> str {
  when score {
    "A" => "excellent"
    "B" => "good"
    _   => "needs work"
  }
}

Io.show(grade("A"))
Io.show(grade("B"))
Io.show(grade("C"))
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\nstderr: {stderr}");
    assert!(stdout.contains("excellent"), "stdout: {stdout}");
    assert!(stdout.contains("good"), "stdout: {stdout}");
    assert!(stdout.contains("needs work"), "stdout: {stdout}");
}

#[test]
fn when_expr_result_assigned_to_variable() {
    let src = r#"
task label(n: int) -> str {
  result = when n {
    0 => "zero"
    1 => "one"
    _ => "many"
  }
  result
}

Io.show(label(0))
Io.show(label(1))
Io.show(label(5))
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\nstderr: {stderr}");
    assert!(stdout.contains("zero"), "stdout: {stdout}");
    assert!(stdout.contains("one"), "stdout: {stdout}");
    assert!(stdout.contains("many"), "stdout: {stdout}");
}

// ---------------------------------------------------------------------------
// Augmented assignment (+=, -=, *=, /=)
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
    assert!(stdout.contains("15"), "expected 15, stdout: {stdout}");
}

// ---------------------------------------------------------------------------
// raise
// ---------------------------------------------------------------------------

#[test]
fn raise_string_is_caught_by_error() {
    let src = r#"
try {
    raise "something went wrong"
} catch err: Error {
    Io.show("caught: {err.message}")
}
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\nstderr: {stderr}");
    assert!(
        stdout.contains("caught: something went wrong"),
        "expected catch message, stdout: {stdout}"
    );
}

#[test]
fn raise_stops_execution_in_block() {
    let src = r#"
try {
    Io.show("before")
    raise "stop"
    Io.show("after")
} catch err: Error {
    Io.show("caught")
}
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\nstderr: {stderr}");
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
task validate(x: int) {
    if x < 0 {
        raise "x must be non-negative"
    }
}

try {
    validate(-1)
} catch err: Error {
    Io.show("task raised: {err.message}")
}
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\nstderr: {stderr}");
    assert!(
        stdout.contains("task raised: x must be non-negative"),
        "stdout: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// break / continue
// ---------------------------------------------------------------------------

#[test]
fn break_exits_loop_early() {
    let src = r#"
agent A {
    @on_start {
        count = 0
        for i in 1..10 {
            if i > 3 {
                break
            }
            count += 1
        }
        Io.show("{count}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\nstderr: {stderr}");
    assert!(stdout.contains("3"), "expected count=3, stdout: {stdout}");
}

#[test]
fn continue_skips_current_iteration() {
    let src = r#"
agent A {
    @on_start {
        sum = 0
        for i in 1..6 {
            if i == 3 {
                continue
            }
            sum += i
        }
        Io.show("{sum}")
    }
}
run(A)
"#;
    // 1+2+4+5+6 = 18 (3 is skipped)
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\nstderr: {stderr}");
    assert!(stdout.contains("18"), "expected sum=18, stdout: {stdout}");
}

#[test]
fn break_inside_if_stmt_in_loop() {
    // Verifies break inside an `if` inside a `for` exits the loop.
    // Items before 99 are counted; break fires at 99 so count stays at 2.
    let src = r#"
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
        Io.show("{count}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\nstderr: {stderr}");
    assert!(stdout.contains("2"), "expected count=2, stdout: {stdout}");
}

#[test]
fn continue_and_break_together() {
    let src = r#"
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
        Io.show("{result}")
    }
}
run(A)
"#;
    // odd numbers 1..6: 1+3+5 = 9 (7 triggers break, evens are skipped)
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\nstderr: {stderr}");
    assert!(stdout.contains("9"), "expected result=9, stdout: {stdout}");
}

#[test]
fn break_in_nested_loop_only_exits_inner() {
    let src = r#"
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
        Io.show("{outer}")
    }
}
run(A)
"#;
    // outer loop runs 3 times; inner break only exits the inner loop
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\nstderr: {stderr}");
    assert!(stdout.contains("3"), "expected outer=3, stdout: {stdout}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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
    assert!(ok, "expected success:\nstderr: {stderr}");
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

// ─── Subscript access (`list[i]`, `str[i]`) ─────────────────────────────────

#[test]
fn subscript_list_in_bounds() {
    let src = r#"
agent A {
    @on_start {
        items = [10, 20, 30]
        v = items[1]
        Io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
    assert!(stdout.contains("20"), "expected 20, got:\n{stdout}");
}

#[test]
fn subscript_list_out_of_bounds_errors() {
    let src = r#"
agent A {
    @on_start {
        items = [10, 20, 30]
        v = items[99]
        Io.show(v)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error on OOB");
    assert!(
        stderr.contains("out of bounds"),
        "expected 'out of bounds' error, got:\n{stderr}"
    );
}

#[test]
fn subscript_list_negative_errors() {
    let src = r#"
agent A {
    @on_start {
        items = [10, 20, 30]
        v = items[-1]
        Io.show(v)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error on negative index");
    assert!(
        stderr.contains("out of bounds"),
        "expected 'out of bounds' error, got:\n{stderr}"
    );
}

#[test]
fn subscript_string_in_bounds() {
    let src = r#"
agent A {
    @on_start {
        word = "hello"
        ch = word[1]
        Io.show(ch)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
    assert!(stdout.contains("e"), "expected 'e', got:\n{stdout}");
}

#[test]
fn subscript_string_out_of_bounds_errors() {
    let src = r#"
agent A {
    @on_start {
        word = "hi"
        ch = word[99]
        Io.show(ch)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error on OOB string index");
    assert!(
        stderr.contains("out of bounds"),
        "expected 'out of bounds' error, got:\n{stderr}"
    );
}

#[test]
fn subscript_list_first_element() {
    let src = r#"
agent A {
    @on_start {
        items = ["alpha", "beta", "gamma"]
        v = items[0]
        Io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
    assert!(stdout.contains("alpha"), "expected 'alpha', got:\n{stdout}");
}

// ─── Map subscript (v0.1.27) ─────────────────────────────────────────────────

#[test]
fn subscript_map_hit() {
    let src = r#"
agent A {
    @on_start {
        scores: map[str, int] = {alice: 90, bob: 85}
        v = scores["alice"]
        Io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
    assert!(stdout.contains("90"), "expected 90, got:\n{stdout}");
}

#[test]
fn subscript_map_miss_returns_none() {
    let src = r#"
agent A {
    @on_start {
        scores: map[str, int] = {alice: 90}
        v = scores["nobody"]
        Io.show(v)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
    assert!(stdout.contains("none"), "expected none for missing key, got:\n{stdout}");
}

// ─── while loop (v0.1.27) ────────────────────────────────────────────────────

#[test]
fn while_basic_countdown() {
    let src = r#"
agent A {
    @on_start {
        n = 3
        while n > 0 {
            Io.show("tick:{n}")
            n -= 1
        }
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
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
agent A {
    @on_start {
        count = 0
        while true {
            count += 1
            if count >= 3 {
                break
            }
        }
        Io.show("{count}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
    assert!(stdout.contains("3"), "expected count=3:\n{stdout}");
}

#[test]
fn while_continue_skips_iteration() {
    let src = r#"
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
        Io.show("{sum}")
    }
}
run(A)
"#;
    // odd numbers 1,3,5 => sum = 9
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
    assert!(stdout.contains("9"), "expected sum=9:\n{stdout}");
}

#[test]
fn while_nested_in_for() {
    let src = r#"
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
        Io.show("{total}")
    }
}
run(A)
"#;
    // i=1: 1 iter, i=2: 2 iters, i=3: 3 iters => total=6
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstderr: {stderr}");
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

// ---------------------------------------------------------------------------
// Stringable — impl Stringable for Type
// ---------------------------------------------------------------------------

#[test]
fn impl_stringable_interpolates_via_to_str() {
    let src = r#"
type Point {
  x: int
  y: int
}

impl Stringable for Point {
  task to_str(self) -> str {
    "({self.x}, {self.y})"
  }
}

task run_test() {
  p: Point = { x: 3, y: 4 }
  Io.show("{p}")
}

run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("(3, 4)"),
        "expected '(3, 4)' in stdout:\n{stdout}"
    );
}

#[test]
fn enum_variant_interpolates_as_variant_name() {
    let src = r#"
type Signal = buy | sell | hold

agent A {
  @on_start {
    s: Signal = Signal.buy
    Io.show("{s}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok, "expected ok");
    assert!(stdout.contains("buy"), "expected 'buy' in stdout:\n{stdout}");
}

#[test]
fn impl_stringable_explicit_to_str_call() {
    let src = r#"
type Color {
  r: int
  g: int
  b: int
}

impl Stringable for Color {
  task to_str(self) -> str {
    "rgb({self.r}, {self.g}, {self.b})"
  }
}

task run_test() {
  c: Color = { r: 255, g: 128, b: 0 }
  Io.show(c.to_str())
}

run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("rgb(255, 128, 0)"),
        "expected 'rgb(255, 128, 0)' in stdout:\n{stdout}"
    );
}

#[test]
fn impl_stringable_multiple_types() {
    let src = r#"
type Celsius { value: float }
type Fahrenheit { value: float }

impl Stringable for Celsius {
  task to_str(self) -> str {
    "{self.value}°C"
  }
}

impl Stringable for Fahrenheit {
  task to_str(self) -> str {
    "{self.value}°F"
  }
}

task run_test() {
  hot: Celsius    = { value: 37.0 }
  cold: Fahrenheit = { value: 32.0 }
  Io.show("{hot}")
  Io.show("{cold}")
}

run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("37"), "expected Celsius output:\n{stdout}");
    assert!(
        stdout.contains("32"),
        "expected Fahrenheit output:\n{stdout}"
    );
}

#[test]
fn primitives_still_interpolate_without_impl() {
    let src = r#"
task run_test() {
  n = 42
  f = 3.14
  b = true
  Io.show("{n} {f} {b}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("42") && stdout.contains("3.14") && stdout.contains("true"),
        "primitives: {stdout}"
    );
}

#[test]
fn user_defined_interface_and_impl() {
    let src = r#"
interface Greetable {
  task greet(self) -> str
}

type Person {
  name: str
}

impl Greetable for Person {
  task greet(self) -> str {
    "Hello, {self.name}!"
  }
}

task run_test() {
  p: Person = { name: "Alice" }
  Io.show(p.greet())
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("Hello, Alice!"), "got: {stdout}");
}

#[test]
fn impl_unknown_interface_is_an_error() {
    let src = r#"
type Dog { name: str }
impl Unknown for Dog {
  task bark(self) -> str { "Woof" }
}
task run_test() { Io.show("x") }
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed");
    assert!(
        stderr.contains("unknown interface") || stderr.contains("Unknown"),
        "expected unknown-interface error, got: {stderr}"
    );
}

#[test]
fn impl_missing_required_method_is_an_error() {
    let src = r#"
interface Describable {
  task describe(self) -> str
  task short(self) -> str
}
type Item { label: str }
impl Describable for Item {
  task describe(self) -> str { self.label }
}
task run_test() { Io.show("x") }
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed");
    assert!(
        stderr.contains("missing") || stderr.contains("short"),
        "expected missing-method error, got: {stderr}"
    );
}

#[test]
fn impl_extra_method_not_in_interface_is_an_error() {
    let src = r#"
interface Labeled {
  task label(self) -> str
}
type Tag { value: str }
impl Labeled for Tag {
  task label(self) -> str { self.value }
  task extra(self) -> str { "oops" }
}
task run_test() { Io.show("x") }
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed");
    assert!(
        stderr.contains("not part of interface") || stderr.contains("extra"),
        "expected extra-method error, got: {stderr}"
    );
}

#[test]
fn impl_wrong_return_type_is_an_error() {
    let src = r#"
interface Scorer {
  task score(self) -> int
}
type Game { pts: int }
impl Scorer for Game {
  task score(self) -> str { "oops" }
}
task run_test() { Io.show("x") }
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed");
    assert!(
        stderr.contains("return") || stderr.contains("score") || stderr.contains("str"),
        "expected return-type mismatch error, got: {stderr}"
    );
}

#[test]
fn interface_declared_after_impl_still_works() {
    let src = r#"
type Square { side: int }
impl Sizable for Square {
  task size(self) -> int { self.side * self.side }
}
interface Sizable {
  task size(self) -> int
}
task run_test() {
  b: Square = { side: 4 }
  Io.show("{b.size()}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("16"), "got: {stdout}");
}

#[test]
fn serializable_to_json_used_by_json_stringify() {
    let src = r#"
type Event { name: str, score: int }
impl Serializable for Event {
  task to_json(self) -> str {
    "name={self.name};score={self.score}"
  }
}
task run_test() {
  e: Event = { name: "goal", score: 3 }
  Io.show(Json.stringify(e))
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("goal"), "got: {stdout}");
    assert!(stdout.contains('3'), "score: {stdout}");
}

#[test]
fn equatable_equals_method_is_callable() {
    let src = r#"
type Point { x: int, y: int }
impl Equatable for Point {
  task equals(self, other: Point) -> bool {
    self.x == other.x and self.y == other.y
  }
}
task run_test() {
  a: Point = { x: 1, y: 2 }
  b: Point = { x: 1, y: 2 }
  c: Point = { x: 9, y: 0 }
  Io.show("{a.equals(b)}")
  Io.show("{a.equals(c)}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("true"), "equals true: {stdout}");
    assert!(stdout.contains("false"), "equals false: {stdout}");
}

#[test]
fn comparable_sort_orders_structs_ascending() {
    let src = r#"
type Score { val: int }
impl Comparable for Score {
  task compare(self, other: Score) -> int {
    self.val - other.val
  }
}
task run_test() {
  items = [{ val: 30 }, { val: 10 }, { val: 20 }]
  sorted = items.sort()
  for s in sorted {
    Io.show("{s.val}")
  }
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    let lines: Vec<&str> = stdout.lines().collect();
    let vals: Vec<&str> = lines
        .iter()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(vals, vec!["10", "20", "30"], "sorted: {stdout}");
}

#[test]
fn comparable_min_max_on_structs() {
    let src = r#"
type Score { val: int }
impl Comparable for Score {
  task compare(self, other: Score) -> int {
    self.val - other.val
  }
}
task run_test() {
  items = [{ val: 30 }, { val: 10 }, { val: 20 }]
  lo = items.min()
  hi = items.max()
  Io.show("{lo.val}")
  Io.show("{hi.val}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("10"), "min: {stdout}");
    assert!(stdout.contains("30"), "max: {stdout}");
}

#[test]
fn iterable_items_used_in_for_loop() {
    let src = r#"
type Range { lo: int, hi: int }
impl Iterable for Range {
  task items(self) -> list[int] {
    result: list[int] = []
    i = self.lo
    while i <= self.hi {
      result += [i]
      i += 1
    }
    result
  }
}
task run_test() {
  r: Range = { lo: 1, hi: 4 }
  for n in r {
    Io.show("{n}")
  }
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('1'), "1: {stdout}");
    assert!(stdout.contains('2'), "2: {stdout}");
    assert!(stdout.contains('3'), "3: {stdout}");
    assert!(stdout.contains('4'), "4: {stdout}");
}

#[test]
fn builtin_interfaces_cannot_be_redeclared() {
    for iface in [
        "Stringable",
        "Comparable",
        "Equatable",
        "Serializable",
        "Iterable",
    ] {
        let src = format!(
            "interface {iface} {{ task dummy(self) -> str }}\ntask run_test() {{ Io.show(\"ok\") }}\nrun_test()"
        );
        let (ok, _stdout, stderr) = run_inline(&src, false);
        assert!(!ok, "{iface} should be rejected");
        assert!(
            stderr.contains("built-in"),
            "{iface}: expected 'built-in' in stderr, got: {stderr}"
        );
    }
}

#[test]
fn iterable_return_type_can_be_concrete_list() {
    let src = r#"
type Pair { a: int, b: int }
impl Iterable for Pair {
  task items(self) -> list[int] {
    [self.a, self.b]
  }
}
task run_test() {
  p: Pair = { a: 7, b: 8 }
  for n in p {
    Io.show("{n}")
  }
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('7'), "7: {stdout}");
    assert!(stdout.contains('8'), "8: {stdout}");
}

// ---------------------------------------------------------------------------
// Format specifiers in string interpolation
// ---------------------------------------------------------------------------

#[test]
fn format_spec_float_precision() {
    let src = r#"
agent A {
    @on_start {
        pi = 3.14159
        Io.show("{pi:.2f}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("3.14"),
        "expected '3.14' in stdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("3.1415"),
        "unexpected extra precision in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_int_as_float() {
    let src = r#"
agent A {
    @on_start {
        n = 42
        Io.show("{n:.2f}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("42.00"),
        "expected '42.00' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_right_align() {
    let src = r#"
agent A {
    @on_start {
        n = 7
        Io.show("{n:>5}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("    7"),
        "expected right-aligned '    7' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_left_align() {
    let src = r#"
agent A {
    @on_start {
        s = "hi"
        Io.show("{s:<6}!")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("hi    !"),
        "expected left-aligned 'hi    !' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_center_align() {
    let src = r#"
agent A {
    @on_start {
        s = "hi"
        Io.show("{s:^6}!")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("  hi  !"),
        "expected centered '  hi  !' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_combined_align_and_precision() {
    let src = r#"
agent A {
    @on_start {
        x = 3.14159
        Io.show("{x:>10.2f}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("      3.14"),
        "expected right-aligned '      3.14' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_named_arg_colon_not_confused_with_spec() {
    let src = r#"
task greet(name: str) -> str {
    "hello {name}"
}
agent A {
    @on_start {
        Io.show(greet(name: "world"))
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("hello world"),
        "expected 'hello world' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_bare_width_right_aligns() {
    let src = r#"
agent A {
    @on_start {
        n = 5
        Io.show("{n:4}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("   5"),
        "expected right-aligned '   5' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_alignment_respects_custom_to_str() {
    // Alignment specs must call to_str() via impl dispatch, not fall back to
    // to_display_string(), so {x:>10} and {x} produce the same base string.
    let src = r#"
type Tag { value: str }
impl Stringable for Tag {
    task to_str(self) -> str {
        "[{self.value}]"
    }
}
agent A {
    @on_start {
        t: Tag = { value: "ok" }
        Io.show("{t}")
        Io.show("{t:>8}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("[ok]"),
        "expected '[ok]' from bare interp:\n{stdout}"
    );
    assert!(
        stdout.contains("    [ok]"),
        "expected '    [ok]' from aligned interp (custom to_str must be respected):\n{stdout}"
    );
}

#[test]
fn format_spec_unknown_type_flag_is_runtime_error() {
    let src = r#"
agent A {
    @on_start {
        pi = 3.14
        Io.show("{pi:.2x}")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error for unknown format spec type");
    assert!(
        stderr.contains("unknown format spec type"),
        "expected 'unknown format spec type' in stderr:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// as T casts and typeof()
// ---------------------------------------------------------------------------

#[test]
fn cast_int_to_float() {
    let src = r#"
agent A {
  @on_start {
    x: int = 5
    Io.show("{x as float}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("5"), "got: {stdout}");
}

#[test]
fn cast_float_to_int_truncates() {
    let src = r#"
agent A {
  @on_start {
    Io.show("{1.9 as int}")
    Io.show("{-1.9 as int}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains('1'), "got: {stdout}");
    assert!(stdout.contains("-1"), "got: {stdout}");
}

#[test]
fn cast_int_to_str() {
    let src = r#"
agent A {
  @on_start {
    Io.show("{42 as str}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("42"), "got: {stdout}");
}

#[test]
fn cast_str_to_int() {
    let src = r#"
agent A {
  @on_start {
    n = "99" as int
    Io.show("{n}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("99"), "got: {stdout}");
}

#[test]
fn cast_str_to_float() {
    let src = r#"
agent A {
  @on_start {
    f = "3.14" as float
    Io.show("{f}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("3.14"), "got: {stdout}");
}

#[test]
fn cast_invalid_str_to_int_raises() {
    let src = r#"
agent A {
  @on_start {
    x = "abc" as int
    Io.show("{x}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, true);
    assert!(!ok, "expected runtime error");
    assert!(stderr.contains("cannot cast"), "got: {stderr}");
}

#[test]
fn cast_none_raises() {
    let src = r#"
agent A {
  @on_start {
    x = none as int
    Io.show("{x}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, true);
    assert!(!ok, "expected runtime error");
    assert!(stderr.contains("cannot cast none"), "got: {stderr}");
}

#[test]
fn typeof_primitives() {
    let src = r#"
agent A {
  @on_start {
    Io.show(typeof(42))
    Io.show(typeof(3.14))
    Io.show(typeof("hi"))
    Io.show(typeof(true))
    Io.show(typeof(none))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("int"), "got: {stdout}");
    assert!(stdout.contains("float"), "got: {stdout}");
    assert!(stdout.contains("str"), "got: {stdout}");
    assert!(stdout.contains("bool"), "got: {stdout}");
    assert!(stdout.contains("none"), "got: {stdout}");
}

#[test]
fn typeof_struct_returns_declared_name() {
    let src = r#"
type Point { x: int, y: int }

agent A {
  @on_start {
    p: Point = { x: 1, y: 2 }
    Io.show(typeof(p))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("Point"), "got: {stdout}");
}

#[test]
fn typeof_enum_returns_declared_name() {
    let src = r#"
type Color = red | green | blue

agent A {
  @on_start {
    c: Color = Color.red
    Io.show(typeof(c))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("Color"), "got: {stdout}");
}

#[test]
fn cast_bool_to_str() {
    let src = r#"
agent A {
  @on_start {
    Io.show("{true as str}")
    Io.show("{false as str}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("true"), "got: {stdout}");
    assert!(stdout.contains("false"), "got: {stdout}");
}

#[test]
fn cast_str_to_bool() {
    let src = r#"
agent A {
  @on_start {
    Io.show("{"true" as bool}")
    Io.show("{"false" as bool}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("true"), "got: {stdout}");
    assert!(stdout.contains("false"), "got: {stdout}");
}

#[test]
fn cast_str_to_bool_invalid_raises() {
    let src = r#"
agent A {
  @on_start {
    x = "yes" as bool
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, true);
    assert!(!ok, "expected error");
    assert!(stderr.contains("cannot cast"), "got: {stderr}");
}

#[test]
fn cast_same_type_identity() {
    let src = r#"
agent A {
  @on_start {
    Io.show("{42 as int}")
    Io.show("{"hi" as str}")
    Io.show("{3.14 as float}")
    Io.show("{true as bool}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("42"), "got: {stdout}");
    assert!(stdout.contains("hi"), "got: {stdout}");
    assert!(stdout.contains("3.14"), "got: {stdout}");
    assert!(stdout.contains("true"), "got: {stdout}");
}

#[test]
fn cast_uuid_to_str() {
    let src = r#"
agent A {
  @on_start {
    id = uuid()
    s = id as str
    Io.show(typeof(s))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("str"), "got: {stdout}");
}

#[test]
fn cast_str_to_uuid_valid() {
    let src = r#"
agent A {
  @on_start {
    id = "f47ac10b-58cc-4372-a567-0e02b2c3d479" as Uuid
    Io.show(typeof(id))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("Uuid"), "got: {stdout}");
}

#[test]
fn cast_str_to_uuid_invalid_raises() {
    let src = r#"
agent A {
  @on_start {
    id = "not-a-uuid" as Uuid
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, true);
    assert!(!ok, "expected error");
    assert!(stderr.contains("cannot cast"), "got: {stderr}");
}

#[test]
fn typeof_list_map_duration_uuid() {
    let src = r#"
agent A {
  @on_start {
    Io.show(typeof([1, 2, 3]))
    Io.show(typeof(1.s))
    Io.show(typeof(uuid()))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, _) = run_inline(src, true);
    assert!(ok);
    assert!(stdout.contains("list"), "got: {stdout}");
    assert!(stdout.contains("duration"), "got: {stdout}");
    assert!(stdout.contains("Uuid"), "got: {stdout}");
}

// ---------------------------------------------------------------------------
// Struct spread-update  { ...base, field: new }
// ---------------------------------------------------------------------------

#[test]
fn struct_spread_update_single_field() {
    let src = r#"
type Order { id: str, status: str, amount: float }
task run_test() {
  o: Order = { id: "ord-1", status: "pending", amount: 9.99 }
  filled = { ...o, status: "filled" }
  Io.show(filled.id)
  Io.show(filled.status)
  Io.show("{filled.amount}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("ord-1"), "id preserved: {stdout}");
    assert!(stdout.contains("filled"), "status updated: {stdout}");
    assert!(stdout.contains("9.99"), "amount preserved: {stdout}");
}

#[test]
fn struct_spread_update_multiple_overrides() {
    let src = r#"
type Point { x: int, y: int, z: int }
task run_test() {
  p: Point = { x: 1, y: 2, z: 3 }
  q = { ...p, x: 10, z: 30 }
  Io.show("{q.x}")
  Io.show("{q.y}")
  Io.show("{q.z}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("10"), "x updated: {stdout}");
    assert!(stdout.contains('2'), "y preserved: {stdout}");
    assert!(stdout.contains("30"), "z updated: {stdout}");
}

#[test]
fn struct_spread_update_no_overrides_is_copy() {
    let src = r#"
type Rec { a: int, b: str }
task run_test() {
  r: Rec = { a: 7, b: "hello" }
  r2 = { ...r }
  Io.show("{r2.a}")
  Io.show(r2.b)
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('7'), "a: {stdout}");
    assert!(stdout.contains("hello"), "b: {stdout}");
}

#[test]
fn struct_spread_update_preserves_type_tag() {
    let src = r#"
type Item { name: str, price: float }
task run_test() {
  item: Item = { name: "Widget", price: 9.99 }
  updated = { ...item, price: 4.99 }
  Io.show(typeof(updated))
  Io.show(updated.name)
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("Item"), "type tag preserved: {stdout}");
    assert!(stdout.contains("Widget"), "name preserved: {stdout}");
}

#[test]
fn struct_spread_update_chained() {
    let src = r#"
type Config { host: str, port: int, debug: bool }
task run_test() {
  base: Config = { host: "localhost", port: 8080, debug: false }
  dev = { ...base, debug: true }
  prod = { ...dev, host: "prod.example.com", debug: false }
  Io.show(prod.host)
  Io.show("{prod.port}")
  Io.show("{prod.debug}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("prod.example.com"), "host: {stdout}");
    assert!(stdout.contains("8080"), "port: {stdout}");
    assert!(stdout.contains("false"), "debug: {stdout}");
}

#[test]
fn struct_spread_update_unknown_field_is_type_error() {
    let src = r#"
type Rec { a: int }
task run_test() {
  r: Rec = { a: 1 }
  bad = { ...r, nonexistent: 99 }
  Io.show("{bad.a}")
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected type error for unknown field");
    assert!(
        stderr.contains("nonexistent") || stderr.contains("unknown field"),
        "got: {stderr}"
    );
}

#[test]
fn struct_spread_update_formatter_roundtrip() {
    // Format a program containing spread-update twice; formatter must be idempotent.
    let src = r#"
type Point { x: int, y: int }
task run_test() {
  p: Point = { x: 1, y: 2 }
  q = { ...p, x: 10 }
  Io.show("{q.x}")
}
run_test()
"#;
    use keel_lang::formatter::format_program;
    use keel_lang::lexer::lex;
    use keel_lang::parser::parse;
    use miette::NamedSource;
    let named = NamedSource::new("t.keel", src.to_string());
    let tokens = lex(src, &named).expect("lex");
    let program = parse(tokens, src.len(), &named).expect("parse");
    let once = format_program(&program);
    let named2 = NamedSource::new("t.keel", once.clone());
    let tokens2 = lex(&once, &named2).expect("lex 2");
    let program2 = parse(tokens2, once.len(), &named2).expect("parse 2");
    let twice = format_program(&program2);
    assert_eq!(once, twice, "formatter not idempotent:\n--- once ---\n{once}\n--- twice ---\n{twice}");
    assert!(once.contains("...p"), "spread not in formatted output: {once}");
}

#[test]
fn struct_spread_update_untyped_map_base() {
    // Untyped struct literals are Value::Map at runtime (not Value::Struct).
    // Spread-update must work through the Value::Map branch, not just Value::Struct.
    let src = r#"
task run_test() {
  r = { a: 1, b: "hello" }
  q = { ...r, a: 99 }
  Io.show("{q.a}")
  Io.show(q.b)
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("99"), "a overridden: {stdout}");
    assert!(stdout.contains("hello"), "b preserved: {stdout}");
}

#[test]
fn struct_spread_update_duplicate_override_is_type_error() {
    let src = r#"
type Rec { x: int, y: int }
task run_test() {
  r: Rec = { x: 1, y: 2 }
  bad = { ...r, x: 10, x: 20 }
  Io.show("{bad.x}")
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected type error for duplicate override field");
    assert!(
        stderr.contains("duplicate") || stderr.contains('x'),
        "got: {stderr}"
    );
}

#[test]
fn struct_spread_update_dynamic_base_unknown_field_is_runtime_error() {
    // When the base is typed `dynamic` the checker skips field validation.
    // The runtime guard must reject an unknown override field.
    let src = r#"
type Config { host: str, port: int }
task apply(cfg: dynamic) -> dynamic {
  return { ...cfg, phantom: true }
}
task run_test() {
  c: Config = { host: "localhost", port: 8080 }
  apply(c)
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error for unknown field on dynamic base");
    assert!(
        stderr.contains("phantom") || stderr.contains("unknown field"),
        "got: {stderr}"
    );
}

#[test]
fn struct_spread_update_map_base_works() {
    // Spread-update on an explicit map[str, int] variable — keys are unrestricted.
    let src = r#"
task run_test() {
  m: map[str, int] = { "a": 1, "b": 2 }
  m2 = { ...m, "c": 3 }
  Io.show("{m2.a}")
  Io.show("{m2.b}")
  Io.show("{m2.c}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains('1'), "a: {stdout}");
    assert!(stdout.contains('2'), "b: {stdout}");
    assert!(stdout.contains('3'), "c: {stdout}");
}

#[test]
fn struct_spread_update_map_base_wrong_value_type_is_error() {
    // Override value type must match the map's declared value type.
    let src = r#"
task run_test() {
  m: map[str, int] = { "a": 1 }
  bad = { ...m, "b": "not-an-int" }
  Io.show("{bad.b}")
}
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected type error for wrong value type in map spread-update");
    assert!(
        stderr.contains("str") || stderr.contains("int") || stderr.contains("expected"),
        "got: {stderr}"
    );
}
