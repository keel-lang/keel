use crate::common::*;

// ---------------------------------------------------------------------------
// Math namespace
// ---------------------------------------------------------------------------

#[test]
fn math_namespace_core_functions() {
    let src = r#"
agent MathTest {
    @on_start {
        sq   = Math.sqrt(4)
        pw   = Math.pow(2, 10)
        lg   = Math.log(Math.E())
        lg2  = Math.log2(8)
        lg10 = Math.log10(100)
        ex   = Math.exp(0)
        sn   = Math.sin(0)
        cs   = Math.cos(0)
        pi   = Math.PI()
        Io.show("sqrt={sq} pow={pw} log={lg} log2={lg2} log10={lg10} exp={ex} sin={sn} cos={cs} pi_ok={pi > 3.14}")
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
agent MathErr {
    @on_start {
        try {
            Math.sqrt(-1)
        } catch e: Error {
            Io.show("caught={e.message}")
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
