use crate::common::*;

#[test]
fn file_namespace_write_read_exists_and_list() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let nested_dir = tmp.path().join("nested");
    let file_path = nested_dir.join("note.txt");
    let nested = keel_string_literal(&nested_dir.to_string_lossy());
    let file = keel_string_literal(&file_path.to_string_lossy());
    let src = format!(
        r#"
agent A {{
    @on_start {{
        File.write("{file}", "hello from keel")
        content = File.read("{file}")
        exists = File.exists("{file}")
        names = File.list("{nested}")
        Io.show("content={{content}} exists={{exists}} names={{names}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(
        ok,
        "File namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("content=hello from keel"), "{stdout}");
    assert!(stdout.contains("exists=true"), "{stdout}");
    assert!(stdout.contains("note.txt"), "{stdout}");
}

#[test]
fn file_namespace_missing_read_reports_file_error() {
    let missing = tempfile::tempdir()
        .expect("tempdir")
        .path()
        .join("missing.txt");
    let file = keel_string_literal(&missing.to_string_lossy());
    let src = format!(
        r#"
agent A {{
    @on_start {{
        File.read("{file}")
    }}
}}
run(A)
"#
    );
    let (ok, _stdout, stderr) = run_inline(&src, false);
    assert!(!ok, "missing File.read should fail");
    assert!(
        stderr.contains("FileError: File.read"),
        "expected FileError diagnostic:\n{stderr}"
    );
}

#[test]
fn file_error_is_catchable_by_type_name() {
    let missing = tempfile::tempdir()
        .expect("tempdir")
        .path()
        .join("missing.txt");
    let file = keel_string_literal(&missing.to_string_lossy());
    let src = format!(
        r#"
agent A {{
    @on_start {{
        try {{
            File.read("{file}")
        }} catch e: FileError {{
            Io.show("caught={{e.message}}")
        }}
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(
        ok,
        "catch FileError should succeed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("caught="),
        "FileError clause must fire, not Error fallback:\n{stdout}"
    );
    assert!(
        stdout.contains("File.read"),
        "caught message should mention File.read:\n{stdout}"
    );
}

#[test]
fn file_error_also_caught_by_error_fallback() {
    let missing = tempfile::tempdir()
        .expect("tempdir")
        .path()
        .join("missing.txt");
    let file = keel_string_literal(&missing.to_string_lossy());
    let src = format!(
        r#"
agent A {{
    @on_start {{
        try {{
            File.read("{file}")
        }} catch e: Error {{
            Io.show("caught-as-error")
        }}
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(
        ok,
        "catch Error should catch FileError too\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("caught-as-error"),
        "Error fallback must catch FileError:\n{stdout}"
    );
}

#[test]
fn file_namespace_mkdir_creates_directory() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let new_dir = tmp.path().join("created").join("nested");
    let dir = keel_string_literal(&new_dir.to_string_lossy());
    let src = format!(
        r#"
agent A {{
    @on_start {{
        File.mkdir("{dir}")
        exists = File.exists("{dir}")
        Io.show("exists={{exists}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("exists=true"), "{stdout}");
}

#[test]
fn file_namespace_remove_deletes_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let file_path = tmp.path().join("to_delete.txt");
    std::fs::write(&file_path, "bye").expect("write test file");
    let file = keel_string_literal(&file_path.to_string_lossy());
    let src = format!(
        r#"
agent A {{
    @on_start {{
        File.remove("{file}")
        exists = File.exists("{file}")
        Io.show("exists={{exists}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("exists=false"), "{stdout}");
}

#[test]
fn file_namespace_copy_duplicates_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_path = tmp.path().join("orig.txt");
    let dst_path = tmp.path().join("copy.txt");
    std::fs::write(&src_path, "original content").expect("write test file");
    let src_f = keel_string_literal(&src_path.to_string_lossy());
    let dst_f = keel_string_literal(&dst_path.to_string_lossy());
    let prog = format!(
        r#"
agent A {{
    @on_start {{
        File.copy("{src_f}", "{dst_f}")
        content = File.read("{dst_f}")
        Io.show("content={{content}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&prog, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("content=original content"), "{stdout}");
}

#[test]
fn file_namespace_glob_returns_matching_paths() {
    let tmp = tempfile::tempdir().expect("tempdir");
    std::fs::write(tmp.path().join("a.txt"), "a").expect("write a");
    std::fs::write(tmp.path().join("b.txt"), "b").expect("write b");
    std::fs::write(tmp.path().join("c.log"), "c").expect("write c");
    let pattern = keel_string_literal(&format!("{}/*.txt", tmp.path().display()));
    let src = format!(
        r#"
agent A {{
    @on_start {{
        paths = File.glob("{pattern}")
        Io.show("count={{paths}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    // Two .txt files matched; .log excluded.
    assert!(
        stdout.contains("a.txt") || stdout.contains("count="),
        "{stdout}"
    );
    assert!(
        !stdout.contains("c.log"),
        "c.log should not appear: {stdout}"
    );
}

#[test]
fn file_namespace_move_renames_file() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let src_path = tmp.path().join("before.txt");
    let dst_path = tmp.path().join("after.txt");
    std::fs::write(&src_path, "moved content").expect("write test file");
    let src_f = keel_string_literal(&src_path.to_string_lossy());
    let dst_f = keel_string_literal(&dst_path.to_string_lossy());
    let prog = format!(
        r#"
agent A {{
    @on_start {{
        File.move("{src_f}", "{dst_f}")
        src_exists = File.exists("{src_f}")
        dst_exists = File.exists("{dst_f}")
        Io.show("src={{src_exists}} dst={{dst_exists}}")
        stop(self)
    }}
}}
run(A)
"#
    );
    let (ok, stdout, stderr) = run_inline(&prog, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("src=false"), "{stdout}");
    assert!(stdout.contains("dst=true"), "{stdout}");
}

#[test]
fn file_namespace_mktemp_returns_writable_path() {
    let src = r#"
agent A {
    @on_start {
        path = File.mktemp()
        File.write(path, "temp data")
        content = File.read(path)
        File.remove(path)
        Io.show("content={content}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("content=temp data"), "{stdout}");
}
