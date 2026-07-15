use crate::common::*;

// ---------------------------------------------------------------------------
// String interpolation
// ---------------------------------------------------------------------------

#[test]
fn string_interp_method_call() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        items = [1, 2, 3]
        msg = "size={items.count()}"
        io.show(msg)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("size=3"),
        "expected 'size=3' in stdout:\n{stdout}"
    );
}

#[test]
fn string_interp_binary_expr() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        x = 5
        msg = "doubled={x * 2}"
        io.show(msg)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("doubled=10"),
        "expected 'doubled=10' in stdout:\n{stdout}"
    );
}

#[test]
fn nested_string_in_interpolation_slot() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        score = 0.9
        msg = "label={if score > 0.8 { "high" } else { "low" }}"
        io.show(msg)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("label=high"),
        "expected 'label=high' in:\n{stdout}"
    );
}

#[test]
fn nested_string_double_layer() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        x = "world"
        msg = "hi {"there {x}"}"
        io.show(msg)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("hi there world"),
        "expected nested interp resolution:\n{stdout}"
    );
}

// ---------------------------------------------------------------------------
// Interpolation regressions (issue #14)
// ---------------------------------------------------------------------------

#[test]
fn interp_underscore_ident_is_not_mangled() {
    // An identifier like x1_2 must resolve as the variable x1_2 at runtime,
    // not as x12 (which would be undefined).
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    x1_2 = "hello"
    io.show("{x1_2}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("hello"),
        "underscore ident not resolved correctly: {stdout}"
    );
}

#[test]
fn interp_digit_separator_in_numeric_literal() {
    // 1_000 in an interpolation slot must be treated as the integer 1000.
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    n = 2000
    io.show("{n > 1_000}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("true"),
        "digit separator comparison gave wrong result: {stdout}"
    );
}

// ---------------------------------------------------------------------------
// String methods — repeat, slice, index_of, trim_start, trim_end,
//                  to_int, to_float
// ---------------------------------------------------------------------------

#[test]
fn string_repeat_produces_n_copies() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    io.show("{"ha".repeat(3)}")
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
    assert!(stdout.contains("hahaha"), "repeat: {stdout}");
}

#[test]
fn string_slice_extracts_chars() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    io.show("{"hello world".slice(6, 11)}")
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
    assert!(stdout.contains("world"), "slice: {stdout}");
}

#[test]
fn string_index_of_returns_position_or_none() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    io.show("{"hello world".index_of("world")}")
    io.show("{"hello world".index_of("xyz")}")
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
    assert!(stdout.contains("6"), "index_of found: {stdout}");
    assert!(stdout.contains("none"), "index_of miss: {stdout}");
}

#[test]
fn string_trim_start_and_trim_end() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    s = "  hi  "
    io.show("{s.trim_start()}")
    io.show("{s.trim_end()}")
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
    assert!(stdout.contains("hi  "), "trim_start: {stdout}");
    assert!(stdout.contains("  hi"), "trim_end: {stdout}");
}

#[test]
fn string_to_int_parses_or_returns_none() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    io.show("{"42".to_int()}")
    io.show("{"bad".to_int()}")
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
    assert!(stdout.contains("42"), "to_int parse: {stdout}");
    assert!(stdout.contains("none"), "to_int fail: {stdout}");
}

#[test]
fn string_to_float_parses_or_returns_none() {
    let src = r#"
use std/io
agent A {
  @tools [io]
  @on_start {
    io.show("{"3.14".to_float()}")
    io.show("{"nope".to_float()}")
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
    assert!(stdout.contains("3.14"), "to_float parse: {stdout}");
    assert!(stdout.contains("none"), "to_float fail: {stdout}");
}

// ---------------------------------------------------------------------------
// Format specifiers in string interpolation
// ---------------------------------------------------------------------------

#[test]
fn format_spec_float_precision() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        pi = 3.14159
        io.show("{pi:.2f}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("3.14"),
        "expected '3.14' in stdout:\n{stdout}"
    );
    assert!(
        !stdout.contains("3.1415"),
        "unexpected extra precision in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_int_as_float() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        n = 42
        io.show("{n:.2f}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("42.00"),
        "expected '42.00' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_right_align() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        n = 7
        io.show("{n:>5}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("    7"),
        "expected right-aligned '    7' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_left_align() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        s = "hi"
        io.show("{s:<6}!")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("hi    !"),
        "expected left-aligned 'hi    !' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_center_align() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        s = "hi"
        io.show("{s:^6}!")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("  hi  !"),
        "expected centered '  hi  !' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_combined_align_and_precision() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        x = 3.14159
        io.show("{x:>10.2f}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("      3.14"),
        "expected right-aligned '      3.14' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_named_arg_colon_not_confused_with_spec() {
    let src = r#"
use std/io
task greet(name: str) -> str {
    "hello {name}"
}
agent A {
    @tools [io]
    @on_start {
        io.show(greet(name: "world"))
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("hello world"),
        "expected 'hello world' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_bare_width_right_aligns() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        n = 5
        io.show("{n:4}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("   5"),
        "expected right-aligned '   5' in stdout:\n{stdout}"
    );
}

#[test]
fn format_spec_alignment_respects_custom_to_str() {
    // Alignment specs must call to_str() via impl dispatch, not fall back to
    // the default Display formatting, so {x:>10} and {x} produce the same base string.
    let src = r#"
use std/io
type Tag { value: str }
impl Stringable for Tag {
    task to_str(self) -> str {
        "[{self.value}]"
    }
}
agent A {
    @tools [io]
    @on_start {
        t: Tag = { value: "ok" }
        io.show("{t}")
        io.show("{t:>8}")
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("[ok]"),
        "expected '[ok]' from bare interp:\n{stdout}"
    );
    assert!(
        stdout.contains("    [ok]"),
        "expected '    [ok]' from aligned interp (custom to_str must be respected):\n{stdout}"
    );
}

#[test]
fn format_spec_unknown_type_flag_is_runtime_error() {
    let src = r#"
use std/io
agent A {
    @tools [io]
    @on_start {
        pi = 3.14
        io.show("{pi:.2x}")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected runtime error for unknown format spec type");
    assert!(
        stderr.contains("unknown format spec type"),
        "expected 'unknown format spec type' in stderr:\n{stderr}"
    );
}
