//! Exit criterion for issue #214: a compiled program calling a `str`
//! value method produces byte-identical output to the interpreter running
//! the same source — proving `CallTarget::ValueMethod` codegen reaches the
//! exact same `call_method_on_value` match arms via
//! `keel_rt_call_value_method`/`CompiledHost`, not a separately-written
//! compiled-path reimplementation that could drift. Mirrors
//! `namespace_calls.rs`'s shape.

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
fn str_upper_matches_the_interpreter_byte_for_byte() {
    let source = "use std/io\n\nio.show(\"keel\".upper())\n";

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert!(
        compiled.stdout.ends_with(b"KEEL\n"),
        "expected str.upper()'s output on stdout, got: {:?}",
        String::from_utf8_lossy(&compiled.stdout)
    );
}

#[test]
fn str_contains_with_an_argument_matches_the_interpreter_byte_for_byte() {
    let source = "use std/io\n\nio.show(\"{\"hello world\".contains(\"world\")}\")\n";

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert!(
        compiled.stdout.ends_with(b"true\n"),
        "expected str.contains(...)'s output on stdout, got: {:?}",
        String::from_utf8_lossy(&compiled.stdout)
    );
}

#[test]
fn str_length_matches_the_interpreter_byte_for_byte() {
    let source = "use std/io\n\nio.show(\"{\"hello\".length()}\")\n";

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert!(
        compiled.stdout.ends_with(b"5\n"),
        "expected str.length()'s output on stdout, got: {:?}",
        String::from_utf8_lossy(&compiled.stdout)
    );
}

#[test]
fn multiple_str_method_calls_in_one_program_all_run() {
    let source = "use std/io\n\nio.show(\"a\".upper())\nio.show(\"{\"b\".is_empty()}\")\n";

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout)
    );
}

#[test]
fn distinct_method_name_constants_do_not_collide() {
    // Two different method-name globals (`emit_cstr_const`'s "method_name"
    // symbol, auto-uniquified by LLVM) in one program — pins that each call
    // site's global keeps its own content rather than one shadowing the
    // other, which would surface as a wrong dispatch, not a compile error.
    let source = "use std/io\n\nio.show(\"keel\".upper())\nio.show(\"{\"keel\".length()}\")\n";

    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout)
    );
    let out = String::from_utf8_lossy(&compiled.stdout);
    assert!(out.contains("KEEL"), "expected KEEL in output: {out}");
    assert!(out.contains('4'), "expected 4 in output: {out}");
}
