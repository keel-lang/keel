use crate::common::*;
use std::process::Command;

// ---------------------------------------------------------------------------
// keel check --strict
// ---------------------------------------------------------------------------

#[test]
fn strict_mode_passes_fully_typed_program() {
    use std::io::Write;
    let src = r#"
agent A {
  @on_start {
    n: int = 42
    s: str = "hello"
    Io.show("{n} {s}")
    stop(self)
  }
}
run(A)
"#;
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let output = Command::new(keel_binary())
        .args(["check", "--strict", path.to_str().unwrap()])
        .output()
        .expect("failed to run keel check --strict");
    assert!(
        output.status.success(),
        "strict check failed on typed program\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn strict_mode_rejects_unknown_typed_binding() {
    use std::io::Write;
    let src = r#"
agent A {
  @on_start {
    data = Json.parse("{}")
    Io.show("{data}")
    stop(self)
  }
}
run(A)
"#;
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let output = Command::new(keel_binary())
        .args(["check", "--strict", path.to_str().unwrap()])
        .output()
        .expect("failed to run keel check --strict");
    assert!(
        !output.status.success(),
        "strict check should fail on Unknown-typed binding"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("cannot infer type of `data`"),
        "expected strict diagnostic:\n{stderr}"
    );
}

#[test]
fn normal_check_accepts_json_parse_without_annotation() {
    use std::io::Write;
    let src = r#"
agent A {
  @on_start {
    data = Json.parse("{}")
    Io.show("{data}")
    stop(self)
  }
}
run(A)
"#;
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let output = Command::new(keel_binary())
        .args(["check", path.to_str().unwrap()])
        .output()
        .expect("failed to run keel check");
    assert!(
        output.status.success(),
        "normal check should accept unannotated Json.parse\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn strict_mode_accepts_explicit_dynamic_annotation() {
    use std::io::Write;
    // An explicit `dynamic` annotation is an intentional programmer choice —
    // it must never trigger a --strict error, even though Json.parse returns
    // Unknown(ExternalDynamic) when unannotated.
    let src = r#"
agent A {
  @on_start {
    data: dynamic = Json.parse("{}")
    Io.show("{data}")
    stop(self)
  }
}
run(A)
"#;
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let output = Command::new(keel_binary())
        .args(["check", "--strict", path.to_str().unwrap()])
        .output()
        .expect("failed to run keel check --strict");
    assert!(
        output.status.success(),
        "strict check should accept explicit `dynamic` annotation\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ─── P2: map literal value-type inference with opaque first element ─────────

/// When the first value is concrete the type is pinned; a subsequent opaque
/// value is silently accepted by `expect`'s opaque short-circuit and does not
/// pollute the inferred value type.  The whole map literal should pass strict.
#[test]
fn strict_mode_accepts_map_concrete_first_opaque_second() {
    use std::io::Write;
    // First entry pins val_ty = Str. Second entry is Unknown(ExternalDynamic);
    // `expect(Unknown, Str, ...)` short-circuits on opaque actual → no error.
    // Strict passes because the inferred map type is fully concrete: map[int,str].
    let src = r#"
agent A {
  @on_start {
    m = {1: "x", 2: Json.parse("{}")}
    stop(self)
  }
}
run(A)
"#;
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let path = tmp.path().to_owned();
    let output = Command::new(keel_binary())
        .args(["check", "--strict", path.to_str().unwrap()])
        .output()
        .expect("failed to run keel check --strict");
    assert!(
        output.status.success(),
        "strict should accept map with concrete-first, opaque-second values\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ─── v0.1.17: readonly state fields ────────────────────────────────────────

#[test]
fn readonly_state_check_rejects_assignment() {
    let src = r#"
agent Bot {
  state {
    session_id: readonly str = "abc"
  }
  @on_start {
    self.session_id = "overwritten"
  }
}
run(Bot)
"#;
    let (ok, _stdout, stderr) = check_inline_output(src);
    assert!(!ok, "assignment to readonly field should fail check");
    assert!(
        stderr.contains("readonly"),
        "error should mention readonly:\n{stderr}"
    );
}

#[test]
fn readonly_state_readable_in_on_start() {
    let src = r#"
agent Bot {
  state {
    session_id: readonly str = "s42"
  }
  @on_start {
    Io.show(self.session_id)
    stop(self)
  }
}
run(Bot)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "reading a readonly field should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("s42"),
        "expected field value in output:\n{stdout}"
    );
}
