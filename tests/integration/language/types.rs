use crate::common::*;

// ---------------------------------------------------------------------------
// as T casts and typeof()
// ---------------------------------------------------------------------------

#[test]
fn cast_int_to_float() {
    let src = r#"
use std/io
agent A {
  @on_start {
    x: int = 5
    io.show("{x as float}")
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
    assert!(stdout.contains("5"), "got: {stdout}");
}

#[test]
fn cast_float_to_int_truncates() {
    let src = r#"
use std/io
agent A {
  @on_start {
    io.show("{1.9 as int}")
    io.show("{-1.9 as int}")
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
    assert!(stdout.contains('1'), "got: {stdout}");
    assert!(stdout.contains("-1"), "got: {stdout}");
}

#[test]
fn cast_int_to_str() {
    let src = r#"
use std/io
agent A {
  @on_start {
    io.show("{42 as str}")
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
    assert!(stdout.contains("42"), "got: {stdout}");
}

#[test]
fn cast_str_to_int() {
    let src = r#"
use std/io
agent A {
  @on_start {
    n = "99" as int
    io.show("{n}")
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
    assert!(stdout.contains("99"), "got: {stdout}");
}

#[test]
fn cast_str_to_float() {
    let src = r#"
use std/io
agent A {
  @on_start {
    f = "3.14" as float
    io.show("{f}")
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
    assert!(stdout.contains("3.14"), "got: {stdout}");
}

#[test]
fn cast_invalid_str_to_int_raises() {
    let src = r#"
use std/io
agent A {
  @on_start {
    x = "abc" as int
    io.show("{x}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, true);
    assert!(!ok, "expected runtime error");
    assert!(stderr.contains("cannot cast"), "got: {stderr}");
}

#[test]
fn cast_none_raises() {
    let src = r#"
use std/io
agent A {
  @on_start {
    x = none as int
    io.show("{x}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, true);
    assert!(!ok, "expected runtime error");
    assert!(stderr.contains("cannot cast none"), "got: {stderr}");
}

#[test]
fn typeof_primitives() {
    let src = r#"
use std/io
agent A {
  @on_start {
    io.show(typeof(42))
    io.show(typeof(3.14))
    io.show(typeof("hi"))
    io.show(typeof(true))
    io.show(typeof(none))
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
    assert!(stdout.contains("int"), "got: {stdout}");
    assert!(stdout.contains("float"), "got: {stdout}");
    assert!(stdout.contains("str"), "got: {stdout}");
    assert!(stdout.contains("bool"), "got: {stdout}");
    assert!(stdout.contains("none"), "got: {stdout}");
}

#[test]
fn typeof_struct_returns_declared_name() {
    let src = r#"
use std/io
type Point { x: int, y: int }

agent A {
  @on_start {
    p: Point = { x: 1, y: 2 }
    io.show(typeof(p))
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
    assert!(stdout.contains("Point"), "got: {stdout}");
}

#[test]
fn typeof_enum_returns_declared_name() {
    let src = r#"
use std/io
type Color = red | green | blue

agent A {
  @on_start {
    c: Color = Color.red
    io.show(typeof(c))
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
    assert!(stdout.contains("Color"), "got: {stdout}");
}

#[test]
fn cast_bool_to_str() {
    let src = r#"
use std/io
agent A {
  @on_start {
    io.show("{true as str}")
    io.show("{false as str}")
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
    assert!(stdout.contains("true"), "got: {stdout}");
    assert!(stdout.contains("false"), "got: {stdout}");
}

#[test]
fn cast_str_to_bool() {
    let src = r#"
use std/io
agent A {
  @on_start {
    io.show("{"true" as bool}")
    io.show("{"false" as bool}")
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
    assert!(stdout.contains("true"), "got: {stdout}");
    assert!(stdout.contains("false"), "got: {stdout}");
}

#[test]
fn cast_str_to_bool_invalid_raises() {
    let src = r#"
agent A {
  @on_start {
    x = "yes" as bool
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, true);
    assert!(!ok, "expected error");
    assert!(stderr.contains("cannot cast"), "got: {stderr}");
}

#[test]
fn cast_same_type_identity() {
    let src = r#"
use std/io
agent A {
  @on_start {
    io.show("{42 as int}")
    io.show("{"hi" as str}")
    io.show("{3.14 as float}")
    io.show("{true as bool}")
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
    assert!(stdout.contains("42"), "got: {stdout}");
    assert!(stdout.contains("hi"), "got: {stdout}");
    assert!(stdout.contains("3.14"), "got: {stdout}");
    assert!(stdout.contains("true"), "got: {stdout}");
}

#[test]
fn cast_uuid_to_str() {
    let src = r#"
use std/io
use std/uuid
agent A {
  @on_start {
    id = uuid.v4()
    s = id as str
    io.show(typeof(s))
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
    assert!(stdout.contains("str"), "got: {stdout}");
}

#[test]
fn cast_str_to_uuid_valid() {
    let src = r#"
use std/io
agent A {
  @on_start {
    id = "f47ac10b-58cc-4372-a567-0e02b2c3d479" as Uuid
    io.show(typeof(id))
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
    assert!(stdout.contains("Uuid"), "got: {stdout}");
}

#[test]
fn cast_str_to_uuid_invalid_raises() {
    let src = r#"
agent A {
  @on_start {
    id = "not-a-uuid" as Uuid
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, true);
    assert!(!ok, "expected error");
    assert!(stderr.contains("cannot cast"), "got: {stderr}");
}

#[test]
fn typeof_list_map_duration_uuid() {
    let src = r#"
use std/io
use std/uuid
agent A {
  @on_start {
    io.show(typeof([1, 2, 3]))
    io.show(typeof(1.s))
    io.show(typeof(uuid.v4()))
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
    assert!(stdout.contains("list"), "got: {stdout}");
    assert!(stdout.contains("duration"), "got: {stdout}");
    assert!(stdout.contains("Uuid"), "got: {stdout}");
}

// ---------------------------------------------------------------------------
// Type annotations
// ---------------------------------------------------------------------------

#[test]
fn let_annotation_valid_runs() {
    let src = r#"
use std/io
agent A {
    @on_start {
        greeting: str = "hello annotated"
        io.show(greeting)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("hello annotated"), "stdout:\n{stdout}");
}

// ---------------------------------------------------------------------------
// Null-assert operator
// ---------------------------------------------------------------------------

#[test]
fn null_assert_unwraps_non_none_value() {
    let src = r#"
use std/io
agent A {
    @on_start {
        x = "present"
        val = x!
        io.show(val)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("present"), "stdout:\n{stdout}");
}
