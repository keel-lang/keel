use crate::common::*;

// ---------------------------------------------------------------------------
// Stringable — impl Stringable for Type
// ---------------------------------------------------------------------------

#[test]
fn impl_stringable_interpolates_via_to_str() {
    let src = r#"
use std/io
type Point {
  x: int
  y: int
}

impl Stringable for Point {
  task to_str(self) -> str {
    "({self.x}, {self.y})"
  }
}

task run_test() {
  p: Point = { x: 3, y: 4 }
  io.show("{p}")
}

run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("(3, 4)"),
        "expected '(3, 4)' in stdout:\n{stdout}"
    );
}

#[test]
fn enum_variant_interpolates_as_variant_name() {
    let src = r#"
use std/io
type Signal = buy | sell | hold

agent A {
  @on_start {
    s: Signal = Signal.buy
    io.show("{s}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, true);
    assert!(
        ok,
        "program failed
stdout: {stdout}
stderr: {stderr}"
    );
    assert!(
        stdout.contains("buy"),
        "expected 'buy' in stdout:\n{stdout}"
    );
}

#[test]
fn impl_stringable_explicit_to_str_call() {
    let src = r#"
use std/io
type Color {
  r: int
  g: int
  b: int
}

impl Stringable for Color {
  task to_str(self) -> str {
    "rgb({self.r}, {self.g}, {self.b})"
  }
}

task run_test() {
  c: Color = { r: 255, g: 128, b: 0 }
  io.show(c.to_str())
}

run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("rgb(255, 128, 0)"),
        "expected 'rgb(255, 128, 0)' in stdout:\n{stdout}"
    );
}

#[test]
fn impl_stringable_multiple_types() {
    let src = r#"
use std/io
type Celsius { value: float }
type Fahrenheit { value: float }

impl Stringable for Celsius {
  task to_str(self) -> str {
    "{self.value}°C"
  }
}

impl Stringable for Fahrenheit {
  task to_str(self) -> str {
    "{self.value}°F"
  }
}

task run_test() {
  hot: Celsius    = { value: 37.0 }
  cold: Fahrenheit = { value: 32.0 }
  io.show("{hot}")
  io.show("{cold}")
}

run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("37"), "expected Celsius output:\n{stdout}");
    assert!(
        stdout.contains("32"),
        "expected Fahrenheit output:\n{stdout}"
    );
}

#[test]
fn primitives_still_interpolate_without_impl() {
    let src = r#"
use std/io
task run_test() {
  n = 42
  f = 3.14
  b = true
  io.show("{n} {f} {b}")
}
run_test()
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("42") && stdout.contains("3.14") && stdout.contains("true"),
        "primitives: {stdout}"
    );
}
