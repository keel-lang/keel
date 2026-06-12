use crate::common::*;
use std::io::Write;
use std::process::Command;

// ---------------------------------------------------------------------------
// Scheduling
// ---------------------------------------------------------------------------

#[test]
fn scheduling_ticks_at_least_once() {
    let src = r#"
use std/io
use std/schedule
agent Ticker {
  @tools [io]
  state { tick: int = 0 }
  @on_start {
    schedule.every(3.seconds, () => {
      self.tick = self.tick + 1
      io.show("Tick #{self.tick}")
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
    // Without KEEL_ONESHOT, schedule.every must fire repeatedly.
    // Ticking every 3 seconds over a 7-second window should yield >= 2 ticks.
    let src = r#"
use std/io
use std/schedule
agent Ticker {
  @tools [io]
  state { tick: int = 0 }
  @on_start {
    schedule.every(3.seconds, () => {
      self.tick = self.tick + 1
      io.show("Tick #{self.tick}")
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
// v0.1.7 — Structured Concurrency & Agent Constraints
// ---------------------------------------------------------------------------

#[test]
fn schedule_cron_accepts_expression() {
    let src = r#"
use std/io
use std/schedule
agent CronTest {
    @tools [io]
    @on_start {
        schedule.cron("0 9 * * 1-5", () => {
            io.show("morning")
        })
        io.show("cron-parsed")
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
        "schedule.cron should accept cron expressions:\n{stdout}"
    );
}
