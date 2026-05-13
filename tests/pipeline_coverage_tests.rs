use std::fs;
use std::io::Write as _;
use std::process::Command;

use keel_lang::lint::LintWarning;
use keel_lang::pipeline;
use keel_lang::vm;

fn keel_binary() -> std::path::PathBuf {
    std::path::PathBuf::from(env!("CARGO_BIN_EXE_keel"))
}

fn write_keel_file(source: &str) -> tempfile::NamedTempFile {
    let mut file = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("create temporary keel file");
    file.write_all(source.as_bytes())
        .expect("write temporary keel file");
    file
}

#[test]
fn pipeline_check_reports_missing_file_as_named_path_error() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let missing = dir.path().join("missing.keel");

    let err = pipeline::check_file(&missing, false).expect_err("missing file should fail");

    let message = err.to_string();
    assert!(
        message.contains("Could not read") && message.contains("missing.keel"),
        "expected readable missing-file diagnostic, got: {message}"
    );
}

#[test]
fn pipeline_format_rewrites_valid_program_in_place() {
    let file = write_keel_file(
        r#"
task greet(name: str) -> str { "hi {name}" }
"#,
    );

    pipeline::fmt_file(file.path()).expect("format should succeed");

    let formatted = fs::read_to_string(file.path()).expect("read formatted source");
    assert!(
        formatted.contains("task greet(name: str) -> str"),
        "formatted output should retain task signature:\n{formatted}"
    );
    assert!(
        formatted.ends_with('\n'),
        "formatter should write a trailing newline:\n{formatted:?}"
    );
}

#[test]
fn pipeline_lint_fix_removes_safe_unused_binding_and_keeps_program_valid() {
    let file = write_keel_file(
        r#"
agent A {
  @on_start {
    unused = "hello"
    Io.show("done")
  }
}
run(A)
"#,
    );

    let err = pipeline::lint_file(file.path(), true).expect_err("lint warnings still fail command");
    let fixed = fs::read_to_string(file.path()).expect("read fixed source");

    assert!(
        err.to_string().contains("lint warning"),
        "expected lint warning summary, got: {err}"
    );
    assert!(
        !fixed.contains("unused ="),
        "fixable unused binding should be removed:\n{fixed}"
    );
    pipeline::check_file(file.path(), false).expect("fixed program should still type-check");
}

#[test]
fn pipeline_build_reaches_deferred_vm_compiler_without_writing_bytecode() {
    let file = write_keel_file(
        r#"
task answer() -> int {
  42
}
"#,
    );
    let bytecode_path = file.path().with_extension("keelc");

    let err = pipeline::build_file(file.path()).expect_err("build is deferred in v0.1");

    let message = err.to_string();
    assert!(
        message.contains("deferred post-v0.1"),
        "expected deferred build diagnostic, got: {message}"
    );
    assert!(
        !bytecode_path.exists(),
        "deferred build must not write stale bytecode at {}",
        bytecode_path.display()
    );
}

#[tokio::test]
async fn pipeline_run_keelc_reports_vm_deferred_error() {
    let mut file = tempfile::Builder::new()
        .suffix(".keelc")
        .tempfile()
        .expect("create temporary keelc file");
    let program = vm::bytecode::CompiledProgram::default();
    serde_json::to_writer(file.as_file_mut(), &program).expect("write bytecode JSON");

    let err = pipeline::run_file(file.path())
        .await
        .expect_err(".keelc execution should be rejected");

    let message = err.to_string();
    assert!(
        message.contains("Bytecode execution (.keelc) is not yet supported"),
        "expected bytecode-deferred diagnostic, got: {message}"
    );
}

#[tokio::test]
async fn pipeline_run_keelc_reports_invalid_bytecode_json() {
    let mut file = tempfile::Builder::new()
        .suffix(".keelc")
        .tempfile()
        .expect("create temporary keelc file");
    file.write_all(b"not-json")
        .expect("write invalid bytecode JSON");

    let err = pipeline::run_file(file.path())
        .await
        .expect_err(".keelc execution should be rejected");

    let message = err.to_string();
    assert!(
        message.contains("Bytecode execution (.keelc) is not yet supported"),
        "expected bytecode-deferred diagnostic, got: {message}"
    );
}

#[tokio::test]
async fn pipeline_run_file_reports_type_errors_before_execution() {
    let file = write_keel_file(
        r#"
agent A {
  @on_start {
    x: int = "wrong"
    Io.show("should not run")
  }
}
run(A)
"#,
    );

    let err = pipeline::run_file(file.path())
        .await
        .expect_err("type errors should prevent execution");

    let message = err.to_string();
    assert!(
        message.contains("type error"),
        "expected type error summary, got: {message}"
    );
}

#[test]
fn pipeline_build_reports_type_errors_before_deferred_compiler() {
    let file = write_keel_file(
        r#"
task answer() -> int {
  return "wrong"
}
"#,
    );

    let err = pipeline::build_file(file.path()).expect_err("type error should fail build");

    let message = err.to_string();
    assert!(
        message.contains("type error"),
        "expected build type-error summary, got: {message}"
    );
}

#[test]
fn pipeline_lint_clean_program_succeeds_without_fixes() {
    let file = write_keel_file(
        r#"
task greet(name: str) -> str {
  "hello {name}"
}

agent A {
  @on_start {
    msg = greet("keel")
    Io.show(msg)
  }
}
run(A)
"#,
    );

    pipeline::lint_file(file.path(), false).expect("clean program should lint cleanly");
}

#[test]
fn pipeline_lint_reports_type_errors_before_warnings() {
    let file = write_keel_file(
        r#"
agent A {
  @on_start {
    unused = "still not the first problem"
    x: int = "wrong"
  }
}
run(A)
"#,
    );

    let err = pipeline::lint_file(file.path(), false).expect_err("type error should stop lint");

    let message = err.to_string();
    assert!(
        message.contains("fix before linting"),
        "expected lint type-check guard, got: {message}"
    );
}

