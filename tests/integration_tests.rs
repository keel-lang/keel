// v0.1 integration tests: verify representative example programs
// parse, type-check (stub), and execute end-to-end via `keel run`.

use std::path::PathBuf;
use std::process::Command;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn keel_binary() -> PathBuf {
    project_root().join("target").join("release").join("keel")
}

fn ensure_binary_built() {
    // Build the release binary once for the test suite.
    let status = Command::new("cargo")
        .args(["build", "--release", "--quiet"])
        .current_dir(project_root())
        .status()
        .expect("cargo build failed to launch");
    assert!(status.success(), "cargo build --release failed");
}

fn run_example(name: &str) -> (bool, String, String) {
    let bin = keel_binary();
    let example = project_root().join("examples").join(format!("{name}.keel"));
    let output = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .arg("run")
        .arg(&example)
        .output()
        .expect("failed to run keel binary");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

fn check_example(name: &str) -> bool {
    let bin = keel_binary();
    let example = project_root().join("examples").join(format!("{name}.keel"));
    Command::new(&bin)
        .arg("check")
        .arg(&example)
        .status()
        .expect("failed to run keel check")
        .success()
}

#[test]
fn examples_all_parse() {
    ensure_binary_built();
    for name in [
        "minimal", "hello_world", "test_scheduling", "test_ollama",
        "data_pipeline", "daily_digest", "meeting_prep", "code_reviewer",
        "customer_support", "email_agent", "multi_agent_inbox",
    ] {
        assert!(check_example(name), "`keel check {name}.keel` failed");
    }
}

#[test]
fn minimal_prints_greeting() {
    ensure_binary_built();
    let (ok, stdout, stderr) = run_example("minimal");
    assert!(ok, "minimal.keel exited non-zero.\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("Hello, World!"), "stdout missing greeting:\n{stdout}");
    assert!(stdout.contains("Greeted 1 times"), "stdout missing counter:\n{stdout}");
}

#[test]
fn data_pipeline_runs_through_all_records() {
    ensure_binary_built();
    let (ok, stdout, _stderr) = run_example("data_pipeline");
    assert!(ok);
    assert!(stdout.contains("Processing 5 records"));
    assert!(stdout.contains("Stats: 2/5 valid"));
}

#[test]
fn test_ollama_exercises_ai_stubs() {
    ensure_binary_built();
    let (ok, stdout, _stderr) = run_example("test_ollama");
    assert!(ok);
    assert!(stdout.contains("Classify test"));
    assert!(stdout.contains("Summarize test"));
    assert!(stdout.contains("Draft test"));
    assert!(stdout.contains("Done"));
}

#[test]
fn scheduling_ticks_at_least_once() {
    ensure_binary_built();
    let (ok, stdout, _stderr) = run_example("test_scheduling");
    assert!(ok);
    assert!(stdout.contains("Tick #1"));
}
