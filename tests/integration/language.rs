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
// List zip
// ---------------------------------------------------------------------------

#[test]
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
