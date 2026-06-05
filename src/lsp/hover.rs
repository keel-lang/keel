//! Hover type resolution for the language server.
//!
//! When a [`SemanticIndex`] is available (populated after every successful
//! `did_open` / `did_change`), hover is answered from the pre-built index
//! with zero reparsing.  On parse failure the index is absent and hover
//! falls back to the reparse-on-demand [`checker::type_at`] helper.

use crate::types::checker;

use super::diagnostics::SemanticIndex;

/// Resolve the inferred type label for the identifier at `offset`.
///
/// Queries the semantic index when available (fast path, no reparse).
/// Falls back to [`checker::type_at`] when the index is absent (parse failure).
pub(crate) fn resolve_hover_type(
    text: &str,
    offset: usize,
    index: Option<&SemanticIndex>,
) -> Option<String> {
    if let Some(idx) = index {
        // Fast path: span-keyed lookup (scope-correct for reference sites).
        let ident_span = checker::ident_span_at_offset(text, offset)?;
        if let Some(ty_str) = idx.span_types.get(&ident_span) {
            return Some(ty_str.clone());
        }
        // Fallback within index: name-keyed lookup for declaration sites.
        let name = checker::ident_at_offset(text, offset)?;
        return idx.name_types.get(&name).cloned();
    }
    // No index (parse failure): reparse on demand.
    checker::type_at(text, offset)
}
