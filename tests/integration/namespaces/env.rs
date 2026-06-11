use crate::common::*;

// ---------------------------------------------------------------------------
// Env namespace
// ---------------------------------------------------------------------------

#[test]
fn env_require_returns_set_value_and_errors_when_missing() {
    let ok_src = r#"
use std/env
use std/io
agent A {
    @on_start {
        val = env.require("KEEL_TEST_REQUIRED")
        io.show("required={val}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline_with_env(ok_src, &[("KEEL_TEST_REQUIRED", "present")]);
    assert!(
        ok,
        "env.require success case failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("required=present"), "{stdout}");

    let missing_src = r#"
use std/env
agent A {
    @on_start {
        env.require("__KEEL_TEST_REQUIRED_MISSING__")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(missing_src, false);
    assert!(!ok, "missing env.require should fail");
    assert!(
        stderr.contains("env.require: `__KEEL_TEST_REQUIRED_MISSING__` is not set"),
        "expected env.require diagnostic:\n{stderr}"
    );
}

#[test]
fn env_require_missing_is_catchable_as_env_error() {
    let src = r#"
use std/env
use std/io
agent A {
    @on_start {
        try {
            env.require("__KEEL_TEST_MISSING__")
        } catch e: EnvError {
            io.show("kind=EnvError")
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
        stdout.contains("kind=EnvError"),
        "expected EnvError caught by specific type:\n{stdout}"
    );
    assert!(
        stdout.contains("msg=true"),
        "expected message field populated:\n{stdout}"
    );
}

#[test]
fn json_parse_error_is_catchable_as_json_error() {
    let src = r#"
use std/io
use std/json
agent A {
    @on_start {
        try {
            json.parse("not valid json")
        } catch e: JsonError {
            io.show("kind=JsonError")
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
        stdout.contains("kind=JsonError"),
        "expected JsonError caught by specific type:\n{stdout}"
    );
    assert!(
        stdout.contains("msg=true"),
        "expected message field populated:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Log namespace
// ---------------------------------------------------------------------------

#[test]
fn log_namespace_level_controls_output() {
    let src = r#"
use std/io
use std/log
agent A {
    @on_start {
        log.info("visible info")
        log.set_level("error")
        io.show("level={log.level()}")
        log.debug("hidden debug")
        log.warn("hidden warn")
        log.error("visible error")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Log namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("level=error"), "{stdout}");
    assert!(
        stderr.contains("[info] visible info") && stderr.contains("[error] visible error"),
        "expected visible log lines:\n{stderr}"
    );
    assert!(
        !stderr.contains("hidden debug") && !stderr.contains("hidden warn"),
        "log threshold should hide lower-priority lines:\n{stderr}"
    );
}

#[test]
fn log_namespace_rejects_invalid_level() {
    let src = r#"
use std/log
agent A {
    @on_start {
        log.set_level("verbose")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "invalid log.set_level should fail");
    assert!(
        stderr.contains("log.set_level: `verbose` is not a valid level"),
        "expected log.set_level diagnostic:\n{stderr}"
    );
}
