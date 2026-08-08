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
