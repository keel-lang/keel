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
fn nested_tuple_access_parses_without_parentheses() {
    // Issue #185: the lexer's Float regex (`[0-9]+\.[0-9]+`) claims `0.1` as
    // one token, so the grammar never sees `Integer Dot Integer`. Postfix
    // position splits that token back into two indices; both forms must give
    // the same answer.
    let src = r#"
use std/io
t: ((int, int), int) = ((1, 2), 3)
io.show("{t.0.1} {(t.0).1}")
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "`t.0.1` must parse: {stderr}");
    assert!(
        stdout.contains("2 2"),
        "bare and parenthesized forms must agree: {stdout}"
    );
}

#[test]
fn three_deep_positional_access() {
    // `deep.0.0.1` lexes as `Ident Dot Float("0.0") Dot Integer("1")` — the
    // split arm and the flat arm have to compose.
    let src = r#"
use std/io
deep: (((int, str), int), int) = (((7, "x"), 8), 9)
io.show("{deep.0.0.0} {deep.0.0.1} {deep.0.1} {deep.1}")
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "three-deep positional access must parse: {stderr}");
    assert!(stdout.contains("7 x 8 9"), "expected `7 x 8 9`: {stdout}");
}

#[test]
fn nested_index_bounds_errors_name_the_right_tuple() {
    // The two indices must be checked against their own tuples, which only
    // holds if the split produced nested `FieldAccess` nodes in the right
    // order (outer index first).
    let src = r#"
t: ((int, int), int) = ((1, 2), 3)
b = t.0.5
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "`.5` on the inner 2-tuple is out of bounds");
    assert!(
        stderr.contains("index 5") && stderr.contains("(int, int)"),
        "inner index should be checked against the inner tuple: {stderr}"
    );

    let src = r#"
t: ((int, int), int) = ((1, 2), 3)
b = t.2.1
"#;
    let (ok, _stdout, stderr) = run_inline(src, false);
    assert!(!ok, "`.2` on the outer 2-tuple is out of bounds");
    assert!(
        stderr.contains("index 2") && stderr.contains("((int, int), int)"),
        "outer index should be checked against the outer tuple: {stderr}"
    );
}

#[test]
fn float_literals_are_untouched_by_the_split() {
    // The split arm only fires after `.` / `?.`. A float in value position —
    // and the `5.minutes` duration sugar that shares the same lexer mechanic —
    // must keep working.
    let src = r#"
use std/io
f = 0.1
io.show("{f} {5.minutes}")
"#;
    let (ok, stdout, stderr) = run_inline(src, false);
    assert!(ok, "float literals must still parse: {stderr}");
    assert!(stdout.contains("0.1"), "expected the float back: {stdout}");
}

#[test]
fn nested_positional_access_after_null_safe_index() {
    // `?.` opens the access but covers the first index only — exactly like
    // `o?.a.b` on a nullable struct, where the second `.` is a plain access.
    // Both spellings must agree; `?.0?.1` is the short-circuiting form.
    let present = r#"
use std/io
maybe: ((int, int), int)? = ((4, 5), 6)
io.show("{maybe?.0.1} {(maybe?.0).1} {maybe?.0?.1}")
"#;
    let (ok, stdout, stderr) = run_inline(present, false);
    assert!(ok, "`maybe?.0.1` must parse: {stderr}");
    assert!(stdout.contains("5 5 5"), "all three must agree: {stdout}");

    // On `none`, the trailing plain `.1` raises — the pre-existing behavior of
    // any `?.`-opened chain, not something the split introduces.
    let absent = r#"
use std/io
maybe: ((int, int), int)? = none
io.show("{maybe?.0.1}")
"#;
    let (ok, _stdout, stderr) = run_inline(absent, false);
    assert!(!ok, "a plain `.1` past a `none` must not silently succeed");
    assert!(
        stderr.contains("Cannot access `.1` on none"),
        "expected the same error `(maybe?.0).1` raises: {stderr}"
    );

    let short_circuit = r#"
use std/io
maybe: ((int, int), int)? = none
io.show("{maybe?.0?.1}")
"#;
    let (ok, stdout, stderr) = run_inline(short_circuit, false);
    assert!(ok, "`?.0?.1` must short-circuit: {stderr}");
    assert!(stdout.contains("none"), "expected `none`: {stdout}");
}

#[test]
fn nested_positional_access_formatter_roundtrip() {
    // The formatter drops the redundant parentheses in `(t.0).1`, so before
    // #185 it turned working source into source that no longer parsed.
    let src = r#"
use std/io
t: ((int, int), int) = ((1, 2), 3)
io.show("{(t.0).1}")
"#;
    let once = keel_lang::session::fmt_source(src, "t.keel").expect("fmt once");
    let twice = keel_lang::session::fmt_source(&once, "t.keel").expect("fmt twice");
    assert_eq!(
        once, twice,
        "formatter not idempotent:\n--- once ---\n{once}\n--- twice ---\n{twice}"
    );
    assert!(
        once.contains("t.0.1"),
        "formatter should print the unparenthesized form: {once}"
    );
    let (ok, stdout, stderr) = run_inline(&once, false);
    assert!(ok, "formatted output must still run: {stderr}");
    assert!(stdout.contains('2'), "expected `2`: {stdout}");
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
