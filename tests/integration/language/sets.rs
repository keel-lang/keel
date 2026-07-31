use crate::common::*;

// ---------------------------------------------------------------------------
// Set operations
// ---------------------------------------------------------------------------

#[test]
fn set_literal_deduplicates_and_preserves_insertion_order() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        nums = set[3, 1, 3, 2, 1]
        io.show("count={nums.count()} value={nums}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("count=3 value=set[3, 1, 2]"),
        "expected first-occurrence order with duplicates dropped:\n{stdout}"
    );
}

#[test]
fn add_returns_a_new_set_and_leaves_the_receiver_alone() {
    // The value-method contract shared with `list.push`: `.add` on its own is
    // a no-op, and an aliased binding never observes the reassignment.
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        a = set[1, 2]
        b = a
        a.add(99)
        a = a.add(3)
        io.show("a={a.count()} b={b.count()} discarded={a.contains(99)}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("a=3 b=2 discarded=false"),
        "expected .add to return a fresh set:\n{stdout}"
    );
}

#[test]
fn adding_an_existing_element_is_a_no_op() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        nums = set[1, 2]
        nums = nums.add(2)
        io.show("count={nums.count()}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("count=2"), "expected no growth:\n{stdout}");
}

#[test]
fn set_dedups_structs_by_field_values() {
    // Dedup runs on `Value`'s own equality, so it reaches element types a
    // hash-backed set could never hold — this is the case that justifies the
    // `Vec` representation.
    let src = r#"
use std/io
type Point { x: int, y: int }
agent A {
    @tools [io]
    @on_start {
        p: Point = {x: 1, y: 2}
        q: Point = {x: 1, y: 2}
        pts = set[p, q]
        io.show("count={pts.count()}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("count=1"),
        "expected structurally equal structs to collapse:\n{stdout}"
    );
}

#[test]
fn typeof_a_set_is_set_not_list() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        io.show("t={typeof(set[1, 2])}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("t=set"), "expected `set`:\n{stdout}");
}

#[test]
fn for_loop_iterates_a_set_in_insertion_order() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        seen = ""
        for x in set[3, 1, 3, 2] {
            seen += "{x},"
        }
        io.show("seen={seen}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("seen=3,1,2,"),
        "expected insertion order, deduplicated:\n{stdout}"
    );
}

#[test]
fn set_contains_and_is_empty_are_typed_as_bool() {
    // These resolved to `Ty::Unknown` before sets had a checker method table,
    // so a wrong-typed binding silently passed. Now it must not.
    let src = r#"
task t() {
    nums = set[1, 2]
    n: int = nums.contains(1)
}
"#;
    let (ok, stdout, stderr) = check_inline_output(src);
    let combined = format!("{stdout}{stderr}");
    assert!(!ok, "expected check to fail on bool-to-int binding");
    assert!(
        combined.contains("bool"),
        "expected a bool-mismatch diagnostic:\n{combined}"
    );
}

#[test]
fn set_borrows_the_read_only_list_pipeline() {
    // `.map`/`.join` have no set-specific implementation — a set is rebound
    // as a list for them (see `SET_LIST_METHODS`), and they yield lists and
    // scalars, never sets.
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        nums = set[1, 2, 2, 3]
        doubled = nums.map(x => x * 2)
        io.show("doubled={doubled} joined={nums.join("-")}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("doubled=[2, 4, 6] joined=1-2-3"),
        "expected list-shaped results over the deduplicated elements:\n{stdout}"
    );
}

#[test]
fn spreading_a_set_expands_into_variadic_slots() {
    let src = r#"
use std/io
task total(...nums: int) -> int {
    sum = 0
    for n in nums {
        sum += n
    }
    return sum
}
agent A {
    @tools [io]
    @on_start {
        io.show("total={total(...set[1, 2, 2, 3])}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("total=6"),
        "expected the deduplicated elements to spread:\n{stdout}"
    );
}

#[test]
fn push_on_a_set_points_at_add() {
    // `.push` is excluded from the borrowed list pipeline on purpose (it
    // would return a list). Without a checker arm it would type-check as
    // unknown and only fail at runtime.
    let src = r#"
task t() {
    nums = set[1, 2]
    nums = nums.push(3)
}
"#;
    let (ok, stdout, stderr) = check_inline_output(src);
    let combined = format!("{stdout}{stderr}");
    assert!(!ok, "expected check to reject .push on a set");
    assert!(
        combined.contains(".add"),
        "expected the diagnostic to suggest .add:\n{combined}"
    );
}
