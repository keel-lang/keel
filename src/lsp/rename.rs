//! Rename support for the language server.
//!
//! [`is_rename_blocked`] guards prelude namespaces, primitive types, and
//! other identifiers that the user cannot legally rename.  The namespace set
//! is derived dynamically from [`prelude_names()`] so it stays in sync as new
//! namespaces are added to the catalog.
//!
//! [`is_top_level_at_offset`] and [`get_usages`] query the semantic index when
//! available, falling back to reparse-on-demand helpers otherwise.

use crate::lexer::Span;
use crate::types::checker;
use crate::types::prelude::namespace_names;

use super::diagnostics::SemanticIndex;

/// Return `true` if `name` is a prelude namespace, primitive type, or
/// built-in identifier that must not be renamed.
pub(crate) fn is_rename_blocked(name: &str) -> bool {
    if namespace_names().contains(name) {
        return true;
    }
    matches!(
        name,
        "int"
            | "float"
            | "str"
            | "bool"
            | "none"
            | "datetime"
            | "duration"
            | "true"
            | "false"
            | "run"
            | "stop"
            | "uuid"
    )
}

/// Return `true` if the identifier at `offset` resolves to a top-level symbol
/// (task, agent, type, interface, extern) — the only symbols safe to rename
/// with the file-wide v0.1 strategy.
///
/// Queries the semantic index when available (fast path, no reparse).
pub(crate) fn is_top_level_at_offset(
    text: &str,
    offset: usize,
    index: Option<&SemanticIndex>,
) -> bool {
    if let Some(idx) = index {
        let Some(ident_span) = checker::ident_span_at_offset(text, offset) else {
            return false;
        };
        return idx.top_level_refs.contains(&ident_span);
    }
    checker::is_top_level_symbol(text, offset)
}

/// Return all spans where `name` appears as an identifier token.
///
/// Queries the pre-built usages index when available; falls back to a token
/// scan when the index is absent.
pub(crate) fn get_usages(text: &str, name: &str, index: Option<&SemanticIndex>) -> Vec<Span> {
    if let Some(idx) = index
        && let Some(spans) = idx.usages.get(name)
    {
        return spans.clone();
    }
    checker::usages_of(text, name)
}
