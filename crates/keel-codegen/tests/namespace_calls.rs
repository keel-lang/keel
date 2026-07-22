//! Exit criterion for issue #135: a compiled program calling a namespace
//! method produces byte-identical output to the interpreter running the
//! same source — proving `CallTarget::Ns` codegen reaches the exact same
//! `Namespace.methods` closure via `keel_rt_call_ns`/`CompiledHost`, not a
//! separately-written compiled-path reimplementation that could drift.

use std::process::Command;

use keel_codegen::BuildOptions;

#[path = "support/mod.rs"]
mod support;

fn compile_and_run(source: &str) -> std::process::Output {
    let kir = support::parse_check_and_lower(source);

    let out_dir = tempfile::tempdir().expect("create temp out dir");
    let opts = BuildOptions {
        out_dir: out_dir.path().to_path_buf(),
        runtime_link_args: support::runtime_link_args().clone(),
    };
    let bin = keel_codegen::compile(&kir, &opts).expect("compile must succeed");
    Command::new(&bin).output().expect("run compiled binary")
}

#[test]
fn io_show_matches_the_interpreter_byte_for_byte() {
    let source = "use std/io\n\nio.show(\"hello\")\n";

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert!(
        compiled.stdout.ends_with(b"hello\n"),
        "expected io.show's output on stdout, got: {:?}",
        String::from_utf8_lossy(&compiled.stdout)
    );
}

#[test]
fn log_info_matches_the_interpreter_byte_for_byte() {
    let source = "use std/log\n\nlog.info(\"started\")\n";

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stderr),
        String::from_utf8_lossy(&interpreted.stderr),
        "compiled stderr must match the interpreter's (log.* writes to stderr)"
    );
    assert!(
        compiled.stderr.ends_with(b"[info] started\n"),
        "expected log.info's output on stderr, got: {:?}",
        String::from_utf8_lossy(&compiled.stderr)
    );
}

#[test]
fn multiple_namespace_calls_in_one_program_all_run() {
    let source = "use std/io\nuse std/log\n\nio.show(\"hello\")\nlog.info(\"started\")\n";

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout)
    );
    assert_eq!(
        String::from_utf8_lossy(&compiled.stderr),
        String::from_utf8_lossy(&interpreted.stderr)
    );
}