#[test]
fn vm_compiler_and_machine_fail_loudly_while_bytecode_is_deferred() {
    let source = r#"task answer() -> int { 42 }"#;
    let named = miette::NamedSource::new("test.keel", source.to_string());
    let tokens = keel_lang::lexer::lex(source, &named).expect("lex source");
    let program = keel_lang::parser::parse(tokens, source.len(), &named).expect("parse source");

    let compile_err = vm::compiler::compile(&program).expect_err("compiler should be deferred");
    let mut machine = vm::machine::VM::new();
    let execute_err = machine
        .execute(&vm::bytecode::CompiledProgram::default())
        .expect_err("machine should be deferred");

    assert!(
        compile_err.contains("keel build is deferred"),
        "unexpected compiler error: {compile_err}"
    );
    assert!(
        execute_err.contains("Keel VM is deferred"),
        "unexpected VM error: {execute_err}"
    );
}

#[test]
fn cli_init_creates_real_project_and_check_accepts_generated_program() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let project = dir.path().join("daily-helper");

    let init = Command::new(keel_binary())
        .arg("init")
        .arg(&project)
        .output()
        .expect("run keel init");

    assert!(
        init.status.success(),
        "keel init failed\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&init.stdout),
        String::from_utf8_lossy(&init.stderr)
    );
    assert!(
        project.join(".gitignore").exists(),
        "init should create a .gitignore"
    );

    let main_keel = project.join("main.keel");
    let check = Command::new(keel_binary())
        .arg("check")
        .arg(&main_keel)
        .output()
        .expect("run keel check on generated project");

    assert!(
        check.status.success(),
        "generated project should type-check\nstdout: {}\nstderr: {}",
        String::from_utf8_lossy(&check.stdout),
        String::from_utf8_lossy(&check.stderr)
    );
}

#[test]
fn cli_rejects_invalid_log_level_before_running_program() {
    let file = write_keel_file(
        r#"
agent A {
  @on_start {
    Io.show("should not run")
  }
}
run(A)
"#,
    );

    let output = Command::new(keel_binary())
        .arg("--log-level")
        .arg("verbose")
        .arg("run")
        .arg(file.path())
        .output()
        .expect("run keel with invalid log level");

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(!output.status.success(), "invalid log level must fail");
    assert!(
        stderr.contains("not a valid level"),
        "expected log-level diagnostic:\n{stderr}"
    );
}

// ── Regression: apply_lint_fixes with overlapping ranges (B4) ────────────────

#[test]
fn apply_lint_fixes_two_non_overlapping_warnings_both_removed() {
    // Two separate fixable warnings on different lines — both must be removed
    // and the result must not panic on the second replace_range call.
    //
    // Source: "aaa\nbbb\nccc\n" (each token is on its own line).
    // Spans point into the middle of each line so line expansion stays on that
    // line and the ranges don't bleed into each other.
    let source = "aaa\nbbb\nccc\n";
    //             0123 4567 8901

    let warnings = vec![
        LintWarning {
            // points to 'a' inside "aaa\n" → expands to line (0, 4)
            message: "unused".into(),
            span: Some(1..2),
            fixable: true,
            hint: None,
        },
        LintWarning {
            // points to 'b' inside "bbb\n" → expands to line (4, 8)
            message: "unused".into(),
            span: Some(5..6),
            fixable: true,
            hint: None,
        },
    ];

    let result = pipeline::apply_lint_fixes(source, &warnings);
    assert!(
        !result.contains("aaa"),
        "first fixable line should be removed:\n{result:?}"
    );
    assert!(
        !result.contains("bbb"),
        "second fixable line should be removed:\n{result:?}"
    );
    assert!(
        result.contains("ccc"),
        "unfixed line should remain:\n{result:?}"
    );
}

#[test]
fn apply_lint_fixes_overlapping_ranges_do_not_panic() {
    // Two warnings whose expanded line spans overlap — the merge must prevent a
    // panicking replace_range call with an out-of-bounds index after the first
    // replacement shifts string indices.
    //
    // Produce overlapping line-spans by making two warnings whose spans land on
    // the same line: both "aaa\n" and "bbb\n" point to bytes that expand to
    // include bytes inside the other warning's line when the span.end falls at
    // the start of the next line.
    let source = "aaa\nbbb\nccc\n";

    // span 0..4 covers "aaa\n". apply_lint_fixes expands to (line_start=0,
    // line_end = source[4..].find('\n') = 3 → 4+3+1 = 8), i.e. (0,8).
    // span 4..8 covers "bbb\n". Expands to (line_start=source[..4].rfind('\n')=3→4,
    // line_end = source[8..].find('\n')=3→8+3+1=12), i.e. (4,12).
    // These overlap at (4,8). Without the merge fix the second replace_range
    // would panic because indices 4..8 are gone after the first removal.
    let warnings = vec![
        LintWarning {
            message: "unused".into(),
            span: Some(0..4), // "aaa\n" — expands to line range (0, 8)
            fixable: true,
            hint: None,
        },
        LintWarning {
            message: "unused".into(),
            span: Some(4..8), // "bbb\n" — expands to line range (4, 12)
            fixable: true,
            hint: None,
        },
    ];

    // Must not panic.
    let result = pipeline::apply_lint_fixes(source, &warnings);
    assert!(
        !result.contains("aaa"),
        "first overlapping span should be removed:\n{result:?}"
    );
    assert!(
        !result.contains("bbb"),
        "second overlapping span should be removed:\n{result:?}"
    );
}
