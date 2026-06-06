use crate::common::*;

// ---------------------------------------------------------------------------
// Time namespace
// ---------------------------------------------------------------------------

#[test]
fn time_epoch_ms_returns_positive_integer() {
    let src = r#"
agent EpochTest {
    @on_start {
        ms = Time.epoch_ms()
        if ms > 0 {
            Io.show("ok={ms > 1_000_000_000_000}")
        }
        stop(self)
    }
}
run(EpochTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("ok=true"),
        "Time.epoch_ms() should exceed 1_000_000_000_000:\n{stdout}"
    );
}
