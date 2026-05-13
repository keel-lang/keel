use crate::common::*;
use keel_lang::pipeline;

// ---------------------------------------------------------------------------
// Smoke-check: every example must pass `keel check`
// ---------------------------------------------------------------------------

#[test]
fn examples_all_parse() {
    let examples_dir = project_root().join("examples");
    let mut names: Vec<String> = std::fs::read_dir(&examples_dir)
        .expect("read examples directory")
        .filter_map(|entry| {
            let path = entry.expect("read examples entry").path();
            if path.extension().and_then(|ext| ext.to_str()) == Some("keel") {
                path.file_stem()
                    .and_then(|stem| stem.to_str())
                    .map(str::to_owned)
            } else {
                None
            }
        })
        .collect();
    names.sort();

    for name in names {
        let path = examples_dir.join(format!("{name}.keel"));
        pipeline::check_file(&path, false)
            .unwrap_or_else(|e| panic!("`keel check {name}.keel` failed: {e}"));
    }
}

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

    // list + list and list.push produce 4 incidents; count() in interpolation
    assert!(
        stdout.contains("4 incidents in queue"),
        "list concat/push or string interpolation missing:\n{stdout}"
    );

    // All incident IDs present, including the one added via push
    assert!(stdout.contains("INC-101"), "INC-101 missing:\n{stdout}");
    assert!(
        stdout.contains("INC-104"),
        "pushed INC-104 missing:\n{stdout}"
    );

    // @on_stop fired for OnCall before removal
    assert!(
        stdout.contains("OnCall shift complete"),
        "OnCall @on_stop missing:\n{stdout}"
    );

    // Shift summary line present (fallback value in mock mode)
    assert!(
        stdout.contains("Shift summary:"),
        "shift summary line missing:\n{stdout}"
    );
}
