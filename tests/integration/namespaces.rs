use crate::common::*;

#[test]
fn search_stub_raises_v2_error() {
    let src = r#"
agent A {
    @on_start {
        Search.web("query")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for Search stub");
    assert!(
        stderr.contains("v0.2"),
        "expected 'v0.2' in error message:\n{stderr}"
    );
}

#[test]
fn db_stub_raises_v2_error() {
    let src = r#"
agent A {
    @on_start {
        Db.query("SELECT 1")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "expected non-zero exit for Db stub");
    assert!(
        stderr.contains("v0.2"),
        "expected 'v0.2' in error message:\n{stderr}"
    );
}

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
fn json_namespace_stringifies_and_parses_maps() {
    let src = r#"
agent A {
    @on_start {
        data = {name: "Ada", age: 42, tags: ["ai", "lang"], active: true}
        text = Json.stringify(data)
        parsed = Json.parse(text)
        Io.show(text)
        Io.show("{parsed.name}:{parsed.age}:{parsed.tags.join("|")}:{parsed.active}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Json namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("\"name\":\"Ada\""), "{stdout}");
    assert!(stdout.contains("Ada:42:ai|lang:true"), "{stdout}");
}

#[test]
fn json_namespace_invalid_parse_reports_json_error() {
    let src = r#"
agent A {
    @on_start {
        Json.parse("not json")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "invalid Json.parse should fail");
    assert!(
        stderr.contains("JsonError: Json.parse invalid JSON"),
        "expected JsonError diagnostic:\n{stderr}"
    );
}

#[test]
fn env_require_returns_set_value_and_errors_when_missing() {
    let ok_src = r#"
agent A {
    @on_start {
        val = Env.require("KEEL_TEST_REQUIRED")
        Io.show("required={val}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline_with_env(ok_src, &[("KEEL_TEST_REQUIRED", "present")]);
    assert!(
        ok,
        "Env.require success case failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("required=present"), "{stdout}");

    let missing_src = r#"
agent A {
    @on_start {
        Env.require("__KEEL_TEST_REQUIRED_MISSING__")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(missing_src, false);
    assert!(!ok, "missing Env.require should fail");
    assert!(
        stderr.contains("Env.require: `__KEEL_TEST_REQUIRED_MISSING__` is not set"),
        "expected Env.require diagnostic:\n{stderr}"
    );
}

#[test]
fn log_namespace_level_controls_output() {
    let src = r#"
agent A {
    @on_start {
        Log.info("visible info")
        Log.set_level("error")
        Io.show("level={Log.level()}")
        Log.debug("hidden debug")
        Log.warn("hidden warn")
        Log.error("visible error")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Log namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("level=error"), "{stdout}");
    assert!(
        stderr.contains("[info] visible info") && stderr.contains("[error] visible error"),
        "expected visible log lines:\n{stderr}"
    );
    assert!(
        !stderr.contains("hidden debug") && !stderr.contains("hidden warn"),
        "log threshold should hide lower-priority lines:\n{stderr}"
    );
}

#[test]
fn log_namespace_rejects_invalid_level() {
    let src = r#"
agent A {
    @on_start {
        Log.set_level("verbose")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "invalid Log.set_level should fail");
    assert!(
        stderr.contains("Log.set_level: `verbose` is not a valid level"),
        "expected Log.set_level diagnostic:\n{stderr}"
    );
}

// ---------------------------------------------------------------------------
// v0.1.8 — Reactive Agents & Text Processing
// ---------------------------------------------------------------------------

#[test]
fn cache_set_get() {
    let src = r#"
agent CacheTest {
    @on_start {
        Cache.set("key", "value")
        v = Cache.get("key")
        Io.show("got={v}")
    }
}
run(CacheTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("got=value"),
        "Cache.set/get failed:\n{stdout}"
    );
}

#[test]
fn cache_delete() {
    let src = r#"
agent CacheTest {
    @on_start {
        Cache.set("temp", "x")
        Cache.delete("temp")
        v = Cache.get("temp")
        if v == none {
            Io.show("deleted")
        }
    }
}
run(CacheTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("deleted"), "Cache.delete failed:\n{stdout}");
}

#[test]
fn cache_clear() {
    let src = r#"
agent CacheTest {
    @on_start {
        Cache.set("a", "1")
        Cache.set("b", "2")
        Cache.clear()
        v = Cache.get("a")
        if v == none {
            Io.show("cleared")
        }
    }
}
run(CacheTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("cleared"), "Cache.clear failed:\n{stdout}");
}

#[test]
fn str_matches_true() {
    let src = r#"
agent StrTest {
    @on_start {
        result = "hello world".matches("\\w+")
        if result {
            Io.show("matched")
        }
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("matched"),
        "matches true case failed:\n{stdout}"
    );
}

#[test]
fn str_matches_false() {
    let src = r#"
agent StrTest {
    @on_start {
        result = "hello world".matches("^\\d+$")
        if result {
            Io.show("matched")
        } else {
            Io.show("no-match")
        }
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("no-match"),
        "matches false case failed:\n{stdout}"
    );
}

#[test]
fn str_extract() {
    let src = r#"
agent StrTest {
    @on_start {
        v = "Total: $99.99".extract("\\$(\\S+)")
        Io.show("amount={v}")
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("amount=99.99"), "extract failed:\n{stdout}");
}

#[test]
fn str_truncate() {
    let src = r#"
agent StrTest {
    @on_start {
        v = "hello world".truncate(5)
        Io.show("short={v}")
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("short=hello…"),
        "truncate failed:\n{stdout}"
    );
}

#[test]
fn str_pad() {
    let src = r#"
agent StrTest {
    @on_start {
        v = "42".pad(5)
        Io.show("padded={v}")
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("padded=   42"), "pad failed:\n{stdout}");
}

#[test]
fn str_find_all() {
    let src = r#"
agent StrTest {
    @on_start {
        matches = "one 1 two 2 three 3".find_all("\\d+")
        Io.show("count={matches.count}")
        Io.show("first={matches.first}")
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("count=3"),
        "find_all count failed:\n{stdout}"
    );
    assert!(
        stdout.contains("first=1"),
        "find_all first failed:\n{stdout}"
    );
}

#[test]
fn str_sub() {
    let src = r#"
agent StrTest {
    @on_start {
        v = "hello world hello".sub("hello", "hi")
        Io.show("result={v}")
    }
}
run(StrTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("result=hi world hi"),
        "sub failed:\n{stdout}"
    );
}
