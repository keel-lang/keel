// Pipeline and VM unit tests moved to src/pipeline.rs and src/vm/mod.rs.
// This file retains only tests that require spawning the keel CLI binary.

use std::io::Write as _;
use std::process::Command;

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
