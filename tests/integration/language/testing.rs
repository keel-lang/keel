use crate::common::*;

#[test]
fn test_blocks_mock_ai_classify() {
    let src = r#"
use std/ai
use std/testing

type Severity = low | medium | critical

task classify(text: str) -> Severity {
  ai.classify(text, as: Severity) ?? Severity.low
}

test "mocked classify returns critical" {
  testing.mock(ai.classify).returns(Severity.critical)
  assert classify("payment outage") == Severity.critical
}
"#;
    let (ok, stdout, stderr) = test_inline(src);
    assert!(
        ok,
        "keel test failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stderr.contains("PASS mocked classify returns critical"));
    assert!(stderr.contains("ms)") || stderr.contains("s)"));
    assert!(stderr.contains("1 test passed"));
    assert!(stdout.is_empty(), "stdout should be empty: {stdout}");
}

#[test]
fn test_mocks_do_not_leak_between_tests() {
    let src = r#"
use std/ai
use std/testing

type Severity = low | medium | critical

task classify(text: str) -> Severity {
  ai.classify(text, as: Severity) ?? Severity.low
}

test "critical" {
  testing.mock(ai.classify).returns(Severity.critical)
  assert classify("payment outage") == Severity.critical
}

test "medium" {
  testing.mock(ai.classify).returns(Severity.medium)
  assert classify("question") == Severity.medium
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("PASS critical"));
    assert!(stderr.contains("PASS medium"));
    assert!(stderr.contains("2 tests passed"));
}

#[test]
fn test_setup_block_binds_values_for_assertions() {
    let src = r#"
test "setup shares values with body" {
  setup {
    expected: str = "ready"
    actual: str = "ready"
  }

  assert actual == expected
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("PASS setup shares values with body"));
}

#[test]
fn test_repeated_mocks_return_sequence_then_repeat_last_value() {
    let src = r#"
use std/ai
use std/testing

task summarize(text: str) -> str {
  ai.summarize(text) ?? "fallback"
}

test "mock sequence" {
  testing.mock(ai.summarize).returns("first")
  testing.mock(ai.summarize).returns("second")

  assert summarize("a") == "first"
  assert summarize("b") == "second"
  assert summarize("c") == "second"
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("PASS mock sequence"));
}

#[test]
fn test_mock_metadata_tracks_calls_and_arguments() {
    let src = r#"
use std/ai
use std/testing

task draft_reply(body: str) -> str {
  ai.draft("response to {body}", tone: "friendly", max_length: 150) ?? "fallback"
}

test "mock metadata" {
  testing.mock(ai.draft).returns("Thanks")

  assert ai.draft.called == false
  assert ai.draft.call_count == 0
  assert ai.draft.called_with("response to Can you review this?") == false

  assert draft_reply("Can you review this?") == "Thanks"

  assert ai.draft.called
  assert ai.draft.call_count == 1
  assert ai.draft.called_with("response to Can you review this?")
  assert ai.draft.called_with("response to Can you review this?", tone: "friendly")
  assert ai.draft.called_with("response to Can you review this?", max_length: 150)
  assert ai.draft.called_with("different prompt") == false
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("PASS mock metadata"));
}

#[test]
fn test_mock_metadata_counts_async_spawned_calls() {
    let src = r#"
use std/ai
use std/async
use std/testing

task summarize(text: str) -> str {
  ai.summarize(text) ?? "fallback"
}

test "mock metadata crosses spawn" {
  testing.mock(ai.summarize).returns("inside")
  testing.mock(ai.summarize).returns("outside")

  h = async.spawn(() => {
    return summarize("inside")
  })
  assert async.join_all([h])[0] == "inside"
  assert summarize("outside") == "outside"
  assert ai.summarize.call_count == 2
  assert ai.summarize.called_with("inside")
  assert ai.summarize.called_with("outside")
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("PASS mock metadata crosses spawn"));
}

#[test]
fn parameterized_tests_run_each_case() {
    let src = r#"
test "parity" for case in [
  { value: 1, even: false },
  { value: 2, even: true },
  { value: 3, even: false }
] {
  actual = case.value % 2 == 0
  assert actual == case.even
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("PASS parity [0]"), "stderr: {stderr}");
    assert!(stderr.contains("PASS parity [1]"), "stderr: {stderr}");
    assert!(stderr.contains("PASS parity [2]"), "stderr: {stderr}");
    assert!(stderr.contains("3 tests passed"), "stderr: {stderr}");
}

#[test]
fn parameterized_test_cases_must_be_list() {
    let src = r#"
test "bad cases" for case in 42 {
  assert true
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(!ok, "type check should fail");
    assert!(
        stderr.contains("parameterized test cases must be a list"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_filter_runs_matching_tests_only() {
    let src = r#"
test "fast path" {
  assert true
}

test "slow path" {
  assert false
}
"#;
    let (ok, _stdout, stderr) = test_inline_with_filter(src, Some("fast"));
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("PASS fast path"), "stderr: {stderr}");
    assert!(!stderr.contains("slow path"), "stderr: {stderr}");
    assert!(stderr.contains("1 test passed"), "stderr: {stderr}");
}

#[test]
fn test_filter_without_matches_fails() {
    let src = r#"
test "fast path" {
  assert true
}
"#;
    let (ok, _stdout, stderr) = test_inline_with_filter(src, Some("missing"));
    assert!(!ok, "filter typo should fail");
    assert!(
        stderr.contains("no tests matched filter `missing`"),
        "stderr: {stderr}"
    );
}

#[test]
fn test_list_prints_matching_test_names_without_running() {
    let src = r#"
test "fast path" {
  assert false
}

test "slow path" {
  assert false
}
"#;
    let (ok, stdout, stderr) = test_inline_list(src, Some("fast"));
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.is_empty(), "stdout should be empty: {stdout}");
    assert!(stderr.contains("fast path"), "stderr: {stderr}");
    assert!(!stderr.contains("slow path"), "stderr: {stderr}");
    assert!(
        !stderr.contains("FAIL"),
        "list must not run tests: {stderr}"
    );
}

#[test]
fn test_list_without_matches_prints_zero_tests_found() {
    let src = r#"
test "fast path" {
  assert true
}
"#;
    let (ok, _stdout, stderr) = test_inline_list(src, Some("missing"));
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("0 tests found"), "stderr: {stderr}");
}

#[test]
fn test_file_without_tests_prints_zero_tests_found() {
    let src = r#"
task helper() -> bool {
  true
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("0 tests found"), "stderr: {stderr}");
}

#[test]
fn assert_message_is_reported_on_failure() {
    let src = r#"
test "custom message" {
  assert false, "expected the fallback path"
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(!ok, "test command should fail");
    assert!(stderr.contains("FAIL custom message"), "stderr: {stderr}");
    assert!(
        stderr.contains("expected the fallback path"),
        "stderr: {stderr}"
    );
}

#[test]
fn assertion_failure_reports_source_location() {
    let src = r#"
test "location" {
  assert false
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(!ok, "test command should fail");
    assert!(stderr.contains("FAIL location"), "stderr: {stderr}");
    assert!(stderr.contains(".keel:3:3"), "stderr: {stderr}");
    assert!(stderr.contains("assertion failed"), "stderr: {stderr}");
}

#[test]
fn assert_message_must_be_string() {
    let src = r#"
test "bad message" {
  assert false, 42
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(!ok, "type check should fail");
    assert!(stderr.contains("`assert` message"), "stderr: {stderr}");
    assert!(stderr.contains("expected str"), "stderr: {stderr}");
}

#[test]
fn test_mocks_apply_inside_async_spawn() {
    let src = r#"
use std/ai
use std/async
use std/testing

type Severity = low | medium | critical

task classify(text: str) -> Severity {
  ai.classify(text, as: Severity) ?? Severity.low
}

test "spawned classify sees mock" {
  testing.mock(ai.classify).returns(Severity.critical)
  h = async.spawn(() => {
    return classify("payment outage")
  })
  results = async.join_all([h])
  result = results[0]
  assert result == Severity.critical
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("PASS spawned classify sees mock"));
}

#[test]
fn test_mock_sequences_are_shared_with_async_spawn() {
    let src = r#"
use std/ai
use std/async
use std/testing

task summarize(text: str) -> str {
  ai.summarize(text) ?? "fallback"
}

test "spawned calls share mock sequence" {
  testing.mock(ai.summarize).returns("first")
  testing.mock(ai.summarize).returns("second")

  h = async.spawn(() => {
    return summarize("inside")
  })
  inside = async.join_all([h])[0]
  outside = summarize("outside")
  assert inside == "first"
  assert outside == "second"
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("PASS spawned calls share mock sequence"));
}

#[test]
fn bare_assert_call_remains_callable_identifier() {
    let src = r#"
use std/io
task assert(message: str) -> str {
  message
}

io.show(assert("hello"))
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "bare assert call should run as a normal task call
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(stdout.contains("hello"), "stdout: {stdout}");
}

#[test]
fn assertion_failure_fails_test_command() {
    let src = r#"
test "fails" {
  assert false
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(!ok, "test command should fail");
    assert!(stderr.contains("FAIL fails"), "stderr: {stderr}");
    assert!(stderr.contains("assertion failed"), "stderr: {stderr}");
}

#[test]
fn test_fail_fast_stops_after_first_failure() {
    let src = r#"
test "first failure" {
  assert false, "first failed"
}

test "second failure" {
  assert false, "second failed"
}
"#;
    let (ok, _stdout, stderr) = test_inline_fail_fast(src);
    assert!(!ok, "test command should fail");
    assert!(stderr.contains("FAIL first failure"), "stderr: {stderr}");
    assert!(stderr.contains("first failed"), "stderr: {stderr}");
    assert!(!stderr.contains("second failure"), "stderr: {stderr}");
    assert!(!stderr.contains("second failed"), "stderr: {stderr}");
    assert!(stderr.contains("0 tests passed"), "stderr: {stderr}");
    assert!(stderr.contains("1 test failed"), "stderr: {stderr}");
}

#[test]
fn test_quiet_prints_failures_and_summary_only() {
    let src = r#"
test "passes" {
  assert true
}

test "fails" {
  assert false
}
"#;
    let (ok, _stdout, stderr) = test_inline_quiet(src);
    assert!(!ok, "test command should fail");
    assert!(!stderr.contains("PASS passes"), "stderr: {stderr}");
    assert!(stderr.contains("FAIL fails"), "stderr: {stderr}");
    assert!(stderr.contains("1 test passed"), "stderr: {stderr}");
    assert!(stderr.contains("1 test failed"), "stderr: {stderr}");
}

#[test]
fn test_directory_runs_keel_files_recursively() {
    let dir = tempfile::tempdir().expect("create test directory");
    let nested = dir.path().join("nested");
    std::fs::create_dir(&nested).expect("create nested test directory");
    std::fs::write(
        dir.path().join("alpha.keel"),
        r#"
test "alpha" {
  assert true
}
"#,
    )
    .expect("write alpha test file");
    std::fs::write(
        nested.join("beta.keel"),
        r#"
test "beta" {
  assert true
}
"#,
    )
    .expect("write beta test file");
    std::fs::write(
        dir.path().join("helper.keel"),
        r#"
task helper() -> bool {
  true
}
"#,
    )
    .expect("write helper file");

    let (ok, _stdout, stderr) = test_path(dir.path(), &[]);
    assert!(ok, "stderr: {stderr}");
    assert!(stderr.contains("PASS"), "stderr: {stderr}");
    assert!(stderr.contains("alpha.keel: alpha"), "stderr: {stderr}");
    assert!(stderr.contains("beta.keel: beta"), "stderr: {stderr}");
    assert!(!stderr.contains("helper.keel"), "stderr: {stderr}");
    assert!(stderr.contains("2 tests passed"), "stderr: {stderr}");
}

#[test]
fn test_directory_list_and_filter_apply_across_files() {
    let dir = tempfile::tempdir().expect("create test directory");
    std::fs::write(
        dir.path().join("alpha.keel"),
        r#"
test "alpha path" {
  assert false
}
"#,
    )
    .expect("write alpha test file");
    std::fs::write(
        dir.path().join("beta.keel"),
        r#"
test "beta path" {
  assert false
}
"#,
    )
    .expect("write beta test file");

    let (ok, stdout, stderr) = test_path(dir.path(), &["--list", "--filter", "beta"]);
    assert!(ok, "stderr: {stderr}");
    assert!(stdout.is_empty(), "stdout should be empty: {stdout}");
    assert!(stderr.contains("beta.keel: beta path"), "stderr: {stderr}");
    assert!(!stderr.contains("alpha path"), "stderr: {stderr}");
    assert!(!stderr.contains("FAIL"), "stderr: {stderr}");
}

#[test]
fn test_directory_quiet_and_fail_fast_apply_across_files() {
    let dir = tempfile::tempdir().expect("create test directory");
    std::fs::write(
        dir.path().join("a_fail.keel"),
        r#"
test "first failure" {
  assert false, "first failed"
}
"#,
    )
    .expect("write first failing test file");
    std::fs::write(
        dir.path().join("b_fail.keel"),
        r#"
test "second failure" {
  assert false, "second failed"
}
"#,
    )
    .expect("write second failing test file");

    let (ok, _stdout, stderr) = test_path(dir.path(), &["--quiet", "--fail-fast"]);
    assert!(!ok, "directory test command should fail");
    assert!(stderr.contains("FAIL"), "stderr: {stderr}");
    assert!(
        stderr.contains("a_fail.keel: first failure"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("first failed"), "stderr: {stderr}");
    assert!(!stderr.contains("b_fail.keel"), "stderr: {stderr}");
    assert!(!stderr.contains("second failed"), "stderr: {stderr}");
    assert!(stderr.contains("0 tests passed"), "stderr: {stderr}");
    assert!(stderr.contains("1 test failed"), "stderr: {stderr}");
}

#[test]
fn test_output_colors_labels_and_reports_sub_millisecond_runs() {
    let src = r#"
test "passes" {
  assert true
}

test "fails" {
  assert false
}
"#;
    let (ok, _stdout, stderr) = test_inline_with_env(src, &[("CLICOLOR_FORCE", "1")]);
    assert!(!ok, "test command should fail");
    assert!(
        stderr.contains("\u{1b}[32mPASS\u{1b}[0m passes"),
        "stderr: {stderr}"
    );
    assert!(
        stderr.contains("\u{1b}[31mFAIL\u{1b}[0m fails"),
        "stderr: {stderr}"
    );
    assert!(stderr.contains("(<1ms)"), "stderr: {stderr}");
    assert!(stderr.contains("1 test passed"), "stderr: {stderr}");
    assert!(stderr.contains("1 test failed"), "stderr: {stderr}");
}

#[test]
fn examples_with_test_blocks_pass() {
    let (ok, stdout, stderr) = test_path(&project_root().join("examples"), &["--quiet"]);
    assert!(
        ok,
        "`keel test examples --quiet` failed\nstdout:\n{stdout}\nstderr:\n{stderr}"
    );
    assert!(stderr.contains("tests passed"), "stderr: {stderr}");
}

#[test]
fn bad_mock_target_is_check_error() {
    let src = r#"
use std/ai
use std/testing

test "bad mock" {
  testing.mock(ai.nope).returns("x")
  assert true
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(!ok, "test command should fail");
    assert!(
        stderr.contains("unknown mock target `ai.nope`"),
        "stderr: {stderr}"
    );
}

#[test]
fn mock_metadata_requires_matching_mock() {
    let src = r#"
use std/ai
test "missing mock" {
  assert ai.draft.call_count == 0
}
"#;
    let (ok, _stdout, stderr) = test_inline(src);
    assert!(!ok, "test command should fail");
    assert!(
        stderr.contains("requires `testing.mock(ai.draft).returns(...)`"),
        "stderr: {stderr}"
    );
}

#[test]
fn run_ignores_test_blocks() {
    let src = r#"
test "not run by keel run" {
  assert false
}
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "keel run should ignore test blocks
stdout: {stdout}
stderr: {stderr}"
    );
}
