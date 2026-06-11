use crate::common::*;

// ---------------------------------------------------------------------------
// Shell namespace
// ---------------------------------------------------------------------------

#[test]
fn shell_run_captures_stdout_and_exit_code() {
    let src = r#"
use std/io
use std/shell
agent A {
    @tools [shell, io]
    @on_start {
        r = shell.run("echo hello")
        io.show("code={r.exit_code}")
        io.show("out={r.stdout}")
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
use std/io
use std/shell
agent A {
    @tools [shell, io]
    @on_start {
        r = shell.run("exit 7")
        io.show("code={r.exit_code}")
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
use std/io
use std/shell
agent A {
    @tools [shell, io]
    @on_start {
        r = shell.run("cat", stdin: "from-stdin")
        io.show("got={r.stdout}")
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
    // When @tools restricts the agent to specific namespaces, shell.run must
    // raise CapabilityError if Shell is not in the list.
    let src = r#"
use std/shell
agent A {
    @tools [io]
    @on_start {
        shell.run("echo hi")
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
    // env.require reads the keel process env; shell.run spawns with a clean
    // environment (only PATH/HOME/TMPDIR/USER/LANG are forwarded), so custom
    // vars injected into the keel process are NOT visible to the subprocess.
    let src = r#"
use std/env
use std/io
use std/shell
agent A {
    @tools [shell, env, io]
    @on_start {
        via_env = env.require("KEEL_TEST_SHELL_VAR")
        r       = shell.run("printf '%s' \"$KEEL_TEST_SHELL_VAR\"")
        io.show("env={via_env}")
        io.show("shell_empty={r.stdout == ""}")
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
        "env.require should still see the var:\n{stdout}"
    );
    assert!(
        stdout.contains("shell_empty=true"),
        "Shell subprocess should not inherit custom var:\n{stdout}"
    );
}

#[test]
fn shell_forwards_safe_env_vars() {
    // HOME is in the safe forwarded set (PATH/HOME/TMPDIR/USER/LANG), so the
    // subprocess should see the same value that env.require returns.
    let src = r#"
use std/env
use std/io
use std/shell
agent A {
    @tools [shell, env, io]
    @on_start {
        home = env.require("HOME")
        r = shell.run("printf '%s' \"$HOME\"")
        io.show("match={home == r.stdout}")
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
