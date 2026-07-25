//! Exit criterion for issue #162 (set containers, construct+pass-through
//! subset): a `set[T]` literal built, bound, and passed/printed through —
//! compiles, links, and matches the interpreter byte-for-byte.
//!
//! The interpreter has no `Value::Set` variant — a `set[...]` literal
//! evaluates to a plain, non-deduplicating `Value::List` ("v0.1: sets share
//! list repr", see `KirType::Set`'s doc) — so there's no dedup behavior to
//! observe and no set methods lowered yet (`unknown_...` cases live in
//! `keel-kir`'s `lower_errors.rs`, not here). This file only pins the
//! construct+pass-through shape that #151's fixtures actually need.

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
    assert_eq!(
        compiled.status.code(),
        interpreted.status.code(),
        "compiled exit code must match the interpreter's"
    );
    assert_eq!(compiled.stdout, expected_stdout);
}

#[test]
fn set_literal_construction_and_pass_through_matches_the_interpreter() {
    let source = r#"
use std/io

nums = set[1, 2, 3]
other = nums
io.show(other)
"#;
    assert_matches_interpreter(source, b"  1. 1\n  2. 2\n  3. 3\n");
}

#[test]
fn set_of_str_elements_matches_the_interpreter() {
    let source = r#"
use std/io

names = set["alice", "bob"]
io.show(names)
"#;
    assert_matches_interpreter(source, b"  1. alice\n  2. bob\n");
}
