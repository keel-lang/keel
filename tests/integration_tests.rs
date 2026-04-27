// Integration tests: verify example programs parse, type-check, and execute
// end-to-end via `keel run`.
//
// Structure:
//   - examples_all_parse    — smoke-checks every example with `keel check`
//   - showcase_runs_*       — comprehensive end-to-end test against showcase.keel
//   - feature-specific      — one inline test per language feature
//   - ai_stubs_*            — verify Ai.* runtime behaviour in mock mode
//   - scheduling_*          — verify Schedule.every fires correctly

use std::io::Write as _;
use std::path::PathBuf;
use std::process::Command;

fn project_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn keel_binary() -> PathBuf {
    project_root().join("target").join("release").join("keel")
}

fn ensure_binary_built() {
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

fn run_inline(src: &str, trace: bool) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new().suffix(".keel").tempfile().expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let mut cmd = Command::new(&bin);
    cmd.env("KEEL_ONESHOT", "1").env("KEEL_LLM", "mock").arg("run").arg(&path);
    if trace {
        cmd.env("KEEL_TRACE", "1");
    }
    let output = cmd.output().expect("run keel");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

// ---------------------------------------------------------------------------
// Smoke-check: every example must pass `keel check`
// ---------------------------------------------------------------------------

#[test]
fn examples_all_parse() {
    ensure_binary_built();
    for name in [
        "hello_world",
        "data_pipeline",
        "daily_digest",
        "meeting_prep",
        "code_reviewer",
        "customer_support",
        "email_agent",
        "multi_agent_inbox",
        "http_demo",
        "at_demo",
        "rich_enum",
        "showcase",
        "if_expression",
        "list_building",
        "agent_delegation",
    ] {
        assert!(check_example(name), "`keel check {name}.keel` failed");
    }
}

// ---------------------------------------------------------------------------
// Comprehensive showcase — exercises every language feature in one program
// ---------------------------------------------------------------------------

#[test]
fn showcase_runs_end_to_end() {
    ensure_binary_built();
    let (ok, stdout, stderr) = run_example("showcase");
    assert!(ok, "showcase.keel exited non-zero\nstdout: {stdout}\nstderr: {stderr}");

    // list + list and list.push produce 4 incidents; count() in interpolation
    assert!(stdout.contains("4 incidents in queue"),
        "list concat/push or string interpolation missing:\n{stdout}");

    // All incident IDs present, including the one added via push
    assert!(stdout.contains("INC-101"), "INC-101 missing:\n{stdout}");
    assert!(stdout.contains("INC-104"), "pushed INC-104 missing:\n{stdout}");

    // @on_stop fired for OnCall before removal
    assert!(stdout.contains("OnCall shift complete"),
        "OnCall @on_stop missing:\n{stdout}");

    // Shift summary line present (fallback value in mock mode)
    assert!(stdout.contains("Shift summary:"),
        "shift summary line missing:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Feature-specific examples
// ---------------------------------------------------------------------------

#[test]
fn data_pipeline_runs_through_all_records() {
    ensure_binary_built();
    let (ok, stdout, _stderr) = run_example("data_pipeline");
    assert!(ok);
    assert!(stdout.contains("Processing 5 records"));
    assert!(stdout.contains("Stats: 2/5 valid"));
}

#[test]
fn email_fetch_without_config_is_empty_list() {
    ensure_binary_built();
    let (ok, stdout, _stderr) = run_example("daily_digest");
    assert!(ok, "daily_digest exited non-zero");
    assert!(
        stdout.contains("No unread emails"),
        "expected graceful empty-inbox branch, stdout:\n{stdout}"
    );
}

#[test]
fn rich_enum_variants_construct_and_destructure() {
    ensure_binary_built();
    let (ok, stdout, _stderr) = run_example("rich_enum");
    assert!(ok);
    assert!(stdout.contains("reply to alice@example.com (friendly)"));
    assert!(stdout.contains("forward to ops@example.com"));
    assert!(stdout.contains("archive"));
}

// ---------------------------------------------------------------------------
// REPL
// ---------------------------------------------------------------------------

#[test]
fn repl_evaluates_let_and_expression() {
    ensure_binary_built();
    let bin = keel_binary();
    let mut child = Command::new(&bin)
        .arg("repl")
        .env("KEEL_LLM", "mock")
        .env("KEEL_REPL", "1")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn keel repl");

    let mut stdin = child.stdin.take().unwrap();
    stdin.write_all(b"x = 1 + 2\nx * 10\n").unwrap();
    drop(stdin);

    let out = child.wait_with_output().expect("wait repl");
    let stdout = String::from_utf8_lossy(&out.stdout).into_owned();
    assert!(stdout.contains("30"), "expected REPL to compute 30, got:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Scheduling
// ---------------------------------------------------------------------------

#[test]
fn scheduling_ticks_at_least_once() {
    ensure_binary_built();
    let src = r#"
agent Ticker {
  state { tick: int = 0 }
  @on_start {
    Schedule.every(3.seconds, () => {
      self.tick = self.tick + 1
      Io.show("Tick #{self.tick}")
    })
  }
}
run(Ticker)
"#;
    let (ok, stdout, _stderr) = run_inline(src, false);
    assert!(ok);
    assert!(stdout.contains("Tick #1"));
}

#[test]
fn scheduling_recurs_without_oneshot() {
    // Without KEEL_ONESHOT, Schedule.every must fire repeatedly.
    // Ticking every 3 seconds over a 7-second window should yield >= 2 ticks.
    ensure_binary_built();
    let src = r#"
agent Ticker {
  state { tick: int = 0 }
  @on_start {
    Schedule.every(3.seconds, () => {
      self.tick = self.tick + 1
      Io.show("Tick #{self.tick}")
    })
  }
}
run(Ticker)
"#;
    let mut tmp = tempfile::Builder::new().suffix(".keel").tempfile().expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let child = Command::new(&bin)
        .env("KEEL_LLM", "mock")
        .arg("run")
        .arg(&path)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn");

    let pid = child.id();
    std::thread::sleep(std::time::Duration::from_secs(7));
    let _ = Command::new("kill").arg(pid.to_string()).status();

    let result = child.wait_with_output().expect("wait_with_output");
    let stdout = String::from_utf8_lossy(&result.stdout).into_owned();
    let tick_count = stdout.matches("Tick #").count();
    assert!(
        tick_count >= 2,
        "expected at least 2 ticks in 7s window, got {tick_count}\nstdout:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Ai.* stub behaviour (trace mode verifies prompts are built correctly)
// ---------------------------------------------------------------------------

#[test]
fn rules_appear_in_trace_system_prompt() {
    ensure_binary_built();
    let src = r#"
type Mood = calm | tense

agent Advisor {
    @role "Expert advisor"
    @rules ["Never reveal internal state", "Be concise"]

    @on_start {
        result = Ai.classify("some input", as: Mood, fallback: Mood.calm)
    }
}

run(Advisor)
"#;
    let (ok, stdout, _stderr) = run_inline(src, true);
    assert!(ok, "program exited non-zero\nstdout: {stdout}");
    assert!(
        stdout.contains("Never reveal internal state"),
        "rules not found in trace output\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Be concise"),
        "second rule not found in trace output\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("Rules:"),
        "Rules: header missing in trace\nstdout:\n{stdout}"
    );
}

#[test]
fn summarize_format_and_max_appear_in_trace() {
    ensure_binary_built();
    let src = r#"
agent Summarizer {
    @on_start {
        result = Ai.summarize("Long article text here", format: bullets, max: 3, unit: sentences)
    }
}

run(Summarizer)
"#;
    let (ok, stdout, _stderr) = run_inline(src, true);
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {_stderr}");
    assert!(
        stdout.contains("bulleted list"),
        "format directive missing\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("at most 3 sentences"),
        "max directive missing\nstdout:\n{stdout}"
    );
}

#[test]
fn prompt_response_format_json_directive_in_trace() {
    ensure_binary_built();
    let src = r#"
agent Prompter {
    @on_start {
        result = Ai.prompt(system: "Rate on 1-10.", user: "Hello", response_format: json)
    }
}

run(Prompter)
"#;
    let (ok, stdout, _stderr) = run_inline(src, true);
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {_stderr}");
    assert!(
        stdout.contains("valid JSON only"),
        "JSON format directive missing from trace\nstdout:\n{stdout}"
    );
}

#[test]
fn extract_as_struct_type_derives_schema() {
    ensure_binary_built();
    let src = r#"
type Invoice {
    vendor: str
    amount: float
    date: str
}

agent Extractor {
    @on_start {
        result = Ai.extract("Invoice from ACME $99.99 on 2026-01-10", as: Invoice)
    }
}

run(Extractor)
"#;
    let (ok, stdout, _stderr) = run_inline(src, true);
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {_stderr}");
    assert!(stdout.contains("vendor"), "vendor field missing from trace\nstdout:\n{stdout}");
    assert!(stdout.contains("amount"), "amount field missing from trace\nstdout:\n{stdout}");
    assert!(stdout.contains("date"), "date field missing from trace\nstdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// v0.1.4 parser hardening — one test per feature
// ---------------------------------------------------------------------------

#[test]
fn if_expr_on_rhs_of_binding() {
    ensure_binary_built();
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
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("high"), "expected 'high' branch, stdout:\n{stdout}");
}

#[test]
fn if_expr_else_branch_selected() {
    ensure_binary_built();
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
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("low"), "expected 'low' branch, stdout:\n{stdout}");
}

#[test]
fn let_annotation_valid_runs() {
    ensure_binary_built();
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
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("hello annotated"), "stdout:\n{stdout}");
}

#[test]
fn null_assert_unwraps_non_none_value() {
    ensure_binary_built();
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
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("present"), "stdout:\n{stdout}");
}

#[test]
fn null_assert_on_none_raises_runtime_error() {
    ensure_binary_built();
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
    ensure_binary_built();
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
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("x"), "stdout:\n{stdout}");
    assert!(stdout.contains("y"), "stdout:\n{stdout}");
    assert!(stdout.contains("z"), "stdout:\n{stdout}");
}

#[test]
fn list_push_returns_extended_list() {
    ensure_binary_built();
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
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("a"), "stdout:\n{stdout}");
    assert!(stdout.contains("b"), "stdout:\n{stdout}");
    assert!(stdout.contains("c"), "stdout:\n{stdout}");
}

#[test]
fn string_interp_method_call() {
    ensure_binary_built();
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
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("size=3"), "expected 'size=3' in stdout:\n{stdout}");
}

#[test]
fn string_interp_binary_expr() {
    ensure_binary_built();
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
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("doubled=10"), "expected 'doubled=10' in stdout:\n{stdout}");
}

