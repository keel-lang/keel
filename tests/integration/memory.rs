use crate::common::*;
use std::process::Command;

// ---------------------------------------------------------------------------
// Memory namespace
// ---------------------------------------------------------------------------

#[test]
fn memory_session_remember_recall() {
    let src = r#"
agent A {
  @memory session
  @on_start {
    Memory.remember("name", "Alice")
    val = Memory.recall("name")
    Io.show("got: {val}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("got: Alice"),
        "recall should return stored value:\n{stdout}"
    );
}

#[test]
fn memory_session_recall_missing_returns_none() {
    let src = r#"
agent A {
  @memory session
  @on_start {
    val = Memory.recall("nonexistent")
    if val == none {
      Io.show("was none")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("was none"),
        "missing key should return none:\n{stdout}"
    );
}

#[test]
fn memory_session_forget() {
    let src = r#"
agent A {
  @memory session
  @on_start {
    Memory.remember("x", "hello")
    Memory.forget("x")
    val = Memory.recall("x")
    if val == none {
      Io.show("forgotten")
    }
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("forgotten"),
        "forget should remove key:\n{stdout}"
    );
}

#[test]
fn memory_none_raises_capability_error() {
    let src = r#"
agent A {
  @memory none
  @on_start {
    Memory.remember("x", "y")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected CapabilityError for @memory none");
    assert!(
        stderr.contains("CapabilityError"),
        "expected CapabilityError in stderr:\n{stderr}"
    );
}

#[test]
fn memory_default_mode_is_session() {
    let src = r#"
agent A {
  @on_start {
    Memory.remember("k", "v")
    val = Memory.recall("k")
    Io.show("val: {val}")
    stop(self)
  }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("val: v"),
        "default mode should act as session:\n{stdout}"
    );
}

#[test]
fn memory_persistent_survives_process_boundary() {
    use std::io::Write as _;
    let home = tempfile::tempdir().expect("tempdir");
    // Both runs must use the same file path so they share the same program stem.
    let prog = home.path().join("memory_test.keel");

    let write_src = r#"
agent A {
  @memory persistent
  @on_start {
    Memory.remember("greeting", "hello-persistent")
    stop(self)
  }
}
run(A)
"#;
    std::fs::File::create(&prog)
        .unwrap()
        .write_all(write_src.as_bytes())
        .unwrap();
    let bin = keel_binary();
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("run keel");
    assert!(
        out.status.success(),
        "write run failed\nstderr: {}",
        String::from_utf8_lossy(&out.stderr)
    );

    let read_src = r#"
agent A {
  @memory persistent
  @on_start {
    val = Memory.recall("greeting")
    Io.show("recalled: {val}")
    stop(self)
  }
}
run(A)
"#;
    std::fs::File::create(&prog)
        .unwrap()
        .write_all(read_src.as_bytes())
        .unwrap();
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("run keel");
    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "read run failed\nstderr: {stderr}");
    assert!(
        stdout.contains("recalled: hello-persistent"),
        "persistent value should survive process boundary:\n{stdout}"
    );
}

#[test]
fn memory_unknown_mode_raises_error() {
    let src = r#"
agent A {
  @memory unknown_mode
  @on_start {
    Memory.remember("x", "y")
    stop(self)
  }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected error for unrecognized @memory value");
    assert!(
        stderr.contains("unrecognized") || stderr.contains("unknown_mode"),
        "expected diagnostic naming the bad value:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// Memory v0.1.11 — identity hash, flock, path safety
// ---------------------------------------------------------------------------

#[test]
fn memory_isolation_same_basename_different_paths() {
    // Two counter.keel files in different directories must have separate memory.
    let home = tempfile::tempdir().expect("tempdir");
    let dir_a = tempfile::tempdir().expect("tempdir_a");
    let dir_b = tempfile::tempdir().expect("tempdir_b");
    let prog_a = dir_a.path().join("counter.keel");
    let prog_b = dir_b.path().join("counter.keel");
    let src_a = r#"
agent Ctr {
  @memory persistent
  @on_start {
    Memory.remember("v", "from_a")
    stop(self)
  }
}
run(Ctr)
"#;
    let src_b = r#"
agent Ctr {
  @memory persistent
  @on_start {
    Memory.remember("v", "from_b")
    stop(self)
  }
}
run(Ctr)
"#;
    std::fs::write(&prog_a, src_a).unwrap();
    std::fs::write(&prog_b, src_b).unwrap();
    let bin = keel_binary();
    let run_prog = |prog: &std::path::Path| -> (bool, String, String) {
        let out = Command::new(&bin)
            .env("KEEL_ONESHOT", "1")
            .env("KEEL_LLM", "mock")
            .env("HOME", home.path())
            .env("USERPROFILE", home.path())
            .arg("run")
            .arg(prog)
            .output()
            .expect("run");
        (
            out.status.success(),
            String::from_utf8_lossy(&out.stdout).into_owned(),
            String::from_utf8_lossy(&out.stderr).into_owned(),
        )
    };
    let (ok_a, _, se_a) = run_prog(&prog_a);
    let (ok_b, _, se_b) = run_prog(&prog_b);
    assert!(ok_a, "prog_a write failed: {se_a}");
    assert!(ok_b, "prog_b write failed: {se_b}");

    // Now recall from each — they must return their own values.
    let recall_src = r#"
agent Ctr {
  @memory persistent
  @on_start {
    val = Memory.recall("v")
    Io.show("val: {val}")
    stop(self)
  }
}
run(Ctr)
"#;
    std::fs::write(&prog_a, recall_src).unwrap();
    std::fs::write(&prog_b, recall_src).unwrap();
    let (ok_ra, out_a, se_ra) = run_prog(&prog_a);
    let (ok_rb, out_b, se_rb) = run_prog(&prog_b);
    assert!(ok_ra, "prog_a recall failed: {se_ra}");
    assert!(ok_rb, "prog_b recall failed: {se_rb}");
    assert!(
        out_a.contains("val: from_a"),
        "prog_a should recall from_a:\n{out_a}"
    );
    assert!(
        out_b.contains("val: from_b"),
        "prog_b should recall from_b:\n{out_b}"
    );
}

#[test]
fn memory_repl_namespace_distinct_from_files() {
    // A file named repl.keel must use "repl_<hash12>", not "__repl__".
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let prog = src_dir.path().join("repl.keel");
    std::fs::write(
        &prog,
        r#"
agent Tester {
  @memory persistent
  @on_start {
    Memory.remember("ns", "file")
    stop(self)
  }
}
run(Tester)
"#,
    )
    .unwrap();
    let bin = keel_binary();
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let memory_root = home.path().join(".keel").join("memory");
    let entries: Vec<_> = std::fs::read_dir(&memory_root)
        .expect("read memory dir")
        .filter_map(|e| e.ok())
        .collect();
    assert_eq!(entries.len(), 1, "expected exactly one memory dir");
    let dir_name = entries[0].file_name();
    let name = dir_name.to_string_lossy();
    assert!(
        name.starts_with("repl_") && name != "__repl__",
        "file-based repl.keel must use 'repl_<hash>', not '__repl__': got {name}"
    );
}

#[test]
fn memory_symlink_resolves_to_same_storage() {
    // Running via a symlink and via the target must share the same memory.
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let orig = src_dir.path().join("original.keel");
    let link = src_dir.path().join("symlink.keel");
    std::fs::write(
        &orig,
        r#"
agent Sym {
  @memory persistent
  @on_start {
    Memory.remember("key", "stored_via_symlink")
    stop(self)
  }
}
run(Sym)
"#,
    )
    .unwrap();
    std::os::unix::fs::symlink(&orig, &link).expect("create symlink");
    let bin = keel_binary();
    // Write via symlink.
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&link)
        .output()
        .expect("run via symlink");
    assert!(
        out.status.success(),
        "symlink run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Recall via the original file.
    std::fs::write(
        &orig,
        r#"
agent Sym {
  @memory persistent
  @on_start {
    val = Memory.recall("key")
    Io.show("got: {val}")
    stop(self)
  }
}
run(Sym)
"#,
    )
    .unwrap();
    let out2 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&orig)
        .output()
        .expect("run via original");
    let stdout = String::from_utf8_lossy(&out2.stdout).into_owned();
    assert!(
        out2.status.success(),
        "original run failed: {}",
        String::from_utf8_lossy(&out2.stderr)
    );
    assert!(
        stdout.contains("got: stored_via_symlink"),
        "original should see memory written via symlink:\n{stdout}"
    );
}

#[test]
fn memory_cross_process_write_race() {
    // Two concurrent keel processes writing to the same persistent store must
    // not corrupt the JSON file. flock guarantees each individual write is
    // atomic; both processes must complete without errors.
    // Note: recall+remember is NOT a single locked operation, so the logical
    // counter value is not guaranteed to be 10 — only file integrity is.
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let prog = src_dir.path().join("race_counter.keel");
    std::fs::write(
        &prog,
        r#"
agent Counter {
  @memory persistent
  @on_start {
    for item in [1, 2, 3, 4, 5] {
      Memory.remember("last", item)
    }
    stop(self)
  }
}
run(Counter)
"#,
    )
    .unwrap();
    let bin = keel_binary();
    let mut p1 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .spawn()
        .expect("spawn p1");
    let mut p2 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .spawn()
        .expect("spawn p2");
    let s1 = p1.wait().expect("wait p1");
    let s2 = p2.wait().expect("wait p2");
    assert!(s1.success(), "process 1 failed");
    assert!(s2.success(), "process 2 failed");
    // Verify the JSON file is valid (flock prevented any torn write).
    let memory_root = home.path().join(".keel").join("memory");
    let mut found = false;
    for entry in std::fs::read_dir(&memory_root).expect("read memory dir") {
        let entry = entry.unwrap();
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("race_counter_")
        {
            let json_path = entry.path().join("Counter.json");
            if json_path.exists() {
                let content = std::fs::read_to_string(&json_path).unwrap();
                let json: serde_json::Value = serde_json::from_str(&content)
                    .expect("JSON must be valid after concurrent writes");
                assert!(json.is_object(), "memory must be a JSON object");
                let last = json["last"].as_i64().unwrap_or(-1);
                assert!(
                    (1..=5).contains(&last),
                    "last must be 1-5 (written by one of the processes), got: {last}"
                );
                found = true;
            }
        }
    }
    assert!(found, "Counter.json not found in {}", memory_root.display());
}

#[test]
fn memory_concurrent_reads_dont_block() {
    // Two processes holding shared locks on the same memory file must both succeed.
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let prog = src_dir.path().join("read_test.keel");
    std::fs::write(
        &prog,
        r#"
agent Reader {
  @memory persistent
  @on_start {
    Memory.remember("msg", "shared")
    stop(self)
  }
}
run(Reader)
"#,
    )
    .unwrap();
    let bin = keel_binary();
    // Setup: write the initial value.
    Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("setup write");
    std::fs::write(
        &prog,
        r#"
agent Reader {
  @memory persistent
  @on_start {
    val = Memory.recall("msg")
    Io.show("val: {val}")
    stop(self)
  }
}
run(Reader)
"#,
    )
    .unwrap();
    let mut p1 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .spawn()
        .expect("spawn p1");
    let mut p2 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .spawn()
        .expect("spawn p2");
    let s1 = p1.wait().expect("wait p1");
    let s2 = p2.wait().expect("wait p2");
    assert!(s1.success(), "concurrent reader p1 failed");
    assert!(s2.success(), "concurrent reader p2 failed");
}

#[test]
fn memory_lockfile_exists_alongside_data() {
    // After a persistent remember, both <agent>.json and <agent>.lock must exist.
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let prog = src_dir.path().join("lock_test.keel");
    std::fs::write(
        &prog,
        r#"
agent LockTester {
  @memory persistent
  @on_start {
    Memory.remember("k", "v")
    stop(self)
  }
}
run(LockTester)
"#,
    )
    .unwrap();
    let bin = keel_binary();
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("run");
    assert!(
        out.status.success(),
        "failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let memory_root = home.path().join(".keel").join("memory");
    let mut json_found = false;
    let mut lock_found = false;
    for entry in std::fs::read_dir(&memory_root).expect("read memory dir") {
        let entry = entry.unwrap();
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("lock_test_")
        {
            let dir = entry.path();
            json_found = dir.join("LockTester.json").exists();
            lock_found = dir.join("LockTester.lock").exists();
        }
    }
    assert!(json_found, "LockTester.json should exist after remember");
    assert!(
        lock_found,
        "LockTester.lock should exist alongside data file"
    );
}

#[test]
fn memory_corrupt_file_renamed_to_bak() {
    // A corrupt JSON file must be renamed to .bak and an error returned.
    let home = tempfile::tempdir().expect("tempdir");
    let src_dir = tempfile::tempdir().expect("tempdir");
    let prog = src_dir.path().join("corrupt_test.keel");
    std::fs::write(
        &prog,
        r#"
agent CT {
  @memory persistent
  @on_start {
    Memory.remember("k", "v")
    stop(self)
  }
}
run(CT)
"#,
    )
    .unwrap();
    let bin = keel_binary();
    // First run: create the memory file.
    let out = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("first run");
    assert!(
        out.status.success(),
        "first run failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    // Locate and corrupt the JSON file.
    let memory_root = home.path().join(".keel").join("memory");
    let mut json_path: Option<std::path::PathBuf> = None;
    for entry in std::fs::read_dir(&memory_root).unwrap() {
        let entry = entry.unwrap();
        if entry
            .file_name()
            .to_string_lossy()
            .starts_with("corrupt_test_")
        {
            let p = entry.path().join("CT.json");
            if p.exists() {
                json_path = Some(p);
            }
        }
    }
    let json_path = json_path.expect("CT.json not found after first run");
    std::fs::write(&json_path, b"not valid json {{{ broken").unwrap();
    // Second run: corrupt file must be renamed to .bak and the run must fail.
    let out2 = Command::new(&bin)
        .env("KEEL_ONESHOT", "1")
        .env("KEEL_LLM", "mock")
        .env("HOME", home.path())
        .env("USERPROFILE", home.path())
        .arg("run")
        .arg(&prog)
        .output()
        .expect("second run");
    assert!(
        !out2.status.success(),
        "second run should fail on corrupt file"
    );
    let bak = json_path.with_extension("json.bak");
    assert!(bak.exists(), ".bak should exist after corrupt-file rename");
    assert!(
        !json_path.exists(),
        ".json should be gone after rename to .bak"
    );
}
