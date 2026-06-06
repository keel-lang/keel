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
