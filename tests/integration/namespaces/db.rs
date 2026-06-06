use crate::common::*;

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
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stderr.contains("hello"),
        "expected row data in output:\n{stderr}"
    );
}
