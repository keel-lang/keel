use crate::common::*;

// ---------------------------------------------------------------------------
// Time namespace
// ---------------------------------------------------------------------------

#[test]
fn time_epoch_ms_returns_positive_integer() {
    let src = r#"
use std/io
use std/time
agent EpochTest {
    @tools [io]
    @on_start {
        ms = time.epoch_ms()
        if ms > 0 {
            io.show("ok={ms > 1_000_000_000_000}")
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
        "time.epoch_ms() should exceed 1_000_000_000_000:\n{stdout}"
    );
}
