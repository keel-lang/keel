use crate::common::*;

// ---------------------------------------------------------------------------
// Shell namespace
// ---------------------------------------------------------------------------

#[test]
fn shell_run_captures_stdout_and_exit_code() {
    let src = r#"
agent A {
    @tools [Shell, Io]
    @on_start {
        r = Shell.run("echo hello")
        Io.show("code={r.exit_code}")
        Io.show("out={r.stdout}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("code=0"), "expected exit_code=0:\n{stdout}");
    assert!(
        stdout.contains("out=hello"),
        "expected 'hello' in stdout:\n{stdout}"
    );
}

#[test]
fn shell_run_nonzero_exit_does_not_raise() {
    let src = r#"
agent A {
    @tools [Shell, Io]
    @on_start {
        r = Shell.run("exit 7")
        Io.show("code={r.exit_code}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "non-zero exit should not raise\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("code=7"), "expected exit_code=7:\n{stdout}");
}

#[test]
fn shell_run_stdin_is_forwarded() {
    let src = r#"
agent A {
    @tools [Shell, Io]
    @on_start {
        r = Shell.run("cat", stdin: "from-stdin")
        Io.show("got={r.stdout}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("got=from-stdin"),
        "expected stdin forwarded:\n{stdout}"
    );
}

#[test]
fn shell_run_capability_error_when_tools_list_excludes_shell() {
    // When @tools restricts the agent to specific namespaces, Shell.run must
    // raise CapabilityError if Shell is not in the list.
    let src = r#"
agent A {
    @tools [Io]
    @on_start {
        Shell.run("echo hi")
        stop(self)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(
        !ok,
        "expected CapabilityError when Shell excluded from @tools"
    );
    assert!(
        stderr.contains("CapabilityError"),
        "expected CapabilityError in stderr:\n{stderr}"
    );
}

#[test]
fn shell_does_not_inherit_custom_env_vars() {
    // Env.require reads the keel process env; Shell.run spawns with a clean
    // environment (only PATH/HOME/TMPDIR/USER/LANG are forwarded), so custom
    // vars injected into the keel process are NOT visible to the subprocess.
    let src = r#"
agent A {
    @tools [Shell, Env, Io]
    @on_start {
        via_env = Env.require("KEEL_TEST_SHELL_VAR")
        r       = Shell.run("printf '%s' \"$KEEL_TEST_SHELL_VAR\"")
        Io.show("env={via_env}")
        Io.show("shell_empty={r.stdout == ""}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) =
        run_inline_with_env(src, &[("KEEL_TEST_SHELL_VAR", "hello-from-env")]);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("env=hello-from-env"),
        "Env.require should still see the var:\n{stdout}"
    );
    assert!(
        stdout.contains("shell_empty=true"),
        "Shell subprocess should not inherit custom var:\n{stdout}"
    );
}

#[test]
fn shell_forwards_safe_env_vars() {
    // HOME is in the safe forwarded set (PATH/HOME/TMPDIR/USER/LANG), so the
    // subprocess should see the same value that Env.require returns.
    let src = r#"
agent A {
    @tools [Shell, Env, Io]
    @on_start {
        home = Env.require("HOME")
        r = Shell.run("printf '%s' \"$HOME\"")
        Io.show("match={home == r.stdout}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("match=true"),
        "expected HOME to match between Env and Shell:\n{stdout}"
    );
}
