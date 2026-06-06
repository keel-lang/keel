use crate::common::*;

#[test]
fn time_parse_shipped_in_v0_1_14() {
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

#[test]
fn control_retry_succeeds_on_third_attempt() {
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
fn control_with_timeout_is_catchable_as_timeout_error() {
    let src = r#"
agent A {
    @on_start {
        try {
            Control.with_timeout(1.seconds, () => {
                Async.sleep(60.seconds)
            })
        } catch e: TimeoutError {
            Io.show("kind=TimeoutError")
            Io.show("msg={e.message.len() > 0}")
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
agent A {
    @on_start {
        try {
            Control.with_deadline("2020-01-01T00:00:00Z", () => {
                Async.sleep(5.seconds)
            })
        } catch e: DeadlineError {
            Io.show("kind=DeadlineError")
            Io.show("msg={e.message.len() > 0}")
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

// ─── Control.retry error paths ───────────────────────────────────────────────

#[test]
fn control_retry_rejects_zero_attempts() {
    let src = r#"
agent A {
    @on_start {
        Control.retry(0, () => {
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
agent A {
    @on_start {
        Control.retry(3)
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

// ─── Control.with_timeout error paths ────────────────────────────────────────

#[test]
fn control_with_timeout_rejects_missing_duration() {
    let src = r#"
agent A {
    @on_start {
        Control.with_timeout(() => {
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
fn control_with_timeout_rejects_missing_closure() {
    let src = r#"
agent A {
    @on_start {
        Control.with_timeout(5.seconds)
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
agent A {
    @on_start {
        Control.with_timeout(5.seconds, () => {
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

// ─── Control.with_deadline ───────────────────────────────────────────────────

#[test]
fn control_with_deadline_completes_before_deadline() {
    let src = r#"
agent A {
    @on_start {
        result = Control.with_deadline("2099-01-01T00:00:00Z", () => {
            return "early"
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
        stdout.contains("result=early"),
        "expected early result:\n{stdout}"
    );
}

#[test]
fn control_with_deadline_aborts_on_past_deadline() {
    let src = r#"
agent A {
    @on_start {
        Control.with_deadline("2020-01-01T00:00:00Z", () => {
            Async.sleep(5.seconds)
            return "too-late"
        })
        Io.show("did-not-time-out")
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
agent A {
    @on_start {
        Control.with_deadline()
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for missing datetime");
    assert!(
        stderr.contains("Control.with_deadline: missing argument at position 0"),
        "expected missing argument error:\n{stderr}"
    );
}

#[test]
fn control_with_deadline_rejects_unparseable_datetime() {
    let src = r#"
agent A {
    @on_start {
        Control.with_deadline("not-a-date", () => {
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
agent A {
    @on_start {
        Control.with_deadline("2099-01-01T00:00:00Z")
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
fn async_join_all_returns_results_and_preserves_agent_context() {
    let src = r#"
agent AsyncTest {
    state { count: int = 41 }

    @on_start {
        h = Async.spawn(() => {
            return self.count + 1
        })
        results = Async.join_all([h])
        Io.show(results)
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
        "Async.join_all should return spawned closure results:\n{stdout}"
    );
    assert!(
        !stdout.contains("_status"),
        "Async.join_all must not return raw handles:\n{stdout}"
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
agent AsyncTest {{
    @on_start {{
        h = Async.spawn(() => {{
            File.read("{file}")
        }})
        Async.join_all([h])
        Io.show("unreachable")
        stop(self)
    }}
}}
run(AsyncTest)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(!ok, "spawned File.read error should fail");
    assert!(
        stderr.contains("Async.join_all: task failed") && stderr.contains("FileError: File.read"),
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

// ─── Async.select ────────────────────────────────────────────────────────────

#[test]
fn async_select_returns_first_completed() {
    let src = r#"
agent AsyncTest {
    @on_start {
        h1 = Async.spawn(() => {
            return "fast"
        })
        h2 = Async.spawn(() => {
            Async.sleep(5.seconds)
            return "slow"
        })
        result = Async.select([h1, h2])
        Io.show(result)
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
agent AsyncTest {
    @on_start {
        Async.select([])
        Io.show("unreachable")
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

// ─── Async.sleep ────────────────────────────────────────────────────────────

#[test]
fn async_sleep_returns_none() {
    let src = r#"
agent AsyncTest {
    @on_start {
        r = Async.sleep(10.millis)
        Io.show("slept")
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

// ─── Async.join_all edge cases ──────────────────────────────────────────────

#[test]
fn async_join_all_empty_list_returns_empty() {
    let src = r#"
agent AsyncTest {
    @on_start {
        results = Async.join_all([])
        Io.show("ok")
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
agent AsyncTest {
    @on_start {
        Async.join_all("not-a-list")
        Io.show("unreachable")
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

// ─── Async.spawn edge cases ─────────────────────────────────────────────────

#[test]
fn async_spawn_missing_closure_fails() {
    let src = r#"
agent AsyncTest {
    @on_start {
        Async.spawn()
        Io.show("unreachable")
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

// ── Regression: Async.spawn can call user-defined tasks (B3) ─────────────────

#[test]
fn async_spawn_can_call_user_defined_task() {
    let src = r#"
task double(n: int) -> int {
    n * 2
}
agent AsyncTest {
    @on_start {
        h = Async.spawn(() => {
            double(21)
        })
        results = Async.join_all([h])
        Io.show("{results}")
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
