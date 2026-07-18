//! End-to-end tests for `keel dap` / `keel test --debug`, driving the DAP
//! protocol over the compiled binary's stdio exactly as a real client
//! (VS Code, `lldb-dap`-style adapters) would.

use std::io::{BufRead, BufReader, Read, Write};
use std::process::{Child, ChildStdin, ChildStdout, Command, Stdio};
use std::sync::mpsc;
use std::time::Duration;

use serde_json::{Value, json};

use crate::common::keel_binary;

/// A DAP client over one `keel dap`/`keel test --debug` subprocess: sends
/// framed requests on stdin, and reads framed responses/events off a
/// background thread so waiting for a response and waiting for an
/// interleaved event never blocks each other.
struct DapClient {
    child: Child,
    stdin: ChildStdin,
    seq: i64,
    events: mpsc::Receiver<Value>,
    responses: mpsc::Receiver<Value>,
}

impl DapClient {
    fn spawn(args: &[&str]) -> Self {
        Self::spawn_with_env(args, &[])
    }

    /// Like `spawn`, but with extra environment variables set on top of the
    /// baseline `KEEL_LLM=mock`. Agent-driven programs need `KEEL_ONESHOT=1`
    /// so the process exits once idle instead of waiting on its event loop
    /// forever (no test here ever calls `stop(...)` explicitly).
    fn spawn_with_env(args: &[&str], extra_env: &[(&str, &str)]) -> Self {
        let mut command = Command::new(keel_binary());
        command
            .args(args)
            .env("KEEL_LLM", "mock")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        for (key, value) in extra_env {
            command.env(key, value);
        }
        let mut child = command.spawn().expect("spawn keel dap");
        let stdin = child.stdin.take().expect("child stdin");
        let stdout = child.stdout.take().expect("child stdout");

        let (event_tx, event_rx) = mpsc::channel();
        let (resp_tx, resp_rx) = mpsc::channel();
        std::thread::spawn(move || read_messages(stdout, event_tx, resp_tx));

        Self {
            child,
            stdin,
            seq: 0,
            events: event_rx,
            responses: resp_rx,
        }
    }

    fn send(&mut self, command: &str, arguments: Value) -> i64 {
        self.seq += 1;
        let msg = json!({
            "seq": self.seq,
            "type": "request",
            "command": command,
            "arguments": arguments,
        });
        let text = serde_json::to_string(&msg).unwrap();
        write!(self.stdin, "Content-Length: {}\r\n\r\n{}", text.len(), text).unwrap();
        self.stdin.flush().unwrap();
        self.seq
    }

    fn wait_response(&self, request_seq: i64) -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(
                !remaining.is_zero(),
                "timed out waiting for response to seq {request_seq}"
            );
            let msg = self
                .responses
                .recv_timeout(remaining)
                .expect("response channel closed before seq arrived");
            if msg["request_seq"] == request_seq {
                return msg;
            }
        }
    }

    fn wait_event(&self, name: &str) -> Value {
        let deadline = std::time::Instant::now() + Duration::from_secs(10);
        loop {
            let remaining = deadline.saturating_duration_since(std::time::Instant::now());
            assert!(!remaining.is_zero(), "timed out waiting for event {name}");
            let msg = self
                .events
                .recv_timeout(remaining)
                .expect("event channel closed before it arrived");
            if msg["event"] == name {
                return msg;
            }
        }
    }

    /// Run the standard handshake (`initialize` → `launch` → `setBreakpoints`
    /// for each `(path, line)` pair → `configurationDone`) and wait for the
    /// first `stopped` event.
    fn start_and_stop_at(&mut self, breakpoints: &[(&str, u32)]) -> Value {
        self.start_and_stop_at_with_init(json!({}), breakpoints)
    }

    /// Like `start_and_stop_at`, but with caller-supplied `initialize`
    /// arguments (e.g. `linesStartAt1`/`columnsStartAt1`) instead of the
    /// default empty object.
    fn start_and_stop_at_with_init(
        &mut self,
        init_args: Value,
        breakpoints: &[(&str, u32)],
    ) -> Value {
        let s = self.send("initialize", init_args);
        assert_eq!(self.wait_response(s)["success"], true);

        let s = self.send("launch", json!({}));
        assert_eq!(self.wait_response(s)["success"], true);
        self.wait_event("initialized");

        for (path, line) in breakpoints {
            let s = self.send(
                "setBreakpoints",
                json!({"source": {"path": path}, "breakpoints": [{"line": line}]}),
            );
            let r = self.wait_response(s);
            assert_eq!(r["body"]["breakpoints"][0]["verified"], true);
        }

        let s = self.send("configurationDone", json!({}));
        assert_eq!(self.wait_response(s)["success"], true);

        self.wait_event("stopped")
    }

    fn resume_and_wait_exit(&mut self) -> std::process::ExitStatus {
        let s = self.send("continue", json!({"threadId": 1}));
        self.wait_response(s);
        self.wait_event("terminated");
        self.child
            .wait_timeout(Duration::from_secs(10))
            .expect("keel dap process to exit")
    }
}

