//! String interpolation lexer and parser for Keel.
//!
//! `parse_interpolation` converts a raw string token body into a
//! `Vec<StringPart>`, splitting on `{expr}` slots.  Nested string
//! literals and escape sequences are handled so inner `{` / `}` do
//! not prematurely close the outer slot.

use chumsky::Stream;
use chumsky::prelude::*;
use logos::Logos;

use crate::ast::{Expr, StringPart};
use crate::lexer::{Token, normalize_newlines};

use super::expr::expr_parser;

/// Parse a raw string token body into a sequence of `StringPart`s,
/// expanding `{expr}` interpolation slots.
pub(super) fn parse_interpolation(raw: &str) -> Vec<StringPart> {
    let mut parts = Vec::new();
    let mut current = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '\\' {
            if let Some(&next) = chars.peek() {
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
            while let Some(c) = chars.next() {
                if c == '\\' {
                    // Preserve escape sequences inside the slot so the
                    // nested expression lexer can resolve them.
                    expr_text.push(c);
                    if let Some(&n) = chars.peek() {
                        chars.next();
                        expr_text.push(n);
                    }
                } else if c == '"' {
                    // Skip over a nested string literal inside the slot
                    // so its `{...}` and `}` characters don't terminate
                    // this interpolation prematurely.
                    expr_text.push(c);
                    let mut inner_depth = 0;
                    while let Some(nc) = chars.next() {
                        if nc == '\\' {
                            expr_text.push(nc);
                            if let Some(&nn) = chars.peek() {
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
            parts.push(StringPart::Interpolation(
                Box::new(parse_interp_expr(expr_src)),
                fmt_spec.map(|s| s.to_string()),
            ));
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
                let expr_part = text[..i].trim();
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

fn parse_interp_expr(text: &str) -> Expr {
    let text = text.trim();
    if text.is_empty() {
        return Expr::StringLit(vec![StringPart::Literal(String::new())]);
    }

    // Lex the interpolation content directly via logos, bypassing the
    // NamedSource wrapper used by the public `lex()` entry point.
    let raw: Vec<_> = Token::lexer(text)
        .spanned()
        .filter_map(|(r, span)| r.ok().map(|t| (t, span)))
        .collect();

    if raw.is_empty() {
        return Expr::Ident(text.to_string());
    }

    let tokens = normalize_newlines(raw);
    let eoi = text.len()..text.len() + 1;
    let stream = Stream::from_iter(eoi, tokens.into_iter());

    // On parse failure, fall back to treating the whole slot as an identifier
    // so a bad expression never silently produces a corrupt AST node.
    expr_parser()
        .parse(stream)
        .unwrap_or_else(|_| Expr::Ident(text.to_string()))
}
