use crate::common::*;

// ---------------------------------------------------------------------------
// Env namespace
// ---------------------------------------------------------------------------

#[test]
fn env_require_returns_set_value_and_errors_when_missing() {
    let ok_src = r#"
agent A {
    @on_start {
        val = Env.require("KEEL_TEST_REQUIRED")
        Io.show("required={val}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline_with_env(ok_src, &[("KEEL_TEST_REQUIRED", "present")]);
    assert!(
        ok,
        "Env.require success case failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("required=present"), "{stdout}");

    let missing_src = r#"
agent A {
    @on_start {
        Env.require("__KEEL_TEST_REQUIRED_MISSING__")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(missing_src, false);
    assert!(!ok, "missing Env.require should fail");
    assert!(
        stderr.contains("Env.require: `__KEEL_TEST_REQUIRED_MISSING__` is not set"),
        "expected Env.require diagnostic:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Log namespace
// ---------------------------------------------------------------------------

#[test]
fn log_namespace_level_controls_output() {
    let src = r#"
agent A {
    @on_start {
        Log.info("visible info")
        Log.set_level("error")
        Io.show("level={Log.level()}")
        Log.debug("hidden debug")
        Log.warn("hidden warn")
        Log.error("visible error")
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
agent A {
    @on_start {
        Log.set_level("verbose")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "invalid Log.set_level should fail");
    assert!(
        stderr.contains("Log.set_level: `verbose` is not a valid level"),
        "expected Log.set_level diagnostic:\n{stderr}"
    );
}
