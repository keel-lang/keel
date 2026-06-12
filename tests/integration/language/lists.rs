use crate::common::*;

// ---------------------------------------------------------------------------
// Basic list operations
// ---------------------------------------------------------------------------

#[test]
fn list_concat_with_plus() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        a = ["x", "y"]
        b = ["z"]
        all = a + b
        for item in all {
            io.show(item)
        }
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("x"), "stdout:\n{stdout}");
    assert!(stdout.contains("y"), "stdout:\n{stdout}");
    assert!(stdout.contains("z"), "stdout:\n{stdout}");
}

#[test]
fn list_push_returns_extended_list() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        items = ["a", "b"]
        items = items.push("c")
        for item in items {
            io.show(item)
        }
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("a"), "stdout:\n{stdout}");
    assert!(stdout.contains("b"), "stdout:\n{stdout}");
    assert!(stdout.contains("c"), "stdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// List operations — any, all, find, reduce, sum, min, max, join, sort,
//                   reverse, flatten, take, skip
// ---------------------------------------------------------------------------

#[test]
fn list_any_returns_true_when_predicate_matches() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [1, 5, 10, 15]
    io.show("{nums.any(n => n > 8)}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains("true"), "any: {stdout}");
}

#[test]
fn list_all_returns_false_when_one_fails() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [1, 5, 10, 15]
    io.show("{nums.all(n => n > 8)}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains("false"), "all: {stdout}");
}

#[test]
fn list_find_returns_first_match_or_none() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [3, 7, 12, 20]
    found = nums.find(n => n > 10)
    io.show("{found}")
    missing = nums.find(n => n > 100)
    io.show("{missing}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains("12"), "find match: {stdout}");
    assert!(stdout.contains("none"), "find none: {stdout}");
}

#[test]
fn list_reduce_sums_with_accumulator() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [1, 2, 3, 4, 5]
    total = nums.reduce((acc, x) => acc + x, 0)
    io.show("{total}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains("15"), "reduce: {stdout}");
}

#[test]
fn list_sum_min_max_on_integers() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [4, 1, 9, 2, 7]
    io.show("{nums.sum()}")
    io.show("{nums.min()}")
    io.show("{nums.max()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains("23"), "sum: {stdout}");
    assert!(stdout.contains("1"), "min: {stdout}");
    assert!(stdout.contains("9"), "max: {stdout}");
}

#[test]
fn list_min_max_with_by_key() {
    let src = r#"
use std/io
type Item { name: str, score: int }
task run_test() {
  items: list[Item] = [
    { name: "b", score: 5 },
    { name: "a", score: 1 },
    { name: "c", score: 9 },
  ]
  io.show(items.min(by: x => x.score).name)
  io.show(items.max(by: x => x.score).name)
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    let names: Vec<&str> = stdout
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(names, vec!["a", "c"], "min/max by key: {stdout}");
}

#[test]
fn list_join_produces_delimited_string() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    tags = ["a", "b", "c"]
    io.show("{tags.join(", ")}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains("a, b, c"), "join: {stdout}");
}

#[test]
fn list_sort_and_reverse() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [3, 1, 4, 1, 5]
    io.show("{nums.sort().join(" ")}")
    io.show("{nums.sort().reverse().first()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains("1 1 3 4 5"), "sort: {stdout}");
    assert!(stdout.contains("5"), "reverse first: {stdout}");
}

#[test]
fn list_sort_by_int_key() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [3, 1, 4, 1, 5, 9, 2]
    sorted = nums.sort(by: x => x)
    io.show(sorted.join(" "))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("1 1 2 3 4 5 9"),
        "sort_by identity: {stdout}"
    );
}

#[test]
fn list_sort_by_field() {
    let src = r#"
use std/io
type Item { name: str, score: int }
task run_test() {
  items: list[Item] = [
    { name: "c", score: 3 },
    { name: "a", score: 1 },
    { name: "b", score: 2 },
  ]
  sorted = items.sort(by: x => x.score)
  for item in sorted {
    io.show(item.name)
  }
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    let names: Vec<&str> = stdout
        .lines()
        .map(|l| l.trim())
        .filter(|l| !l.is_empty())
        .collect();
    assert_eq!(names, vec!["a", "b", "c"], "sort_by score: {stdout}");
}

#[test]
fn list_sort_by_string_key() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    words = ["banana", "apple", "cherry"]
    sorted = words.sort(by: w => w)
    io.show(sorted.join(" "))
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("apple banana cherry"),
        "sort_by str: {stdout}"
    );
}

#[test]
fn list_sort_by_descending_via_negation() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [3, 1, 4, 1, 5]
    sorted = nums.sort(by: x => 0 - x)
    io.show(sorted.join(" "))
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
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [1]
    empty = nums.filter(x => x > 999)
    io.show("{empty.sort(by: x => x).count()}")
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
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [3, 1, 2]
    io.show("{nums.sort(by: 42).count()}")
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
use std/io
agent A {
  @tools [io]
  @on_start {
    nested = [[1, 2], [3], [4, 5]]
    io.show("{nested.flatten().join(" ")}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains("1 2 3 4 5"), "flatten: {stdout}");
}

#[test]
fn list_take_and_skip() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    nums = [10, 20, 30, 40, 50]
    io.show("{nums.take(3).join(" ")}")
    io.show("{nums.skip(3).join(" ")}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
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
use std/io
agent A {
  @tools [io]
  @on_start {
    a = [1, 2, 3]
    b = ["x", "y", "z"]
    pairs = a.zip(b)
    io.show("{pairs.len()}")
    io.show("{pairs[1][0]}")
    io.show("{pairs[1][1]}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains('3'), "len: {stdout}");
    assert!(stdout.contains('2'), "pair[1][0]: {stdout}");
    assert!(stdout.contains('y'), "pair[1][1]: {stdout}");
}

#[test]
#[ignore]
fn list_zip_stops_at_shorter_list() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    pairs = [1, 2, 3].zip(["a", "b"])
    io.show("{pairs.len()}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains('2'), "should stop at shorter: {stdout}");
}

#[test]
#[ignore]
fn list_zip_destructuring_in_for_loop() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    names = ["alice", "bob"]
    scores = [90, 85]
    for (name, score) in names.zip(scores) {
        io.show("{name}:{score}")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains("alice:90"), "destructure: {stdout}");
    assert!(stdout.contains("bob:85"), "destructure: {stdout}");
}
