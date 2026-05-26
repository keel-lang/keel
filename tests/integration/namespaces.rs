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
fn db_connect_query_exec_roundtrip() {
    let src = r#"
agent A {
    @tools [Db, Log]
    @on_start {
        db = Db.connect("sqlite://:memory:")
        db.exec("CREATE TABLE kv (key TEXT, val TEXT)")
        db.exec("INSERT INTO kv VALUES (?, ?)", ["hello", "world"])
        rows = db.query("SELECT key, val FROM kv")
        Log.info("{rows}")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(ok, "expected success:\n{stderr}");
    assert!(stderr.contains("hello"), "expected row data in output:\n{stderr}");
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
    assert!(ok, "File.mkdir failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "File.remove failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "File.copy failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "File.glob failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "File.move failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "File.mktemp failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("content=temp data"), "{stdout}");
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
fn random_namespace_generates_values_in_expected_shapes() {
    let src = r#"
agent A {
    @on_start {
        roll = Random.int(min: 1, max: 6)
        sample = Random.float()
        enabled = Random.bool()
        if roll >= 1 and roll <= 6 and sample >= 0.0 and sample < 1.0 {
            Io.show("random-ok {enabled}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Random namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("random-ok"), "{stdout}");
}

#[test]
fn random_namespace_rejects_inverted_int_bounds() {
    let src = r#"
agent A {
    @on_start {
        Random.int(min: 5, max: 1)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "inverted Random.int bounds should fail");
    assert!(
        stderr.contains("Random.int: `min:` must be <= `max:`"),
        "expected Random.int diagnostic:\n{stderr}"
    );
}

#[test]
fn uuid_namespace_generates_parses_and_formats_values() {
    let src = r#"
agent A {
    @on_start {
        id: Uuid = uuid()
        v4 = id.version()
        v7 = Uuid.v7().version()
        site = Uuid.v5(ns: Uuid.DNS, name: "www.example.com")
        parsed = Uuid.parse("f47ac10b-58cc-4372-a567-0e02b2c3d479")
        missing = Uuid.parse("bad")
        simple = id.format(as: "simple")
        urn = id.format(as: "urn")
        if v4 == 4 and v7 == 7 and site.to_str() == "2ed6657d-e927-568b-95e1-2665a8aea6a2" and parsed != none and missing == none {
            Io.show("uuid-ok {simple} {urn}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Uuid namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("uuid-ok"), "{stdout}");
    assert!(stdout.contains("urn:uuid:"), "{stdout}");
}

#[test]
fn crypto_namespace_hashes_signs_and_generates_random_values() {
    let src = r#"
agent A {
    @on_start {
        digest = Crypto.sha256("hello")
        wide = Crypto.sha384("hello")
        sig = Crypto.hmac_sha256("The quick brown fox jumps over the lazy dog", key: "key")
        token = Crypto.token(bytes: 16)
        bytes = Crypto.random_bytes(4)
        if digest == "2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824" and wide == "59e1748777448c69de6b800d7a33bbfb9ff1b463e44354c3553bcdb9c666fa90125a3c79f90397bdf5f6a13de828684f" and sig == "f7bc83f430538424b13298e6aa6fb143ef4d59a14946175997479dbc2d1a3cd8" and token.len() == 32 and bytes.len() == 4 {
            Io.show("crypto-ok")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "Crypto namespace program failed\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("crypto-ok"), "{stdout}");
}

#[test]
fn crypto_namespace_does_not_expose_generic_hash_selection() {
    let src = r#"
agent A {
    @on_start {
        Crypto.hash("hello", algo: "md5")
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "generic Crypto.hash should fail");
    assert!(
        stderr.contains("Namespace `Crypto` has no method `hash`"),
        "expected missing Crypto.hash diagnostic:\n{stderr}"
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

#[test]
fn numeric_abs_float() {
    let src = r#"
agent NumTest {
    @on_start {
        v = -3.75
        Io.show("abs={v.abs()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("abs=3.75"), "float abs failed:\n{stdout}");
}

#[test]
fn numeric_abs_int() {
    let src = r#"
agent NumTest {
    @on_start {
        v = -5
        Io.show("abs={v.abs()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("abs=5"), "int abs failed:\n{stdout}");
}

#[test]
fn numeric_floor() {
    let src = r#"
agent NumTest {
    @on_start {
        v = 3.7
        Io.show("floor={v.floor()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("floor=3"), "floor failed:\n{stdout}");
}

#[test]
fn numeric_ceil() {
    let src = r#"
agent NumTest {
    @on_start {
        v = 3.2
        Io.show("ceil={v.ceil()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("ceil=4"), "ceil failed:\n{stdout}");
}

#[test]
fn numeric_round() {
    let src = r#"
agent NumTest {
    @on_start {
        v = 3.5
        Io.show("round={v.round()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("round=4"), "round failed:\n{stdout}");
}

#[test]
fn numeric_chain() {
    let src = r#"
agent NumTest {
    @on_start {
        v = -3.75
        Io.show("chained={v.abs().ceil()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("chained=4"),
        "chained abs().ceil() failed:\n{stdout}"
    );
}

#[test]
fn numeric_int_floor_noop() {
    let src = r#"
agent NumTest {
    @on_start {
        v = 7
        Io.show("floor={v.floor()}")
        stop(self)
    }
}
run(NumTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("floor=7"),
        "int floor no-op failed:\n{stdout}"
    );
}

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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("ok=true"),
        "Time.epoch_ms() should exceed 1_000_000_000_000:\n{stdout}"
    );
}

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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
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
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("caught="),
        "expected error to be caught: {stdout}"
    );
}

// ── Shell namespace ────────────────────────────────────────────────────────

#[test]
fn shell_run_captures_stdout_and_exit_code() {
    let src = r#"
agent A {
    @tools [Shell, Io]
    @on_start {
        r = Shell.run("echo hello")
        Io.show("code={r.exit_code}")
        Io.show("out={r.stdout}")
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
    assert!(stdout.contains("code=0"), "expected exit_code=0:\n{stdout}");
    assert!(
        stdout.contains("out=hello"),
        "expected 'hello' in stdout:\n{stdout}"
    );
}

#[test]
fn shell_run_nonzero_exit_does_not_raise() {
    let src = r#"
agent A {
    @tools [Shell, Io]
    @on_start {
        r = Shell.run("exit 7")
        Io.show("code={r.exit_code}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(
        ok,
        "non-zero exit should not raise\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(stdout.contains("code=7"), "expected exit_code=7:\n{stdout}");
}

#[test]
fn shell_run_stdin_is_forwarded() {
    let src = r#"
agent A {
    @tools [Shell, Io]
    @on_start {
        r = Shell.run("cat", stdin: "from-stdin")
        Io.show("got={r.stdout}")
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
        stdout.contains("got=from-stdin"),
        "expected stdin forwarded:\n{stdout}"
    );
}

#[test]
fn shell_run_capability_error_when_tools_list_excludes_shell() {
    // When @tools restricts the agent to specific namespaces, Shell.run must
    // raise CapabilityError if Shell is not in the list.
    let src = r#"
agent A {
    @tools [Io]
    @on_start {
        Shell.run("echo hi")
        stop(self)
    }
}
run(A)
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(
        !ok,
        "expected CapabilityError when Shell excluded from @tools"
    );
    assert!(
        stderr.contains("CapabilityError"),
        "expected CapabilityError in stderr:\n{stderr}"
    );
}

#[test]
fn shell_does_not_inherit_custom_env_vars() {
    // Env.require reads the keel process env; Shell.run spawns with a clean
    // environment (only PATH/HOME/TMPDIR/USER/LANG are forwarded), so custom
    // vars injected into the keel process are NOT visible to the subprocess.
    let src = r#"
agent A {
    @tools [Shell, Env, Io]
    @on_start {
        via_env = Env.require("KEEL_TEST_SHELL_VAR")
        r       = Shell.run("printf '%s' \"$KEEL_TEST_SHELL_VAR\"")
        Io.show("env={via_env}")
        Io.show("shell_empty={r.stdout == ""}")
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) =
        run_inline_with_env(src, &[("KEEL_TEST_SHELL_VAR", "hello-from-env")]);
    assert!(
        ok,
        "program exited non-zero\nstdout: {stdout}\nstderr: {stderr}"
    );
    assert!(
        stdout.contains("env=hello-from-env"),
        "Env.require should still see the var:\n{stdout}"
    );
    assert!(
        stdout.contains("shell_empty=true"),
        "Shell subprocess should not inherit custom var:\n{stdout}"
    );
}

#[test]
fn shell_forwards_safe_env_vars() {
    // HOME is in the safe forwarded set (PATH/HOME/TMPDIR/USER/LANG), so the
    // subprocess should see the same value that Env.require returns.
    let src = r#"
agent A {
    @tools [Shell, Env, Io]
    @on_start {
        home = Env.require("HOME")
        r = Shell.run("printf '%s' \"$HOME\"")
        Io.show("match={home == r.stdout}")
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
        stdout.contains("match=true"),
        "expected HOME to match between Env and Shell:\n{stdout}"
    );
}
