use crate::common::*;

// ---------------------------------------------------------------------------
// User-defined interfaces
// ---------------------------------------------------------------------------

#[test]
fn user_defined_interface_and_impl() {
    let src = r#"
use std/io
interface Greetable {
  task greet(self) -> str
}

type Person {
  name: str
}

impl Greetable for Person {
  task greet(self) -> str {
    "Hello, {self.name}!"
  }
}

task run_test() {
  p: Person = { name: "Alice" }
  io.show(p.greet())
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("Hello, Alice!"), "got: {stdout}");
}

#[test]
fn impl_unknown_interface_is_an_error() {
    let src = r#"
use std/io
type Dog { name: str }
impl Unknown for Dog {
  task bark(self) -> str { "Woof" }
}
task run_test() { io.show("x") }
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed");
    assert!(
        stderr.contains("unknown interface") || stderr.contains("Unknown"),
        "expected unknown-interface error, got: {stderr}"
    );
}

#[test]
fn impl_missing_required_method_is_an_error() {
    let src = r#"
use std/io
interface Describable {
  task describe(self) -> str
  task short(self) -> str
}
type Item { label: str }
impl Describable for Item {
  task describe(self) -> str { self.label }
}
task run_test() { io.show("x") }
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed");
    assert!(
        stderr.contains("missing") || stderr.contains("short"),
        "expected missing-method error, got: {stderr}"
    );
}

#[test]
fn impl_extra_method_not_in_interface_is_an_error() {
    let src = r#"
use std/io
interface Labeled {
  task label(self) -> str
}
type Tag { value: str }
impl Labeled for Tag {
  task label(self) -> str { self.value }
  task extra(self) -> str { "oops" }
}
task run_test() { io.show("x") }
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed");
    assert!(
        stderr.contains("not part of interface") || stderr.contains("extra"),
        "expected extra-method error, got: {stderr}"
    );
}

#[test]
fn impl_wrong_return_type_is_an_error() {
    let src = r#"
use std/io
interface Scorer {
  task score(self) -> int
}
type Game { pts: int }
impl Scorer for Game {
  task score(self) -> str { "oops" }
}
task run_test() { io.show("x") }
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed");
    assert!(
        stderr.contains("return") || stderr.contains("score") || stderr.contains("str"),
        "expected return-type mismatch error, got: {stderr}"
    );
}

#[test]
fn interface_declared_after_impl_still_works() {
    let src = r#"
use std/io
type Square { side: int }
impl Sizable for Square {
  task size(self) -> int { self.side * self.side }
}
interface Sizable {
  task size(self) -> int
}
task run_test() {
  b: Square = { side: 4 }
  io.show("{b.size()}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("16"), "got: {stdout}");
}

// ─── Interface conformance: checker/runtime agreement ────────────────────────
//
// Regression tests for the soundness hole where `keel check` passed a program
// that `keel run` then rejected (or vice versa) on the same conformance rule.
// Both phases now delegate to `types::interface::signature_satisfies`.

#[test]
fn impl_generic_return_type_mismatch_caught_by_checker() {
    // Before the fix, checker collapsed Generic to "unknown" and passed this;
    // only the runtime caught it.  Now both phases must reject it.
    let src = r#"
use std/io
interface R {
  task f(self) -> Result[str, int]
}
type Foo { x: str }
impl R for Foo {
  task f(self) -> Result[bool, str] { "wrong" }
}
task run_test() { io.show("x") }
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed — generic return type mismatch");
    assert!(
        stderr.contains("Result") || stderr.contains("return"),
        "expected conformance error, got: {stderr}"
    );
}

#[test]
fn impl_generic_return_type_exact_match_passes() {
    let src = r#"
use std/io
interface R {
  task f(self) -> Result[str, int]
}
type Foo { x: str }
impl R for Foo {
  task f(self) -> Result[str, int] { "ok" }
}
task run_test() { io.show("ok") }
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
}

#[test]
fn impl_struct_return_type_mismatch_caught_by_checker() {
    // Before the fix, checker collapsed Struct to "unknown" and passed this.
    let src = r#"
use std/io
interface S {
  task f(self) -> {name: str}
}
type Bar { x: int }
impl S for Bar {
  task f(self) -> {age: int} { { age: 42 } }
}
task run_test() { io.show("x") }
run_test()
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "should have failed — struct return type mismatch");
    assert!(
        stderr.contains("return") || stderr.contains("name") || stderr.contains("age"),
        "expected conformance error, got: {stderr}"
    );
}

#[test]
fn impl_struct_return_type_exact_match_passes() {
    let src = r#"
use std/io
interface S {
  task f(self) -> {name: str}
}
type Baz { x: int }
impl S for Baz {
  task f(self) -> {name: str} { { name: "hello" } }
}
task run_test() { io.show("ok") }
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
}

#[test]
fn impl_dynamic_return_type_accepts_any_concrete() {
    // `dynamic` in the interface return type is an explicit wildcard.
    let src = r#"
use std/io
interface Flexible {
  task get(self) -> dynamic
}
type Wrap { n: int }
impl Flexible for Wrap {
  task get(self) -> int { self.n }
}
task run_test() { io.show("ok") }
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "dynamic should accept any concrete type\nstdout: {stdout}\nstderr: {stderr}"
    );
}
