use crate::common::*;

// ---------------------------------------------------------------------------
// Cache namespace
// ---------------------------------------------------------------------------

#[test]
fn cache_set_get() {
    let src = r#"
use std/cache
use std/io
agent CacheTest {
    @tools [io]
    @on_start {
        cache.set("key", "value")
        v = cache.get("key")
        io.show("got={v}")
    }
}
run(CacheTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("got=value"),
        "cache.set/get failed:\n{stdout}"
    );
}

#[test]
fn cache_delete() {
    let src = r#"
use std/cache
use std/io
agent CacheTest {
    @tools [io]
    @on_start {
        cache.set("temp", "x")
        cache.delete("temp")
        v = cache.get("temp")
        if v == none {
            io.show("deleted")
        }
    }
}
run(CacheTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("deleted"), "cache.delete failed:\n{stdout}");
}

#[test]
fn cache_clear() {
    let src = r#"
use std/cache
use std/io
agent CacheTest {
    @tools [io]
    @on_start {
        cache.set("a", "1")
        cache.set("b", "2")
        cache.clear()
        v = cache.get("a")
        if v == none {
            io.show("cleared")
        }
    }
}
run(CacheTest)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(stdout.contains("cleared"), "cache.clear failed:\n{stdout}");
}
