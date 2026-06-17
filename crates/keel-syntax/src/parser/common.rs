//! Shared low-level parser primitives: newlines, separators, identifiers,
//! field names, literal helpers, and reusable control-flow combinators.
//! All grammar sub-modules import from here rather than duplicating these
//! building blocks.

use chumsky::input::{Input, MapExtra, MappedInput, Stream};
use chumsky::prelude::*;

use crate::ast::{Block, Expr, MapLitKey, Node, Pattern, SpannedExpr, Stmt, StringPart, WhenArm};
use crate::lexer::{Span, Token};

/// Parser extra: rich errors over the spanned token stream.
pub(super) type Extra = extra::Err<Rich<'static, Token, SimpleSpan>>;
/// fn-pointer that converts each lexer item's `Range<usize>` span into chumsky's
/// `SimpleSpan` as the stream is consumed.
pub(super) type Mapper = fn((Token, Span)) -> (Token, SimpleSpan);
/// Concrete, `'static` token-stream input, kept nameable so every sub-parser can
/// return a boxed [`P`]. Boxing avoids the macOS linker crash on deeply nested
/// chumsky types.
pub(super) type In =
    MappedInput<'static, Token, SimpleSpan, Stream<std::vec::IntoIter<(Token, Span)>>, Mapper>;
pub(super) type P<T> = Boxed<'static, 'static, In, T, Extra>;

/// Compatibility shim reproducing chumsky 0.9's `map_with_span(|value, span|)`
/// — where `span` is a `crate::lexer::Span` (`Range<usize>`) — on top of 0.13's
/// `map_with`. Keeps the grammar's span-arithmetic closures unchanged.
pub(super) trait KeelExt<T: 'static>:
    Parser<'static, In, T, Extra> + Sized + 'static
{
    fn map_with_span<U: 'static, F>(self, f: F) -> P<U>
    where
        F: Fn(T, Span) -> U + 'static,
    {
        self.map_with(move |v, e: &mut MapExtra<'static, '_, In, Extra>| {
            f(v, e.span().into_range())
        })
        .boxed()
    }
}
impl<T: 'static, Pa> KeelExt<T> for Pa where Pa: Parser<'static, In, T, Extra> + Sized + 'static {}

/// Build the chumsky input from pre-lexed, spanned tokens. The lexer's
/// `Range<usize>` spans are converted to chumsky's `SimpleSpan`; `source_len`
/// supplies the end-of-input span.
pub(super) fn token_stream(tokens: Vec<(Token, Span)>, source_len: usize) -> In {
    let eoi: SimpleSpan = (source_len..source_len + 1).into();
    let mapper: Mapper = |(t, s)| (t, s.into());
    Stream::from_iter(tokens).map(eoi, mapper)
}

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

/// An identifier token together with its byte-range span.
///
/// Use this instead of `ident()` when the call site stores the span for
/// IDE features (rename, go-to-definition, diagnostics).
pub(super) fn spanned_ident() -> P<(String, Span)> {
    select! { Token::Ident(s) => s }
        .map_with_span(|s, span| (s, span))
        .boxed()
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
            .map_err(|_| Rich::custom(span, format!("integer key `{s}` overflows i64")))
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
        .ignore_then(
            entry
                .separated_by(field_sep())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
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
                .allow_trailing()
                .collect::<Vec<_>>(),
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
                .map_err(|_| Rich::custom(span, format!("integer literal `{s}` overflows i64")))
        })
        .boxed()
}

// ---------------------------------------------------------------------------
// Shared control-flow combinators
// ---------------------------------------------------------------------------

/// Pattern parser for `when` arms — used by both statement and expression `when`.
pub(super) fn when_pattern() -> P<Pattern> {
    just(Token::Ident("_".to_string()))
        .to(Pattern::Wildcard)
        .or(ident()
            .then(
                just(Token::LBrace)
                    .ignore_then(
                        ident()
                            .separated_by(just(Token::Comma))
                            .allow_trailing()
                            .collect::<Vec<_>>(),
                    )
                    .then_ignore(just(Token::RBrace))
                    .or_not(),
            )
            .map(|(name, bindings)| match bindings {
                Some(b) => Pattern::Variant { name, bindings: b },
                None => Pattern::Ident(name),
            }))
        .or(just(Token::LBrace)
            .ignore_then(
                ident()
                    .separated_by(just(Token::Comma))
                    .allow_trailing()
                    .collect::<Vec<_>>(),
            )
            .then_ignore(just(Token::RBrace))
            .map(|fields| Pattern::Struct { fields }))
        .or(plain_string().map_with_span(|s, span| {
            Pattern::Literal(Node::new(
                Expr::StringLit(vec![StringPart::Literal(s)]),
                span,
            ))
        }))
        .or(integer_lit()
            .map_with_span(|n, span| Pattern::Literal(Node::new(Expr::Integer(n), span))))
        .boxed()
}

/// A `{ stmt* }` block parameterised on the statement parser to use.
/// Replaces the three previously separate block builders (`block` inside
/// `stmt_parser_with`, `inner_block` inside `expr_parser`, and
/// `block_toplevel()`), all of which had identical structure.
pub(super) fn block_with(stmt: P<Node<Stmt>>) -> P<Block> {
    just(Token::LBrace)
        .ignore_then(newlines())
        .ignore_then(
            stmt.separated_by(sep())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .boxed()
}

/// A single `when` arm (`pattern [, pattern]* [where guard] => body`).
/// Parameterised on `expr` (for guards and single-expression bodies) and
/// `block` (for block bodies).  Used by both `when` statement and expression.
pub(super) fn when_arm(expr: P<SpannedExpr>, block: P<Block>) -> P<WhenArm> {
    let arm_body = block
        .or(expr
            .clone()
            .map_with_span(|e, span| vec![Node::new(Stmt::Expr(e), span)]))
        .boxed();

    when_pattern()
        .separated_by(just(Token::Comma))
        .at_least(1)
        .collect::<Vec<_>>()
        .then(just(Token::Where).ignore_then(expr).or_not())
        .then_ignore(just(Token::FatArrow))
        .then(arm_body)
        .map(|((patterns, guard), body)| WhenArm {
            patterns,
            guard,
            body,
        })
        .boxed()
}

/// Core `when subject { arm* }` grammar shared by statements and expressions.
pub(super) fn when_body(expr: P<SpannedExpr>, arm: P<WhenArm>) -> P<(SpannedExpr, Vec<WhenArm>)> {
    just(Token::When)
        .ignore_then(expr)
        .then_ignore(just(Token::LBrace))
        .then_ignore(newlines())
        .then(
            arm.separated_by(newlines())
                .allow_trailing()
                .collect::<Vec<_>>(),
        )
        .then_ignore(newlines())
        .then_ignore(just(Token::RBrace))
        .boxed()
}

/// Core `if cond block [else else_block]` grammar, returning the three parts
/// as a tuple.  The caller supplies `else_block` — typically a `.or(block)`
/// combined with a recursive self-reference — so that both `if_stmt` (which
/// adds `else if` via recursive `Stmt::If`) and `if_expr` (which recurses into
/// `Expr::IfExpr`) can share this foundation.
pub(super) fn if_body(
    expr: P<SpannedExpr>,
    then_block: P<Block>,
    else_block: P<Block>,
) -> P<(SpannedExpr, Block, Option<Block>)> {
    just(Token::If)
        .ignore_then(expr)
        .then(then_block)
        .then(just(Token::Else).ignore_then(else_block).or_not())
        .map(|((cond, then_body), else_body)| (cond, then_body, else_body))
        .boxed()
}
