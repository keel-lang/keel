//! String interpolation lexer and parser for Keel.
//!
//! `parse_interpolation` converts a raw string token body into a
//! `Vec<StringPart>`, splitting on `{expr}` slots.  Nested string
//! literals and escape sequences are handled so inner `{` / `}` do
//! not prematurely close the outer slot.

use chumsky::Stream;
use chumsky::prelude::*;
use logos::Logos;

use crate::ast::{Expr, Node, SpannedExpr, StringPart};
use crate::lexer::{Span, Token, normalize_newlines};

use super::expr::expr_parser;

/// Parse a raw string token body into a sequence of `StringPart`s,
/// expanding `{expr}` interpolation slots.
pub(super) fn parse_interpolation(raw: &str, string_span: &Span) -> Vec<StringPart> {
    let mut parts = Vec::new();
    let mut current = String::with_capacity(raw.len());
    let mut chars = raw.char_indices().peekable();
    // Keel currently has symmetric ASCII delimiters: `"` and `"""`. The
    // token span includes both delimiters while `raw` contains only the body.
    let delimiter_len = string_span
        .end
        .saturating_sub(string_span.start)
        .saturating_sub(raw.len())
        / 2;
    let body_start = string_span.start + delimiter_len;

    while let Some((slot_start, ch)) = chars.next() {
        if ch == '\\' {
            if let Some(&(_, next)) = chars.peek() {
                match next {
                    'n' => {
                        chars.next();
                        current.push('\n');
                    }
                    't' => {
                        chars.next();
                        current.push('\t');
                    }
                    'r' => {
                        chars.next();
                        current.push('\r');
                    }
                    '\\' => {
                        chars.next();
                        current.push('\\');
                    }
                    '"' => {
                        chars.next();
                        current.push('"');
                    }
                    '{' => {
                        chars.next();
                        current.push('{');
                    }
                    '}' => {
                        chars.next();
                        current.push('}');
                    }
                    _ => {
                        current.push('\\');
                        current.push(next);
                        chars.next();
                    }
                }
            }
        } else if ch == '{' {
            if !current.is_empty() {
                parts.push(StringPart::Literal(std::mem::take(&mut current)));
            }
            let mut depth = 1;
            let mut expr_text = String::new();
            while let Some((_, c)) = chars.next() {
                if c == '\\' {
                    // Preserve escape sequences inside the slot so the
                    // nested expression lexer can resolve them.
                    expr_text.push(c);
                    if let Some(&(_, n)) = chars.peek() {
                        chars.next();
                        expr_text.push(n);
                    }
                } else if c == '"' {
                    // Skip over a nested string literal inside the slot
                    // so its `{...}` and `}` characters don't terminate
                    // this interpolation prematurely.
                    expr_text.push(c);
                    let mut inner_depth = 0;
                    while let Some((_, nc)) = chars.next() {
                        if nc == '\\' {
                            expr_text.push(nc);
                            if let Some(&(_, nn)) = chars.peek() {
                                chars.next();
                                expr_text.push(nn);
                            }
                        } else if nc == '{' {
                            inner_depth += 1;
                            expr_text.push(nc);
                        } else if nc == '}' {
                            if inner_depth > 0 {
                                inner_depth -= 1;
                            }
                            expr_text.push(nc);
                        } else if nc == '"' && inner_depth == 0 {
                            expr_text.push(nc);
                            break;
                        } else {
                            expr_text.push(nc);
                        }
                    }
                } else if c == '{' {
                    depth += 1;
                    expr_text.push(c);
                } else if c == '}' {
                    depth -= 1;
                    if depth == 0 {
                        break;
                    }
                    expr_text.push(c);
                } else {
                    expr_text.push(c);
                }
            }
            let (expr_src, fmt_spec) = split_format_spec(&expr_text);
            // Advance past the opening `{` delimiter to the slot body.
            let expr_start = body_start + slot_start + '{'.len_utf8();
            match parse_interp_expr(expr_src, expr_start) {
                Ok(expr) => parts.push(StringPart::Interpolation(
                    Box::new(expr),
                    fmt_spec.map(|s| s.to_string()),
                )),
                // Store the full slot text (before the format-spec split) so
                // the formatter can reconstruct `{expr:spec}` without data loss.
                Err(()) => parts.push(StringPart::ParseError(expr_text.clone())),
            }
        } else {
            current.push(ch);
        }
    }

    if !current.is_empty() {
        parts.push(StringPart::Literal(current));
    }
    if parts.is_empty() {
        parts.push(StringPart::Literal(String::new()));
    }
    parts
}

/// Split `"expr:spec"` into `("expr", Some("spec"))`, respecting brace/bracket/paren depth
/// so colons inside named-argument calls or struct literals are not treated as the
/// format-spec separator.  Returns `(full_text, None)` when no bare colon is found.
fn split_format_spec(text: &str) -> (&str, Option<&str>) {
    let mut depth: i32 = 0;
    let mut in_str = false;
    let mut chars = text.char_indices().peekable();
    while let Some((i, c)) = chars.next() {
        if in_str {
            if c == '\\' {
                chars.next(); // skip escaped char
            } else if c == '"' {
                in_str = false;
            }
            continue;
        }
        match c {
            '"' => in_str = true,
            '{' | '(' | '[' => depth += 1,
            '}' | ')' | ']' => depth -= 1,
            ':' if depth == 0 => {
                // Keep leading padding so `parse_interp_expr` can include it
                // when rebasing the first token's absolute source position.
                let expr_part = &text[..i];
                let spec_part = text[i + 1..].trim();
                if !spec_part.is_empty() {
                    return (expr_part, Some(spec_part));
                }
                // bare trailing colon with no spec — treat as no spec
                return (text, None);
            }
            _ => {}
        }
    }
    (text, None)
}

