//! M0's lowering is a *rejecting* subset, not a best-effort one (AGENTS.md:
//! "no silent fallbacks"). These tests pin that every construct outside the
//! scalar subset fails loudly with a `LowerError`, rather than silently
//! dropping/approximating it.

fn lower_err(source: &str) -> String {
    let (program, _named) = keel_syntax::parse_source(source, "t.keel").expect("must parse");
    let err = keel_kir::lower(&program, "t.keel").expect_err("must be rejected by M0 lowering");
    err.to_string()
}

#[test]
fn for_loop_is_rejected() {
    let msg = lower_err(
        r#"
task sum(xs: list[int]) -> int {
  total = 0
  for x in xs {
    total += x
  }
  return total
}
"#,
    );
    assert!(msg.contains("list type"), "unexpected message: {msg}");
}

#[test]
fn string_interpolation_is_rejected() {
    let msg = lower_err(
        r#"
task greet(name: str) -> str {
  return "hi {name}"
}
"#,
    );
    assert!(
        msg.contains("string interpolation"),
        "unexpected message: {msg}"
    );
}

#[test]
fn struct_literal_is_rejected() {
    let msg = lower_err(
        r#"
task make() -> int {
  p = {x: 1, y: 2}
  return 1
}
"#,
    );
    assert!(msg.contains("struct literal"), "unexpected message: {msg}");
}

#[test]
fn generic_task_is_rejected() {
    let msg = lower_err(
        r#"
task first[T](xs: list[T]) -> int {
  return 0
}
"#,
    );
    assert!(msg.contains("generic task"), "unexpected message: {msg}");
}

#[test]
fn agent_declaration_is_rejected() {
    let msg = lower_err(
        r#"
agent A {
  @role "x"
}
"#,
    );
    assert!(
        msg.contains("agent declaration"),
        "unexpected message: {msg}"
    );
}

#[test]
fn type_mismatch_in_let_annotation_is_rejected() {
    let msg = lower_err(
        r#"
task f() -> int {
  x: str = 1
  return 0
}
"#,
    );
    assert!(msg.contains("annotated"), "unexpected message: {msg}");
}

#[test]
fn wrong_arg_count_is_rejected() {
    let msg = lower_err(
        r#"
task add(a: int, b: int) -> int {
  return a + b
}

x = add(1)
"#,
    );
    assert!(msg.contains("argument"), "unexpected message: {msg}");
}
