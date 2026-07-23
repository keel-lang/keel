//! Exit criterion for issue #147 (list slice): a program building a
//! `list[int]`, pushing/indexing/iterating it, and observing correct
//! copy-on-write *behavior* — two bindings that alias the same list before
//! one is reassigned via `.push(...)` must not see each other's change —
//! compiles, links, and matches the interpreter byte-for-byte.
//!
//! `push`/every list mutation is a pure value method (confirmed against the
//! interpreter: a bare `a.push(4)` statement is a no-op on `a`'s binding —
//! the result must be reassigned, `a = a.push(4)`, matching Keel's
//! always-declares assignment scoping). Codegen implements this as
//! always-clone-on-push (see `keel-rt-ffi`'s `keel_list_push` doc) rather
//! than real reference-counted copy-on-write — the RC-insertion pass
//! (retain-on-alias, release-on-scope-exit) that real CoW would need
//! doesn't exist in this codebase yet, so always-clone is what makes the
//! aliasing scenario below correct *by construction*, not just by testing.

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
fn aliased_bindings_do_not_observe_each_others_push() {
    // The differential aliasing test: `b` is bound to `a` *before* `a` is
    // reassigned via push, and both are printed *after* — the only shape
    // that actually exercises whether the two engines agree on the
    // mutation model (see this file's module doc). A bare `a.push(4)`
    // (discarding the result) would make this pass vacuously.
    let source = r#"
use std/io

a = [1, 2, 3]
b = a
a = a.push(4)
io.show(a)
io.show(b)
"#;
    assert_matches_interpreter(
        source,
        b"  1. 1\n  2. 2\n  3. 3\n  4. 4\n  1. 1\n  2. 2\n  3. 3\n",
    );
}

#[test]
fn push_len_index_and_for_each_match_the_interpreter() {
    let source = r#"
use std/io

task sum(xs: list[int]) -> int {
  total = 0
  for x in xs {
    total += x
  }
  return total
}

a = [1, 2, 3]
a = a.push(4)
io.show(a.len())
io.show(a[0])
io.show(a[3])
io.show(sum(a))
"#;
    assert_matches_interpreter(source, b"  4\n  1\n  4\n  10\n");
}

#[test]
fn list_of_str_elements_matches_the_interpreter() {
    let source = r#"
use std/io

names = ["alice", "bob"]
names = names.push("carol")
io.show(names.len())
io.show(names[2])
"#;
    assert_matches_interpreter(source, b"  3\n  carol\n");
}
