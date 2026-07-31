//! Exit criterion for issue #172 (map containers, end to end): a program
//! building a `map[str, V]`, mutating it via `.insert`, reading it back
//! through every lowered method (`get`, `contains`/`has`, `keys`, `values`,
//! `len`, `is_empty`), iterating it, and observing correct copy-on-write
//! behavior on aliased bindings — compiles, links, and matches the
//! interpreter byte-for-byte.
//!
//! `.insert` is always-clone, same as list's `.push` (see
//! `keel-codegen/tests/lists.rs`'s module doc for why that makes the
//! aliasing case correct by construction rather than by luck).
//!
//! `get`'s miss path is the highest-risk shape in this file: it exercises a
//! genuinely different LLVM basic block (the `keel_is_none`-guarded none
//! branch) than the hit path, and a `map[str, str]`'s `get` exercises the
//! pointer-based nullable repr instead of `map[str, int]`'s scalar `{i1,T}`
//! repr — both are covered explicitly below rather than only ever hitting.

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
fn aliased_bindings_do_not_observe_each_others_insert() {
    // The differential aliasing test, mirroring `lists.rs`'s
    // `aliased_bindings_do_not_observe_each_others_push`: `b` aliases `a`
    // *before* `a` is reassigned via `.insert`, and both are read *after*.
    // `.len()` is the observation rather than `io.show(a)` because a map
    // renders as a multi-line box; the counts alone distinguish the two
    // bindings, which is all the aliasing question turns on.
    let source = r#"
use std/io

a: map[str, int] = {apples: 1}
b = a
a = a.insert("pears", 2)
io.show(a.len())
io.show(b.len())
io.show(a.get("pears") ?? -1)
io.show(b.get("pears") ?? -1)
"#;
    assert_matches_interpreter(source, b"  2\n  1\n  2\n  -1\n");
}

#[test]
fn insert_overwrites_an_existing_key_in_both_engines() {
    let source = r#"
use std/io

stock: map[str, int] = {apples: 1}
stock = stock.insert("apples", 9)
io.show(stock.len())
io.show(stock.get("apples") ?? -1)
"#;
    assert_matches_interpreter(source, b"  1\n  9\n");
}

#[test]
fn get_hit_and_miss_match_the_interpreter_scalar_value() {
    let source = r#"
use std/io

stock: map[str, int] = {apples: 1}
found = stock.get("apples") ?? -1
missing = stock.get("bananas") ?? -1
io.show(found)
io.show(missing)
"#;
    assert_matches_interpreter(source, b"  1\n  -1\n");
}

#[test]
fn get_hit_and_miss_match_the_interpreter_str_value() {
    // A `str`-valued map's `.get` returns `str?`, which uses the
    // pointer-based nullable repr — a different codegen branch than the
    // scalar `{i1,T}` repr the `int`-valued test above exercises.
    let source = r#"
use std/io

labels: map[str, str] = {a: "excellent", b: "good"}
io.show(labels.get("a") ?? "unknown")
io.show(labels.get("z") ?? "unknown")
"#;
    assert_matches_interpreter(source, b"  excellent\n  unknown\n");
}

#[test]
fn len_contains_and_is_empty_match_the_interpreter() {
    let source = r#"
use std/io

stock: map[str, int] = {apples: 1, pears: 2}
io.show(stock.len())
io.show(stock.contains("apples"))
io.show(stock.contains("bananas"))
io.show(stock.has("pears"))
io.show(stock.is_empty())
"#;
    assert_matches_interpreter(source, b"  2\n  true\n  false\n  true\n  false\n");
}

#[test]
fn keys_and_values_match_the_interpreter() {
    let source = r#"
use std/io

stock: map[str, int] = {apples: 1, pears: 2}
io.show(stock.keys())
io.show(stock.values())
"#;
    // Both are documented as sorted by key so codegen and the interpreter
    // agree on order without relying on hash-map iteration order.
    assert_matches_interpreter(source, b"  1. apples\n  2. pears\n  1. 1\n  2. 2\n");
}

#[test]
fn iterating_keys_and_summing_looked_up_values_matches_the_interpreter() {
    let source = r#"
use std/io

task total_stock(stock: map[str, int]) -> int {
  sum = 0
  for key in stock.keys() {
    sum += (stock.get(key) ?? 0)
  }
  return sum
}

stock: map[str, int] = {apples: 1, pears: 2}
io.show(total_stock(stock))
"#;
    assert_matches_interpreter(source, b"  3\n");
}
