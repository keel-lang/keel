//! Token-level IDE helpers: identifier lookup and source-navigation
//! primitives used by the language server.
//!
//! These functions operate entirely on the raw source text via the lexer
//! — they do not run the type checker.

use crate::lexer::{Span, Token};
use logos::Logos;

/// Return the identifier at the given UTF-8 byte offset, or `None` if the
/// cursor is not on an identifier token.
pub fn ident_at_offset(text: &str, offset: usize) -> Option<String> {
    for (result, span) in Token::lexer(text).spanned() {
        if span.start > offset {
            break;
        }
        if span.end < offset {
            continue;
        }
        if let Ok(Token::Ident(n)) = result {
            return Some(n);
        }
    }
    None
}

/// Return the span of the identifier at the given byte offset, or `None` if
/// the cursor is not on an identifier token.
pub fn ident_span_at_offset(text: &str, offset: usize) -> Option<Span> {
    for (result, span) in Token::lexer(text).spanned() {
        if span.start > offset {
            break;
        }
        if span.end < offset {
            continue;
        }
        if let Ok(Token::Ident(_)) = result {
            return Some(span);
        }
    }
    None
}

/// Find the declaration span of the identifier at `offset`.
///
/// Returns the span of the declared name in `task`, `agent`, or `type`
/// declarations.  Returns `None` if the cursor is not on an identifier or
/// no matching declaration is found.
pub fn definition_of(text: &str, offset: usize) -> Option<Span> {
    let name = ident_at_offset(text, offset)?;

    let tokens: Vec<(Token, Span)> = Token::lexer(text)
        .spanned()
        .filter_map(|(r, s)| r.ok().map(|t| (t, s)))
        .collect();

    for i in 0..tokens.len().saturating_sub(1) {
        match (&tokens[i].0, &tokens[i + 1].0) {
            (Token::Task | Token::Agent | Token::Type, Token::Ident(n)) if n == &name => {
                return Some(tokens[i + 1].1.clone());
            }
            _ => {}
        }
    }
    None
}

/// Return all spans where `name` appears as an identifier token.
pub fn usages_of(text: &str, name: &str) -> Vec<Span> {
    Token::lexer(text)
        .spanned()
        .filter_map(|(r, s)| r.ok().map(|t| (t, s)))
        .filter_map(|(tok, span)| {
            if let Token::Ident(n) = tok
                && n == name
            {
                return Some(span);
            }
            None
        })
        .collect()
}
