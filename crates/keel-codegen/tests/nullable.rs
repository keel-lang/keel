//! Exit criterion for issue #148 (nullable `?`/`??`/`?.`): a program using
//! `?.`, `??`, and a scalar nullable (`int?`) compiles, links, and matches
//! the interpreter — including the case where the nullable is genuinely
//! `none` at runtime, not just the non-null happy path.

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
fn null_coalesce_on_a_scalar_nullable_matches_the_interpreter_both_ways() {
    let source = r#"
use std/io

task pick(n: int? = none) -> int {
  return n ?? 0
}

io.show(pick(5))
io.show(pick())
"#;
    assert_matches_interpreter(source, b"  5\n  0\n");
}

#[test]
fn null_safe_field_access_matches_the_interpreter_both_ways() {
    let source = r#"
use std/io

type Email {
  subject: str
}

task greet(email: Email? = none) -> str {
  return email?.subject ?? "(none)"
}

some_email: Email = { subject: "hello" }
io.show(greet(some_email))
io.show(greet())
"#;
    assert_matches_interpreter(source, b"  hello\n  (none)\n");
}
