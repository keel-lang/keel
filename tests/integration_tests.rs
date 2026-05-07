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

fn lint_inline(src: &str) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let output = Command::new(&bin)
        .arg("lint")
        .arg(&path)
        .output()
        .expect("run keel lint");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

fn check_inline_output(src: &str) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let output = Command::new(&bin)
        .arg("check")
        .arg(&path)
        .output()
        .expect("run keel check");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

fn run_inline(src: &str, trace: bool) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let mut cmd = Command::new(&bin);
    cmd.env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .arg("run")
        .arg(&path);
    if trace {
        cmd.env("KEEL_TRACE", "1");
    }
    let output = cmd.output().expect("run keel");
    let ok = output.status.success();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
    (ok, stdout, stderr)
}

fn run_inline_with_home(src: &str, home: &std::path::Path) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let bin = keel_binary();
    let output = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home)
        .env("USERPROFILE", home)
        .arg("run")
        .arg(&path)
        .output()
        .expect("run keel");
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
    let examples_dir = project_root().join("examples");
    let mut names: Vec<String> = std::fs::read_dir(&examples_dir)
        .expect("read examples directory")
        .filter_map(|entry| {
            let path = entry.expect("read examples entry").path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("keel") {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(str::to_owned)
            } else {
                None
            }
        })
        .collect();
    names.sort();

    for name in names {
        assert!(check_example(&name), "`keel check {name}.keel` failed");
    }
}

// ---------------------------------------------------------------------------
// Comprehensive showcase — exercises every language feature in one program
// ---------------------------------------------------------------------------

