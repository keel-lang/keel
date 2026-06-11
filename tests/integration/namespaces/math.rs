use crate::common::*;

// ---------------------------------------------------------------------------
// Math namespace
// ---------------------------------------------------------------------------

#[test]
fn math_namespace_core_functions() {
    let src = r#"
use std/io
use std/math
agent MathTest {
    @on_start {
        sq   = math.sqrt(4)
        pw   = math.pow(2, 10)
        lg   = math.log(math.E())
        lg2  = math.log2(8)
        lg10 = math.log10(100)
        ex   = math.exp(0)
        sn   = math.sin(0)
        cs   = math.cos(0)
        pi   = math.PI()
        io.show("sqrt={sq} pow={pw} log={lg} log2={lg2} log10={lg10} exp={ex} sin={sn} cos={cs} pi_ok={pi > 3.14}")
        stop(self)
    }
}
run(MathTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("sqrt=2"), "sqrt failed: {stdout}");
    assert!(stdout.contains("pow=1024"), "pow failed: {stdout}");
    assert!(stdout.contains("log=1"), "log failed: {stdout}");
    assert!(stdout.contains("log2=3"), "log2 failed: {stdout}");
    assert!(stdout.contains("log10=2"), "log10 failed: {stdout}");
    assert!(stdout.contains("exp=1"), "exp failed: {stdout}");
    assert!(stdout.contains("sin=0"), "sin failed: {stdout}");
    assert!(stdout.contains("cos=1"), "cos failed: {stdout}");
    assert!(stdout.contains("pi_ok=true"), "PI failed: {stdout}");
}

#[test]
fn math_sqrt_rejects_negative() {
    let src = r#"
use std/io
use std/math
agent MathErr {
    @on_start {
        try {
            math.sqrt(-1)
        } catch e: Error {
            io.show("caught={e.message}")
        }
        stop(self)
    }
}
run(MathErr)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("caught="),
        "expected error to be caught: {stdout}"
    );
}

#[test]
fn math_error_is_catchable_by_specific_type() {
    let src = r#"
use std/io
use std/math
agent MathTyped {
    @on_start {
        try {
            math.sqrt(-4.0)
        } catch e: MathError {
            io.show("kind=MathError")
            io.show("msg={e.message.len() > 0}")
        }
        stop(self)
    }
}
run(MathTyped)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("kind=MathError"),
        "expected MathError caught by specific type:\n{stdout}"
    );
    assert!(
        stdout.contains("msg=true"),
        "expected message field populated:\n{stdout}"
    );
}