/// Small local extension so `resume_and_wait_exit` can bound how long it
/// waits for the child to exit, instead of blocking indefinitely.
trait WaitTimeout {
    fn wait_timeout(&mut self, timeout: Duration) -> std::io::Result<std::process::ExitStatus>;
}

impl WaitTimeout for Child {
    fn wait_timeout(&mut self, timeout: Duration) -> std::io::Result<std::process::ExitStatus> {
        let deadline = std::time::Instant::now() + timeout;
        loop {
            if let Some(status) = self.try_wait()? {
                return Ok(status);
            }
            if std::time::Instant::now() >= deadline {
                let _ = self.kill();
                panic!("keel dap process did not exit within {timeout:?}");
            }
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

fn read_messages(stdout: ChildStdout, events: mpsc::Sender<Value>, responses: mpsc::Sender<Value>) {
    let mut reader = BufReader::new(stdout);
    loop {
        let mut content_length: Option<usize> = None;
        loop {
            let mut line = String::new();
            match reader.read_line(&mut line) {
                Ok(0) | Err(_) => return,
                Ok(_) => {}
            }
            let line = line.trim_end();
            if line.is_empty() {
                break;
            }
            if let Some(value) = line.strip_prefix("Content-Length:") {
                content_length = value.trim().parse().ok();
            }
        }
        let Some(len) = content_length else { return };
        let mut buf = vec![0u8; len];
        if reader.read_exact(&mut buf).is_err() {
            return;
        }
        let Ok(msg) = serde_json::from_slice::<Value>(&buf) else {
            continue;
        };
        match msg["type"].as_str() {
            Some("event") => {
                let _ = events.send(msg);
            }
            Some("response") => {
                let _ = responses.send(msg);
            }
            _ => {}
        }
    }
}

fn write_keel_file(dir: &std::path::Path, name: &str, source: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    std::fs::write(&path, source).expect("write test .keel file");
    path.canonicalize().expect("canonicalize test file path")
}

#[test]
fn breakpoint_step_variables_and_evaluate_work_end_to_end() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_keel_file(
        dir.path(),
        "main.keel",
        r#"
task add(a: int, b: int) -> int {
  sum = a + b
  return sum
}

x = 1
y = 2
z = add(x, y)
"#,
    );
    let path_str = path.to_str().unwrap().to_string();

    let mut client = DapClient::spawn(&["dap", &path_str]);
    let stopped = client.start_and_stop_at(&[(&path_str, 3)]);
    assert_eq!(stopped["body"]["reason"], "breakpoint");

    let s = client.send("stackTrace", json!({"threadId": 1}));
    let frames = client.wait_response(s);
    let frame = &frames["body"]["stackFrames"][0];
    assert_eq!(frame["name"], "add");
    assert_eq!(frame["line"], 3);
    assert_eq!(frame["source"]["path"], path_str);

    let s = client.send("scopes", json!({"frameId": 0}));
    let scopes = client.wait_response(s);
    let locals_ref = scopes["body"]["scopes"][0]["variablesReference"].clone();

    let s = client.send("variables", json!({"variablesReference": locals_ref}));
    let vars = client.wait_response(s);
    let names: Vec<&str> = vars["body"]["variables"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v["name"].as_str().unwrap())
        .collect();
    assert_eq!(names, vec!["a", "b"]);
    assert_eq!(vars["body"]["variables"][0]["value"], "1");
    assert_eq!(vars["body"]["variables"][1]["value"], "2");

    let s = client.send(
        "evaluate",
        json!({"expression": "a + b", "frameId": 0, "context": "watch"}),
    );
    let evaluated = client.wait_response(s);
    assert_eq!(evaluated["body"]["result"], "3");
    assert_eq!(evaluated["body"]["type"], "int");

    // Step over `sum = a + b` and confirm we land on the next line, still
    // inside the same frame.
    let s = client.send("next", json!({"threadId": 1}));
    client.wait_response(s);
    client.wait_event("stopped");
    let s = client.send("stackTrace", json!({"threadId": 1}));
    let frames = client.wait_response(s);
    assert_eq!(frames["body"]["stackFrames"][0]["line"], 4);

    let status = client.resume_and_wait_exit();
    assert!(status.success());
}

#[test]
fn zero_indexed_client_receives_zero_indexed_lines_and_columns() {
    // Regression test: the server used to always assume 1-indexed
    // lines/columns, ignoring `initialize`'s `linesStartAt1`/
    // `columnsStartAt1` capabilities entirely. A client that declares
    // 0-indexed lines sends breakpoint requests in that convention and
    // expects `stackTrace` to answer in the same convention.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_keel_file(
        dir.path(),
        "main.keel",
        r#"
task add(a: int, b: int) -> int {
  sum = a + b
  return sum
}

x = 1
y = 2
z = add(x, y)
"#,
    );
    let path_str = path.to_str().unwrap().to_string();

    let mut client = DapClient::spawn(&["dap", &path_str]);
    // Internal line 3 (`sum = a + b`) is line 2 in a 0-indexed convention.
    let stopped = client.start_and_stop_at_with_init(
        json!({"linesStartAt1": false, "columnsStartAt1": false}),
        &[(&path_str, 2)],
    );
    assert_eq!(stopped["body"]["reason"], "breakpoint");

    let s = client.send("stackTrace", json!({"threadId": 1}));
    let frames = client.wait_response(s);
    let frame = &frames["body"]["stackFrames"][0];
    assert_eq!(
        frame["line"], 2,
        "stackTrace must echo the client's 0-indexed convention, not the server's internal 1-indexed one"
    );
    assert_eq!(frame["column"], 0);

    let status = client.resume_and_wait_exit();
    assert!(status.success());
}

#[test]
fn breakpoint_in_an_imported_module_reports_that_module_as_the_source() {
    let dir = tempfile::tempdir().expect("tempdir");
    let lib_path = write_keel_file(
        dir.path(),
        "lib.keel",
        "task helper(n: int) -> int {\n  doubled = n * 2\n  return doubled\n}\n",
    );
    let main_path = write_keel_file(
        dir.path(),
        "main.keel",
        "use helper from \"./lib.keel\"\nresult = helper(21)\n",
    );
    let lib_str = lib_path.to_str().unwrap().to_string();
    let main_str = main_path.to_str().unwrap().to_string();

    let mut client = DapClient::spawn(&["dap", &main_str]);
    client.start_and_stop_at(&[(&lib_str, 2)]);

    let s = client.send("stackTrace", json!({"threadId": 1}));
    let frames = client.wait_response(s);
    let frame = &frames["body"]["stackFrames"][0];
    assert_eq!(
        frame["source"]["path"], lib_str,
        "breakpoint should attribute to the imported module, not the entry file"
    );
    assert_eq!(frame["name"], "helper");

    let s = client.send("scopes", json!({"frameId": 0}));
    let scopes = client.wait_response(s);
    let locals_ref = scopes["body"]["scopes"][0]["variablesReference"].clone();
    let s = client.send("variables", json!({"variablesReference": locals_ref}));
    let vars = client.wait_response(s);
    assert_eq!(vars["body"]["variables"][0]["name"], "n");
    assert_eq!(vars["body"]["variables"][0]["value"], "21");

    let status = client.resume_and_wait_exit();
    assert!(status.success());
}

#[test]
fn test_debug_requires_filter_to_match_exactly_one_test() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_keel_file(
        dir.path(),
        "suite.keel",
        r#"
task square(n: int) -> int {
  return n * n
}

test "first" {
  assert square(2) == 4
}

test "second" {
  assert square(3) == 9
}
"#,
    );
    let path_str = path.to_str().unwrap().to_string();

