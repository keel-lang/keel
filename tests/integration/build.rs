//! `keel build` — M0 wires the CLI up to the KIR skeleton only
//! (`designs/llvm-compilation.md` §4). `--emit=kir` prints the dump to real
//! stdout (it's the deliverable, not a status message), so this exercises
//! it through the compiled binary rather than the in-process pipeline API —
//! see the note on `pipeline::tests::pipeline_build_emit_kir_succeeds_for_scalar_program`.

use crate::common::*;
use std::io::Write as _;
use std::process::Command;

fn build_emit_kir(src: &str) -> (bool, String, String) {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(src.as_bytes()).expect("write tempfile");
    let output = Command::new(keel_binary())
        .arg("build")
        .arg(tmp.path())
        .arg("--emit=kir")
        .output()
        .expect("run keel build --emit=kir");
    (
        output.status.success(),
        String::from_utf8_lossy(&output.stdout).into_owned(),
        String::from_utf8_lossy(&output.stderr).into_owned(),
    )
}

#[test]
fn emit_kir_prints_dump_for_scalar_program() {
    let (ok, stdout, stderr) = build_emit_kir(
        r#"
task answer() -> int {
  return 42
}
"#,
    );
    assert!(ok, "keel build --emit=kir failed\nstderr: {stderr}");
    assert!(
        stdout.contains("fn answer() -> int {"),
        "expected KIR dump of `answer`, got:\n{stdout}"
    );
    assert!(
        stdout.contains("return 42"),
        "expected lowered return statement, got:\n{stdout}"
    );
}

#[test]
fn build_without_emit_errors_codegen_not_implemented() {
    let mut tmp = tempfile::Builder::new()
        .suffix(".keel")
        .tempfile()
        .expect("tempfile");
    tmp.write_all(b"task answer() -> int {\n  return 42\n}\n")
        .expect("write tempfile");
    let output = Command::new(keel_binary())
        .arg("build")
        .arg(tmp.path())
        .output()
        .expect("run keel build");
    assert!(
        !output.status.success(),
        "keel build without --emit should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("native codegen not yet implemented"),
        "expected codegen-not-implemented diagnostic, got:\n{stderr}"
    );
}

#[test]
fn emit_kir_rejects_construct_outside_scalar_subset() {
    let (ok, _stdout, stderr) = build_emit_kir(
        r#"
agent A {
  @role "x"
}
run(A)
"#,
    );
    assert!(!ok, "agent declarations are outside the M0 scalar subset");
    assert!(
        stderr.contains("agent declaration"),
        "expected scalar-subset rejection, got:\n{stderr}"
    );
}
