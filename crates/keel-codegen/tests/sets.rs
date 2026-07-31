//! Exit criterion for issue #172 (set containers, end to end): a program
//! building a `set[T]`, mutating and reading it, and observing correct
//! copy-on-write behavior on aliased bindings compiles, links, and matches
//! the interpreter byte-for-byte.
//!
//! A set is now its own `Value::Set` with real dedup, not the `Value::List`
//! issue #162 deferred it as. Both engines dedup through the *same* function
//! — `keel-runtime`'s `value::set_insert`, which `keel-rt-ffi`'s
//! `keel_set_insert` calls — so membership agrees by construction; these
//! tests pin the surrounding lowering and rendering, which don't.
//!
//! Mutation is always-clone, same as list's `.push` (see
//! `keel-codegen/tests/lists.rs`'s module doc for why that makes the
//! aliasing case correct by construction rather than by luck).

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
fn aliased_bindings_do_not_observe_each_others_add() {
    // The differential aliasing test, mirroring `lists.rs`'s
    // `aliased_bindings_do_not_observe_each_others_push`: `b` aliases `a`
    // *before* `a` is reassigned via `.add`, and both are printed *after*.
    // A bare `a.add(4)` discarding the result would make this pass vacuously.
    let source = r#"
use std/io

a = set[1, 2, 3]
b = a
a = a.add(4)
io.show(a)
io.show(b)
"#;
    assert_matches_interpreter(
        source,
        b"  1. 1\n  2. 2\n  3. 3\n  4. 4\n  1. 1\n  2. 2\n  3. 3\n",
    );
}

#[test]
fn duplicate_elements_collapse_in_both_engines() {
    // The behavior issue #162 could not test: dedup at literal construction
    // *and* through `.add`, observable in both the element count and the
    // rendered output. Re-adding an existing element is a no-op, not an
    // error and not a second entry.
    let source = r#"
use std/io

nums = set[1, 2, 2, 3]
io.show(nums.len())
io.show(nums)
again = nums.add(2)
io.show(again.len())
io.show(again)
"#;
    assert_matches_interpreter(
        source,
        b"  3\n  1. 1\n  2. 2\n  3. 3\n  3\n  1. 1\n  2. 2\n  3. 3\n",
    );
}

#[test]
fn contains_len_and_is_empty_match_the_interpreter() {
    let source = r#"
use std/io

nums = set[1, 2, 3]
io.show(nums.contains(2))
io.show(nums.contains(9))
io.show(nums.count())
io.show(nums.is_empty())
"#;
    assert_matches_interpreter(source, b"  true\n  false\n  3\n  false\n");
}

#[test]
fn set_of_str_elements_matches_the_interpreter() {
    let source = r#"
use std/io

names = set["alice", "bob", "alice"]
names = names.add("carol")
io.show(names.len())
io.show(names.contains("bob"))
io.show(names)
"#;
    assert_matches_interpreter(source, b"  3\n  true\n  1. alice\n  2. bob\n  3. carol\n");
}
