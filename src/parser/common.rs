//! Shared low-level parser primitives: newlines, separators, identifiers,
//! field names, and literal helpers.  All grammar sub-modules import from
//! here rather than duplicating these building blocks.

use chumsky::prelude::*;

use crate::ast::MapLitKey;
use crate::lexer::Token;

pub(super) type P<T> = BoxedParser<'static, Token, T, Simple<Token>>;

// ---------------------------------------------------------------------------
// Whitespace / separators
// ---------------------------------------------------------------------------

pub(super) fn newlines() -> P<()> {
    just(Token::Newline).repeated().ignored().boxed()
}

pub(super) fn sep() -> P<()> {
    just(Token::Newline)
        .repeated()
        .at_least(1)
        .ignored()
        .boxed()
}

/// Separator for struct fields and items: comma, newline, or both.
pub(super) fn field_sep() -> P<()> {
    just(Token::Comma)
        .then_ignore(newlines())
        .ignored()
        .or(sep())
        .boxed()
}

// ---------------------------------------------------------------------------
// Identifiers and names
// ---------------------------------------------------------------------------

pub(super) fn ident() -> P<String> {
    select! { Token::Ident(s) => s }.boxed()
}

/// Identifier OR a small set of contextual keywords that users routinely
/// want as field / argument names (e.g. `{from: str}`, `{type: "x"}`,
/// `Email.fetch(from: box)`). These remain reserved in their normal
/// positions — only here do we allow them as names.
pub(super) fn field_name() -> P<String> {
    select! {
        Token::Ident(s) => s,
        Token::From => "from".to_string(),
        Token::As => "as".to_string(),
        Token::In => "in".to_string(),
        Token::Where => "where".to_string(),
        Token::Type => "type".to_string(),
        Token::On => "on".to_string(),
        Token::State => "state".to_string(),
        Token::For => "for".to_string(),
        Token::Return => "return".to_string(),
        Token::Set => "set".to_string(),
    }
    .boxed()
}

/// Map / struct-literal key: a field_name or a string literal (strings
/// are raw-decoded into the key).  Used for `StructSpreadUpdate` overrides,
/// which are always identifier-shaped.
pub(super) fn map_key() -> P<String> {
    field_name().or(plain_string()).boxed()
}

/// Full map-literal key parser — extends `map_key` with integer and boolean
/// literals so that `{1: "one"}` and `{true: "on"}` parse correctly.
pub(super) fn map_lit_key() -> P<MapLitKey> {
    let ident_key = field_name().map(MapLitKey::Ident);
    let str_key = plain_string().map(MapLitKey::Str);
    let int_key = select! { Token::Integer(s) => s }.try_map(|s, span| {
        s.parse::<i64>()
            .map(MapLitKey::Int)
            .map_err(|_| Simple::custom(span, format!("integer key `{s}` overflows i64")))
    });
    let bool_key = just(Token::True)
        .to(MapLitKey::Bool(true))
        .or(just(Token::False).to(MapLitKey::Bool(false)));
    ident_key.or(str_key).or(int_key).or(bool_key).boxed()
}

// ---------------------------------------------------------------------------
// Destructure patterns
// ---------------------------------------------------------------------------

/// Parses `{ field }` or `{ field: rename, field2, ... }`.
/// Returns `Vec<(source_field, local_name)>`.
/// Uses `field_name()` so keyword-named fields like `from` are accepted.
/// Used for struct destructure patterns in let bindings, for loops, and task params.
pub(super) fn struct_destruct_pat() -> P<Vec<(String, String)>> {
    // Rename target must be a plain ident (not a keyword) to become a valid variable name.
    let entry = field_name()
        .then(just(Token::Colon).ignore_then(ident()).or_not())
        .map(|(src, rename)| {
            let local = rename.unwrap_or_else(|| src.clone());
            (src, local)
        });

    just(Token::LBrace)
        .ignore_then(newlines())
        .ignore_then(entry.separated_by(field_sep()).allow_trailing())
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .boxed()
}

/// Parses `( a, b, c )` with at least 2 elements.
/// Returns `Vec<String>` of positional names.
pub(super) fn tuple_destruct_pat() -> P<Vec<String>> {
    just(Token::LParen)
        .ignore_then(newlines())
        .ignore_then(
            ident()
                .separated_by(just(Token::Comma).then_ignore(newlines()))
                .at_least(2)
                .allow_trailing(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RParen))
        .boxed()
}

// ---------------------------------------------------------------------------
// Literal helpers
// ---------------------------------------------------------------------------

pub(super) fn string_lit() -> P<String> {
    select! { Token::StringLit(s) => s }.boxed()
}

/// Decode `\n`, `\t`, `\\`, `\"`, `\{`, `\}` in a raw string literal (no
/// interpolation). Used for attribute values, criteria keys, etc.
pub(super) fn unescape_plain(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(ch) = chars.next() {
        if ch != '\\' {
            out.push(ch);
            continue;
        }
        match chars.peek() {
            Some('n') => {
                chars.next();
                out.push('\n');
            }
            Some('t') => {
                chars.next();
                out.push('\t');
            }
            Some('r') => {
                chars.next();
                out.push('\r');
            }
            Some('\\') => {
                chars.next();
                out.push('\\');
            }
            Some('"') => {
                chars.next();
                out.push('"');
            }
            Some('{') => {
                chars.next();
                out.push('{');
            }
            Some('}') => {
                chars.next();
                out.push('}');
            }
            Some(_) | None => out.push('\\'),
        }
    }
    out
}

pub(super) fn plain_string() -> P<String> {
    string_lit().map(|s| unescape_plain(&s)).boxed()
}

pub(super) fn integer_lit() -> P<i64> {
    select! { Token::Integer(s) => s }
        .try_map(|s, span| {
            s.parse::<i64>()
                .map_err(|_| Simple::custom(span, format!("integer literal `{s}` overflows i64")))
        })
        .boxed()
}