/// Merge `Integer(n) Ident("_digits…")` and `Float(n) Ident("_digits…")`
/// pairs that result from lexing digit-group separators.
///
/// The logos regex `[0-9]+` stops at `_`, so `1_000_000` lexes as
/// `Integer("1") Ident("_000_000")`.  This function fuses those pairs
/// into a single `Integer("1000000")` token while leaving identifiers
/// (`x1_2`), string literals (`"1_2"`), and all other tokens untouched.
///
/// The merged token inherits the *start* of the integer/float span and the
/// *end* of the last fused identifier span so downstream error reporting
/// remains approximately correct.
fn merge_numeric_separator_tokens(
    raw: Vec<(Token, std::ops::Range<usize>)>,
) -> Vec<(Token, std::ops::Range<usize>)> {
    let mut result: Vec<(Token, std::ops::Range<usize>)> = Vec::with_capacity(raw.len());
    let mut iter = raw.into_iter().peekable();

    while let Some((tok, span)) = iter.next() {
        match tok {
            Token::Integer(ref digits) | Token::Float(ref digits) => {
                let is_int = matches!(tok, Token::Integer(_));
                let mut merged = digits.clone();
                let mut end = span.end;

                // Consume any trailing `_NNN` identifier continuations.
                loop {
                    match iter.peek() {
                        Some((Token::Ident(s), _)) if is_digit_separator_tail(s) => {
                            let (tail_tok, tail_span) = iter.next().unwrap();
                            let Token::Ident(tail) = tail_tok else {
                                unreachable!()
                            };
                            // Append only the digit characters, stripping underscores.
                            for ch in tail.chars().filter(|c| c.is_ascii_digit()) {
                                merged.push(ch);
                            }
                            end = tail_span.end;
                        }
                        _ => break,
                    }
                }

                let merged_tok = if is_int {
                    Token::Integer(merged)
                } else {
                    Token::Float(merged)
                };
                result.push((merged_tok, span.start..end));
            }
            other => result.push((other, span)),
        }
    }
    result
}

/// Returns `true` when `s` looks like a digit-separator tail: starts with
/// `_`, contains at least one ASCII digit, and the remainder is all digits
/// or underscores.  For example `_000`, `_000_000`.
fn is_digit_separator_tail(s: &str) -> bool {
    let after_leading = match s.strip_prefix('_') {
        Some(rest) => rest,
        None => return false,
    };
    !after_leading.is_empty()
        && after_leading
            .chars()
            .all(|c| c.is_ascii_digit() || c == '_')
        && after_leading.chars().any(|c| c.is_ascii_digit())
}

/// Parse a single interpolation slot expression.
///
/// Returns `Ok(expr)` on success, `Err(())` when the slot text is not a valid
/// expression.  The caller is responsible for recording a `StringPart::ParseError`
/// so the type checker can surface a diagnostic.
fn parse_interp_expr(text: &str, source_start: usize) -> Result<SpannedExpr, ()> {
    // Only leading padding shifts token positions. Trailing padding can be
    // trimmed for parsing without changing any token span.
    let leading_padding_len = text.len() - text.trim_start().len();
    let source_start = source_start + leading_padding_len;
    let text = text.trim();
    if text.is_empty() {
        return Ok(Node::synthetic(Expr::StringLit(vec![StringPart::Literal(
            String::new(),
        )])));
    }

    // Lex the interpolation content directly via logos, bypassing the
    // NamedSource wrapper used by the public `lex()` entry point.
    let raw: Vec<_> = Token::lexer(text)
        .spanned()
        .filter_map(|(r, span)| {
            r.ok()
                .map(|t| (t, source_start + span.start..source_start + span.end))
        })
        .collect();

    if raw.is_empty() {
        return Err(());
    }

    // Fuse digit-group-separator pairs such as `Integer("1") Ident("_000")`
    // that arise because the logos integer regex `[0-9]+` does not match `_`.
    // This operates on token payloads only, so string literals (`"1_2"`),
    // plain identifiers (`x1_2`), and all other tokens are unaffected.
    let raw = merge_numeric_separator_tokens(raw);

    // `from` is a reserved keyword that `field_name()` also accepts as a
    // struct-field name, so it can appear as a local variable after
    // destructuring (e.g. `{from, subject} = email`).  The expression
    // parser only recognises `Token::Ident` for variable references, so
    // normalise `Token::From` here so that `{from}` in a slot resolves to
    // `Expr::Ident("from")` rather than a parse failure.
    //
    // Only `from` is remapped — other contextual keywords like `as` (`as`
    // cast operator), `in` (containment), and `set` (set-literal) have
    // expression-level roles and must stay as keyword tokens.
    let raw = raw
        .into_iter()
        .map(|(tok, span)| {
            let normalized = match tok {
                Token::From => Token::Ident("from".to_string()),
                other => other,
            };
            (normalized, span)
        })
        .collect::<Vec<_>>();

    let tokens = normalize_newlines(raw);
    let eoi = source_start + text.len()..source_start + text.len() + 1;
    let stream = Stream::from_iter(eoi, tokens.into_iter());

    // Require the entire slot to be consumed so that a trailing stray
    // token (e.g. `1 +` — missing right operand) is reported as a parse
    // failure rather than silently accepted as `1`.
    expr_parser()
        .then_ignore(end())
        .parse(stream)
        .map_err(|_| ())
}
