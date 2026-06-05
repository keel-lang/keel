use crate::common::*;

// `examples_all_parse` moved to src/pipeline.rs (pipeline::tests::examples_all_parse)
// to avoid accessing the pub(crate) pipeline module from the integration test crate.

// ---------------------------------------------------------------------------
// Comprehensive showcase — exercises every language feature in one program
// ---------------------------------------------------------------------------

#[test]
fn showcase_runs_end_to_end() {
    let (ok, stdout, stderr) = run_example("showcase");
    assert!(
        ok,
        "showcase.keel exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );

    assert!(
        stdout.contains("4 incidents in queue"),
        "list concat/push or string interpolation missing:\n{stdout}"
    );
    assert!(stdout.contains("INC-101"), "INC-101 missing:\n{stdout}");
    assert!(
        stdout.contains("INC-104"),
        "pushed INC-104 missing:\n{stdout}"
    );
    assert!(
        stdout.contains("OnCall shift complete"),
        "OnCall @on_stop missing:\n{stdout}"
    );
    assert!(
        stdout.contains("Shift summary:"),
        "shift summary line missing:\n{stdout}"
    );
}
