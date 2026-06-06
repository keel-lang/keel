use crate::common::*;

// ---------------------------------------------------------------------------
// Cache namespace
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
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("cleared"), "Cache.clear failed:\n{stdout}");
}
