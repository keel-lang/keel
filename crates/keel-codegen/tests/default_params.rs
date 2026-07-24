//! Exit criterion for issue #164 (task default parameter values): a task
//! declared with a trailing defaulted parameter, called both with the
//! argument supplied and with it omitted, compiles, links, and matches the
//! interpreter for both call shapes.

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
fn omitted_and_supplied_trailing_default_args_match_the_interpreter() {
    // `name` is unused by the return value on purpose — str concatenation
    // isn't wired up in keel-codegen yet (a separate, later M2 gap), so
    // this sticks to returning the defaulted str param directly rather
    // than combining it with `name`.
    let source = r#"
use std/io

task greeting_for(name: str, greeting: str = "Hello") -> str {
  return greeting
}

io.show(greeting_for("Ada", "Hi"))
io.show(greeting_for("Ada"))
"#;
    assert_matches_interpreter(source, b"  Hi\n  Hello\n");
}

#[test]
fn multiple_trailing_defaults_can_be_partially_omitted() {
    let source = r#"
use std/io

task line(x: int, y: int = 0, z: int = 0) -> int {
  return x + y + z
}

io.show(line(1, 2, 3))
io.show(line(1, 2))
io.show(line(1))
"#;
    assert_matches_interpreter(source, b"  6\n  3\n  1\n");
}
