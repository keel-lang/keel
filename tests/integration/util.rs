use crate::common::*;

#[test]
fn time_parse_shipped_in_v0_1_14() {
    let src = r#"
use std/io
use std/time
agent A {
    @tools [io]
    @on_start {
        p = time.parse("2026-01-01T00:00:00Z")
        io.show(p)
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "time.parse should work in v0.1.14\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("2026-01-01"),
        "parsed date should appear:\n{stdout}"
    );
}

#[test]
fn control_retry_succeeds_on_third_attempt() {
    let src = r#"
use std/control
use std/io
agent A {
    @tools [io]
    state { count: int = 0 }
    @on_start {
        result = control.retry(5, () => {
            self.count = self.count + 1
            if self.count < 3 {
                x = none
                y = x!
                return "won't reach"
            }
            return "ok"
        })
        io.show("attempts={self.count}")
        io.show("result={result}")
    }
}
run(A)
"#;
    // The closure raises NullError on the first 2 attempts (via `!` on none)
    // and returns "ok" on the 3rd. control.retry must catch the runtime
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
    let src = r#"
use std/control
use std/io
agent A {
    @tools [io]
    @on_start {
        result = control.with_timeout(5.seconds, () => {
            return "fast"
        })
        io.show("result={result}")
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
    let src = r#"
use std/async
use std/control
use std/io
agent A {
    @tools [io]
    @on_start {
        control.with_timeout(1.seconds, () => {
            async.sleep(5.seconds)
            return "done"
        })
        io.show("did-not-time-out")
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
fn control_with_timeout_is_catchable_as_timeout_error() {
    let src = r#"
use std/async
use std/control
use std/io
agent A {
    @tools [io]
    @on_start {
        try {
            control.with_timeout(1.seconds, () => {
                async.sleep(60.seconds)
            })
        } catch e: TimeoutError {
            io.show("kind=TimeoutError")
            io.show("msg={e.message.len() > 0}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("kind=TimeoutError"),
        "expected TimeoutError caught by specific type:\n{stdout}"
    );
    assert!(
        stdout.contains("msg=true"),
        "expected message field populated:\n{stdout}"
    );
}

#[test]
fn control_with_deadline_past_is_catchable_as_deadline_error() {
    let src = r#"
use std/async
use std/control
use std/io
agent A {
    @tools [io]
    @on_start {
        try {
            control.with_deadline("2020-01-01T00:00:00Z", () => {
                async.sleep(5.seconds)
            })
        } catch e: DeadlineError {
            io.show("kind=DeadlineError")
            io.show("msg={e.message.len() > 0}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("kind=DeadlineError"),
        "expected DeadlineError caught by specific type:\n{stdout}"
    );
    assert!(
        stdout.contains("msg=true"),
        "expected message field populated:\n{stdout}"
    );
}

// ─── control.retry error paths ───────────────────────────────────────────────

#[test]
fn control_retry_rejects_zero_attempts() {
    let src = r#"
use std/control
agent A {
    @on_start {
        control.retry(0, () => {
            return "never"
        })
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for zero attempts");
    assert!(
        stderr.contains("positive integer"),
        "expected 'positive integer' error:\n{stderr}"
    );
}

#[test]
fn control_retry_rejects_missing_closure() {
    let src = r#"
use std/control
agent A {
    @on_start {
        control.retry(3)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for missing closure");
    assert!(
        stderr.contains("missing closure"),
        "expected 'missing closure' error:\n{stderr}"
    );
}

// ─── control.with_timeout error paths ────────────────────────────────────────

#[test]
fn control_with_timeout_rejects_missing_duration() {
    let src = r#"
use std/control
agent A {
    @on_start {
        control.with_timeout(() => {
            return "ok"
        })
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for missing duration");
    assert!(
        stderr.contains("missing duration"),
        "expected 'missing duration' error:\n{stderr}"
    );
}

#[test]
fn control_with_timeout_rejects_missing_duration_with_a_named_task() {
    let src = r#"
use std/control

task work() -> str {
    "ok"
}

agent A {
    @on_start {
        control.with_timeout(work)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(
        !ok,
        "expected non-zero exit for missing duration with a named task"
    );
    assert!(
        stderr.contains("missing duration"),
        "expected 'missing duration' error:\n{stderr}"
    );
}

#[test]
fn control_with_timeout_rejects_missing_closure() {
    let src = r#"
use std/control
agent A {
    @on_start {
        control.with_timeout(5.seconds)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for missing closure");
    assert!(
        stderr.contains("missing closure"),
        "expected 'missing closure' error:\n{stderr}"
    );
}

#[test]
fn control_with_timeout_propagates_closure_error() {
    let src = r#"
use std/control
agent A {
    @on_start {
        control.with_timeout(5.seconds, () => {
            x = none
            y = x!
            return "never"
        })
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for closure error");
    assert!(
        stderr.contains("NullError"),
        "expected NullError in stderr:\n{stderr}"
    );
    assert!(
        !stderr.contains("TimeoutError"),
        "must not be a timeout:\n{stderr}"
    );
}

// ─── control.with_deadline ───────────────────────────────────────────────────

#[test]
fn control_with_deadline_completes_before_deadline() {
    let src = r#"
use std/control
use std/io
agent A {
    @tools [io]
    @on_start {
        result = control.with_deadline("2099-01-01T00:00:00Z", () => {
            return "early"
        })
        io.show("result={result}")
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
        stdout.contains("result=early"),
        "expected early result:\n{stdout}"
    );
}

#[test]
fn control_with_deadline_aborts_on_past_deadline() {
    let src = r#"
use std/async
use std/control
use std/io
agent A {
    @tools [io]
    @on_start {
        control.with_deadline("2020-01-01T00:00:00Z", () => {
            async.sleep(5.seconds)
            return "too-late"
        })
        io.show("did-not-time-out")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        !ok,
        "expected non-zero exit on deadline\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stderr.contains("DeadlineError"),
        "expected DeadlineError diagnostic:\n{stderr}"
    );
    assert!(
        !stdout.contains("did-not-time-out"),
        "long call must not complete:\n{stdout}"
    );
}

#[test]
fn control_with_deadline_rejects_missing_datetime() {
    let src = r#"
use std/control
agent A {
    @on_start {
        control.with_deadline()
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for missing datetime");
    assert!(
        stderr.contains("control.with_deadline: missing argument at position 0"),
        "expected missing argument error:\n{stderr}"
    );
}

#[test]
fn control_with_deadline_rejects_unparseable_datetime() {
    let src = r#"
use std/control
agent A {
    @on_start {
        control.with_deadline("not-a-date", () => {
            return "ok"
        })
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for unparseable datetime");
    assert!(
        stderr.contains("cannot parse"),
        "expected 'cannot parse' error:\n{stderr}"
    );
}

#[test]
fn control_with_deadline_rejects_missing_closure() {
    let src = r#"
use std/control
agent A {
    @on_start {
        control.with_deadline("2099-01-01T00:00:00Z")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for missing closure");
    assert!(
        stderr.contains("missing closure"),
        "expected 'missing closure' error:\n{stderr}"
    );
}

#[test]
fn async_spawn_returns_handle() {
    let src = r#"
use std/async
use std/io
agent AsyncTest {
    @tools [io]
    @on_start {
        h = async.spawn(() => {
            io.show("spawned")
        })
        io.show("spawn-ok")
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
        "async.spawn should work:\n{stdout}"
    );
}

#[test]
fn async_join_all_returns_results_and_preserves_agent_context() {
    let src = r#"
use std/async
use std/io
agent AsyncTest {
    @tools [io]
    state { count: int = 41 }

    @on_start {
        h = async.spawn(() => {
            return self.count + 1
        })
        results = async.join_all([h])
        io.show(results)
        stop(self)
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
        stdout.contains("42"),
        "async.join_all should return spawned closure results:\n{stdout}"
    );
    assert!(
        !stdout.contains("_status"),
        "async.join_all must not return raw handles:\n{stdout}"
    );
}

#[test]
fn async_join_all_propagates_spawned_errors() {
    let missing = tempfile::tempdir()
        .expect("tempdir")
        .path()
        .join("missing.txt");
    let file = keel_string_literal(&missing.to_string_lossy());
    let src = format!(
        r#"
use std/async
use std/file
use std/io
agent AsyncTest {{
    @tools [file, io]
    @on_start {{
        h = async.spawn(() => {{
            file.read("{file}")
        }})
        async.join_all([h])
        io.show("unreachable")
        stop(self)
    }}
}}
run(AsyncTest)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(!ok, "spawned file.read error should fail");
    assert!(
        stderr.contains("async.join_all: task failed") && stderr.contains("FileError: file.read"),
        "expected propagated async task error\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        !stdout.contains("unreachable"),
        "program continued after async task error:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// v0.1.14 — Time namespace
// ---------------------------------------------------------------------------

#[test]
fn time_now_returns_iso_string() {
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    t = time.now()
    io.show(t)
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
        "time.now() should return RFC 3339:\n{stdout}"
    );
}

#[test]
fn time_parse_normalises_date() {
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    p = time.parse("2026-05-01T00:00:00Z")
    io.show(p)
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
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    p = time.parse("2026-05-01")
    if p == none {
      io.show("none")
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
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    p = time.parse("2026-05-01", tz: "UTC")
    io.show(p)
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
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    dt = time.parse("2026-05-01T09:30:00Z")
    s = dt.format(as: "%Y-%m-%d")
    io.show(s)
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
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    a = time.parse("2026-05-02T00:00:00Z")
    b = time.parse("2026-05-01T00:00:00Z")
    d = a - b
    io.show(d)
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
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    dt = time.parse("2026-05-06T14:30:45Z")
    p = dt.parts()
    io.show(p.year)
    io.show(p.month)
    io.show(p.day)
    io.show(p.hour)
    io.show(p.tz)
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
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    t = time.now(tz: "Europe/Paris")
    io.show(t)
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
        "time.now(tz: Paris) should emit a European offset:\n{stdout}"
    );
}

#[test]
fn time_datetime_arithmetic() {
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    base = time.parse("2026-05-01T00:00:00Z")
    future = base + 1.days
    io.show(future)
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
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    a = time.parse("2026-05-02T00:00:00Z")
    b = time.parse("2026-05-01T00:00:00Z")
    if a > b {
      io.show("later")
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
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    d = 500.ms
    io.show(d)
    d2 = 1500.millis
    io.show(d2)
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
    let src = r#"
use std/io
use std/time
agent A {
  @tools [io]
  @on_start {
    t = time.now()
    io.show(t)
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
        "time.now() should emit millisecond-precision RFC 3339:\n{stdout}"
    );
}

// ─── async.select ────────────────────────────────────────────────────────────

#[test]
fn async_select_returns_first_completed() {
    let src = r#"
use std/async
use std/io
agent AsyncTest {
    @tools [io]
    @on_start {
        h1 = async.spawn(() => {
            return "fast"
        })
        h2 = async.spawn(() => {
            async.sleep(5.seconds)
            return "slow"
        })
        result = async.select([h1, h2])
        io.show(result)
    }
}
run(AsyncTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    // The fast task should complete first, so select returns "fast"
    assert!(
        stdout.contains("fast"),
        "select should return first completed value:\n{stdout}"
    );
    // The slow task should be aborted, so "slow" should NOT appear
    assert!(
        !stdout.contains("slow"),
        "select should abort the slow task:\n{stdout}"
    );
}

#[test]
fn async_select_with_empty_list_fails() {
    let src = r#"
use std/async
use std/io
agent AsyncTest {
    @tools [io]
    @on_start {
        async.select([])
        io.show("unreachable")
    }
}
run(AsyncTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    let _ = stdout; // used in assertion messages below
    assert!(!ok, "expected non-zero exit for empty select list");
    assert!(
        stderr.contains("non-empty list"),
        "expected 'non-empty list' error:\n{stderr}"
    );
    assert!(
        !stdout.contains("unreachable"),
        "program continued after error:\n{stdout}"
    );
}

// ─── async.sleep ────────────────────────────────────────────────────────────

#[test]
fn async_sleep_returns_none() {
    let src = r#"
use std/async
use std/io
agent AsyncTest {
    @tools [io]
    @on_start {
        r = async.sleep(10.millis)
        io.show("slept")
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
        stdout.contains("slept"),
        "sleep should complete and not block:\n{stdout}"
    );
}

// ─── async.join_all edge cases ──────────────────────────────────────────────

#[test]
fn async_join_all_empty_list_returns_empty() {
    let src = r#"
use std/async
use std/io
agent AsyncTest {
    @tools [io]
    @on_start {
        results = async.join_all([])
        io.show("ok")
        stop(self)
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
        stdout.contains("ok"),
        "join_all on empty list should succeed:\n{stdout}"
    );
}

#[test]
fn async_join_all_with_non_list_fails() {
    let src = r#"
use std/async
use std/io
agent AsyncTest {
    @tools [io]
    @on_start {
        async.join_all("not-a-list")
        io.show("unreachable")
    }
}
run(AsyncTest)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for non-list join_all");
    assert!(
        stderr.contains("expected a list"),
        "expected 'expected a list' error:\n{stderr}"
    );
}

// ─── async.spawn edge cases ─────────────────────────────────────────────────

#[test]
fn async_spawn_missing_closure_fails() {
    let src = r#"
use std/async
use std/io
agent AsyncTest {
    @tools [io]
    @on_start {
        async.spawn()
        io.show("unreachable")
    }
}
run(AsyncTest)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for spawn without closure");
    assert!(
        stderr.contains("missing closure"),
        "expected 'missing closure' error:\n{stderr}"
    );
}

// ── Regression: async.spawn can call user-defined tasks (B3) ─────────────────

#[test]
fn async_spawn_can_call_user_defined_task() {
    let src = r#"
use std/async
use std/io
task double(n: int) -> int {
    n * 2
}
agent AsyncTest {
    @tools [io]
    @on_start {
        h = async.spawn(() => {
            double(21)
        })
        results = async.join_all([h])
        io.show("{results}")
        stop(self)
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
        stdout.contains("42"),
        "spawned closure should be able to call user-defined task 'double':\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// A named task works everywhere a lambda does, in closure-taking namespace
// methods and the global min/max — issue #217. #216 fixed the 9 value
// methods (`list.map`, …); these are the remaining sites that destructured
// the argument into `Value::Closure`'s `(params, body)` directly and
// rejected `Value::Task`.
// ---------------------------------------------------------------------------

#[test]
fn control_retry_accepts_a_named_task() {
    let src = r#"
use std/control
use std/io

task work() -> int {
  42
}

task main() {
  io.show("{control.retry(3, work)}")
}

main()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected clean run:\n{stderr}");
    assert!(stdout.contains("42"), "control.retry(work):\n{stdout}");
}

#[test]
fn control_with_timeout_accepts_a_named_task() {
    let src = r#"
use std/control
use std/io

task work() -> int {
  42
}

task main() {
  io.show("{control.with_timeout(5.seconds, work)}")
}

main()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected clean run:\n{stderr}");
    assert!(
        stdout.contains("42"),
        "control.with_timeout(work):\n{stdout}"
    );
}

#[test]
fn control_with_deadline_accepts_a_named_task() {
    let src = r#"
use std/control
use std/io

task work() -> int {
  42
}

task main() {
  result = control.with_deadline("2099-01-01T00:00:00Z", work)
  io.show("{result}")
}

main()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected clean run:\n{stderr}");
    assert!(
        stdout.contains("42"),
        "control.with_deadline(work):\n{stdout}"
    );
}

#[test]
fn schedule_every_accepts_a_named_task() {
    let src = r#"
use std/io
use std/schedule

task tick() {
  io.show("tick")
}

agent Ticker {
  @tools [io]
  @on_start {
    schedule.every(3.seconds, tick)
  }
}
run(Ticker)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected clean run:\n{stderr}");
    assert!(stdout.contains("tick"), "schedule.every(tick):\n{stdout}");
}

#[test]
fn async_spawn_accepts_a_named_task() {
    let src = r#"
use std/async
use std/io

task work() -> int {
  42
}

agent AsyncTest {
    @tools [io]
    @on_start {
        h = async.spawn(work)
        results = async.join_all([h])
        io.show("{results}")
        stop(self)
    }
}
run(AsyncTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected clean run:\n{stderr}");
    assert!(stdout.contains("42"), "async.spawn(work):\n{stdout}");
}

#[test]
fn min_and_max_by_accept_a_named_task() {
    let src = r#"
use std/io

task negate(x: int) -> int {
  0 - x
}

task main() {
  io.show("{min([1, 2, 3], by: negate)}")
  io.show("{max([1, 2, 3], by: negate)}")
}

main()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected clean run:\n{stderr}");
    assert!(stdout.contains('3'), "min(by: negate):\n{stdout}");
    assert!(stdout.contains('1'), "max(by: negate):\n{stdout}");
}
