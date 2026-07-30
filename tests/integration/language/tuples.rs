use crate::common::*;

// ---------------------------------------------------------------------------
// Tuple positional access (issue #157) — `SPEC.md` §2.8
// ---------------------------------------------------------------------------

#[test]
fn spec_2_8_example_runs() {
    // The exact example from SPEC.md §2.8. Before #157 this failed to parse:
    // `field_name()` never accepted `Token::Integer` in postfix position.
    let src = r#"
use std/io
pair: (str, int) = ("hello", 42)
(name, count) = pair
x = pair.0
io.show("{x} {pair.1} {name} {count}")
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "SPEC §2.8's own example must run: {stderr}");
    assert!(
        stdout.contains("hello 42 hello 42"),
        "expected positional and destructured reads to agree: {stdout}"
    );
}

#[test]
fn positional_access_resolves_element_type_not_unknown() {
    // Regression guard for the checker arm: if `pair.0` fell through to
    // `Ty::Unknown` the annotation mismatch below would go unreported.
    let src = r#"
pair: (str, int) = ("hello", 42)
bad: int = pair.0
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "pair.0 is str, not int — must be rejected");
    assert!(
        stderr.contains("expected int") && stderr.contains("str"),
        "expected an int/str mismatch proving .0 resolved to str: {stderr}"
    );
}

#[test]
fn out_of_bounds_index_is_a_static_error() {
    let src = r#"
pair: (str, int) = ("hello", 42)
y = pair.5
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "index past the tuple's arity must fail the checker");
    assert!(
        stderr.contains("out of bounds") && stderr.contains("arity 2"),
        "expected a bounds error naming the arity: {stderr}"
    );
}

#[test]
fn named_field_on_a_tuple_is_rejected() {
    let src = r#"
pair: (str, int) = ("hello", 42)
y = pair.nope
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "tuples have no named fields");
    assert!(
        stderr.contains("has no field") && stderr.contains("position"),
        "error should point the user at positional access: {stderr}"
    );
}

#[test]
fn positional_access_on_a_list_is_rejected() {
    // The load-bearing test for the shared repr: tuples and lists are both
    // `Value::List` at runtime (v0.1), so `xs.0` would silently *succeed* in
    // the interpreter. Only the checker can reject it.
    let src = r#"
xs: list[int] = [1, 2, 3]
y = xs.0
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "`.0` on a list must not typecheck — lists use `[0]`");
    assert!(
        stderr.contains("only valid on tuples"),
        "expected a tuples-only error suggesting subscript syntax: {stderr}"
    );
}

#[test]
fn positional_access_on_a_map_is_rejected() {
    let src = r#"
m: map[str, int] = {"a": 1}
y = m.0
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "`.0` on a map must not typecheck");
    assert!(
        stderr.contains("only valid on tuples"),
        "expected a tuples-only error: {stderr}"
    );
}

#[test]
fn null_safe_positional_access_on_a_nullable_tuple() {
    let src = r#"
use std/io
maybe: (str, int)? = ("hi", 7)
io.show("{maybe?.0}")
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "`?.0` on a nullable tuple must work: {stderr}");
    assert!(stdout.contains("hi"), "expected `hi`: {stdout}");
}

#[test]
fn nested_tuple_access_requires_parentheses() {
    // `t.0.1` cannot parse: the lexer's Float regex (`[0-9]+\.[0-9]+`) claims
    // `0.1` as one token, and logos picks the longest match. Parenthesizing
    // is the documented workaround; lifting the restriction is tracked
    // separately. This test pins both halves so the follow-up has a target.
    let parenthesized = r#"
use std/io
t: ((int, int), int) = ((1, 2), 3)
io.show("{(t.0).1}")
"#;
    let (ok, stdout, stderr) = run_inline(parenthesized, false);
    assert!(ok, "`(t.0).1` must work: {stderr}");
    assert!(stdout.contains('2'), "expected `2`: {stdout}");

    let unparenthesized = r#"
t: ((int, int), int) = ((1, 2), 3)
b = t.0.1
"#;
    let (ok, _stdout, stderr) = run_inline(unparenthesized, false);
    assert!(!ok, "`t.0.1` is a known parse limitation, not valid today");
    assert!(
        stderr.contains("0.1"),
        "error should show the Float token that swallowed the indices: {stderr}"
    );
}

#[test]
fn numeric_field_names_stay_invalid_in_declarations() {
    // `postfix_field_name()` is deliberately separate from `field_name()`.
    // If the numeric case leaked into the shared parser, this would parse.
    let src = r#"
type X { 0: int }
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "a numeric struct field must stay a syntax error");
    assert!(
        stderr.contains("Parse error"),
        "expected a parse error, not a type error: {stderr}"
    );
}
