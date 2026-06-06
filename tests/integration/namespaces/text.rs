use crate::common::*;

// ---------------------------------------------------------------------------
// Str namespace — regex and string utilities
// ---------------------------------------------------------------------------

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
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
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
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("result=hi world hi"),
        "sub failed:\n{stdout}"
    );
}

#[test]
fn string_regex_methods_reject_non_string_patterns() {
    let src = r#"
agent A {
    @on_start {
        try {
            "hello".matches(42)
        } catch e: Error {
            Io.show("matches={e.message}")
        }
        try {
            "hello".find_all(42)
        } catch e: Error {
            Io.show("find_all={e.message}")
        }
        try {
            "hello".sub(42, "x")
        } catch e: Error {
            Io.show("sub={e.message}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("matches=str.matches: argument at position 0 must be str, got int"),
        "expected strict matches error: {stdout}"
    );
    assert!(
        stdout.contains("find_all=str.find_all: argument at position 0 must be str, got int"),
        "expected strict find_all error: {stdout}"
    );
    assert!(
        stdout.contains("sub=str.sub: argument at position 0 must be str, got int"),
        "expected strict sub error: {stdout}"
    );
}

#[test]
fn string_contains_rejects_missing_argument() {
    let src = r#"
agent A {
    @on_start {
        try {
            "hello".contains()
        } catch e: Error {
            Io.show("caught={e.message}")
        }
        stop(self)
    }
}
run(A)
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "program failed\nstdout: {stdout}\nstderr: {stderr}");
    assert!(
        stdout.contains("caught=str.contains: missing argument at position 0"),
        "expected missing-argument error: {stdout}"
    );
}
