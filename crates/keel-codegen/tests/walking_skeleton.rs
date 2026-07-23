//! Exit criterion for issue #132: a `.keel` file containing only arithmetic
//! on ints compiles, links, and the resulting binary's exit code matches
//! the computed value. See `func.rs`'s module doc for the temporary
//! "bare top-level `int` expression -> exit code" convention this relies on
//! — the interpreter never surfaces a computed value as a process exit code
//! (`keel run` always exits 0/1/130), so "matches the interpreter" here
//! means "the arithmetic gives the same answer," independently checked by
//! ordinary Rust arithmetic in each assertion below.

use std::path::Path;
use std::process::Command;

use keel_codegen::BuildOptions;

#[path = "support/mod.rs"]
mod support;

fn compile_and_run(source: &str) -> (i32, tempfile::TempDir) {
    let kir = support::parse_check_and_lower(source);

    let out_dir = tempfile::tempdir().expect("create temp out dir");
    let opts = BuildOptions {
        out_dir: out_dir.path().to_path_buf(),
        runtime_link_args: support::runtime_link_args().clone(),
    };
    let bin = keel_codegen::compile(&kir, &opts).expect("compile must succeed");

    let run = Command::new(&bin).output().expect("run compiled binary");
    (
        run.status.code().expect("process exited via a signal"),
        out_dir,
    )
}

#[test]
fn int_arithmetic_becomes_the_exit_code() {
    // 2 + 2 * 10 = 22 (mul binds tighter than add).
    let (code, _dir) = compile_and_run("2 + 2 * 10\n");
    assert_eq!(code, 22);
}

#[test]
fn let_and_augmented_assign_become_the_exit_code() {
    // A bare top-level `n = 5\nn += 3` (no function) type-checks fine at
    // runtime but is rejected by `keel check`/`keel run` today — a
    // pre-existing checker bug (`check_body` gives every top-level
    // `Decl::Stmt` its own fresh `Scope`, so a later statement can't see an
    // earlier one's binding for augmented-assignment purposes; tracked
    // separately, not a codegen/KIR concern). Wrapping in a function keeps
    // this test green without depending on that bug being fixed, and still
    // exercises Let/AugAssign/BinOp/call/exit-code exactly as before —
    // `control_flow_and_calls.rs` already established the bare-call-as-
    // exit-code convention this relies on.
    let (code, _dir) = compile_and_run(
        r#"
task run() -> int {
  n = 5
  n += 3
  return n * 2
}
run()
"#,
    );
    assert_eq!(code, 16);
}

#[test]
fn negative_and_comparison_ints_compile_and_run() {
    // -(3 + 4) < 0 is true (1); as a bare top-level expr its KirType is
    // bool, not int, so the M1 exit-code convention doesn't apply — exit 0.
    let (code, _dir) = compile_and_run("-(3 + 4) < 0\n");
    assert_eq!(code, 0);
}

#[test]
fn float_and_bool_arithmetic_compiles_and_runs() {
    // Exercises the F64/Bool codegen paths in layout.rs/expr.rs; only an
    // `int`-typed bare expression becomes the exit code (see func.rs), so
    // this just proves it compiles, links, and exits cleanly.
    let (code, _dir) = compile_and_run("1.5 + 2.5\ntrue and not false\n");
    assert_eq!(code, 0);
}

#[test]
fn empty_program_compiles_and_exits_zero() {
    let (code, _dir) = compile_and_run("");
    assert_eq!(code, 0);
}

#[test]
fn bare_str_expression_compiles_and_exits_zero() {
    // `str` is a full general expression now (M2 needs it for struct
    // fields — see `layout.rs`'s module doc): a bare top-level string
    // literal is no longer rejected, it just isn't `int`-typed, so the
    // exit-code convention falls through to 0, same as the float/bool case
    // below.
    let (code, _dir) = compile_and_run("\"hi\"\n");
    assert_eq!(code, 0);
}

#[test]
fn binary_actually_exists_on_disk() {
    let (_code, dir) = compile_and_run("1\n");
    let bin = dir.path().join("keel_program");
    assert!(Path::new(&bin).exists(), "expected a linked binary on disk");
}
