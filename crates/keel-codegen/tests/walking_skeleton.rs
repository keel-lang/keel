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

use keel_codegen::{BuildOptions, CodegenError};

#[path = "support/mod.rs"]
mod support;

fn compile_and_run(source: &str) -> (i32, tempfile::TempDir) {
    let (program, _named) =
        keel_syntax::parse_source(source, "t.keel").expect("fixture must parse");
    let kir = keel_kir::lower(&program, "t.keel").expect("fixture must lower to KIR");

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

fn compile_err(source: &str) -> CodegenError {
    let (program, _named) =
        keel_syntax::parse_source(source, "t.keel").expect("fixture must parse");
    let kir = keel_kir::lower(&program, "t.keel").expect("fixture must lower to KIR");

    let out_dir = tempfile::tempdir().expect("create temp out dir");
    let opts = BuildOptions {
        out_dir: out_dir.path().to_path_buf(),
        runtime_link_args: support::runtime_link_args().clone(),
    };
    keel_codegen::compile(&kir, &opts).expect_err("compile must be rejected")
}

#[test]
fn int_arithmetic_becomes_the_exit_code() {
    // 2 + 2 * 10 = 22 (mul binds tighter than add).
    let (code, _dir) = compile_and_run("2 + 2 * 10\n");
    assert_eq!(code, 22);
}

#[test]
fn let_and_augmented_assign_become_the_exit_code() {
    let (code, _dir) = compile_and_run("n = 5\nn += 3\nn * 2\n");
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
fn str_expression_is_rejected_not_silently_dropped() {
    let err = compile_err("\"hi\"\n");
    assert!(
        matches!(err, CodegenError::Unsupported(ref msg) if msg.contains("str")),
        "unexpected error: {err}"
    );
}

#[test]
fn binary_actually_exists_on_disk() {
    let (_code, dir) = compile_and_run("1\n");
    let bin = dir.path().join("keel_program");
    assert!(Path::new(&bin).exists(), "expected a linked binary on disk");
}