#[test]
fn showcase_runs_end_to_end() {
    ensure_binary_built();
    let (ok, stdout, stderr) = run_example("showcase");
    assert!(
        ok,
        "showcase.keel exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );

    // list + list and list.push produce 4 incidents; count() in interpolation
    assert!(
        stdout.contains("4 incidents in queue"),
        "list concat/push or string interpolation missing:\n{stdout}"
    );

    // All incident IDs present, including the one added via push
    assert!(stdout.contains("INC-101"), "INC-101 missing:\n{stdout}");
    assert!(
        stdout.contains("INC-104"),
        "pushed INC-104 missing:\n{stdout}"
    );

    // @on_stop fired for OnCall before removal
    assert!(
        stdout.contains("OnCall shift complete"),
        "OnCall @on_stop missing:\n{stdout}"
    );

    // Shift summary line present (fallback value in mock mode)
    assert!(
        stdout.contains("Shift summary:"),
        "shift summary line missing:\n{stdout}"
    );
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
    assert!(
        stdout.contains("30"),
        "expected REPL to compute 30, got:\n{stdout}"
    );
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
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
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
        result = Ai.classify("some input", as: Mood) ?? Mood.calm
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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {_stderr}"
    );
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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {_stderr}"
    );
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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {_stderr}"
    );
    assert!(
        stdout.contains("vendor"),
        "vendor field missing from trace\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("amount"),
        "amount field missing from trace\nstdout:\n{stdout}"
    );
    assert!(
        stdout.contains("date"),
        "date field missing from trace\nstdout:\n{stdout}"
    );
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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("doubled=10"),
        "expected 'doubled=10' in stdout:\n{stdout}"
    );
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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
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
fn time_parse_shipped_in_v0_1_14() {
    ensure_binary_built();
    let src = r#"
agent A {
    @on_start {
        p = Time.parse("2026-01-01T00:00:00Z")
        Io.show(p)
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Time.parse should work in v0.1.14\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("2026-01-01"),
        "parsed date should appear:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// v0.1.6 — wiring & ergonomics
// ---------------------------------------------------------------------------

#[test]
fn nested_string_in_interpolation_slot() {
    ensure_binary_built();
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
    ensure_binary_built();
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
fn control_retry_succeeds_on_third_attempt() {
    ensure_binary_built();
    let src = r#"
agent A {
    state { count: int = 0 }
    @on_start {
        result = Control.retry(5, () => {
            self.count = self.count + 1
            if self.count < 3 {
                x = none
                y = x!
                return "won't reach"
            }
            return "ok"
        })
        Io.show("attempts={self.count}")
        Io.show("result={result}")
    }
}
run(A)
"#;
    // The closure raises NullError on the first 2 attempts (via `!` on none)
    // and returns "ok" on the 3rd. Control.retry must catch the runtime
    // errors and re-invoke the closure until success.
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("attempts=3"),
        "expected 3 attempts:\n{stdout}"
    );
    assert!(
        stdout.contains("result=ok"),
        "expected ok result:\n{stdout}"
    );
}

#[test]
fn control_with_timeout_returns_value_on_fast_path() {
    ensure_binary_built();
    let src = r#"
agent A {
    @on_start {
        result = Control.with_timeout(5.seconds, () => {
            return "fast"
        })
        Io.show("result={result}")
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
        stdout.contains("result=fast"),
        "expected fast result:\n{stdout}"
    );
}

#[test]
fn control_with_timeout_aborts_long_call() {
    ensure_binary_built();
    let src = r#"
agent A {
    @on_start {
        Control.with_timeout(1.seconds, () => {
            Async.sleep(5.seconds)
            return "done"
        })
        Io.show("did-not-time-out")
    }
}
run(A)
"#;
    // try/catch is not yet wired in the interpreter, so a tripped timeout
    // surfaces as a non-zero exit. Validate the error is the right one.
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        !ok,
        "expected non-zero exit on timeout\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("TimeoutError"),
        "expected TimeoutError diagnostic:\n{stderr}"
    );
    assert!(
        !stdout.contains("did-not-time-out"),
        "long call must not complete:\n{stdout}"
    );
}

#[test]
fn agent_broadcast_dispatches_to_team_members() {
    ensure_binary_built();
    let src = r#"
agent Alpha {
    @team ["frontline"]
    on alert(msg: str) {
        Io.show("Alpha got {msg}")
    }
}

agent Beta {
    @team ["frontline"]
    on alert(msg: str) {
        Io.show("Beta got {msg}")
    }
}

agent Gamma {
    @team ["backoffice"]
    on alert(msg: str) {
        Io.show("Gamma got {msg}")
    }
}

agent Coordinator {
    @on_start {
        Agent.run(Alpha)
        Agent.run(Beta)
        Agent.run(Gamma)
        Agent.broadcast("frontline", "incident", event: "alert")
    }
}

run(Coordinator)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("Alpha got incident"),
        "Alpha should fire:\n{stdout}"
    );
    assert!(
        stdout.contains("Beta got incident"),
        "Beta should fire:\n{stdout}"
    );
    assert!(
        !stdout.contains("Gamma got"),
        "Gamma must not fire (different team):\n{stdout}"
    );
}

#[test]
fn email_archive_without_config_is_graceful() {
    ensure_binary_built();
    let src = r#"
agent A {
    @on_start {
        msg = {uid: 42, body: "hi", subject: "x", from: "y"}
        Email.archive(msg)
        Io.show("archived")
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
        stdout.contains("archived"),
        "expected archive call to no-op silently:\n{stdout}"
    );
}

#[test]
fn map_get_method_inferred_as_nullable_value() {
    // map.get returns T?, so assigning to a non-nullable should fail check.
    ensure_binary_built();
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
    ensure_binary_built();
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
fn lsp_hover_reports_let_binding_type() {
    use keel_lang::types::checker;
    let src = "agent A {\n    @on_start {\n        items = [1, 2, 3]\n    }\n}\n";
    // Cursor on `items` (line 2, column 8 → byte offset of `items` in source).
    let offset = src.find("items").unwrap() + 1;
    let label = checker::type_at(src, offset).expect("hover should resolve `items`");
    assert!(label.contains("list"), "expected list type, got: {label}");
    assert!(
        label.contains("int"),
        "expected int element type, got: {label}"
    );
}

#[test]
fn lsp_hover_reports_namespace() {
    use keel_lang::types::checker;
    let src = "agent A { @on_start { Io.show(\"x\") } }\n";
    let offset = src.find("Io").unwrap() + 1;
    let label = checker::type_at(src, offset).expect("hover on Io");
    assert!(
        label.contains("namespace"),
        "expected namespace label, got: {label}"
    );
}

// ---------------------------------------------------------------------------
// v0.1.7 — Structured Concurrency & Agent Constraints
// ---------------------------------------------------------------------------

#[test]
fn schedule_cron_accepts_expression() {
    let src = r#"
agent CronTest {
    @on_start {
        Schedule.cron("0 9 * * 1-5", () => {
            Io.show("morning")
        })
        Io.show("cron-parsed")
    }
}
run(CronTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("cron-parsed"),
        "Schedule.cron should accept cron expressions:\n{stdout}"
    );
}

#[test]
fn async_spawn_returns_handle() {
    let src = r#"
agent AsyncTest {
    @on_start {
        h = Async.spawn(() => {
            Io.show("spawned")
        })
        Io.show("spawn-ok")
    }
}
run(AsyncTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("spawn-ok"),
        "Async.spawn should work:\n{stdout}"
    );
}

#[test]
fn tools_capability_gating_parses() {
    let src = r#"
agent RestrictedAgent {
    @tools [Io, Schedule]

    @on_start {
        Io.show("allowed")
    }
}
run(RestrictedAgent)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("allowed"),
        "@tools attribute should parse:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// v0.1.8 — Reactive Agents & Text Processing
// ---------------------------------------------------------------------------

#[test]
fn cache_set_get() {
    let src = r#"
agent CacheTest {
    @on_start {
        Cache.set("key", "value")
        v = Cache.get("key")
        Io.show("got={v}")
    }
}
run(CacheTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("got=value"),
        "Cache.set/get failed:\n{stdout}"
    );
}

#[test]
fn cache_delete() {
    let src = r#"
agent CacheTest {
    @on_start {
        Cache.set("temp", "x")
        Cache.delete("temp")
        v = Cache.get("temp")
        if v == none {
            Io.show("deleted")
        }
    }
}
run(CacheTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("deleted"), "Cache.delete failed:\n{stdout}");
}

#[test]
fn cache_clear() {
    let src = r#"
agent CacheTest {
    @on_start {
        Cache.set("a", "1")
        Cache.set("b", "2")
        Cache.clear()
        v = Cache.get("a")
        if v == none {
            Io.show("cleared")
        }
    }
}
run(CacheTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("cleared"), "Cache.clear failed:\n{stdout}");
}

#[test]
fn str_match_true() {
    let src = r#"
agent StrTest {
    @on_start {
        result = Str.match("hello world", "\\w+")
        if result {
            Io.show("matched")
        }
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("matched"),
        "Str.match true case failed:\n{stdout}"
    );
}

#[test]
fn str_match_false() {
    let src = r#"
agent StrTest {
    @on_start {
        result = Str.match("hello world", "^\\d+$")
        if result {
            Io.show("matched")
        } else {
            Io.show("no-match")
        }
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("no-match"),
        "Str.match false case failed:\n{stdout}"
    );
}

#[test]
fn str_extract() {
    let src = r#"
agent StrTest {
    @on_start {
        v = Str.extract("Total: $99.99", "\\$(\\S+)")
        Io.show("amount={v}")
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("amount=99.99"),
        "Str.extract failed:\n{stdout}"
    );
}

#[test]
fn str_truncate() {
    let src = r#"
agent StrTest {
    @on_start {
        v = Str.truncate("hello world", 5)
        Io.show("short={v}")
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("short=hello…"),
        "Str.truncate failed:\n{stdout}"
    );
}

#[test]
fn str_pad() {
    let src = r#"
agent StrTest {
    @on_start {
        v = Str.pad("42", 5)
        Io.show("padded={v}")
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("padded=   42"), "Str.pad failed:\n{stdout}");
}

#[test]
fn lsp_goto_definition_finds_task() {
    use keel_lang::types::checker;
    let src = "task greet() -> str {\n    \"hello\"\n}\nagent A {\n    @on_start {\n        r = greet()\n    }\n}\n";
    let offset = src.find("greet").unwrap() + 1;
    let span = checker::definition_of(src, offset);
    assert!(
        span.is_some(),
        "definition_of should find `task greet` declaration"
    );
    let s = span.unwrap();
    let name = &src[s.clone()];
    assert_eq!(
        name, "greet",
        "span should cover the identifier, got: {name:?}"
    );
}

#[test]
fn lsp_usages_of_finds_all_occurrences() {
    use keel_lang::types::checker;
    let src = "task foo() -> str { \"x\" }\nagent A { @on_start { r = foo() s = foo() } }\n";
    let spans = checker::usages_of(src, "foo");
    assert!(
        spans.len() >= 3,
        "expected at least 3 occurrences of `foo` (decl + 2 calls), got {}",
        spans.len()
    );
}

// ---------------------------------------------------------------------------
// v0.1.9 — Tooling: keel lint + keel check error quality
// ---------------------------------------------------------------------------

#[test]
fn lint_unused_variable_warns() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    unused = "hello"
    Io.show("done")
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(!ok, "lint should exit non-zero when there are warnings");
    assert!(
        stderr.contains("unused"),
        "expected unused-variable warning:\n{stderr}"
    );
}

#[test]
fn lint_underscore_prefix_suppresses_unused_warning() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    _ignored = "hello"
    Io.show("done")
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(
        ok,
        "underscore-prefixed binding should suppress lint warning:\n{stderr}"
    );
}

#[test]
fn lint_uncalled_task_warns() {
    ensure_binary_built();
    let src = r#"
task unused_helper() {
  Io.show("never")
}
agent A {
  @on_start {
    Io.show("start")
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(!ok, "lint should exit non-zero for uncalled task");
    assert!(
        stderr.contains("unused_helper"),
        "expected uncalled-task warning:\n{stderr}"
    );
}

#[test]
fn lint_called_task_no_warning() {
    ensure_binary_built();
    let src = r#"
task greet() {
  Io.show("hi")
}
agent A {
  @on_start {
    greet()
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(ok, "called task should not produce warnings:\n{stderr}");
}

#[test]
fn lint_ai_call_outside_agent_warns() {
    ensure_binary_built();
    let src = r#"
task process(text: str) -> str {
  result = Ai.summarize(text)
  result ?? "none"
}
agent A {
  @on_start {
    Io.show(process("hi"))
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(!ok, "Ai.* outside agent should produce a warning");
    assert!(
        stderr.contains("outside an agent"),
        "expected outside-agent warning:\n{stderr}"
    );
}

#[test]
fn lint_ai_call_inside_agent_no_warning() {
    ensure_binary_built();
    let src = r#"
agent Assistant {
  @role "helper"
  @model "ollama:llama3.2"

  @on_start {
    result = Ai.summarize("some text")
    Io.show(result ?? "none")
  }
}
run(Assistant)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(ok, "Ai.* inside agent should not warn:\n{stderr}");
}

#[test]
fn lint_state_written_not_read_warns() {
    ensure_binary_built();
    let src = r#"
agent Sink {
  state {
    events: int = 0
  }
  on tick(n: int) {
    self.events = 42
    Io.show("ticked")
  }
}
run(Sink)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(!ok, "write-only state field should produce a warning");
    assert!(
        stderr.contains("events"),
        "expected state-field warning:\n{stderr}"
    );
}

#[test]
fn lint_state_written_and_read_no_warning() {
    ensure_binary_built();
    let src = r#"
agent Counter {
  state {
    count: int = 0
  }
  @on_start {
    self.count = self.count + 1
    Io.show("count ok")
  }
}
run(Counter)
"#;
    let (ok, _stdout, stderr) = lint_inline(src);
    assert!(
        ok,
        "state field written and read should not warn:\n{stderr}"
    );
}

#[test]
fn lint_clean_program_exits_zero() {
    ensure_binary_built();
    let (ok, _stdout, stderr) = lint_inline(
        r#"
task greet(name: str) -> str {
  msg = "Hello, " + name + "!"
  msg
}

agent Greeter {
  state {
    call_count: int = 0
  }

  @on_start {
    result = greet("World")
    Io.show(result)
    self.call_count = self.call_count + 1
    total = self.call_count
    Io.show(total)
  }
}

run(Greeter)
"#,
    );
    assert!(
        ok,
        "clean program should produce no lint warnings:\n{stderr}"
    );
}

#[test]
fn type_error_includes_source_span() {
    ensure_binary_built();
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
    ensure_binary_built();
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

#[test]
fn stop_self_exits_cleanly() {
    ensure_binary_built();
    let src = r#"
agent Greeter {
  @on_start {
    Io.show("hi")
    stop(self)
  }
}
run(Greeter)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("hi"), "expected output:\n{stdout}");
}

#[test]
fn stop_self_resolves_to_current_agent() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    Agent.run(B)
    stop(self)
  }
}
agent B {
  @on_start {
    Io.show("B ran")
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
    assert!(stdout.contains("B ran"), "B should have run:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Memory namespace
// ---------------------------------------------------------------------------

#[test]
fn memory_session_remember_recall() {
    ensure_binary_built();
    let src = r#"
agent A {
  @memory session
  @on_start {
    Memory.remember("name", "Alice")
    val = Memory.recall("name")
    Io.show("got: {val}")
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
        stdout.contains("got: Alice"),
        "recall should return stored value:\n{stdout}"
    );
}

#[test]
fn memory_session_recall_missing_returns_none() {
    ensure_binary_built();
    let src = r#"
agent A {
  @memory session
  @on_start {
    val = Memory.recall("nonexistent")
    if val == none {
      Io.show("was none")
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
        stdout.contains("was none"),
        "missing key should return none:\n{stdout}"
    );
}

#[test]
fn memory_session_forget() {
    ensure_binary_built();
    let src = r#"
agent A {
  @memory session
  @on_start {
    Memory.remember("x", "hello")
    Memory.forget("x")
    val = Memory.recall("x")
    if val == none {
      Io.show("forgotten")
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
        stdout.contains("forgotten"),
        "forget should remove key:\n{stdout}"
    );
}

#[test]
fn memory_none_raises_capability_error() {
    ensure_binary_built();
    let src = r#"
agent A {
  @memory none
  @on_start {
    Memory.remember("x", "y")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected CapabilityError for @memory none");
    assert!(
        stderr.contains("CapabilityError"),
        "expected CapabilityError in stderr:\n{stderr}"
    );
}

#[test]
fn memory_default_mode_is_session() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    Memory.remember("k", "v")
    val = Memory.recall("k")
    Io.show("val: {val}")
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
        stdout.contains("val: v"),
        "default mode should act as session:\n{stdout}"
    );
}

#[test]
fn memory_persistent_survives_process_boundary() {
    use std::io::Write as _;
    ensure_binary_built();
    let home = tempfile::tempdir().expect("tempdir");
    // Both runs must use the same file path so they share the same program stem.
    let prog = home.path().join("memory_test.keel");

    let write_src = r#"
agent A {
  @memory persistent
  @on_start {
    Memory.remember("greeting", "hello-persistent")
    stop(self)
  }
}
run(A)
"#;
    std::fs::File::create(&prog)
        .unwrap()
        .write_all(write_src.as_bytes())
        .unwrap();
    let bin = keel_binary();
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("run keel");
    assert!(
        out.status.success(),
        "write run failed\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let read_src = r#"
agent A {
  @memory persistent
  @on_start {
    val = Memory.recall("greeting")
    Io.show("recalled: {val}")
    stop(self)
  }
}
run(A)
"#;
    std::fs::File::create(&prog)
        .unwrap()
        .write_all(read_src.as_bytes())
        .unwrap();
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("run keel");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "read run failed\nstderr: {stderr}");
    assert!(
        stdout.contains("recalled: hello-persistent"),
        "persistent value should survive process boundary:\n{stdout}"
    );
}

#[test]
fn memory_unknown_mode_raises_error() {
    ensure_binary_built();
    let src = r#"
agent A {
  @memory unknown_mode
  @on_start {
    Memory.remember("x", "y")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected error for unrecognized @memory value");
    assert!(
        stderr.contains("unrecognized") || stderr.contains("unknown_mode"),
        "expected diagnostic naming the bad value:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Memory v0.1.11 — identity hash, flock, path safety
// ---------------------------------------------------------------------------

#[test]
fn memory_isolation_same_basename_different_paths() {
    // Two counter.keel files in different directories must have separate memory.
    ensure_binary_built();
    let home = tempfile::tempdir().expect("tempdir");
    let dir_a = tempfile::tempdir().expect("tempdir_a");
    let dir_b = tempfile::tempdir().expect("tempdir_b");
    let prog_a = dir_a.path().join("counter.keel");
    let prog_b = dir_b.path().join("counter.keel");
    let src_a = r#"
agent Ctr {
  @memory persistent
  @on_start {
    Memory.remember("v", "from_a")
    stop(self)
  }
}
run(Ctr)
"#;
    let src_b = r#"
agent Ctr {
  @memory persistent
  @on_start {
    Memory.remember("v", "from_b")
    stop(self)
  }
}
run(Ctr)
"#;
    std::fs::write(&prog_a, src_a).unwrap();
    std::fs::write(&prog_b, src_b).unwrap();
    let bin = keel_binary();
    let run_prog = |prog: &std::path::Path| -> (bool, String, String) {
        let out = Command::new(&bin)
            .env("KEEL_ONESHOT", "1")
            .env("KEEL_LLM", "mock")
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .arg("run")
            .arg(prog)
            .output()
            .expect("run");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let (ok_a, _, se_a) = run_prog(&prog_a);
    let (ok_b, _, se_b) = run_prog(&prog_b);
    assert!(ok_a, "prog_a write failed: {se_a}");
    assert!(ok_b, "prog_b write failed: {se_b}");

    // Now recall from each — they must return their own values.
    let recall_src = r#"
agent Ctr {
  @memory persistent
  @on_start {
    val = Memory.recall("v")
    Io.show("val: {val}")
    stop(self)
  }
}
run(Ctr)
"#;
    std::fs::write(&prog_a, recall_src).unwrap();
    std::fs::write(&prog_b, recall_src).unwrap();
    let (ok_ra, out_a, se_ra) = run_prog(&prog_a);
    let (ok_rb, out_b, se_rb) = run_prog(&prog_b);
    assert!(ok_ra, "prog_a recall failed: {se_ra}");
    assert!(ok_rb, "prog_b recall failed: {se_rb}");
    assert!(
        out_a.contains("val: from_a"),
        "prog_a should recall from_a:\n{out_a}"
    );
    assert!(
        out_b.contains("val: from_b"),
        "prog_b should recall from_b:\n{out_b}"
    );
}

#[test]
fn memory_repl_namespace_distinct_from_files() {
    // A file named repl.keel must use "repl_<hash12>", not "__repl__".
    ensure_binary_built();
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let prog = src_dir.path().join("repl.keel");
    std::fs::write(
        &prog,
        r#"
agent Tester {
  @memory persistent
  @on_start {
    Memory.remember("ns", "file")
    stop(self)
  }
}
run(Tester)
"#,
    )
    .unwrap();
    let bin = keel_binary();
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let memory_root = home.path().join(".keel").join("memory");
    let entries: Vec<_> = std::fs::read_dir(&memory_root)
        .expect("read memory dir")
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one memory dir");
    let dir_name = entries[0].file_name();
    let name = dir_name.to_string_lossy();
    assert!(
        name.starts_with("repl_") && name != "__repl__",
        "file-based repl.keel must use 'repl_<hash>', not '__repl__': got {name}"
    );
}

#[test]
fn memory_symlink_resolves_to_same_storage() {
    // Running via a symlink and via the target must share the same memory.
    ensure_binary_built();
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let orig = src_dir.path().join("original.keel");
    let link = src_dir.path().join("symlink.keel");
    std::fs::write(
        &orig,
        r#"
agent Sym {
  @memory persistent
  @on_start {
    Memory.remember("key", "stored_via_symlink")
    stop(self)
  }
}
run(Sym)
"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(&orig, &link).expect("create symlink");
    let bin = keel_binary();
    // Write via symlink.
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&link)
        .output()
        .expect("run via symlink");
    assert!(
        out.status.success(),
        "symlink run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Recall via the original file.
    std::fs::write(
        &orig,
        r#"
agent Sym {
  @memory persistent
  @on_start {
    val = Memory.recall("key")
    Io.show("got: {val}")
    stop(self)
  }
}
run(Sym)
"#,
    )
    .unwrap();
    let out2 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&orig)
        .output()
        .expect("run via original");
    let stdout = String::from_utf8_lossy(&out2.stdout).into_owned();
    assert!(
        out2.status.success(),
        "original run failed: {}",
        String::from_utf8_lossy(&out2.stderr)
    );
    assert!(
        stdout.contains("got: stored_via_symlink"),
        "original should see memory written via symlink:\n{stdout}"
    );
}

#[test]
fn memory_cross_process_write_race() {
    // Two concurrent keel processes writing to the same persistent store must
    // not corrupt the JSON file. flock guarantees each individual write is
    // atomic; both processes must complete without errors.
    // Note: recall+remember is NOT a single locked operation, so the logical
    // counter value is not guaranteed to be 10 — only file integrity is.
    ensure_binary_built();
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let prog = src_dir.path().join("race_counter.keel");
    std::fs::write(
        &prog,
        r#"
agent Counter {
  @memory persistent
  @on_start {
    for item in [1, 2, 3, 4, 5] {
      Memory.remember("last", item)
    }
    stop(self)
  }
}
run(Counter)
"#,
    )
    .unwrap();
    let bin = keel_binary();
    let mut p1 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .spawn()
        .expect("spawn p1");
    let mut p2 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .spawn()
        .expect("spawn p2");
    let s1 = p1.wait().expect("wait p1");
    let s2 = p2.wait().expect("wait p2");
    assert!(s1.success(), "process 1 failed");
    assert!(s2.success(), "process 2 failed");
    // Verify the JSON file is valid (flock prevented any torn write).
    let memory_root = home.path().join(".keel").join("memory");
    let mut found = false;
    for entry in std::fs::read_dir(&memory_root).expect("read memory dir") {
        let entry = entry.unwrap();
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("race_counter_")
        {
            let json_path = entry.path().join("Counter.json");
            if json_path.exists() {
                let content = std::fs::read_to_string(&json_path).unwrap();
                let json: serde_json::Value = serde_json::from_str(&content)
                    .expect("JSON must be valid after concurrent writes");
                assert!(json.is_object(), "memory must be a JSON object");
                let last = json["last"].as_i64().unwrap_or(-1);
                assert!(
                    (1..=5).contains(&last),
                    "last must be 1-5 (written by one of the processes), got: {last}"
                );
                found = true;
            }
        }
    }
    assert!(found, "Counter.json not found in {}", memory_root.display());
}

#[test]
fn memory_concurrent_reads_dont_block() {
    // Two processes holding shared locks on the same memory file must both succeed.
    ensure_binary_built();
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let prog = src_dir.path().join("read_test.keel");
    std::fs::write(
        &prog,
        r#"
agent Reader {
  @memory persistent
  @on_start {
    Memory.remember("msg", "shared")
    stop(self)
  }
}
run(Reader)
"#,
    )
    .unwrap();
    let bin = keel_binary();
    // Setup: write the initial value.
    Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("setup write");
    std::fs::write(
        &prog,
        r#"
agent Reader {
  @memory persistent
  @on_start {
    val = Memory.recall("msg")
    Io.show("val: {val}")
    stop(self)
  }
}
run(Reader)
"#,
    )
    .unwrap();
    let mut p1 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .spawn()
        .expect("spawn p1");
    let mut p2 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .spawn()
        .expect("spawn p2");
    let s1 = p1.wait().expect("wait p1");
    let s2 = p2.wait().expect("wait p2");
    assert!(s1.success(), "concurrent reader p1 failed");
    assert!(s2.success(), "concurrent reader p2 failed");
}

#[test]
fn memory_lockfile_exists_alongside_data() {
    // After a persistent remember, both <agent>.json and <agent>.lock must exist.
    ensure_binary_built();
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let prog = src_dir.path().join("lock_test.keel");
    std::fs::write(
        &prog,
        r#"
agent LockTester {
  @memory persistent
  @on_start {
    Memory.remember("k", "v")
    stop(self)
  }
}
run(LockTester)
"#,
    )
    .unwrap();
    let bin = keel_binary();
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let memory_root = home.path().join(".keel").join("memory");
    let mut json_found = false;
    let mut lock_found = false;
    for entry in std::fs::read_dir(&memory_root).expect("read memory dir") {
        let entry = entry.unwrap();
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("lock_test_")
        {
            let dir = entry.path();
            json_found = dir.join("LockTester.json").exists();
            lock_found = dir.join("LockTester.lock").exists();
        }
    }
    assert!(json_found, "LockTester.json should exist after remember");
    assert!(
        lock_found,
        "LockTester.lock should exist alongside data file"
    );
}

#[test]
fn memory_corrupt_file_renamed_to_bak() {
    // A corrupt JSON file must be renamed to .bak and an error returned.
    ensure_binary_built();
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let prog = src_dir.path().join("corrupt_test.keel");
    std::fs::write(
        &prog,
        r#"
agent CT {
  @memory persistent
  @on_start {
    Memory.remember("k", "v")
    stop(self)
  }
}
run(CT)
"#,
    )
    .unwrap();
    let bin = keel_binary();
    // First run: create the memory file.
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("first run");
    assert!(
        out.status.success(),
        "first run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Locate and corrupt the JSON file.
    let memory_root = home.path().join(".keel").join("memory");
    let mut json_path: Option<std::path::PathBuf> = None;
    for entry in std::fs::read_dir(&memory_root).unwrap() {
        let entry = entry.unwrap();
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("corrupt_test_")
        {
            let p = entry.path().join("CT.json");
            if p.exists() {
                json_path = Some(p);
            }
        }
    }
    let json_path = json_path.expect("CT.json not found after first run");
    std::fs::write(&json_path, b"not valid json {{{ broken").unwrap();
    // Second run: corrupt file must be renamed to .bak and the run must fail.
    let out2 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("second run");
    assert!(
        !out2.status.success(),
        "second run should fail on corrupt file"
    );
    let bak = json_path.with_extension("json.bak");
    assert!(bak.exists(), ".bak should exist after corrupt-file rename");
    assert!(
        !json_path.exists(),
        ".json should be gone after rename to .bak"
    );
}

// ---------------------------------------------------------------------------
// v0.1.12 — Range operator `..`
// ---------------------------------------------------------------------------

#[test]
fn range_basic_for_loop() {
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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

#[test]
fn examples_all_parse_includes_destructure() {
    ensure_binary_built();
    assert!(
        check_example("destructure"),
        "`keel check destructure.keel` failed"
    );
}

// ---------------------------------------------------------------------------
// v0.1.14 — if guards (for loops and when arms)
// ---------------------------------------------------------------------------

#[test]
fn if_guard_for_filters_elements() {
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
// v0.1.14 — Time namespace
// ---------------------------------------------------------------------------

#[test]
fn time_now_returns_iso_string() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    t = Time.now()
    Io.show(t)
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
        stdout.contains('T') && (stdout.contains('+') || stdout.contains('Z')),
        "Time.now() should return RFC 3339:\n{stdout}"
    );
}

#[test]
fn time_parse_normalises_date() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    p = Time.parse("2026-05-01T00:00:00Z")
    Io.show(p)
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
        stdout.contains("2026-05-01"),
        "parsed date should appear:\n{stdout}"
    );
}

#[test]
fn time_parse_rejects_naive_without_tz() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    p = Time.parse("2026-05-01")
    if p == none {
      Io.show("none")
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
        stdout.contains("none"),
        "naive parse without tz should return none:\n{stdout}"
    );
}

#[test]
fn time_parse_with_tz_coerces_naive() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    p = Time.parse("2026-05-01", tz: "UTC")
    Io.show(p)
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
        stdout.contains("2026-05-01"),
        "naive parse with tz: should succeed:\n{stdout}"
    );
}

#[test]
fn time_format_strftime() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    dt = Time.parse("2026-05-01T09:30:00Z")
    s = dt.format(as: "%Y-%m-%d")
    Io.show(s)
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
        stdout.contains("2026-05-01"),
        "formatted date should appear:\n{stdout}"
    );
}

#[test]
fn time_diff_one_day() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    a = Time.parse("2026-05-02T00:00:00Z")
    b = Time.parse("2026-05-01T00:00:00Z")
    d = a - b
    Io.show(d)
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
        stdout.contains("1 days") || stdout.contains("86400"),
        "diff should be 1 day:\n{stdout}"
    );
}

#[test]
fn time_parts_returns_map() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    dt = Time.parse("2026-05-06T14:30:45Z")
    p = dt.parts()
    Io.show(p.year)
    Io.show(p.month)
    Io.show(p.day)
    Io.show(p.hour)
    Io.show(p.tz)
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
    assert!(stdout.contains("2026"), "year should be 2026:\n{stdout}");
    assert!(stdout.contains("5"), "month should be 5:\n{stdout}");
    assert!(stdout.contains("6"), "day should be 6:\n{stdout}");
    assert!(stdout.contains("14"), "hour should be 14:\n{stdout}");
    assert!(
        stdout.contains("+00:00"),
        "tz should be UTC offset:\n{stdout}"
    );
}

#[test]
fn time_now_with_tz() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    t = Time.now(tz: "Europe/Paris")
    Io.show(t)
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
    // Paris is UTC+1 or UTC+2 — either offset appears
    assert!(
        stdout.contains("+01:00") || stdout.contains("+02:00"),
        "Time.now(tz: Paris) should emit a European offset:\n{stdout}"
    );
}

#[test]
fn time_datetime_arithmetic() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    base = Time.parse("2026-05-01T00:00:00Z")
    future = base + 1.days
    Io.show(future)
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
        stdout.contains("2026-05-02"),
        "adding 1 day should yield May 2:\n{stdout}"
    );
}

#[test]
fn time_datetime_comparison() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    a = Time.parse("2026-05-02T00:00:00Z")
    b = Time.parse("2026-05-01T00:00:00Z")
    if a > b {
      Io.show("later")
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
        stdout.contains("later"),
        "May 2 should be > May 1:\n{stdout}"
    );
}

#[test]
fn millisecond_duration_literal() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    d = 500.ms
    Io.show(d)
    d2 = 1500.millis
    Io.show(d2)
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
        stdout.contains("0.5"),
        "500.ms should equal 0.5 seconds:\n{stdout}"
    );
    assert!(
        stdout.contains("1.5"),
        "1500.millis should equal 1.5 seconds:\n{stdout}"
    );
}

#[test]
fn time_now_emits_millisecond_precision() {
    ensure_binary_built();
    let src = r#"
agent A {
  @on_start {
    t = Time.now()
    Io.show(t)
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
    // RFC 3339 with milliseconds: contains a dot before Z or +
    assert!(
        stdout.contains('.') && (stdout.contains('Z') || stdout.contains('+')),
        "Time.now() should emit millisecond-precision RFC 3339:\n{stdout}"
    );
}

#[test]
fn examples_all_parse_includes_if_guard_and_time() {
    ensure_binary_built();
    assert!(
        check_example("if_guard"),
        "`keel check if_guard.keel` failed"
    );
    assert!(
        check_example("time_basic"),
        "`keel check time_basic.keel` failed"
    );
}

// ---------------------------------------------------------------------------
// try/catch + AiError typed errors
// ---------------------------------------------------------------------------

#[test]
fn try_catch_catches_ai_schema_error() {
    ensure_binary_built();
    // Trigger a NullError inside a try block and confirm the catch clause
    // runs and execution continues normally after try/catch.
    let src = r#"
agent A {
  @role "tester"
  @on_start {
    try {
      val = Env.get("__KEEL_TEST_NONEXISTENT_VAR__")
      x = val!
      Io.show("try body completed")
    } catch err: Error {
      Io.show("caught: {err.message}")
    }
    Io.show("done")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("caught:"),
        "catch block not reached:\n{stdout}"
    );
    assert!(
        !stdout.contains("try body completed"),
        "try body should have thrown:\n{stdout}"
    );
    assert!(
        stdout.contains("done"),
        "execution did not continue after catch:\n{stdout}"
    );
}

#[test]
fn try_catch_reraises_unmatched_error() {
    ensure_binary_built();
    // A catch clause that doesn't match the thrown type re-propagates.
    // Here we throw a NullError but only catch NetworkError — expect failure.
    let src = r#"
agent A {
  @role "tester"
  @on_start {
    try {
      val = Env.get("__KEEL_TEST_NONEXISTENT_VAR__")
      x = val!
    } catch err: NetworkError {
      Io.show("should not reach")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, _stderr) = run_inline(src, true);
    assert!(
        !ok,
        "unmatched catch should propagate error and exit non-zero"
    );
}

#[test]
fn try_catch_error_binding_has_message() {
    ensure_binary_built();
    let src = r#"
agent A {
  @role "tester"
  @on_start {
    try {
      val = Env.get("__KEEL_TEST_NONEXISTENT_VAR__")
      x = val!
    } catch err: Error {
      Io.show(err.message)
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stdout.trim().is_empty(),
        "err.message should be non-empty:\n{stdout}"
    );
}

#[test]
fn ai_classify_null_coalesces_in_mock_mode() {
    ensure_binary_built();
    // In mock mode, classify() returns none (call failed gracefully).
    // The ?? operator should provide the default without an error.
    let src = r#"
type Mood = happy | sad | neutral

agent A {
  @role "tester"
  @on_start {
    result = Ai.classify("hello", as: Mood) ?? Mood.neutral
    Io.show("{result}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("neutral"), "?? default not used:\n{stdout}");
}

#[test]
fn examples_all_parse_includes_ai_error() {
    ensure_binary_built();
    assert!(
        check_example("ai_error"),
        "`keel check ai_error.keel` failed"
    );
}

// ---------------------------------------------------------------------------
// List operations — any, all, find, reduce, sum, min, max, join, sort,
//                   reverse, flatten, take, skip
// ---------------------------------------------------------------------------

#[test]
fn list_any_returns_true_when_predicate_matches() {
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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
    ensure_binary_built();
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

#[test]
fn examples_all_parse_includes_list_ops() {
    ensure_binary_built();
    assert!(
        check_example("list_ops"),
        "`keel check list_ops.keel` failed"
    );
}