    // No --filter: zero tests match "the one test" requirement.
    let output = Command::new(keel_binary())
        .env("KEEL_LLM", "mock")
        .args(["test", "--debug", &path_str])
        .output()
        .expect("run keel test --debug");
    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("exactly one test"),
        "expected the exactly-one-test error, got: {stderr}"
    );
}

#[test]
fn test_debug_runs_the_filtered_test_under_a_breakpoint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_keel_file(
        dir.path(),
        "suite.keel",
        r#"
task square(n: int) -> int {
  return n * n
}

test "square works" {
  v = square(5)
  assert v == 25
}
"#,
    );
    let path_str = path.to_str().unwrap().to_string();

    let mut client = DapClient::spawn(&["test", "--debug", "--filter", "square works", &path_str]);
    let stopped = client.start_and_stop_at(&[(&path_str, 8)]);
    assert_eq!(stopped["body"]["reason"], "breakpoint");

    let s = client.send("evaluate", json!({"expression": "v", "frameId": 0}));
    let evaluated = client.wait_response(s);
    assert_eq!(evaluated["body"]["result"], "25");

    let s = client.send("continue", json!({"threadId": 1}));
    client.wait_response(s);
    let output_event = client.wait_event("output");
    assert!(
        output_event["body"]["output"]
            .as_str()
            .unwrap()
            .contains("PASS"),
        "expected a PASS output event, got: {output_event}"
    );
    client.wait_event("terminated");
    let status = client
        .child
        .wait_timeout(Duration::from_secs(10))
        .expect("process to exit");
    assert!(status.success());
}