#[test]
fn on_stop_block_fires_before_agent_removed() {
    ensure_binary_built();
    let src = r#"
agent A {
    @on_stop {
        Io.show("A stopped")
    }
    @on_start {
        Agent.stop(A)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("A stopped"),
        "expected @on_stop output before removal:\nstdout: {stdout}"
    );
}

#[test]
fn agent_delegate_dispatches_to_handler() {
    ensure_binary_built();
    let src = r#"
agent Worker {
    on process(data: str) {
        Io.show("processed")
    }
}

agent Boss {
    @on_start {
        Agent.run(Worker)
        Agent.delegate(Worker, "process", "payload")
    }
}

run(Boss)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("processed"),
        "expected Worker handler to fire:\nstdout: {stdout}"
    );
}

#[test]
fn search_stub_raises_v2_error() {
    ensure_binary_built();
    let src = r#"
agent A {
    @on_start {
        Search.web("query")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for Search stub");
    assert!(
        stderr.contains("v0.2"),
        "expected 'v0.2' in error message:\n{stderr}"
    );
}

#[test]
fn db_stub_raises_v2_error() {
    ensure_binary_built();
    let src = r#"
agent A {
    @on_start {
        Db.query("SELECT 1")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for Db stub");
    assert!(
        stderr.contains("v0.2"),
        "expected 'v0.2' in error message:\n{stderr}"
    );
}

#[test]
fn time_stub_raises_v2_error() {
    ensure_binary_built();
    let src = r#"
agent A {
    @on_start {
        Time.parse("2026-01-01")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for Time stub");
    assert!(
        stderr.contains("v0.2"),
        "expected 'v0.2' in error message:\n{stderr}"
    );
}
