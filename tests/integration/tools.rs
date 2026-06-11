use crate::common::*;

#[test]
fn tools_capability_gating_parses() {
    let src = r#"
use std/io
agent RestrictedAgent {
    @tools [io, schedule]

    @on_start {
        io.show("allowed")
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

#[test]
fn tools_when_guard_blocks_unconfirmed() {
    let src = r#"
use std/io
agent GuardedBot {
    state { confirmed: bool = false }

    @tools [
        log,
        io.show if self.confirmed,
    ]

    on message(msg: str) {
        if msg == "confirm" {
            self.confirmed = true
        } else {
            io.show(msg)
        }
    }
}
run(GuardedBot)
send(GuardedBot, "hello")
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected CapabilityError but program succeeded");
    assert!(
        stderr.contains("CapabilityError"),
        "expected CapabilityError in stderr:\n{stderr}"
    );
}

#[test]
fn tools_when_guard_allows_after_state_change() {
    let src = r#"
use std/io
agent GuardedBot {
    state { confirmed: bool = false }

    @tools [
        log,
        io,
    ]

    on message(msg: str) {
        if msg == "confirm" {
            self.confirmed = true
            io.show("confirmed")
        }
    }
}
run(GuardedBot)
send(GuardedBot, "confirm")
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("confirmed"), "expected output:\n{stdout}");
}
