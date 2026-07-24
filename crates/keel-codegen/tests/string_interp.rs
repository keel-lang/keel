//! Exit criterion for issue #149 (string interpolation): a program
//! interpolating int/float/bool values into a string (e.g. `"total: {x +
//! y}"`) compiles, links, and produces byte-identical output to the
//! interpreter.

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

fn assert_matches_interpreter(source: &str, expected_stdout: &[u8]) {
    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, expected_stdout);
}

#[test]
fn interpolating_an_arithmetic_expression_matches_the_interpreter() {
    let source = r#"
use std/io

task total_line(x: int, y: int) -> str {
  return "total: {x + y}"
}

io.show(total_line(2, 3))
"#;
    assert_matches_interpreter(source, b"  total: 5\n");
}

#[test]
fn interpolating_multiple_slots_and_types_matches_the_interpreter() {
    let source = r#"
use std/io

name = "Ada"
pi = 3.5
ready = true
io.show("Hello, {name}! pi={pi} ready={ready}")
"#;
    assert_matches_interpreter(source, b"  Hello, Ada! pi=3.5 ready=true\n");
}
