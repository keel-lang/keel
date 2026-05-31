//! Token-level and HIR-level IDE helpers: identifier lookup and
//! source-navigation primitives used by the language server.
//!
//! [`definition_of`] is the primary entry point.  It parses and lowers the
//! source, then reads the declaration span through HIR symbol IDs,
//! falling back to token-level scanning when parsing fails (e.g. mid-edit).

use crate::hir::{Hir, SymbolKind};
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
/// Parses and lowers the source, then returns the HIR symbol span for the
/// matching `task`, `agent`, `type`, or `interface` declaration.  Falls back
/// to token-level scanning when the source does not parse cleanly (e.g.
/// mid-edit in the LSP).  Returns `None` if the cursor is not on an
/// identifier or no matching declaration exists.
pub fn definition_of(text: &str, offset: usize) -> Option<Span> {
    let name = ident_at_offset(text, offset)?;

    // Attempt a full parse and resolve through HIR symbol IDs.
    let named_src = miette::NamedSource::new("<lsp>", text.to_string());
    if let Ok(tokens) = crate::lexer::lex(text, &named_src)
        && let Ok(prog) = crate::parser::parse(tokens, text.len(), &named_src)
    {
        let hir = crate::hir::lower_ast(&prog);
        let ident_span = ident_span_at_offset(text, offset)?;
        return definition_of_in_hir(&hir, &name, &ident_span);
    }

    // Parse failed (e.g. incomplete edit) — fall back to token scanning.
    definition_of_token_scan(text, &name)
}

/// Return whether the identifier at `offset` resolves to a top-level symbol.
///
/// Rename remains file-wide in v0.1, so local symbols must not pass this gate
/// even though HIR-backed go-to-definition can navigate to them.
pub fn is_top_level_symbol(text: &str, offset: usize) -> bool {
    let Some(name) = ident_at_offset(text, offset) else {
        return false;
    };
    let Some(ident_span) = ident_span_at_offset(text, offset) else {
        return false;
    };
    let named_src = miette::NamedSource::new("<lsp>", text.to_string());
    let Ok(tokens) = crate::lexer::lex(text, &named_src) else {
        return is_top_level_declaration_token(text, &name, &ident_span);
    };
    let Ok(program) = crate::parser::parse(tokens, text.len(), &named_src) else {
        return is_top_level_declaration_token(text, &name, &ident_span);
    };
    let hir = crate::hir::lower_ast(&program);

    hir.resolution_at(&ident_span)
        .symbol
        .and_then(|id| hir.symbol(id))
        .or_else(|| {
            hir.symbols()
                .iter()
                .find(|symbol| symbol.name == name && symbol.span == ident_span)
        })
        .is_some_and(|symbol| {
            matches!(
                symbol.kind,
                SymbolKind::TopTask
                    | SymbolKind::Agent
                    | SymbolKind::Enum
                    | SymbolKind::TypeName
                    | SymbolKind::Interface
                    | SymbolKind::Extern
            )
        })
}

/// Return whether `span` is the name token of a top-level declaration.
///
/// This intentionally recognizes declaration sites only. When parsing fails we
/// cannot safely resolve reference sites without risking a file-wide local
/// rename, but declaration tokens before the broken edit remain unambiguous.
fn is_top_level_declaration_token(text: &str, name: &str, span: &Span) -> bool {
    let tokens: Vec<(Token, Span)> = Token::lexer(text)
        .spanned()
        .filter_map(|(result, span)| result.ok().map(|token| (token, span)))
        .collect();
    let mut brace_depth = 0usize;

    for tokens in tokens.windows(2) {
        match tokens[0].0 {
            Token::LBrace => brace_depth += 1,
            Token::RBrace => brace_depth = brace_depth.saturating_sub(1),
            Token::Task | Token::Agent | Token::Type | Token::Interface if brace_depth == 0 => {
                if let Token::Ident(candidate) = &tokens[1].0
                    && candidate == name
                    && tokens[1].1 == *span
                {
                    return true;
                }
            }
            _ => {}
        }
    }
    false
}

/// Resolve a declaration span through HIR symbols.
fn definition_of_in_hir(hir: &Hir<'_>, name: &str, ident_span: &Span) -> Option<Span> {
    if let Some(symbol) = hir
        .resolution_at(ident_span)
        .symbol
        .and_then(|id| hir.symbol(id))
    {
        return Some(symbol.span.clone());
    }

    hir.symbols()
        .iter()
        .find(|symbol| {
            symbol.name == name && symbol.span == *ident_span && is_definition_symbol(symbol.kind)
        })
        .or_else(|| {
            hir.resolve_global(name)
                .symbol
                .and_then(|id| hir.symbol(id))
        })
        .map(|symbol| symbol.span.clone())
}

fn is_definition_symbol(kind: SymbolKind) -> bool {
    matches!(
        kind,
        SymbolKind::TopTask
            | SymbolKind::Agent
            | SymbolKind::Enum
            | SymbolKind::TypeName
            | SymbolKind::Interface
            | SymbolKind::Extern
            | SymbolKind::Method
    )
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
