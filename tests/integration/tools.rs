use crate::common::*;

#[test]
fn tools_capability_gating_parses() {
    let src = r#"
use std/io
agent RestrictedAgent {
    @tools [io]

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

#[test]
fn agent_without_tools_is_denied_statically() {
    // Deny-by-default: no @tools attribute means no std-module calls,
    // and the checker rejects direct ones at compile time with both fixes.
    let src = r#"
use std/io
agent NoTools {
    @on_start {
        io.show("hi")
    }
}
run(NoTools)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "agent without @tools must not call std modules");
    assert!(
        stderr.contains("@tools does not allow it"),
        "expected static capability error:\n{stderr}"
    );
    assert!(
        stderr.contains("declare `@tools [io]`") && stderr.contains("@tools all"),
        "error must name both fixes:\n{stderr}"
    );
}

#[test]
fn tools_all_is_the_explicit_unrestricted_form() {
    let src = r#"
use std/env
use std/io
agent Trusted {
    @tools all
    @on_start {
        io.show(env.get("KEEL_TOOLS_ALL_TEST") ?? "true")
    }
}
run(Trusted)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "@tools all must allow everything\nstderr: {stderr}");
    assert!(stdout.contains("true"), "stdout: {stdout}");
}

#[test]
fn transitive_helper_calls_are_gated_at_runtime() {
    // The static check covers direct calls in the agent body; an effectful
    // call inside a helper task is caught by the runtime gate, with the
    // same actionable message.
    let src = r#"
use std/file
use std/io

task helper() -> str {
    file.read("definitely-missing.txt")
}

agent Shallow {
    @tools [io]
    @on_start {
        io.show(helper())
    }
}
run(Shallow)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "transitive effectful call must be denied at runtime");
    assert!(
        stderr.contains("CapabilityError"),
        "expected CapabilityError:\n{stderr}"
    );
    assert!(
        stderr.contains("`file.read` is not allowed by @tools"),
        "error must name the missing module:\n{stderr}"
    );
}

#[test]
fn pure_compute_modules_are_never_gated() {
    // Capabilities guard effects. json/math/time are pure or internal —
    // an agent may use them without declaring anything.
    let src = r#"
use std/io
use std/json
use std/math
agent PureOk {
    @tools [io]
    @on_start {
        io.show(json.stringify({root: math.sqrt(81.0)}))
    }
}
run(PureOk)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "pure modules must not require @tools\nstderr: {stderr}");
    assert!(stdout.contains("9"), "stdout: {stdout}");
}

#[test]
fn std_symbol_imports_are_gated_like_qualified_calls() {
    // `use read from std/file` then calling it inside an agent is the
    // same capability as `file.read`.
    let src = r#"
use std/io
use read from std/file
agent A {
    @tools [io]
    @on_start {
        io.show(read("nope.txt"))
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "symbol-imported std call must be gated");
    assert!(
        stderr.contains("@tools does not allow it"),
        "expected static capability error:\n{stderr}"
    );
}

#[test]
fn examples_run_capability_clean_under_mock() {
    // The static check only sees direct calls in agent bodies; capability
    // gaps in helper-task call chains surface at runtime. Run every example
    // end-to-end and fail on any unexpected CapabilityError — and keep the
    // one intentional demo honest by requiring it to raise one.
    use std::path::PathBuf;
    use std::process::{Command, Stdio};

    fn collect(dir: &std::path::Path, out: &mut Vec<PathBuf>) {
        for entry in std::fs::read_dir(dir).expect("read examples dir") {
            let path = entry.expect("dir entry").path();
            if path.is_dir() {
                collect(&path, out);
            } else if path.extension().and_then(|e| e.to_str()) == Some("keel") {
                out.push(path);
            }
        }
    }

    let mut files = Vec::new();
    collect(&project_root().join("examples"), &mut files);
    files.sort();
    assert!(
        files.len() >= 60,
        "expected the example corpus, found {}",
        files.len()
    );

    for file in files {
        let output = Command::new(keel_binary())
            .env("KEEL_ONESHOT", "1")
            .env("KEEL_LLM", "mock")
            .arg("run")
            .arg(&file)
            .stdin(Stdio::null())
            .output()
            .expect("run keel binary");
        let stderr = String::from_utf8_lossy(&output.stderr);
        let is_gating_demo = file.ends_with("capability_gating_fail.keel");
        if is_gating_demo {
            assert!(
                stderr.contains("CapabilityError"),
                "{} must keep demonstrating the runtime denial:\n{stderr}",
                file.display()
            );
        } else {
            assert!(
                !stderr.contains("CapabilityError"),
                "{} has an undeclared transitive capability — add the module \
                 named below to its @tools:\n{stderr}",
                file.display()
            );
        }
    }
}