#[test]
fn evaluate_calling_a_task_with_its_own_breakpoint_does_not_reenter_the_pause_loop() {
    // Regression test: `evaluate`'s expression can itself call a task, whose
    // body runs back through the same `DebugHook::on_statement` this pause
    // is already inside. If that task's body has its own breakpoint (a
    // perfectly normal thing to have set), the hook must not try to pause a
    // second time — doing so overwrites the single `paused_tx` slot and
    // either deadlocks (the outer `evaluate` response never arrives, since
    // the interpreter is now stuck servicing the inner pause loop instead)
    // or misdirects later commands to the wrong paused frame.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_keel_file(
        dir.path(),
        "main.keel",
        r#"
task add(a: int, b: int) -> int {
  sum = a + b
  return sum
}

x = 1
y = 2
z = add(x, y)
"#,
    );
    let path_str = path.to_str().unwrap().to_string();

    // Breakpoint 3 is the outer stop (inside the first, real `add` call);
    // it also happens to sit inside `add`'s body, so any nested call the
    // `evaluate` below makes hits the very same line.
    let mut client = DapClient::spawn(&["dap", &path_str]);
    client.start_and_stop_at(&[(&path_str, 3)]);

    let s = client.send(
        "evaluate",
        json!({"expression": "add(5, 6)", "frameId": 0, "context": "watch"}),
    );
    // A pre-fix deadlock would hang here until `wait_response`'s own 10s
    // timeout panics — that failure mode alone proves the bug. Success
    // additionally checks the nested call actually computed the right
    // value instead of silently short-circuiting.
    let evaluated = client.wait_response(s);
    assert_eq!(evaluated["body"]["result"], "11");

    // The reentrant call must not have emitted a second `stopped` event —
    // `on_statement`'s reentrancy guard returns before ever calling
    // `transport.emit`.
    assert!(
        matches!(client.events.try_recv(), Err(mpsc::TryRecvError::Empty)),
        "evaluate must not trigger a second `stopped` event"
    );

    // The outer pause's `paused_tx` must be intact — `continue` should
    // resume the real, outer frame and let the program finish normally.
    let status = client.resume_and_wait_exit();
    assert!(status.success());
}

