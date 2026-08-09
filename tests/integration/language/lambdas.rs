use crate::common::*;

// ---------------------------------------------------------------------------
// Lambdas are non-capturing (issue #208)
// ---------------------------------------------------------------------------

#[test]
fn lambda_reading_outer_local_is_a_check_error() {
    let src = r#"
use std/io

task main() {
  n = 10
  add_n = x => x + n
  io.show("{add_n(5)}")
}

main()
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok, "expected a check error for capturing an outer local");
    assert!(
        stderr.contains("do not capture"),
        "expected a non-capturing-lambda diagnostic:\n{stderr}"
    );
}

#[test]
fn augmented_assign_to_outer_local_inside_lambda_is_a_check_error() {
    let src = r#"
task main() {
  n = 10
  f = () => { n += 1 }
  f()
}

main()
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(
        !ok,
        "expected a check error for mutating an outer local from a lambda"
    );
    assert!(
        stderr.contains("undefined variable"),
        "expected an undefined-variable diagnostic:\n{stderr}"
    );
}

#[test]
fn nested_lambda_cannot_capture_outer_lambdas_param() {
    let src = r#"
use std/io

task main() {
  outer = x => (y => x + y)
  io.show("done")
}

main()
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(
        !ok,
        "expected a check error: inner lambda cannot see outer lambda's param"
    );
    assert!(
        stderr.contains("do not capture"),
        "expected a non-capturing-lambda diagnostic:\n{stderr}"
    );
}

#[test]
fn lambda_can_reference_its_own_param_and_global_task() {
    let src = r#"
use std/io

task score(x: int) -> int {
  x * 2
}

task main() {
  f = x => score(x) + x
  io.show("{f(5)}")
}

main()
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(ok, "expected clean check:\n{stderr}");
}

#[test]
fn self_access_inside_lambda_still_works() {
    let src = r#"
use std/io

agent SelfInLambda {
  @tools [io]
  state {
    count: int = 0
  }
  @on_start {
    self.count = 5
    f = () => io.show("count is {self.count}")
    f()
  }
}

run(SelfInLambda)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected clean run:\n{stderr}");
    assert!(
        stdout.contains("count is 5"),
        "expected self access to resolve via ambient agent state:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Named function as a value (SPEC §7) — issue #216
// ---------------------------------------------------------------------------

#[test]
fn list_map_accepts_a_named_task_as_a_function_value() {
    let src = r#"
use std/io

task triage(x: int) -> int {
  x * 2
}

task main() {
  results = [1, 2, 3].map(triage)
  io.show("{results}")
}

main()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected clean run:\n{stderr}");
    assert!(
        stdout.contains("[2, 4, 6]"),
        "expected map(triage) to dispatch through call_task:\n{stdout}"
    );
}

#[test]
fn list_filter_find_any_all_reduce_and_sort_by_accept_a_named_task() {
    let src = r#"
use std/io

task is_even(x: int) -> bool {
  x % 2 == 0
}

task add(acc: int, x: int) -> int {
  acc + x
}

task negate(x: int) -> int {
  0 - x
}

task main() {
  nums = [1, 2, 3, 4, 5]
  io.show("{nums.filter(is_even)}")
  io.show("{nums.find(is_even)}")
  io.show("{nums.any(is_even)}")
  io.show("{nums.all(is_even)}")
  io.show("{nums.reduce(add, 0)}")
  io.show("{nums.sort(by: negate)}")
}

main()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected clean run:\n{stderr}");
    assert!(stdout.contains("[2, 4]"), "filter:\n{stdout}");
    assert!(
        stdout.contains("2\n") || stdout.contains("  2\n"),
        "find:\n{stdout}"
    );
    assert!(stdout.contains("true"), "any:\n{stdout}");
    assert!(stdout.contains("false"), "all:\n{stdout}");
    assert!(stdout.contains("15"), "reduce:\n{stdout}");
    assert!(
        stdout.contains("[5, 4, 3, 2, 1]"),
        "sort(by: negate) descending:\n{stdout}"
    );
}

#[test]
fn range_map_and_filter_accept_a_named_task() {
    let src = r#"
use std/io

task double(x: int) -> int {
  x * 2
}

task is_odd(x: int) -> bool {
  x % 2 != 0
}

task main() {
  io.show("{(1..3).map(double)}")
  io.show("{(1..5).filter(is_odd)}")
}

main()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected clean run:\n{stderr}");
    assert!(stdout.contains("[2, 4, 6]"), "range map:\n{stdout}");
    assert!(stdout.contains("[1, 3, 5]"), "range filter:\n{stdout}");
}

#[test]
fn a_non_function_value_still_produces_a_clean_error() {
    let src = r#"
task main() {
  [1, 2, 3].map(5)
}

main()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(
        !ok,
        "expected a runtime error for a non-function map argument"
    );
    assert!(
        stderr.contains("must be a function"),
        "expected a clean type error, not a panic:\n{stderr}"
    );
}
