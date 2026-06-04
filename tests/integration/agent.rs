use crate::common::*;
use std::io::Write;
use std::process::Command;

// ---------------------------------------------------------------------------
// Feature-specific examples
// ---------------------------------------------------------------------------

#[test]
fn data_pipeline_runs_through_all_records() {
    let (ok, stdout, _stderr) = run_example("data_pipeline");
    assert!(ok);
    assert!(stdout.contains("Processing 5 records"));
    assert!(stdout.contains("Stats: 2/5 valid"));
}

#[test]
fn email_fetch_without_config_is_empty_list() {
    let (ok, stdout, _stderr) = run_example("daily_digest");
    assert!(ok, "daily_digest exited non-zero");
    assert!(
        stdout.contains("No unread emails"),
        "expected graceful empty-inbox branch, stdout:\n{stdout}"
    );
}

#[test]
fn rich_enum_variants_construct_and_destructure() {
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

#[test]
fn io_ask_reads_from_stdin_and_returns_trimmed_answer() {
    let src = r#"
agent A {
  @on_start {
    answer = Io.ask("Name?")
    Io.show("answer={answer}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline_with_stdin(src, "Keel\n");
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("answer=Keel"),
        "Io.ask should return trimmed stdin answer:\n{stdout}"
    );
}

#[test]
fn io_confirm_accepts_yes_and_rejects_no() {
    let src = r#"
agent A {
  @on_start {
    first = Io.confirm("Ship?")
    second = Io.confirm("Rollback?")
    Io.show("first={first}, second={second}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline_with_stdin(src, "yes\nn\n");
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("first=true, second=false"),
        "Io.confirm should parse yes/no answers:\n{stdout}"
    );
}

#[test]
fn on_stop_block_fires_before_agent_removed() {
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
fn agent_delegate_symbol_form_dispatches_to_handler() {
    let src = r#"
agent Worker {
    on process(data: str) {
        Io.show("symbol-form: {data}")
    }
}

agent Boss {
    @on_start {
        Agent.run(Worker)
        Agent.delegate(Worker.process, "hello")
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
        stdout.contains("symbol-form: hello"),
        "expected Worker.process handler to fire:\nstdout: {stdout}"
    );
}

#[test]
fn agent_delegate_symbol_form_forwards_correct_payload() {
    // Verify the payload is the second arg (not shifted by a handler-name arg).
    let src = r#"
agent Printer {
    on print(msg: str) {
        Io.show(msg)
    }
}

agent Sender {
    @on_start {
        Agent.run(Printer)
        Agent.delegate(Printer.print, "hello-world")
    }
}

run(Sender)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "stderr: {stderr}");
    assert!(
        stdout.contains("hello-world"),
        "payload was not passed through:\nstdout: {stdout}"
    );
}

#[test]
fn agent_broadcast_dispatches_to_team_members() {
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
fn agent_send_returns_runtime_busy_when_queue_full() {
    // Use a capacity-2 queue so the 3rd Agent.send in @on_start triggers RuntimeBusy.
    // The Keel code catches it and records the count — verifies the error is catchable.
    let src = r#"
agent BurstBot {
    state {
        sent: int = 0
        caught: int = 0
    }

    @on_start {
        for i in 1..3 {
            try {
                Agent.send(BurstBot, i)
                self.sent = self.sent + 1
            } catch e: RuntimeBusy {
                self.caught = self.caught + 1
            }
        }
        Io.show("sent={self.sent} caught={self.caught}")
    }

    on message(n: int) { }
}
run(BurstBot)
"#;
    let (ok, stdout, stderr) = run_inline_with_env(src, &[("KEEL_EVENT_QUEUE_CAPACITY", "2")]);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("sent=2"),
        "expected 2 successful sends with capacity-2 queue\nstdout: {stdout}"
    );
    assert!(
        stdout.contains("caught=1"),
        "expected 1 RuntimeBusy catch with 3 sends into capacity-2 queue\nstdout: {stdout}"
    );
}

#[test]
fn stop_self_exits_cleanly() {
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
