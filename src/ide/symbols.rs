//! Token-level and AST-level IDE helpers: identifier lookup and
//! source-navigation primitives used by the language server.
//!
//! [`definition_of`] is the primary entry point.  It parses the source into
//! an AST and reads the stored `name_span` from the relevant declaration,
//! falling back to token-level scanning when parsing fails (e.g. mid-edit).

use crate::ast::{AgentItem, Decl, Program};
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
/// Parses the source into an AST and returns the stored `name_span` for the
/// matching `task`, `agent`, `type`, or `interface` declaration.  Falls back
/// to token-level scanning when the source does not parse cleanly (e.g.
/// mid-edit in the LSP).  Returns `None` if the cursor is not on an
/// identifier or no matching declaration exists.
pub fn definition_of(text: &str, offset: usize) -> Option<Span> {
    let name = ident_at_offset(text, offset)?;

    // Attempt a full parse and walk stored spans.
    let named_src = miette::NamedSource::new("<lsp>", text.to_string());
    if let Ok(tokens) = crate::lexer::lex(text, &named_src)
        && let Ok(prog) = crate::parser::parse(tokens, text.len(), &named_src)
    {
        return definition_of_in_program(&prog, &name);
    }

    // Parse failed (e.g. incomplete edit) — fall back to token scanning.
    definition_of_token_scan(text, &name)
}

/// Walk a parsed [`Program`] looking for a declaration whose stored `name_span`
/// belongs to `name`.  Checks top-level `task`, `agent`, `type`, `interface`,
/// agent-nested tasks, and `impl` method bodies.
fn definition_of_in_program(prog: &Program, name: &str) -> Option<Span> {
    for node in &prog.declarations {
        match &node.kind {
            Decl::Task(t) if t.name == name => return Some(t.name_span.clone()),
            Decl::Agent(a) => {
                if a.name == name {
                    return Some(a.name_span.clone());
                }
                for item in &a.items {
                    if let AgentItem::Task(t) = item
                        && t.name == name
                    {
                        return Some(t.name_span.clone());
                    }
                }
            }
            Decl::Type(t) if t.name == name => return Some(t.name_span.clone()),
            Decl::Interface(i) if i.name == name => return Some(i.name_span.clone()),
            Decl::Impl(impl_decl) => {
                for m in &impl_decl.methods {
                    if m.name == name {
                        return Some(m.name_span.clone());
                    }
                }
            }
            _ => {}
        }
    }
    None
}

/// Token-level fallback for when the source cannot be parsed.
///
/// Scans for `task NAME`, `agent NAME`, or `type NAME` bigrams and returns
/// the span of the `NAME` token.
fn definition_of_token_scan(text: &str, name: &str) -> Option<Span> {
    let tokens: Vec<(Token, Span)> = Token::lexer(text)
        .spanned()
        .filter_map(|(r, s)| r.ok().map(|t| (t, s)))
        .collect();

    for i in 0..tokens.len().saturating_sub(1) {
        match (&tokens[i].0, &tokens[i + 1].0) {
            (Token::Task | Token::Agent | Token::Type, Token::Ident(n)) if n == name => {
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
