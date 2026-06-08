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

#[test]
fn examples_with_unit_tests_pass() {
    let examples_dir = project_root().join("examples");
    let mut names: Vec<String> = std::fs::read_dir(&examples_dir)
        .expect("read examples directory")
        .filter_map(|entry| {
            let path = entry.expect("read examples entry").path();
            let source = std::fs::read_to_string(&path).ok()?;
            if source.contains("\ntest \"") {
                path.file_stem()?.to_str().map(str::to_owned)
            } else {
                None
            }
        })
        .collect();
    names.sort();

    assert!(!names.is_empty(), "expected examples with unit tests");
    for name in names {
        let (ok, stdout, stderr) = test_example(&name);
        assert!(
            ok,
            "keel test examples/{name}.keel failed\nstdout: {stdout}\nstderr: {stderr}"
        );
        assert!(
            stderr.contains("test") && stderr.contains("passed"),
            "expected test summary for examples/{name}.keel\nstderr: {stderr}"
        );
    }
}