#[test]
fn set_breakpoints_can_be_updated_after_configuration_done() {
    // Regression test: `setBreakpoints` arriving after `configurationDone`
    // (VS Code sends this routinely — e.g. toggling a breakpoint while the
    // program is already running or paused) used to fall through to the
    // "unsupported request" catch-all. Prove it actually updates the live
    // breakpoint set rather than merely returning success.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_keel_file(
        dir.path(),
        "main.keel",
        r#"
task add(a: int, b: int) -> int {
  sum = a + b
  return sum
}

x = 1
y = 2
z = add(x, y)
w = add(x, z)
"#,
    );
    let path_str = path.to_str().unwrap().to_string();

    // Breakpoint at line 3 hits on the first `add` call (computing `z`).
    let mut client = DapClient::spawn(&["dap", &path_str]);
    client.start_and_stop_at(&[(&path_str, 3)]);

    // Clear it while paused.
    let s = client.send(
        "setBreakpoints",
        json!({"source": {"path": &path_str}, "breakpoints": []}),
    );
    let r = client.wait_response(s);
    assert_eq!(r["success"], true);
    assert_eq!(r["body"]["breakpoints"].as_array().unwrap().len(), 0);

    // The second `add` call (computing `w`) also passes through line 3.
    // If the clear above hadn't taken effect, that would hit the same
    // breakpoint again instead of letting the program finish.
    let status = client.resume_and_wait_exit();
    assert!(status.success());
}

#[test]
fn disconnect_while_paused_terminates_the_debuggee_process() {
    // Regression test: a client `disconnect` used to just reply and leave
    // the interpreter (and process) running — since `keel dap` only ever
    // launches (never attaches to) the debuggee, that left it orphaned
    // with no client able to reach it again.
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_keel_file(dir.path(), "main.keel", "x = 1\ny = 2\nz = x + y\n");
    let path_str = path.to_str().unwrap().to_string();

    let mut client = DapClient::spawn(&["dap", &path_str]);
    client.start_and_stop_at(&[(&path_str, 3)]);

    let s = client.send("disconnect", json!({}));
    let r = client.wait_response(s);
    assert_eq!(r["success"], true);

    let status = client
        .child
        .wait_timeout(Duration::from_secs(5))
        .expect("keel dap process to exit after disconnect");
    assert!(status.success());
}

#[test]
fn agent_state_scope_and_multi_frame_stack_work_end_to_end() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = write_keel_file(
        dir.path(),
        "main.keel",
        r#"
agent Counter {
  state {
    count: int = 0
  }

  task bump(n: int) -> int {
    result = n + 1
    return result
  }

  @on_start {
    self.count = self.bump(self.count)
  }
}

run(Counter)
"#,
    );
    let path_str = path.to_str().unwrap().to_string();

    let mut client = DapClient::spawn_with_env(&["dap", &path_str], &[("KEEL_ONESHOT", "1")]);
    // Line 8 is `result = n + 1`, inside `bump`, called from Counter's
    // `@on_start` — the one path that exercises both a real call stack
    // (on_start -> bump) and live agent state.
    let stopped = client.start_and_stop_at(&[(&path_str, 8)]);
    assert_eq!(stopped["body"]["reason"], "breakpoint");

    let s = client.send("stackTrace", json!({"threadId": 1}));
    let frames = client.wait_response(s);
    let stack_frames = frames["body"]["stackFrames"].as_array().unwrap();
    assert_eq!(
        stack_frames.len(),
        2,
        "expected on_start -> bump call to produce two frames, got: {frames}"
    );
    assert_eq!(stack_frames[0]["name"], "bump");
    assert_eq!(stack_frames[0]["line"], 8);
    assert_eq!(stack_frames[1]["name"], "Counter.on_start");

    let s = client.send("scopes", json!({"frameId": 0}));
    let scopes = client.wait_response(s);
    let scope_names: Vec<&str> = scopes["body"]["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|s| s["name"].as_str().unwrap())
        .collect();
    assert_eq!(
        scope_names,
        vec!["Locals", "Agent State"],
        "paused inside a live agent should expose an Agent State scope alongside Locals"
    );
    let agent_state_ref = scopes["body"]["scopes"][1]["variablesReference"].clone();

    let s = client.send("variables", json!({"variablesReference": agent_state_ref}));
    let vars = client.wait_response(s);
    let variables = vars["body"]["variables"].as_array().unwrap();
    assert_eq!(variables.len(), 1);
    assert_eq!(variables[0]["name"], "count");
    assert_eq!(
        variables[0]["value"], "0",
        "self.count is only assigned once bump(...) returns, so it should \
         still read 0 while paused inside bump"
    );

    let status = client.resume_and_wait_exit();
    assert!(status.success());
}
