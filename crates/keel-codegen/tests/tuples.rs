//! Exit criterion for issue #157: a program declaring a tuple type,
//! constructing a literal, and reading elements by position compiles, links,
//! and matches the interpreter byte-for-byte.
//!
//! Tuples are the first genuinely **by-value aggregate** in the backend —
//! an unnamed LLVM struct built with `insertvalue` and read with
//! `extractvalue`, with no heap allocation, no `ptr` indirection, and no RC
//! (`designs/llvm-compilation.md` §1.1). That makes them a different path
//! from `structs.rs`, which exercises heap allocation and field `GEP`s;
//! `layout.rs` still rejects an all-scalar *struct* for exactly the
//! by-value-codegen reason these tests cover for tuples.

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

const SCALAR_SOURCE: &str = r#"
use std/io

task sum_pair(p: (int, int)) -> int {
  return p.0 + p.1
}

pair: (int, int) = (7, 35)
io.show(pair.0)
io.show(pair.1)
io.show(sum_pair(pair))
"#;

#[test]
fn scalar_tuple_construct_and_positional_read_match_the_interpreter() {
    let compiled = compile_and_run(SCALAR_SOURCE);
    let interpreted = support::run_interpreter(SCALAR_SOURCE);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, b"  7\n  35\n  42\n");
}

#[test]
fn tuple_crosses_a_call_boundary_by_value() {
    // The aggregate is passed as an LLVM struct argument and returned as an
    // LLVM struct return — the two paths most likely to need aggregate-
    // specific handling beyond insertvalue/extractvalue.
    let source = r#"
use std/io

task swap(p: (int, float)) -> (float, int) {
  return (p.1, p.0)
}

original: (int, float) = (3, 2.5)
swapped = swap(original)
io.show(swapped.0)
io.show(swapped.1)
"#;
    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, b"  2.5\n  3\n");
}

#[test]
fn str_element_rides_as_a_boxed_pointer_inside_the_aggregate() {
    // A `str` element is a boxed `Value*`, so the aggregate mixes a pointer
    // with a scalar. Copying the tuple duplicates that pointer without a
    // retain, which is consistent with the rest of the backend today
    // (`passes::rc::insert_rc` is a no-op, so nothing releases either) —
    // see `TupleLayout`'s doc.
    let source = r#"
use std/io

pair: (str, int) = ("hello", 42)
io.show(pair.0)
io.show(pair.1)
"#;
    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, b"  hello\n  42\n");
}

#[test]
fn nested_tuple_nests_as_a_nested_aggregate() {
    // Nested tuples are allowed where nested containers are not: a by-value
    // aggregate nests for free, with no `Value` marshaling
    // (`is_tuple_element_ty`). Both spellings of the nested read — `t.0.1`
    // and `(t.0).1` — parse to the same AST, so one KIR path covers both.
    let source = r#"
use std/io

t: ((int, int), int) = ((1, 2), 3)
io.show(t.0.1)
io.show(t.1)
"#;
    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, b"  2\n  3\n");
}

#[test]
fn positional_destructuring_binds_every_element() {
    // `(a, b) = pair` lowers to a subject local plus one `TupleGet` per
    // name, so the right-hand side is evaluated once regardless of arity.
    let source = r#"
use std/io

pair: (str, int) = ("hello", 42)
(name, count) = pair
io.show(name)
io.show(count)
"#;
    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, b"  hello\n  42\n");
}

#[test]
fn destructuring_evaluates_the_subject_once() {
    // If the subject were re-lowered per name, `bump` would run twice and
    // the counter would read 2 instead of 1.
    let source = r#"
use std/io

state_holder: (int, int) = (0, 0)

task make_pair(seed: int) -> (int, int) {
  io.show("building")
  return (seed, seed + 1)
}

(low, high) = make_pair(10)
io.show(low)
io.show(high)
"#;
    let compiled = compile_and_run(source);
    let interpreted = support::run_interpreter(source);

    assert_eq!(
        String::from_utf8_lossy(&compiled.stdout),
        String::from_utf8_lossy(&interpreted.stdout),
        "compiled stdout must match the interpreter's"
    );
    assert_eq!(compiled.stdout, b"  building\n  10\n  11\n");
}

#[test]
fn whole_tuple_as_a_namespace_argument_is_a_clean_error() {
    // There is no `keel_box_tuple`, so passing an entire aggregate across
    // the namespace boundary must be a diagnostic, not a miscompile. Pins
    // the explicit `emit_box_arg` arm against being folded into a
    // fallthrough that would treat the aggregate as a pointer.
    let source = r#"
use std/io

pair: (int, int) = (1, 2)
io.show(pair)
"#;
    let kir = support::parse_check_and_lower(source);
    let out_dir = tempfile::tempdir().expect("create temp out dir");
    let opts = BuildOptions {
        out_dir: out_dir.path().to_path_buf(),
        runtime_link_args: support::runtime_link_args().clone(),
    };
    let err = keel_codegen::compile(&kir, &opts)
        .expect_err("passing a whole tuple to a namespace call has no representation");
    let message = err.to_string();
    assert!(
        message.contains("tuple-typed argument"),
        "expected a tuple-specific diagnostic, got: {message}"
    );
}
